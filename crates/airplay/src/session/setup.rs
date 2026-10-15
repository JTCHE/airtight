//! The SETUP bodies for a PTP realtime session, and what we read back.

use crate::identity::{Identity, random_uuid};
use airtight_wire::plist::{self, Value};
use std::net::Ipv4Addr;

/// ALAC, 44100 Hz, 16 bits, 2 channels.
const AUDIO_FORMAT: i64 = 0x40000;
const REALTIME: i64 = 96;
const COMPRESSION_ALAC: i64 = 2;
const LATENCY_MIN: i64 = 11025;
const LATENCY_MAX: i64 = 88200;

pub struct Ids {
    pub session_id: u32,
    pub session_uuid: String,
    pub group_uuid: String,
}

fn timing_peer(peer_id: &str, clock_id: u64, address: Ipv4Addr) -> Value {
    Value::dict(vec![
        ("ID", Value::string(peer_id)),
        ("DeviceType", Value::Int(0)),
        ("ClockID", Value::Int(clock_id as i64)),
        ("SupportsClockPortMatchingOverride", Value::Bool(false)),
        ("Addresses", Value::Array(vec![Value::string(&address.to_string())])),
    ])
}

pub fn session(identity: &Identity, ids: &Ids, address: Ipv4Addr) -> Vec<u8> {
    let peer_id = random_uuid();
    plist::serialize(&Value::dict(vec![
        ("timingProtocol", Value::string("PTP")),
        ("deviceID", Value::string(&identity.device_id)),
        ("sessionUUID", Value::string(&ids.session_uuid)),
        ("name", Value::string(&identity.name)),
        ("macAddress", Value::string(&identity.mac_address)),
        ("groupUUID", Value::string(&ids.group_uuid)),
        ("groupContainsGroupLeader", Value::Bool(false)),
        ("timingPeerInfo", timing_peer(&peer_id, identity.clock_id, address)),
        ("timingPeerList", Value::Array(vec![timing_peer(&peer_id, identity.clock_id, address)])),
    ]))
}

pub fn stream(ids: &Ids, key: &[u8; 32], data_port: u16, control_port: u16) -> Vec<u8> {
    let stream = Value::dict(vec![
        ("audioFormat", Value::Int(AUDIO_FORMAT)),
        ("audioMode", Value::string("default")),
        ("controlPort", Value::Int(i64::from(control_port))),
        ("ct", Value::Int(COMPRESSION_ALAC)),
        ("dataPort", Value::Int(i64::from(data_port))),
        ("isMedia", Value::Bool(true)),
        ("latencyMax", Value::Int(LATENCY_MAX)),
        ("latencyMin", Value::Int(LATENCY_MIN)),
        ("shk", Value::Data(key.to_vec())),
        ("spf", Value::Int(i64::from(airtight_wire::rtp::FRAMES_PER_PACKET))),
        ("sr", Value::Int(i64::from(crate::audio::SAMPLE_RATE))),
        ("streamConnectionID", Value::Int(i64::from(ids.session_id))),
        ("supportsDynamicStreamID", Value::Bool(false)),
        ("type", Value::Int(REALTIME)),
    ]);
    plist::serialize(&Value::dict(vec![("streams", Value::Array(vec![stream]))]))
}

pub fn peers(receiver: Ipv4Addr, ours: Ipv4Addr) -> Vec<u8> {
    plist::serialize(&Value::Array(vec![
        Value::string(&receiver.to_string()),
        Value::string(&ours.to_string()),
    ]))
}

/// The receiver's event port from the session SETUP reply.
pub fn event_port(reply: &[u8]) -> Option<u16> {
    port(plist::parse(reply)?.get("eventPort")?)
}

fn port(value: &Value) -> Option<u16> {
    value.as_int().and_then(|p| u16::try_from(p).ok()).filter(|&p| p >= 1024)
}

pub struct StreamReply {
    pub data_port: u16,
    pub control_port: u16,
    /// The receiver's buffer window in frames, when it says.
    pub latency_min: Option<u32>,
    pub latency_max: Option<u32>,
}

pub fn stream_reply(reply: &[u8]) -> Option<StreamReply> {
    let root = plist::parse(reply)?;
    let stream = root.get("streams")?.as_array()?.first()?;
    let frames = |key: &str| {
        let found = stream.get(key).or_else(|| root.get(key));
        found.and_then(Value::as_int).and_then(|v| u32::try_from(v).ok()).filter(|&v| v > 0)
    };
    Some(StreamReply {
        data_port: port(stream.get("dataPort")?)?,
        control_port: port(stream.get("controlPort")?)?,
        latency_min: frames("latencyMin"),
        latency_max: frames("latencyMax"),
    })
}

/// The volume command: 0..100 maps linearly onto -30..0 dB; 0 mutes.
pub fn volume(percent: u8) -> Vec<u8> {
    let db = if percent == 0 { -144.0 } else { -30.0 + 30.0 * f64::from(percent.min(100)) / 100.0 };
    format!("volume: {db:.6}\r\n").into_bytes()
}
