//! Who we are to the receiver. The DACP id is stable per PC, so the
//! receiver sees the same sender each session; the PTP clock id, the
//! deviceID and the MAC address all derive from it.

use airtight_crypto::{random, sha512::sha512};
use airtight_wire::hex;

#[derive(Clone)]
pub struct Identity {
    /// 16 upper-case hex digits.
    pub dacp_id: String,
    pub active_remote: String,
    pub clock_id: u64,
    /// "1A:2B:..." over all 8 id bytes.
    pub device_id: String,
    /// "1A:2B:..." over the first 6 id bytes.
    pub mac_address: String,
    /// A UUID derived from the id, for MediaRemote.
    pub device_uuid: String,
    /// The name receivers show for us.
    pub name: String,
}

impl Identity {
    pub fn new(name: &str) -> Identity {
        let digest = sha512(&[b"airtight sender ", name.as_bytes()]);
        let id: [u8; 8] = digest[..8].try_into().unwrap();
        let mut uuid: [u8; 16] = digest[8..24].try_into().unwrap();
        uuid[6] = uuid[6] & 0x0f | 0x50;
        uuid[8] = uuid[8] & 0x3f | 0x80;
        Identity {
            dacp_id: hex::upper_joined(&id, ""),
            active_remote: random::u32().to_string(),
            clock_id: u64::from_be_bytes(id),
            device_id: hex::upper_joined(&id, ":"),
            mac_address: hex::upper_joined(&id[..6], ":"),
            device_uuid: hex::uuid(&uuid),
            name: name.to_string(),
        }
    }
}

/// A fresh random UUID string.
pub fn random_uuid() -> String {
    hex::uuid(&random::bytes())
}
