//! Keep the receiver's buffer depth steady while the sound card's clock
//! and the PTP clock run at slightly different rates: drop or repeat one
//! frame at a time, and jump only after a real stall.

/// Smoothing of the depth readings: about 1 s at 100 readings a second.
const SMOOTHING: f64 = 0.01;
/// How long the depth settles before it becomes the target.
const SETTLE_READINGS: u32 = 200;
/// Drift inside this band is left alone.
const DEAD_BAND_FRAMES: f64 = 44.0;
/// Past this, a stall happened: correct at once.
const JUMP_FRAMES: f64 = 1764.0;

#[derive(Default)]
pub struct Drift {
    smoothed: Option<f64>,
    target: Option<f64>,
    readings: u32,
}

/// What to do with the frames about to go out.
#[derive(Debug, PartialEq, Eq)]
pub enum Correction {
    None,
    /// Drop this many frames: the buffer is too deep.
    Drop(usize),
    /// Insert this many frames: the buffer runs low.
    Insert(usize),
}

impl Drift {
    /// Take one reading of the depth (frames between the newest frame sent
    /// and the frame playing now) and decide on a correction.
    pub fn observe(&mut self, depth: f64) -> Correction {
        let smoothed = match self.smoothed {
            Some(s) => s + (depth - s) * SMOOTHING,
            None => depth,
        };
        self.smoothed = Some(smoothed);
        self.readings += 1;
        let Some(target) = self.target else {
            if self.readings >= SETTLE_READINGS {
                self.target = Some(smoothed);
            }
            return Correction::None;
        };
        let error = smoothed - target;
        if error.abs() > JUMP_FRAMES {
            self.smoothed = Some(target);
            let frames = error.abs().round() as usize;
            return if error > 0.0 { Correction::Drop(frames) } else { Correction::Insert(frames) };
        }
        if error > DEAD_BAND_FRAMES {
            self.smoothed = Some(smoothed - 1.0);
            Correction::Drop(1)
        } else if error < -DEAD_BAND_FRAMES {
            self.smoothed = Some(smoothed + 1.0);
            Correction::Insert(1)
        } else {
            Correction::None
        }
    }
}
