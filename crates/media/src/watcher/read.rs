//! Read the playing session into a Track.

use crate::{Command, Track, art};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::core::{Interface, Result};

/// 100 ns ticks from 1601 to 1970.
const UNIX_IN_TICKS: i64 = 116_444_736_000_000_000;

fn playing(session: &Session) -> bool {
    session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == Status::Playing)
}

/// The app that plays. The "current" session of Windows is the last one
/// touched, so a paused video player can hide the music that plays.
fn session(manager: &Manager) -> Option<Session> {
    let sessions = manager.GetSessions().ok()?;
    sessions.into_iter().find(playing).or_else(|| manager.GetCurrentSession().ok())
}

pub fn control(manager: &Manager, command: Command) {
    let Some(session) = session(manager) else { return };
    let sent = match command {
        Command::Play => session.TryPlayAsync(),
        Command::Pause => session.TryPauseAsync(),
        Command::PlayPause => session.TryTogglePlayPauseAsync(),
        Command::Next => session.TrySkipNextAsync(),
        Command::Previous => session.TrySkipPreviousAsync(),
    };
    let _ = sent.and_then(|op| op.join());
}

type Key = (String, String, String, String);

#[derive(Default)]
pub struct Reader {
    /// The tags changed since the last cover read.
    pub stale_art: bool,
    /// (app, title, artist, album) and the cover read for them.
    art: Option<(Key, Option<Arc<Vec<u8>>>)>,
}

impl Reader {
    pub fn read(&mut self, manager: &Manager) -> Option<Track> {
        self.try_read(manager).ok().flatten()
    }

    fn try_read(&mut self, manager: &Manager) -> Result<Option<Track>> {
        let Some(session) = session(manager) else { return Ok(None) };
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let title = props.Title()?.to_string();
        if title.is_empty() {
            return Ok(None);
        }
        let timeline = session.GetTimelineProperties()?;
        let playing = playing(&session);
        let mut position = timeline.Position()?.Duration as f64 / 1e7;
        // Spotify updates the position only on play, pause and seek; in
        // between the stamp ages while the track plays on.
        let updated = timeline.LastUpdatedTime()?.UniversalTime;
        if playing && updated > 0 {
            position += (now_ticks() - updated).max(0) as f64 / 1e7;
        }
        let duration = timeline.EndTime()?.Duration as f64 / 1e7;
        if duration > 0.0 {
            position = position.min(duration);
        }
        let key = (
            session.SourceAppUserModelId()?.to_string(),
            title,
            props.Artist()?.to_string(),
            props.AlbumTitle()?.to_string(),
        );
        // The cover is the expensive part: read it only when it can have changed.
        if self.stale_art || self.art.as_ref().is_none_or(|(k, _)| *k != key) {
            let cover = props.Thumbnail().ok().and_then(|t| t.OpenReadAsync().ok()?.join().ok());
            let cover = cover.and_then(|s| art::read_stream(&s.cast().ok()?).ok()).map(Arc::new);
            self.art = Some((key.clone(), cover));
            self.stale_art = false;
        }
        let art = self.art.as_ref().and_then(|(_, a)| a.clone());
        let (app, title, artist, album) = key;
        Ok(Some(Track { title, artist, album, app, duration, position, playing, art }))
    }
}

fn now_ticks() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    UNIX_IN_TICKS + (since.as_nanos() / 100) as i64
}
