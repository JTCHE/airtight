//! The Windows now-playing track, handed to whichever session runs.
//!
//! One media watcher for the life of the process. The session subscribes
//! when it starts and gets the last track at once, then each change.

mod follower;

pub use follower::{Change, Follower, item_id, placeholder, to_mrp};

use airtight_media::{Command, Track, Watcher};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Inner {
    last: Option<Track>,
    sink: Option<Sender<Option<Track>>>,
}

pub struct Hub {
    watcher: Watcher,
    inner: Arc<Mutex<Inner>>,
}

impl Hub {
    pub fn start() -> Arc<Hub> {
        let inner = Arc::new(Mutex::new(Inner::default()));
        let shared = inner.clone();
        let watcher = Watcher::start(Box::new(move |track| {
            let mut inner = shared.lock().unwrap();
            if let Some(sink) = &inner.sink {
                let _ = sink.send(track.clone());
            }
            inner.last = track;
        }));
        Arc::new(Hub { watcher, inner })
    }

    /// Take over the track feed; the previous subscriber stops getting it.
    pub fn subscribe(&self) -> Receiver<Option<Track>> {
        let (sink, feed) = channel();
        let mut inner = self.inner.lock().unwrap();
        let _ = sink.send(inner.last.clone());
        inner.sink = Some(sink);
        drop(inner);
        self.watcher.refresh();
        feed
    }

    pub fn control(&self, command: Command) {
        self.watcher.control(command);
    }
}
