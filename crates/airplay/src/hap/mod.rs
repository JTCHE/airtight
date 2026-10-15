//! HAP framing for an encrypted TCP channel: 2-byte little-endian length,
//! ChaCha20-Poly1305 ciphertext, 16-byte tag. Each direction counts its
//! own nonces.

use airtight_crypto::chacha;

const MAX_PLAIN: usize = 1024;

pub struct Cipher {
    write_key: [u8; 32],
    read_key: [u8; 32],
    write_count: u64,
    read_count: u64,
    pending: Vec<u8>,
}

fn nonce(count: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&count.to_le_bytes());
    nonce
}

impl Cipher {
    pub fn new(write_key: [u8; 32], read_key: [u8; 32]) -> Self {
        Self { write_key, read_key, write_count: 0, read_count: 0, pending: Vec::new() }
    }

    pub fn seal(&mut self, mut plain: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(plain.len() + plain.len() / MAX_PLAIN * 18 + 18);
        while !plain.is_empty() {
            let take = plain.len().min(MAX_PLAIN);
            let length = (take as u16).to_le_bytes();
            let mut chunk = plain[..take].to_vec();
            let tag = chacha::seal(&self.write_key, &nonce(self.write_count), &length, &mut chunk);
            self.write_count += 1;
            out.extend_from_slice(&length);
            out.extend_from_slice(&chunk);
            out.extend_from_slice(&tag);
            plain = &plain[take..];
        }
        out
    }

    /// Take received bytes; return the plaintext of every whole frame so
    /// far. None when a frame fails to authenticate.
    pub fn open(&mut self, received: &[u8]) -> Option<Vec<u8>> {
        self.pending.extend_from_slice(received);
        let mut plain = Vec::new();
        loop {
            if self.pending.len() < 2 {
                return Some(plain);
            }
            let length = u16::from_le_bytes([self.pending[0], self.pending[1]]) as usize;
            if self.pending.len() < 2 + length + chacha::TAG {
                return Some(plain);
            }
            let mut chunk = self.pending[2..2 + length].to_vec();
            let tag = &self.pending[2 + length..2 + length + chacha::TAG];
            let aad = [self.pending[0], self.pending[1]];
            if !chacha::open(&self.read_key, &nonce(self.read_count), &aad, &mut chunk, tag) {
                return None;
            }
            self.read_count += 1;
            plain.extend_from_slice(&chunk);
            self.pending.drain(..2 + length + chacha::TAG);
        }
    }
}
