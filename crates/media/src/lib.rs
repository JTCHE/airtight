//! What the system says is playing. Spotify, browsers and most players
//! publish their track: to the system media session on Windows (the data
//! the volume flyout shows), to MPRIS on Linux.

#[cfg(windows)]
pub mod art;
#[cfg(target_os = "linux")]
mod mpris;
#[cfg(windows)]
mod watcher;

#[cfg(target_os = "linux")]
pub use mpris::{Watcher, art, is_default_output};
#[cfg(windows)]
pub use watcher::Watcher;

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The app that plays, so two apps with one title stay apart.
    pub app: String,
    /// Seconds; zero when the app does not say.
    pub duration: f64,
    /// Seconds into the track, as of the read.
    pub position: f64,
    pub playing: bool,
    /// The cover as the app hands it out: often PNG.
    pub art: Option<std::sync::Arc<Vec<u8>>>,
}

/// Remote presses from the receiver, to pass on to the app that plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
}

/// True when the render endpoint `endpoint_id` is the default output. The
/// default id wraps the endpoint id: \?\SWD#MMDEVAPI#<endpoint id>#{...}
#[cfg(windows)]
pub fn is_default_output(endpoint_id: &str) -> bool {
    use windows::Media::Devices::{AudioDeviceRole, MediaDevice};
    let default = MediaDevice::GetDefaultAudioRenderId(AudioDeviceRole::Default).map(|id| id.to_string()).unwrap_or_default();
    default.to_lowercase().contains(&endpoint_id.to_lowercase())
}
