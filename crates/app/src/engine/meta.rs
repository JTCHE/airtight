//! Push the Windows now-playing track to the receiver as it changes.

use crate::nowplaying::{Change, Follower, Hub, placeholder, to_mrp};
use airtight_airplay::Session;
use airtight_log::log;
use airtight_media::{Track, art, is_default_output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Meta {
    stop: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Meta {
    pub fn start(session: Arc<Session>, media: &Hub, endpoint_id: String) -> Meta {
        let (stop, playing) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let feed = media.subscribe();
        let flags = (stop.clone(), playing.clone());
        let thread = thread::Builder::new()
            .name("meta".into())
            .spawn(move || run(&session, &feed, &endpoint_id, &flags.0, &flags.1))
            .ok();
        Meta { stop, playing, thread }
    }

    /// Audio started: push the track again, now that it sticks.
    pub fn playing(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }
}

impl Drop for Meta {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(session: &Session, feed: &Receiver<Option<Track>>, endpoint_id: &str, stop: &AtomicBool, playing: &AtomicBool) {
    let mut follower = Follower::default();
    let mut track: Option<Track> = None;
    let mut started = false;
    let clock = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        match feed.recv_timeout(Duration::from_millis(200)) {
            // Only the newest read matters: take all that piled up.
            Ok(first) => track = std::iter::once(first).chain(feed.try_iter()).last().flatten(),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !started && playing.load(Ordering::Relaxed) {
            started = true;
            follower.sent = None;
        }
        // Sound routed elsewhere (headphones) does not play on the receiver,
        // so its track must not show there either.
        // ponytail: default output only; an app pinned to this endpoint in
        // the volume mixer shows the placeholder.
        let shown = track.as_ref().filter(|_| is_default_output(endpoint_id));
        let result = match follower.step(shown, clock.elapsed().as_secs_f64()) {
            Change::None => continue,
            Change::Track => {
                let shown = shown.cloned().unwrap_or_else(placeholder);
                let jpeg = shown.art.as_ref().and_then(|a| art::to_jpeg(a));
                log!("[TRACK] {} - {}", shown.artist, shown.title);
                session.publish_track(&to_mrp(&shown, jpeg.as_deref()))
            }
            Change::Progress => session.publish_progress(&to_mrp(shown.unwrap(), None)),
        };
        if let Err(e) = result {
            log!("[MRP] push failed: {e}");
        }
    }
}
