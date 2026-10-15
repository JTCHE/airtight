//! HMAC-SHA512 (RFC 2104) and HKDF-SHA512 (RFC 5869).

use crate::sha512::{BLOCK, OUTPUT, Sha512, sha512};

pub fn hmac_sha512(key: &[u8], message: &[u8]) -> [u8; OUTPUT] {
    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        padded[..OUTPUT].copy_from_slice(&sha512(&[key]));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha512::default();
    inner.update(&padded.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha512::default();
    outer.update(&padded.map(|b| b ^ 0x5c));
    outer.update(&inner.finish());
    outer.finish()
}

/// Extract and expand in one call; `length` is at most 255 * 64.
pub fn hkdf_sha512(secret: &[u8], salt: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    let pseudo_random_key = hmac_sha512(salt, secret);
    let mut out = Vec::with_capacity(length);
    let mut previous: Vec<u8> = Vec::new();
    let mut counter = 1u8;
    while out.len() < length {
        let mut message = previous.clone();
        message.extend_from_slice(info);
        message.push(counter);
        previous = hmac_sha512(&pseudo_random_key, &message).to_vec();
        out.extend_from_slice(&previous);
        counter += 1;
    }
    out.truncate(length);
    out
}

/// A 32-byte key, the size every AirPlay derivation asks for.
pub fn hkdf_key(secret: &[u8], salt: &str, info: &str) -> [u8; 32] {
    hkdf_sha512(secret, salt.as_bytes(), info.as_bytes(), 32).try_into().unwrap()
}
