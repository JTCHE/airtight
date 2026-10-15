//! Decide what to send as the Windows track changes. No I/O, so it tests.

use airtight_media::Track;
use airtight_wire::mrp;
use std::time::{SystemTime, UNIX_EPOCH};

/// Reads land late; a real seek moves further than this.
const SEEK_SLACK: f64 = 3.0;
/// Seconds from 1970 to 2001, the epoch of Apple timestamps.
const APPLE_EPOCH: f64 = 978_307_200.0;

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    None,
    /// A new item, or new tags or cover: push the whole track.
    Track,
    /// A seek, a pause or a resume: correct the timeline.
    Progress,
}

/// Shown when no app publishes a track: a game, a call, a site without
/// media session support.
pub fn placeholder() -> Track {
    Track {
        title: "Airtight".into(),
        artist: airtight_airplay::discovery::host_name(),
        album: String::new(),
        app: String::new(),
        duration: 0.0,
        position: 0.0,
        playing: true,
        art: None,
    }
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for &b in *part {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        hash = (hash ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Stable per track, so a late album or cover updates the item in place.
pub fn item_id(track: &Track) -> u64 {
    fnv(&[track.title.as_bytes(), track.artist.as_bytes()])
}

type Key = (u64, String, String, String, Option<u64>);

fn key(track: &Track) -> Key {
    let art = track.art.as_ref().map(|a| fnv(&[a]));
    (item_id(track), track.title.clone(), track.artist.clone(), track.album.clone(), art)
}

#[derive(Default)]
pub struct Follower {
    /// What was pushed last; None forces the next push.
    pub sent: Option<Key>,
    /// (position, time) of the last push.
    anchor: (f64, f64),
    was_playing: bool,
}

impl Follower {
    /// What this read must send. `now` is in seconds on any steady clock.
    pub fn step(&mut self, track: Option<&Track>, now: f64) -> Change {
        let Some(track) = track else {
            let placeholder = key(&placeholder());
            if self.sent.as_ref() == Some(&placeholder) {
                return Change::None;
            }
            self.sent = Some(placeholder);
            return Change::Track;
        };
        let key = key(track);
        if self.sent.as_ref() != Some(&key) {
            self.sent = Some(key);
            self.mark(track, now);
            return Change::Track;
        }
        let drift = if self.was_playing { now - self.anchor.1 } else { 0.0 };
        let jumped = (track.position - (self.anchor.0 + drift)).abs() > SEEK_SLACK;
        let flipped = track.playing != self.was_playing;
        if jumped || flipped {
            self.mark(track, now);
            return Change::Progress;
        }
        Change::None
    }

    fn mark(&mut self, track: &Track, now: f64) {
        self.anchor = (track.position, now);
        self.was_playing = track.playing;
    }
}

/// The MediaRemote form of a track, with the cover already in JPEG.
pub fn to_mrp(track: &Track, jpeg: Option<&[u8]>) -> mrp::Track {
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    mrp::Track {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration: track.duration,
        elapsed: track.position,
        playing: track.playing,
        timestamp: unix - APPLE_EPOCH,
        unique_id: item_id(track) as i64,
        artwork: jpeg.map(|j| (format!("{:016x}", fnv(&[j])), j.to_vec())),
    }
}
