//! One streaming session: connect, wait for the receiver to follow our
//! clock, then pump the captured audio until stopped or broken.

mod meta;

use crate::link::Receiver;
use crate::nowplaying::Hub;
use crate::settings::Settings;
use airtight_airplay::{Config, Identity, Session, Stream};
use airtight_audio::{Endpoint, Loopback, endpoint};
use airtight_log::log;
use airtight_media::Command;
use airtight_wire::mrp::RemoteCommand;
use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A capture read normally returns one device period (10 ms). Longer
/// means the pump was late.
const LATE_READ: Duration = Duration::from_millis(100);
/// How often the stream counters go to the log, when any is not zero.
const COUNTS_EVERY: Duration = Duration::from_secs(30);

pub enum Report {
    /// Connected; the receiver is locking onto our clock.
    Syncing,
    Playing,
    /// Something the user should read; the session goes on.
    Error(String),
    /// The session ended without stop().
    End(String),
}

pub type Reporter = Box<dyn Fn(Report) + Send>;

pub struct Engine {
    id: u64,
    stop: Arc<AtomicBool>,
    session: Arc<Mutex<Option<Arc<Session>>>>,
    thread: Option<JoinHandle<()>>,
}

struct Context {
    receiver: Receiver,
    settings: Settings,
    media: Arc<Hub>,
    report: Reporter,
    stop: Arc<AtomicBool>,
    session: Arc<Mutex<Option<Arc<Session>>>>,
}

impl Engine {
    pub fn start(id: u64, receiver: &Receiver, settings: &Settings, media: Arc<Hub>, report: Reporter) -> Engine {
        let stop = Arc::new(AtomicBool::new(false));
        let session = Arc::new(Mutex::new(None));
        let context = Context {
            receiver: receiver.clone(),
            settings: settings.clone(),
            media,
            report,
            stop: stop.clone(),
            session: session.clone(),
        };
        let thread = thread::Builder::new().name("engine".into()).spawn(move || run(&context)).ok();
        Engine { id, stop, session, thread }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn set_volume(&self, percent: u8) {
        let session = self.session.lock().unwrap().clone();
        if let Some(Err(e)) = session.map(|s| s.set_volume(percent)) {
            log!("[ENGINE] volume failed: {e}");
        }
    }

    /// End the session and wait for TEARDOWN: a receiver still holding
    /// the old session refuses the next one.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn remote(media: &Arc<Hub>) -> Box<dyn Fn(RemoteCommand) + Send> {
    let media = media.clone();
    Box::new(move |command| {
        media.control(match command {
            RemoteCommand::Play => Command::Play,
            RemoteCommand::Pause => Command::Pause,
            RemoteCommand::PlayPause => Command::PlayPause,
            RemoteCommand::Next => Command::Next,
            RemoteCommand::Previous => Command::Previous,
        })
    })
}

fn run(context: &Context) {
    let s = &context.settings;
    let Some(endpoint) = endpoint::find(s.endpoint.as_deref()) else {
        (context.report)(Report::End("No audio output to capture".into()));
        return;
    };
    let host = context.receiver.host;
    let latency_ms = endpoint::driver_latency_ms(&endpoint).unwrap_or(s.latency_ms);
    log!("--- start host={host} endpoint={:?} volume={} latency_ms={latency_ms}", endpoint.name, s.volume);
    let config = Config { receiver: SocketAddrV4::new(host, 7000), volume: s.volume, latency_ms };
    let identity = Identity::new(&airtight_airplay::discovery::host_name());
    let session = match Session::connect(&config, identity, remote(&context.media)) {
        Ok(session) => Arc::new(session),
        Err(e) => {
            log!("[ENGINE] connect failed: {e}");
            if !context.stop.load(Ordering::Relaxed) {
                (context.report)(Report::End(e.0));
            }
            return;
        }
    };
    *context.session.lock().unwrap() = Some(session.clone());
    (context.report)(Report::Syncing);
    let meta = meta::Meta::start(session.clone(), &context.media, endpoint.id.clone());
    let ended = stream(context, &session, &endpoint, &meta);
    drop(meta);
    context.session.lock().unwrap().take();
    session.close();
    log!("--- stop  {}", ended.as_deref().unwrap_or("by request"));
    if let Some(reason) = ended.filter(|_| !context.stop.load(Ordering::Relaxed)) {
        (context.report)(Report::End(reason));
    }
}

/// Wait for the clock, then pump audio. Returns why it ended, or None
/// when asked to stop.
fn stream(context: &Context, session: &Session, endpoint: &Endpoint, meta: &meta::Meta) -> Option<String> {
    while !session.clock_ready() {
        if context.stop.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(failure) = session.failure() {
            return Some(failure);
        }
        thread::sleep(Duration::from_millis(20));
    }
    log!("[ENGINE] receiver follows our clock");
    airtight_audio::com();
    airtight_audio::pro_audio_thread();
    let mut capture = match Loopback::open(&endpoint.id) {
        Ok(capture) => capture,
        Err(e) => return Some(format!("Capture failed: {e}")),
    };
    let mut stream = match session.start_stream() {
        Ok(stream) => stream,
        Err(e) => return Some(e.0),
    };
    (context.report)(Report::Playing);
    meta.playing();
    pump(context, session, &mut capture, &mut stream)
}

fn pump(context: &Context, session: &Session, capture: &mut Loopback, stream: &mut Stream) -> Option<String> {
    let (mut samples, mut late, mut lost) = (Vec::with_capacity(8192), 0u32, 0u32);
    let (mut checked, mut counted, mut filled) = (Instant::now(), Instant::now(), 0u32);
    while !context.stop.load(Ordering::Relaxed) {
        if let Err(e) = capture.read(&mut samples) {
            return Some(format!("Capture failed: {e}"));
        }
        let read = Duration::from_secs_f64(samples.len() as f64 / 2.0 / 44100.0);
        if read > LATE_READ {
            late += 1;
            if late <= 20 {
                log!("[CAPTURE] read came back {} ms late", read.as_millis());
            }
        }
        if capture.lost != lost {
            lost = capture.lost;
            if lost <= 20 {
                log!("[CAPTURE] capture buffer overflowed, audio lost");
            }
        }
        if let Err(e) = stream.push(&samples) {
            return Some(format!("Network error: {e}"));
        }
        samples.clear();
        if counted.elapsed() > COUNTS_EVERY {
            counted = Instant::now();
            log_counts(stream.take_counts(), capture.filled - filled);
            filled = capture.filled;
        }
        if checked.elapsed() > Duration::from_secs(1) {
            checked = Instant::now();
            if let Some(failure) = session.failure() {
                return Some(failure);
            }
        }
    }
    log!("[ENGINE] capture_late={late} capture_lost={lost}");
    None
}

fn log_counts(counts: airtight_airplay::Counts, filled: u32) {
    let airtight_airplay::Counts { resent, late, dropped, inserted } = counts;
    // A steady trickle of dropped frames is normal clock drift.
    if resent + inserted + filled > 0 {
        log!("[STREAM] last {} s: resent={resent} late={late} dropped={dropped} inserted={inserted} silence_filled={filled}", COUNTS_EVERY.as_secs());
    }
}
