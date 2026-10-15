//! Realtime (type 96) AirPlay 2 RTP: audio packets, the time-sync packet,
//! and retransmit requests and replies.

use airtight_crypto::chacha;

pub const FRAMES_PER_PACKET: u32 = 352;
pub const PAYLOAD_TYPE: u8 = 0x60;

/// One audio packet: RTP header, ChaCha20-Poly1305 payload, tag, and the
/// 8 nonce bytes the receiver needs to open it.
pub fn audio_packet(key: &[u8; 32], first: bool, sequence: u16, timestamp: u32, alac: &[u8]) -> Vec<u8> {
    let mut header = [0u8; 12];
    header[0] = 0x80;
    header[1] = if first { 0x80 | PAYLOAD_TYPE } else { PAYLOAD_TYPE };
    header[2..4].copy_from_slice(&sequence.to_be_bytes());
    header[4..8].copy_from_slice(&timestamp.to_be_bytes());
    // bytes 8..12, the SSRC, stay zero under PTP
    let mut nonce = [0u8; 12];
    nonce[4..6].copy_from_slice(&sequence.to_le_bytes());
    let mut payload = alac.to_vec();
    let tag = chacha::seal(key, &nonce, &header[4..12], &mut payload);
    let mut out = Vec::with_capacity(12 + payload.len() + 24);
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&tag);
    out.extend_from_slice(&nonce[4..12]);
    out
}

/// The receiver's offset between "the frame that plays now" and the frame
/// number in the sync packet, and the buffer span it derives its latency from.
const SYNC_FRAME_OFFSET: u32 = 11035;
const SYNC_SPAN: u32 = 77175;

/// The anchor packet for the control port: `playing` is the frame that
/// sounds at `wall_ns` on the PTP timeline of clock `clock_id`.
pub fn sync_packet(first: bool, playing: u32, wall_ns: u64, clock_id: u64) -> [u8; 28] {
    let mut out = [0u8; 28];
    out[0] = if first { 0x90 } else { 0x80 };
    out[1] = 0xd7;
    out[3] = 0x06;
    let frame = playing.wrapping_add(SYNC_FRAME_OFFSET);
    out[4..8].copy_from_slice(&frame.to_be_bytes());
    out[8..16].copy_from_slice(&wall_ns.to_be_bytes());
    out[16..20].copy_from_slice(&frame.wrapping_add(SYNC_SPAN).to_be_bytes());
    out[20..28].copy_from_slice(&clock_id.to_be_bytes());
    out
}

/// The RTP time in an audio packet's header.
pub fn timestamp(packet: &[u8]) -> u32 {
    u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]])
}

pub struct ResendRequest {
    /// The request's own sequence number, echoed in each reply.
    pub request: u16,
    pub first: u16,
    pub count: u16,
}

pub fn parse_resend_request(packet: &[u8]) -> Option<ResendRequest> {
    if packet.len() < 8 || packet[1] & 0x7f != 0x55 {
        return None;
    }
    let word = |at: usize| u16::from_be_bytes([packet[at], packet[at + 1]]);
    Some(ResendRequest { request: word(2), first: word(4), count: word(6) })
}

/// The reply that carries one stored packet again.
pub fn resend_reply(reply_sequence: u16, original: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + original.len());
    out.extend_from_slice(&[0x80, 0xd6]);
    out.extend_from_slice(&reply_sequence.to_be_bytes());
    out.extend_from_slice(original);
    out
}
