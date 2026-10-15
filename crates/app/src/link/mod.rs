//! Keep the chosen receiver connected.
//!
//! One thread owns every decision: clicks, engine reports, the sound watch,
//! retries and the stuck-session watchdog all arrive as events on one
//! queue, so a reconnect cannot race a state change.

mod watch;

pub use watch::Gate;

use crate::engine::{Engine, Report};
use crate::nowplaying::Hub;
use crate::settings::Settings;
use airtight_log::log;
use std::net::Ipv4Addr;
use std::sync::mpsc::{Receiver as Inbox, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use watch::SoundWatch;

/// Then give up until the user or sound asks again.
const RETRY: [u64; 5] = [2, 5, 10, 20, 30];
/// A session not playing by then is stuck: start it again.
const PLAY_TIMEOUT: Duration = Duration::from_secs(40);
/// A session that played this long resets the retry count.
const STABLE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq)]
pub struct Receiver {
    pub name: String,
    pub host: Ipv4Addr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Connecting,
    Syncing,
    Playing,
    Retrying,
}

/// What the panel and the tray show.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub receiver: Option<Receiver>,
    pub state: State,
    /// One line for the user; empty while all is well.
    pub note: String,
}

pub enum Event {
    Connect(Receiver),
    Toggle(Receiver),
    Disconnect,
    Volume(u8),
    /// The settings changed; `true` when the stream needs a new session.
    Settings(bool),
    Sound,
    Found(Vec<Receiver>),
    Engine(u64, Report),
    Quit,
}

pub struct Shared {
    pub settings: Arc<Mutex<Settings>>,
    pub media: Arc<Hub>,
    pub on_change: Box<dyn Fn(&Snapshot) + Send>,
    /// The receiver to keep connected changed: remember it.
    pub on_receiver: Box<dyn Fn(&Receiver) + Send>,
}

pub struct Link {
    events: Sender<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Link {
    pub fn start(shared: Shared) -> Link {
        let (events, inbox) = channel();
        let saved = saved(&shared.settings.lock().unwrap());
        let auto = shared.settings.lock().unwrap().auto_connect;
        let poster = events.clone();
        let thread = thread::Builder::new().name("link".into()).spawn(move || Supervisor::new(shared, poster).run(&inbox));
        if let (true, Some(receiver)) = (auto, saved) {
            // Straight to the saved address: discovery can miss a receiver for minutes.
            let _ = events.send(Event::Connect(receiver));
        }
        Link { events, thread: thread.ok() }
    }

    pub fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// End the session and wait, so the receiver gets TEARDOWN.
    pub fn quit(mut self) {
        self.send(Event::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn saved(settings: &Settings) -> Option<Receiver> {
    let host: Ipv4Addr = settings.receiver_host.as_ref()?.parse().ok()?;
    let name = settings.receiver_name.clone().unwrap_or_else(|| host.to_string());
    Some(Receiver { name, host })
}

struct Supervisor {
    shared: Shared,
    events: Sender<Event>,
    receiver: Option<Receiver>,
    state: State,
    note: String,
    engine: Option<Engine>,
    next_id: u64,
    attempts: usize,
    retry_at: Option<Instant>,
    stuck_at: Option<Instant>,
    playing_since: Option<Instant>,
    /// After a manual disconnect, sound must stop before auto start acts.
    hold: bool,
    watch: SoundWatch,
}

impl Supervisor {
    fn new(shared: Shared, events: Sender<Event>) -> Supervisor {
        let sound = events.clone();
        Supervisor {
            shared,
            receiver: None,
            state: State::Idle,
            note: String::new(),
            engine: None,
            next_id: 0,
            attempts: 0,
            retry_at: None,
            stuck_at: None,
            playing_since: None,
            hold: false,
            watch: SoundWatch::new(Box::new(move || drop(sound.send(Event::Sound)))),
            events,
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { receiver: self.receiver.clone(), state: self.state, note: self.note.clone() }
    }

    fn changed(&self) {
        (self.shared.on_change)(&self.snapshot());
    }

    fn run(mut self, inbox: &Inbox<Event>) {
        self.sync_watch();
        self.changed();
        loop {
            let due = [self.retry_at, self.stuck_at].into_iter().flatten().min();
            let event = match due {
                Some(at) => inbox.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => inbox.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match event {
                Ok(Event::Quit) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(event) => self.handle(event),
                Err(RecvTimeoutError::Timeout) => self.tick(),
            }
            self.sync_watch();
            self.changed();
        }
        self.watch.stop();
        self.stop_engine();
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Connect(receiver) => self.connect(receiver),
            Event::Toggle(receiver) if self.receiver.as_ref().is_some_and(|r| r.host == receiver.host) => self.disconnect(),
            Event::Toggle(receiver) => self.connect(receiver),
            Event::Disconnect => self.disconnect(),
            Event::Volume(volume) => {
                if let Some(engine) = &self.engine {
                    engine.set_volume(volume);
                }
            }
            Event::Settings(restart) => {
                self.watch.stop(); // the endpoint may have changed
                if restart && self.engine.is_some() {
                    self.attempts = 0;
                    self.launch();
                }
            }
            Event::Sound => self.sound(),
            Event::Found(found) => self.found(found),
            Event::Engine(id, report) => self.report(id, report),
            Event::Quit => {}
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if self.stuck_at.is_some_and(|at| now >= at) {
            log!("[LINK] session did not start playing, starting again");
            self.retry(Some("The receiver did not start playing".into()));
        } else if self.retry_at.is_some_and(|at| now >= at) {
            self.retry_at = None;
            self.launch();
        }
    }

    fn connect(&mut self, receiver: Receiver) {
        log!("[LINK] connect {}", receiver.name);
        self.hold = false;
        if self.engine.is_some() && self.receiver.as_ref().is_some_and(|r| r.host == receiver.host) {
            return;
        }
        (self.shared.on_receiver)(&receiver);
        self.receiver = Some(receiver);
        self.attempts = 0;
        self.note.clear();
        self.launch();
    }

    fn disconnect(&mut self) {
        log!("[LINK] disconnect");
        self.receiver = None;
        self.hold = true; // do not reconnect on the track that still plays
        self.set_idle(String::new());
    }

    fn sound(&mut self) {
        let (auto, saved) = {
            let settings = self.shared.settings.lock().unwrap();
            (settings.auto_start, saved(&settings))
        };
        if let (None, true, Some(receiver)) = (&self.receiver, auto, saved) {
            log!("[LINK] sound started, connecting");
            self.connect(receiver);
        }
    }

    /// Discovery after failures: has the receiver moved address?
    fn found(&mut self, found: Vec<Receiver>) {
        let Some(current) = &self.receiver else { return };
        let Some(moved) = found.into_iter().find(|r| r.name == current.name && r.host != current.host) else { return };
        log!("[LINK] {} moved from {} to {}", moved.name, current.host, moved.host);
        (self.shared.on_receiver)(&moved);
        self.receiver = Some(moved);
        if self.retry_at.is_some() {
            self.retry_at = Some(Instant::now());
        }
    }

    fn report(&mut self, id: u64, report: Report) {
        if self.engine.as_ref().is_none_or(|e| e.id() != id) {
            return; // from an old session
        }
        match report {
            Report::Syncing => self.state = State::Syncing,
            Report::Playing => {
                self.state = State::Playing;
                self.stuck_at = None;
                self.playing_since = Some(Instant::now());
                self.note.clear();
            }
            Report::Error(message) => self.note = message,
            Report::End(reason) => {
                // The session is already over: no TEARDOWN wait.
                self.engine.take();
                if self.receiver.is_some() {
                    self.retry(Some(reason));
                } else {
                    self.set_idle(String::new());
                }
            }
        }
    }

    /// Start a fresh session for the receiver, ending any current one.
    fn launch(&mut self) {
        let Some(receiver) = self.receiver.clone() else { return };
        self.state = State::Connecting;
        self.retry_at = None;
        self.stop_engine();
        let settings = self.shared.settings.lock().unwrap().clone();
        self.next_id += 1;
        let (id, events) = (self.next_id, self.events.clone());
        let report = Box::new(move |r| drop(events.send(Event::Engine(id, r))));
        self.engine = Some(Engine::start(id, &receiver, &settings, self.shared.media.clone(), report));
        self.stuck_at = Some(Instant::now() + PLAY_TIMEOUT);
        self.playing_since = None;
    }

    fn retry(&mut self, reason: Option<String>) {
        self.stuck_at = None;
        if self.playing_since.take().is_some_and(|t| t.elapsed() > STABLE) {
            self.attempts = 0;
        }
        let Some(receiver) = self.receiver.clone() else { return };
        if self.attempts >= RETRY.len() {
            log!("[LINK] giving up on {}", receiver.name);
            self.receiver = None;
            self.hold = true;
            self.set_idle(format!("Unable to reach {}. Select it to try again.", receiver.name));
            return;
        }
        let delay = RETRY[self.attempts];
        self.attempts += 1;
        if self.attempts == 2 {
            // Two failures in a row: the receiver may have a new address.
            let events = self.events.clone();
            thread::spawn(move || drop(events.send(Event::Found(crate::discover()))));
        }
        self.state = State::Retrying;
        let reason = reason.unwrap_or_else(|| "Connection failed".into());
        self.note = format!("{reason}. Retrying in {delay} s.");
        log!("[LINK] retry {} in {delay} s: {reason}", self.attempts);
        self.stop_engine();
        self.retry_at = Some(Instant::now() + Duration::from_secs(delay));
    }

    fn set_idle(&mut self, note: String) {
        (self.retry_at, self.stuck_at, self.playing_since) = (None, None, None);
        self.state = State::Idle;
        self.note = note;
        self.stop_engine();
    }

    fn stop_engine(&mut self) {
        if let Some(engine) = self.engine.take() {
            // The stop takes a moment: show the new state first, not after.
            self.changed();
            engine.stop();
        }
    }

    /// Watch for sound only while auto start could act on it.
    fn sync_watch(&mut self) {
        let settings = self.shared.settings.lock().unwrap().clone();
        if settings.auto_start && settings.receiver_host.is_some() && self.receiver.is_none() {
            self.watch.start(settings.endpoint.as_deref(), !self.hold);
        } else {
            self.watch.stop();
        }
    }
}
