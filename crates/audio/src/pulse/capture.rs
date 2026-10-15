//! Capture of a sink's monitor source as 16-bit stereo at 44.1 kHz. The
//! monitor keeps the sink awake and gives silence when nothing plays.

use super::Record;
use crate::{CHANNELS, SAMPLE_RATE};
use std::io::Result;

/// One read: 10 ms, like a WASAPI device period.
const PERIOD_BYTES: usize = SAMPLE_RATE as usize / 100 * CHANNELS as usize * 2;
/// Half a second waits unread at most, like the WASAPI buffer.
const BUFFER_BYTES: usize = PERIOD_BYTES * 50;

pub struct Loopback {
    record: Record,
    bytes: Vec<u8>,
    /// Pulse drops what overflows without saying so.
    pub lost: u32,
    /// The monitor never has gaps to fill.
    pub filled: u32,
}

impl Loopback {
    pub fn open(endpoint_id: &str) -> Result<Loopback> {
        let record = Record::open(endpoint_id, SAMPLE_RATE, CHANNELS as u8, PERIOD_BYTES as u32, BUFFER_BYTES as u32)?;
        Ok(Loopback { record, bytes: vec![0; PERIOD_BYTES], lost: 0, filled: 0 })
    }

    /// Append the next period of samples to `out`.
    pub fn read(&mut self, out: &mut Vec<i16>) -> Result<()> {
        self.record.read(&mut self.bytes)?;
        out.extend(self.bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
        Ok(())
    }
}
