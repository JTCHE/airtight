//! Now Playing over MediaRemote, in the order an iPhone sends it:
//! DEVICE_INFO once, then the track, and after the first track the
//! supported commands, the playback state and the client.

use crate::channel::Channel;
use crate::Error;
use airtight_log::log;
use airtight_wire::mrp::{self, PlaybackState, Sender, Track};
use airtight_wire::rtsp::PLIST;

#[derive(Default)]
pub struct Publisher {
    registered: bool,
    extended: bool,
    state: Option<PlaybackState>,
    /// The receiver refused "update" pushes: send whole items instead.
    full_progress: bool,
}

fn post(channel: &mut Channel, label: &str, body: &[u8]) -> Result<u16, Error> {
    let reply = channel.request("POST", "/command", &[("Content-Type", PLIST)], body)?;
    let status = reply.status().unwrap_or(0);
    if status != 200 {
        log!("[MRP] {label} answered {status}");
    }
    Ok(status)
}

fn state_of(track: &Track) -> PlaybackState {
    if track.playing { PlaybackState::Playing } else { PlaybackState::Paused }
}

impl Publisher {
    pub fn register(&mut self, channel: &mut Channel, sender: &Sender) -> Result<(), Error> {
        if !self.registered {
            self.registered = post(channel, "DEVICE_INFO", &mrp::device_info(sender))? == 200;
        }
        Ok(())
    }

    /// Push the whole item: a new track, new tags or a new cover.
    pub fn track(&mut self, channel: &mut Channel, sender: &Sender, track: &Track) -> Result<(), Error> {
        self.register(channel, sender)?;
        if post(channel, "now playing", &mrp::now_playing(track))? != 200 {
            return Ok(());
        }
        if !self.extended {
            let commands = post(channel, "supported commands", &mrp::supported_commands())?;
            self.state = None;
            self.send_state(channel, state_of(track))?;
            let client = mrp::now_playing_client(std::process::id(), sender.name);
            let client = post(channel, "now playing client", &client)?;
            self.extended = commands == 200 && client == 200;
        }
        self.send_state(channel, state_of(track))
    }

    /// Push a seek, a pause or a resume.
    pub fn progress(&mut self, channel: &mut Channel, sender: &Sender, track: &Track) -> Result<(), Error> {
        if !self.full_progress {
            let status = post(channel, "progress", &mrp::progress(track))?;
            if status >= 300 {
                self.full_progress = true;
            } else {
                return self.send_state(channel, state_of(track));
            }
        }
        self.track(channel, sender, track)
    }

    fn send_state(&mut self, channel: &mut Channel, state: PlaybackState) -> Result<(), Error> {
        if self.state != Some(state) && post(channel, "playback state", &mrp::playback_state(state))? == 200 {
            self.state = Some(state);
        }
        Ok(())
    }
}
