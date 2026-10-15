//! The timeline we serve: the wall clock read once at start, then the
//! steady performance counter. It never steps and is never steered.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
pub struct Clock {
    start: Instant,
    start_ns: u64,
}

impl Clock {
    pub fn new() -> Self {
        let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        Self { start: Instant::now(), start_ns: wall.as_nanos() as u64 }
    }

    /// Nanoseconds on the PTP timeline.
    pub fn now_ns(&self) -> u64 {
        self.at(Instant::now())
    }

    pub fn at(&self, instant: Instant) -> u64 {
        self.start_ns + instant.saturating_duration_since(self.start).as_nanos() as u64
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}
