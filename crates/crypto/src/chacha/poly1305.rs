//! Poly1305 with 44-bit limbs (the "donna-64" layout).

const MASK44: u64 = 0xfff_ffff_ffff;
const MASK42: u64 = 0x3ff_ffff_ffff;

pub struct Poly1305 {
    r: [u64; 3],
    h: [u64; 3],
    pad: [u64; 2],
}

fn le64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}

impl Poly1305 {
    pub fn new(key: &[u8; 32]) -> Self {
        let t0 = le64(&key[0..8]);
        let t1 = le64(&key[8..16]);
        let r = [
            t0 & 0xffc_0fff_ffff,
            ((t0 >> 44) | (t1 << 20)) & 0xfff_ffc0_ffff,
            (t1 >> 24) & 0x00f_ffff_fc0f,
        ];
        Self { r, h: [0; 3], pad: [le64(&key[16..24]), le64(&key[24..32])] }
    }

    /// Feed `data`, zero-padded to a whole number of 16-byte blocks (the AEAD layout).
    pub fn update_padded(&mut self, data: &[u8]) {
        for chunk in data.chunks(16) {
            let mut block = [0u8; 16];
            block[..chunk.len()].copy_from_slice(chunk);
            self.block(&block);
        }
    }

    fn block(&mut self, m: &[u8; 16]) {
        let [r0, r1, r2] = self.r.map(u128::from);
        let s1 = r1 * 20;
        let s2 = r2 * 20;
        let t0 = le64(&m[0..8]);
        let t1 = le64(&m[8..16]);
        let h0 = u128::from(self.h[0] + (t0 & MASK44));
        let h1 = u128::from(self.h[1] + (((t0 >> 44) | (t1 << 20)) & MASK44));
        let h2 = u128::from(self.h[2] + (((t1 >> 24) & MASK42) | (1 << 40)));
        let d0 = h0 * r0 + h1 * s2 + h2 * s1;
        let mut d1 = h0 * r1 + h1 * r0 + h2 * s2;
        let mut d2 = h0 * r2 + h1 * r1 + h2 * r0;
        let mut carry = d0 >> 44;
        let mut n0 = (d0 as u64) & MASK44;
        d1 += carry;
        carry = d1 >> 44;
        let n1 = (d1 as u64) & MASK44;
        d2 += carry;
        let c = (d2 >> 42) as u64;
        let n2 = (d2 as u64) & MASK42;
        n0 += c * 5;
        let c = n0 >> 44;
        self.h = [n0 & MASK44, n1 + c, n2];
    }

    pub fn finish(self) -> [u8; 16] {
        let [mut h0, mut h1, mut h2] = self.h;
        for _ in 0..2 {
            let c = h1 >> 44;
            h1 &= MASK44;
            h2 += c;
            let c = h2 >> 42;
            h2 &= MASK42;
            h0 += c * 5;
            let c = h0 >> 44;
            h0 &= MASK44;
            h1 += c;
        }
        // h - p, kept only when h >= p
        let mut g0 = h0 + 5;
        let c = g0 >> 44;
        g0 &= MASK44;
        let mut g1 = h1 + c;
        let c = g1 >> 44;
        g1 &= MASK44;
        let mut g2 = (h2 + c).wrapping_sub(1 << 42);
        let keep = (g2 >> 63).wrapping_sub(1);
        g0 &= keep;
        g1 &= keep;
        g2 &= keep;
        h0 = (h0 & !keep) | g0;
        h1 = (h1 & !keep) | g1;
        h2 = (h2 & !keep) | g2;
        let [t0, t1] = self.pad;
        h0 += t0 & MASK44;
        let c = h0 >> 44;
        h0 &= MASK44;
        h1 += (((t0 >> 44) | (t1 << 20)) & MASK44) + c;
        let c = h1 >> 44;
        h1 &= MASK44;
        h2 = (h2 + ((t1 >> 24) & MASK42) + c) & MASK42;
        let low = h0 | (h1 << 44);
        let high = (h1 >> 20) | (h2 << 24);
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&low.to_le_bytes());
        out[8..].copy_from_slice(&high.to_le_bytes());
        out
    }
}
