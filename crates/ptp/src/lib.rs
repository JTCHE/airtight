//! The PTP grandmaster that AirPlay 2 receivers slave their playback to.
//! The sender always keeps the timeline; it never follows a receiver.

pub mod clock;
pub mod engine;
pub mod message;

pub use clock::Clock;
pub use engine::Engine;
