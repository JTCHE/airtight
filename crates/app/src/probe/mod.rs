//! `airtight --probe <ip> [seconds]`: one real session through the engine,
//! for checks from a shell. It runs from the installed exe, so the firewall
//! asks once for that path and never again for test builds.

use crate::engine::{Engine, Report};
use crate::link::Receiver;
use crate::nowplaying::Hub;
use crate::settings::Settings;
use airtight_log::log;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::time::Duration;

/// Returns the process exit code: 0 when the session played throughout.
pub fn run(args: &[String]) -> i32 {
    let Some(host) = args.first().and_then(|a| a.parse().ok()) else {
        eprintln!("usage: airtight --probe <receiver ip> [seconds]");
        return 2;
    };
    let seconds = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let settings = Settings { volume: 20, latency_ms: 250, ..Settings::load() };
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let report = Box::new(move |r: Report| {
        let text = match r {
            Report::Syncing => "syncing".to_string(),
            Report::Playing => "playing".to_string(),
            Report::Error(e) => format!("error: {e}"),
            Report::End(e) => format!("ended: {e}"),
        };
        let _ = tx.lock().unwrap().send(text);
    });
    let receiver = Receiver { name: "probe".into(), host };
    let engine = Engine::start(1, &receiver, &settings, Hub::start(), report);
    let mut outcome = Ok(());
    for want in ["syncing", "playing"] {
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(got) if got == want => println!("{got}"),
            Ok(got) => outcome = Err(got),
            Err(_) => outcome = Err(format!("no {want} report within 15 s")),
        }
        if outcome.is_err() {
            break;
        }
    }
    if outcome.is_ok() {
        if let Ok(trouble) = rx.recv_timeout(Duration::from_secs(seconds)) {
            outcome = Err(trouble);
        }
    }
    engine.stop();
    let line = match &outcome {
        Ok(()) => format!("probe ok: played {seconds} s"),
        Err(e) => format!("probe failed: {e}"),
    };
    println!("{line}");
    log!("[PROBE] {line}");
    i32::from(outcome.is_err())
}
