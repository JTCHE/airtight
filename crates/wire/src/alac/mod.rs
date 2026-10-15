//! ALAC frames for 16-bit stereo, written uncompressed ("escape" frames).
//! Every ALAC decoder accepts them, and a 352-frame packet still fits in
//! one Ethernet datagram.

struct Bits {
    out: Vec<u8>,
    used: u32, // bits used in the last byte
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        for i in (0..count).rev() {
            if self.used == 0 {
                self.out.push(0);
            }
            let bit = (value >> i & 1) as u8;
            *self.out.last_mut().unwrap() |= bit << (7 - self.used);
            self.used = (self.used + 1) % 8;
        }
    }
}

const CHANNEL_PAIR: u32 = 1;
const END: u32 = 7;

/// One frame from interleaved little-endian samples (left, right, ...).
/// `frames` must equal the frames-per-packet announced in SETUP, so no
/// sample count goes in the header.
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let mut bits = Bits { out: Vec::with_capacity(samples.len() * 2 + 8), used: 0 };
    bits.put(CHANNEL_PAIR, 3);
    bits.put(0, 4); // element instance
    bits.put(0, 12); // unused
    bits.put(0, 1); // no sample count: a full packet
    bits.put(0, 2); // no shifted bytes
    bits.put(1, 1); // escape: samples follow verbatim
    for &sample in samples {
        bits.put(u32::from(sample as u16), 16);
    }
    bits.put(END, 3);
    bits.out
}
