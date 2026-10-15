//! The sound watch: read the endpoint's level meter, and say once when
//! audio starts playing. A level read costs nothing; a capture stream
//! would poll all day.

use airtight_audio::{Meter, endpoint};
use airtight_log::log;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

/// A peak below this counts as nothing playing.
const SILENCE: f32 = 0.0005;
const POLL: Duration = Duration::from_millis(100);
/// Silent polls that arm the gate again: about a second.
const ARM_POLLS: u32 = 10;
/// The endpoint is missing or disabled: look again this often.
const RETRY: Duration = Duration::from_secs(5);

/// Fires when audio plays. It starts armed, so audio that already plays
/// counts. After a manual disconnect it starts disarmed, and a run of
/// silence must pass first, or it would reconnect to the track still playing.
pub struct Gate {
    silent: u32,
    armed: bool,
}

impl Gate {
    pub fn new(armed: bool) -> Gate {
        Gate { silent: 0, armed }
    }

    pub fn step(&mut self, peak: f32) -> bool {
        if peak <= SILENCE {
            self.silent += 1;
            if self.silent >= ARM_POLLS {
                self.armed = true;
            }
            return false;
        }
        self.silent = 0;
        std::mem::replace(&mut self.armed, false)
    }
}

pub struct SoundWatch {
    on_sound: Arc<dyn Fn() + Send + Sync>,
    /// The running thread's own flag: a stopping thread never revives.
    stop: Option<Arc<AtomicBool>>,
}

impl SoundWatch {
    pub fn new(on_sound: Box<dyn Fn() + Send + Sync>) -> SoundWatch {
        SoundWatch { on_sound: Arc::from(on_sound), stop: None }
    }

    pub fn start(&mut self, endpoint: Option<&str>, armed: bool) {
        if self.stop.as_ref().is_some_and(|s| !s.load(Ordering::Relaxed)) {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, on_sound, endpoint) = (stop.clone(), self.on_sound.clone(), endpoint.map(String::from));
        let spawned = thread::Builder::new()
            .name("soundwatch".into())
            .spawn(move || watch(endpoint.as_deref(), armed, &flag, &*on_sound));
        if spawned.is_ok() {
            self.stop = Some(stop);
        }
    }

    pub fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

fn watch(endpoint: Option<&str>, armed: bool, stop: &AtomicBool, on_sound: &dyn Fn()) {
    airtight_audio::com();
    let mut gate = Gate::new(armed);
    let mut meter: Option<Meter> = None;
    let mut warned = false;
    while !stop.load(Ordering::Relaxed) {
        if meter.is_none() {
            meter = endpoint::find(endpoint).and_then(|e| Meter::open(&e.id).ok());
            if meter.is_none() {
                if !warned {
                    log!("[WATCH] cannot read the level meter");
                    warned = true;
                }
                thread::sleep(RETRY);
                continue;
            }
        }
        match meter.as_ref().map(Meter::peak) {
            Some(Ok(peak)) if gate.step(peak) => {
                if !stop.load(Ordering::Relaxed) {
                    on_sound();
                }
                stop.store(true, Ordering::Relaxed);
                return;
            }
            Some(Ok(_)) => {}
            _ => meter = None, // the endpoint went away
        }
        thread::sleep(POLL);
    }
}
