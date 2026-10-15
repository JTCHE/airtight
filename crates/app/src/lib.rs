//! Airtight: stream what the computer plays to an AirPlay 2 receiver, from
//! the tray on Windows and a service on Linux. The link keeps one receiver connected; the engine runs one
//! session; the UI shows it and takes the clicks.

#[cfg(target_os = "linux")]
pub mod cli;
pub mod engine;
pub mod link;
pub mod nowplaying;
pub mod probe;
pub mod settings;
#[cfg(windows)]
pub mod ui;

use std::time::Duration;

/// The AirPlay receivers on the network, by name.
pub fn discover() -> Vec<link::Receiver> {
    let found = airtight_airplay::discovery::browse(Duration::from_millis(2500));
    found.into_iter().map(|s| link::Receiver { name: s.name, host: s.address }).collect()
}
