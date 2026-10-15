//! ChaCha20-Poly1305 AEAD (RFC 8439).

mod poly1305;

use poly1305::Poly1305;

pub const TAG: usize = 16;

fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

fn block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let mut state = [0u32; 16];
    state[..4].copy_from_slice(&[0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]);
    for i in 0..8 {
        state[4 + i] = u32::from_le_bytes(key[i * 4..i * 4 + 4].try_into().unwrap());
    }
    state[12] = counter;
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes(nonce[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let mut working = state;
    for _ in 0..10 {
        quarter_round(&mut working, 0, 4, 8, 12);
        quarter_round(&mut working, 1, 5, 9, 13);
        quarter_round(&mut working, 2, 6, 10, 14);
        quarter_round(&mut working, 3, 7, 11, 15);
        quarter_round(&mut working, 0, 5, 10, 15);
        quarter_round(&mut working, 1, 6, 11, 12);
        quarter_round(&mut working, 2, 7, 8, 13);
        quarter_round(&mut working, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for i in 0..16 {
        let word = working[i].wrapping_add(state[i]);
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// XOR `data` with the key stream that starts at block `counter`.
pub fn chacha20(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8]) {
    for (i, chunk) in data.chunks_mut(64).enumerate() {
        let stream = block(key, counter.wrapping_add(i as u32), nonce);
        for (byte, k) in chunk.iter_mut().zip(stream) {
            *byte ^= k;
        }
    }
}

fn tag(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext: &[u8]) -> [u8; TAG] {
    let one_time_key: [u8; 32] = block(key, 0, nonce)[..32].try_into().unwrap();
    let mut mac = Poly1305::new(&one_time_key);
    mac.update_padded(aad);
    mac.update_padded(ciphertext);
    let mut lengths = [0u8; 16];
    lengths[..8].copy_from_slice(&(aad.len() as u64).to_le_bytes());
    lengths[8..].copy_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    mac.update_padded(&lengths);
    mac.finish()
}

/// Encrypt `data` in place and return its tag.
pub fn seal(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8]) -> [u8; TAG] {
    chacha20(key, 1, nonce, data);
    tag(key, nonce, aad, data)
}

/// Check the tag, then decrypt `data` in place. False if the tag is wrong.
pub fn open(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8], expected: &[u8]) -> bool {
    let actual = tag(key, nonce, aad, data);
    let differ = actual.iter().zip(expected).fold(0u8, |acc, (a, b)| acc | (a ^ b));
    if expected.len() != TAG || differ != 0 {
        return false;
    }
    chacha20(key, 1, nonce, data);
    true
}
