//! An AirPlay 2 sender for one receiver at a time: transient pairing, a PTP
//! session, realtime ALAC audio, Now Playing over MediaRemote and mDNS
//! discovery.

pub mod audio;
pub mod discovery;
pub mod identity;
pub mod session;

mod channel;
mod events;
mod hap;
mod pairing;

pub use audio::{Counts, Stream};
pub use identity::Identity;
pub use session::{Config, Session};

#[derive(Debug, Clone)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(e.to_string())
    }
}

