//! POST /feedback every 2 s: it resets the receiver's idle timer, and a
//! run of failures tells us the session is gone.

use crate::channel::Channel;
use airtight_log::log;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(2);
const MISSES_TO_FAIL: u32 = 3;

pub struct Keepalive {
    running: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Keepalive {
    pub fn start(channel: Arc<Mutex<Channel>>) -> std::io::Result<Keepalive> {
        let running = Arc::new(AtomicBool::new(true));
        let failure = Arc::new(Mutex::new(None));
        let (flag, failed) = (running.clone(), failure.clone());
        let thread = thread::Builder::new()
            .name("airplay-feedback".into())
            .spawn(move || run(&channel, &flag, &failed))?;
        Ok(Keepalive { running, failure, thread: Some(thread) })
    }

    pub fn failure(&self) -> Option<String> {
        self.failure.lock().unwrap().clone()
    }
}

impl Drop for Keepalive {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(channel: &Mutex<Channel>, running: &AtomicBool, failure: &Mutex<Option<String>>) {
    let mut misses = 0;
    let mut next = Instant::now() + EVERY;
    while running.load(Ordering::Relaxed) {
        if Instant::now() < next {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        next += EVERY;
        let result = channel.lock().unwrap().request("POST", "/feedback", &[], b"");
        match result {
            Ok(reply) if reply.status() == Some(200) => misses = 0,
            Ok(reply) => {
                misses += 1;
                log!("[AIRPLAY] /feedback answered {:?}", reply.status());
            }
            Err(e) => {
                misses += 1;
                log!("[AIRPLAY] /feedback failed: {e}");
            }
        }
        if misses >= MISSES_TO_FAIL {
            *failure.lock().unwrap() = Some("The receiver stopped answering".into());
            return;
        }
    }
}
