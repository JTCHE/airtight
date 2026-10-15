//! Follow the system media session through its events, not by polling.
//!
//! One session manager for the life of the process: a new manager per read
//! stays registered with the media service, and after a few hours every
//! change fanned out into hundreds of calls into this process. WinRT raises
//! the events on its thread pool; the handlers only post to our thread,
//! which reads the session and hands the track to the sink.

mod read;

use crate::{Command, Track};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
};
use windows::Media::Devices::MediaDevice;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

/// One track change raises several events at once.
const DEBOUNCE: Duration = Duration::from_millis(150);

enum Event {
    /// Something about a session changed: read again.
    Changed,
    /// Sessions came or went: hook the new set.
    Sessions,
    /// The tags changed: the cover may have too.
    Properties,
    Control(Command),
}

pub type Sink = Box<dyn Fn(Option<Track>) + Send>;

pub struct Watcher {
    events: Sender<Event>,
}

impl Watcher {
    /// Call `sink` with the track, or None, after each change.
    pub fn start(sink: Sink) -> Watcher {
        let (events, inbox) = channel();
        let poster = events.clone();
        let _ = thread::Builder::new().name("nowplaying".into()).spawn(move || run(&poster, &inbox, &sink));
        let _ = events.send(Event::Sessions);
        Watcher { events }
    }

    /// Read the session now, even if no event fired.
    pub fn refresh(&self) {
        let _ = self.events.send(Event::Changed);
    }

    /// Pass a remote press to the app that plays.
    pub fn control(&self, command: Command) {
        let _ = self.events.send(Event::Control(command));
    }
}

fn post(events: &Sender<Event>, make: fn() -> Event) -> impl Fn() + Send + Clone + 'static {
    let events = events.clone();
    move || {
        let _ = events.send(make());
    }
}

fn connect(events: &Sender<Event>) -> Manager {
    loop {
        let manager = Manager::RequestAsync().and_then(|op| op.join());
        if let Ok(manager) = manager {
            let sessions = post(events, || Event::Sessions);
            let (a, b) = (sessions.clone(), sessions);
            let _ = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| Ok(a())));
            let _ = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| Ok(b())));
            let changed = post(events, || Event::Changed);
            let _ = MediaDevice::DefaultAudioRenderDeviceChanged(&TypedEventHandler::new(move |_, _| Ok(changed())));
            return manager;
        }
        // The media service is restarting.
        thread::sleep(Duration::from_secs(5));
    }
}

/// The handlers on each session, so they can come off when it goes.
struct Hooks(Vec<(Session, [i64; 3])>);

impl Hooks {
    fn rehook(&mut self, manager: &Manager, events: &Sender<Event>) {
        for (session, [a, b, c]) in self.0.drain(..) {
            let _ = session.RemoveMediaPropertiesChanged(a);
            let _ = session.RemovePlaybackInfoChanged(b);
            let _ = session.RemoveTimelinePropertiesChanged(c);
        }
        let Ok(sessions) = manager.GetSessions() else { return };
        for session in sessions {
            let properties = post(events, || Event::Properties);
            let (playback, timeline) = (post(events, || Event::Changed), post(events, || Event::Changed));
            let tokens = (
                session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| Ok(properties()))),
                session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| Ok(playback()))),
                session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| Ok(timeline()))),
            );
            if let (Ok(a), Ok(b), Ok(c)) = tokens {
                self.0.push((session, [a, b, c]));
            }
        }
    }
}

fn run(events: &Sender<Event>, inbox: &Receiver<Event>, sink: &Sink) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let manager = connect(events);
    let mut hooks = Hooks(Vec::new());
    let mut reader = read::Reader::default();
    while let Ok(first) = inbox.recv() {
        thread::sleep(DEBOUNCE);
        let (mut rehook, mut read_it) = (false, false);
        for event in std::iter::once(first).chain(inbox.try_iter()) {
            match event {
                Event::Sessions => (rehook, read_it) = (true, true),
                Event::Properties => {
                    reader.stale_art = true;
                    read_it = true;
                }
                Event::Changed => read_it = true,
                Event::Control(command) => read::control(&manager, command),
            }
        }
        if rehook {
            hooks.rehook(&manager, events);
            reader.stale_art = true;
        }
        if read_it {
            sink(reader.read(&manager));
        }
    }
}
