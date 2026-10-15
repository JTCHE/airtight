//! The peak level of a sink, from a small mono stream of its monitor.
//! Pulse has no free meter like WASAPI's; 8 kHz mono costs next to nothing.

use super::Record;
use std::cell::RefCell;
use std::io::Result;

const RATE: u32 = 8000;
/// One read: 100 ms, the sound watch's poll.
const READ_BYTES: usize = RATE as usize / 10 * 2;

pub struct Meter(RefCell<(Record, Vec<u8>)>);

impl Meter {
    pub fn open(endpoint_id: &str) -> Result<Meter> {
        let record = Record::open(endpoint_id, RATE, 1, READ_BYTES as u32, READ_BYTES as u32 * 2)?;
        Ok(Meter(RefCell::new((record, vec![0; READ_BYTES]))))
    }

    /// 0.0 to 1.0 over the last 100 ms. Fails once the sink goes away.
    pub fn peak(&self) -> Result<f32> {
        let (record, bytes) = &mut *self.0.borrow_mut();
        record.read(bytes)?;
        let peak = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]).unsigned_abs()).max().unwrap_or(0);
        Ok(f32::from(peak) / 32768.0)
    }
}
