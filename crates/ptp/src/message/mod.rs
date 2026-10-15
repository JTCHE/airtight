//! PTPv2 messages in the shape iOS senders use (gPTP profile, majorSdoId 1).

pub const HEADER: usize = 34;

pub const SYNC: u8 = 0x0;
pub const DELAY_REQ: u8 = 0x1;
pub const PDELAY_REQ: u8 = 0x2;
pub const PDELAY_RESP: u8 = 0x3;
pub const FOLLOW_UP: u8 = 0x8;
pub const DELAY_RESP: u8 = 0x9;
pub const PDELAY_RESP_FOLLOW_UP: u8 = 0xa;
pub const ANNOUNCE: u8 = 0xb;
pub const SIGNALING: u8 = 0xc;

const TWO_STEP: u16 = 0x0200;
const UNICAST: u16 = 0x0400;
const PTP_TIMESCALE: u16 = 0x0008;

const LOG_SYNC_INTERVAL: i8 = -3;
const REQUEST_UNICAST: u16 = 0x0004;
const GRANT_UNICAST: u16 = 0x0005;

/// The grandmaster dataset iOS announces: it must outrank the receiver's own.
const PRIORITY: u8 = 128;
const CLOCK_CLASS: u8 = 6;
const CLOCK_ACCURACY: u8 = 0x21;
const LOG_VARIANCE: u16 = 0x436a;
const TIME_SOURCE_GPS: u8 = 0x20;

pub fn kind(message: &[u8]) -> u8 {
    message[0] & 0x0f
}

pub fn sequence(message: &[u8]) -> u16 {
    u16::from_be_bytes([message[30], message[31]])
}

pub fn source_clock(message: &[u8]) -> u64 {
    u64::from_be_bytes(message[20..28].try_into().unwrap())
}

fn header(kind: u8, length: usize, flags: u16, clock_id: u64, sequence: u16, control: u8, log: i8) -> Vec<u8> {
    let mut b = vec![0u8; length];
    b[0] = 0x10 | kind;
    b[1] = 2;
    b[2..4].copy_from_slice(&(length as u16).to_be_bytes());
    b[6..8].copy_from_slice(&flags.to_be_bytes());
    b[20..28].copy_from_slice(&clock_id.to_be_bytes());
    b[28..30].copy_from_slice(&[0x80, 0x05]); // port number, as iOS
    b[30..32].copy_from_slice(&sequence.to_be_bytes());
    b[32] = control;
    b[33] = log as u8;
    b
}

/// 48-bit seconds and 32-bit nanoseconds.
pub fn put_timestamp(out: &mut [u8], ns: u64) {
    let seconds = ns / 1_000_000_000;
    out[..6].copy_from_slice(&seconds.to_be_bytes()[2..]);
    out[6..10].copy_from_slice(&((ns % 1_000_000_000) as u32).to_be_bytes());
}

/// Apple ORGANIZATION_EXTENSION TLV header: type 3, length, OUI, subtype.
fn organization_tlv(out: &mut [u8], length: u16, oui: [u8; 3], subtype: u8) {
    out[0..2].copy_from_slice(&3u16.to_be_bytes());
    out[2..4].copy_from_slice(&length.to_be_bytes());
    out[4..7].copy_from_slice(&oui);
    out[9] = subtype;
}

const APPLE_OUI: [u8; 3] = [0x00, 0x0d, 0x93];

pub fn announce(clock_id: u64, sequence: u16) -> Vec<u8> {
    let mut b = header(ANNOUNCE, HEADER + 42, UNICAST | PTP_TIMESCALE, clock_id, sequence, 0, 0);
    let body = &mut b[HEADER..];
    // origin timestamp and UTC offset stay zero, as iOS sends them
    body[13] = PRIORITY;
    body[14] = CLOCK_CLASS;
    body[15] = CLOCK_ACCURACY;
    body[16..18].copy_from_slice(&LOG_VARIANCE.to_be_bytes());
    body[18] = PRIORITY;
    body[19..27].copy_from_slice(&clock_id.to_be_bytes());
    body[29] = TIME_SOURCE_GPS;
    // PATH_TRACE TLV with our clock id
    body[30..34].copy_from_slice(&[0x00, 0x08, 0x00, 0x08]);
    body[34..42].copy_from_slice(&clock_id.to_be_bytes());
    b
}

/// The Signaling iOS pairs with each Announce: two Apple TLVs.
pub fn signaling(clock_id: u64, sequence: u16) -> Vec<u8> {
    let mut b = header(SIGNALING, HEADER + 72, UNICAST | PTP_TIMESCALE, clock_id, sequence, 5, -128);
    let first = HEADER + 10;
    organization_tlv(&mut b[first..], 22, APPLE_OUI, 1);
    b[first + 12..first + 14].copy_from_slice(&[0x03, 0x01]);
    let second = first + 26;
    organization_tlv(&mut b[second..], 32, APPLE_OUI, 5);
    b[second + 12..second + 14].copy_from_slice(&[0x03, 0x01]);
    b
}

/// Two-step Sync: the time goes in the Follow_Up.
pub fn sync(clock_id: u64, sequence: u16) -> Vec<u8> {
    let flags = UNICAST | PTP_TIMESCALE | TWO_STEP;
    header(SYNC, HEADER + 10, flags, clock_id, sequence, 0, LOG_SYNC_INTERVAL)
}

pub fn follow_up(clock_id: u64, sequence: u16, egress_ns: u64) -> Vec<u8> {
    let mut b = header(FOLLOW_UP, HEADER + 62, UNICAST | PTP_TIMESCALE, clock_id, sequence, 0, LOG_SYNC_INTERVAL);
    put_timestamp(&mut b[HEADER..], egress_ns);
    let ieee = HEADER + 10;
    organization_tlv(&mut b[ieee..], 28, [0x00, 0x80, 0xc2], 1);
    let apple = ieee + 32;
    organization_tlv(&mut b[apple..], 16, APPLE_OUI, 4);
    b[apple + 10..apple + 18].copy_from_slice(&clock_id.to_be_bytes());
    b
}

/// The answer to a Delay_Req or Pdelay_Req: our receive time and the
/// requester's port identity. `kind` picks which.
pub fn response(kind: u8, clock_id: u64, request: &[u8], time_ns: u64) -> Vec<u8> {
    let mut flags = UNICAST | PTP_TIMESCALE;
    if kind != PDELAY_RESP_FOLLOW_UP {
        flags |= TWO_STEP;
    }
    let mut b = header(kind, HEADER + 20, flags, clock_id, sequence(request), 0, LOG_SYNC_INTERVAL);
    put_timestamp(&mut b[HEADER..], time_ns);
    b[HEADER + 10..HEADER + 20].copy_from_slice(&request[20..30]);
    b
}

/// Grant every unicast request in a Signaling message. None when it holds none.
pub fn grant(clock_id: u64, request: &[u8]) -> Option<Vec<u8>> {
    let mut grants: Vec<[u8; 12]> = Vec::new();
    let mut at = HEADER + 10;
    while at + 4 <= request.len() && grants.len() < 8 {
        let kind = u16::from_be_bytes([request[at], request[at + 1]]);
        let length = u16::from_be_bytes([request[at + 2], request[at + 3]]) as usize;
        let value = request.get(at + 4..at + 4 + length)?;
        if kind == REQUEST_UNICAST && length >= 6 {
            let mut duration = u32::from_be_bytes(value[2..6].try_into().unwrap());
            if duration == 0 {
                duration = 300;
            }
            let mut tlv = [0u8; 12];
            tlv[0..2].copy_from_slice(&GRANT_UNICAST.to_be_bytes());
            tlv[3] = 8;
            tlv[4] = value[0] & 0xf0;
            tlv[5] = value[1];
            tlv[6..10].copy_from_slice(&duration.to_be_bytes());
            tlv[11] = 1; // renewal invited
            grants.push(tlv);
        }
        at += 4 + length;
    }
    if grants.is_empty() {
        return None;
    }
    let mut b = header(SIGNALING, HEADER + 10 + 12 * grants.len(), UNICAST, clock_id, sequence(request), 5, 0x7f);
    b[HEADER..HEADER + 10].copy_from_slice(&request[20..30]);
    for (i, tlv) in grants.iter().enumerate() {
        b[HEADER + 10 + 12 * i..HEADER + 22 + 12 * i].copy_from_slice(tlv);
    }
    Some(b)
}
