//! The Linux front end: no tray, one command line. `airtight` alone keeps
//! the saved receiver connected until SIGINT or SIGTERM; the systemd user
//! service that install.sh writes runs it that way.

use crate::link::{Link, Receiver, Shared, Snapshot};
use crate::nowplaying::Hub;
use crate::settings::Settings;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = "usage:
  airtight                    keep the saved receiver connected
  airtight --list             find the receivers on the network
  airtight --use <name | ip>  stream to this receiver from now on
  airtight --volume <0-100>   set the receiver volume
  airtight --probe <ip> [s]   play to a receiver for s seconds, then stop";

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

unsafe extern "C" {
    fn signal(number: i32, handler: extern "C" fn(i32)) -> usize;
}

/// Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match (args.first().map(String::as_str), args.get(1)) {
        (None, _) => stream(),
        (Some("--list"), None) => list(),
        (Some("--use"), Some(wanted)) => choose(wanted),
        (Some("--volume"), Some(volume)) => match volume.parse::<u8>() {
            Ok(v) if v <= 100 => edit(|s| s.volume = v),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> i32 {
    eprintln!("{USAGE}");
    2
}

fn list() -> i32 {
    let found = crate::discover();
    for r in &found {
        println!("{}\t{}", r.host, r.name);
    }
    if found.is_empty() {
        eprintln!("No receivers answered.");
        return 1;
    }
    0
}

/// Save a change; a running service reads it on its next start.
fn edit(change: impl FnOnce(&mut Settings)) -> i32 {
    let mut settings = Settings::load();
    change(&mut settings);
    settings.save();
    println!("Saved. If the service runs: systemctl --user restart airtight");
    0
}

/// By address, or by a name that the network answers with.
fn choose(wanted: &str) -> i32 {
    let receiver = match wanted.parse::<Ipv4Addr>() {
        Ok(host) => Some(Receiver { name: wanted.into(), host }),
        Err(_) => {
            let lower = wanted.to_lowercase();
            crate::discover().into_iter().find(|r| r.name.to_lowercase().contains(&lower))
        }
    };
    let Some(receiver) = receiver else {
        eprintln!("No receiver named {wanted:?} answered. Try airtight --list.");
        return 1;
    };
    println!("{} at {}", receiver.name, receiver.host);
    edit(|s| {
        s.receiver_name = Some(receiver.name.clone());
        s.receiver_host = Some(receiver.host.to_string());
        s.auto_connect = true;
    })
}

fn stream() -> i32 {
    let settings = Settings::load();
    if settings.receiver_host.is_none() {
        eprintln!("No receiver yet. Pick one: airtight --list, then airtight --use <name>.");
        return 1;
    }
    unsafe {
        signal(2, on_signal);
        signal(15, on_signal);
    }
    let settings = Arc::new(Mutex::new(settings));
    let saving = settings.clone();
    let shared = Shared {
        settings,
        media: Hub::start(),
        on_change: Box::new(|s: &Snapshot| {
            let name = s.receiver.as_ref().map_or("", |r| r.name.as_str());
            println!("{:?} {name} {}", s.state, s.note);
        }),
        on_receiver: Box::new(move |r: &Receiver| {
            let mut s = saving.lock().unwrap();
            (s.receiver_name, s.receiver_host) = (Some(r.name.clone()), Some(r.host.to_string()));
            s.save();
        }),
    };
    let link = Link::start(shared);
    while !STOP.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
    }
    link.quit();
    0
}
