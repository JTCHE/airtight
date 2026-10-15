//! Follow MPRIS through `playerctl --follow`, which prints a line on each
//! change. Without playerctl, the receiver shows the placeholder.

pub mod art;

use crate::{Command, Track};
use std::io::{BufRead, BufReader};
use std::process::{Command as Process, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const FORMAT: &str = "{{status}}\t{{playerName}}\t{{title}}\t{{artist}}\t{{album}}\t{{mpris:length}}\t{{position}}\t{{mpris:artUrl}}";
/// playerctl quit (the session bus restarted): start it again after this.
const RESTART: Duration = Duration::from_secs(5);

pub type Sink = Box<dyn Fn(Option<Track>) + Send>;

pub struct Watcher {
    sink: Arc<Mutex<Sink>>,
}

impl Watcher {
    /// Call `sink` with the track, or None, after each change.
    pub fn start(sink: Sink) -> Watcher {
        let sink = Arc::new(Mutex::new(sink));
        let shared = sink.clone();
        let _ = thread::Builder::new().name("nowplaying".into()).spawn(move || follow(&shared));
        Watcher { sink }
    }

    /// Read the player now, even if nothing changed.
    pub fn refresh(&self) {
        let sink = self.sink.clone();
        thread::spawn(move || {
            let output = Process::new("playerctl").args(["metadata", "--format", FORMAT]).stderr(Stdio::null()).output();
            let line = output.map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string()).unwrap_or_default();
            (sink.lock().unwrap())(parse(&line));
        });
    }

    /// Pass a remote press to the player.
    pub fn control(&self, command: Command) {
        let verb = match command {
            Command::Play => "play",
            Command::Pause => "pause",
            Command::PlayPause => "play-pause",
            Command::Next => "next",
            Command::Previous => "previous",
        };
        let _ = Process::new("playerctl").arg(verb).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|mut c| c.wait());
    }
}

fn follow(sink: &Mutex<Sink>) {
    loop {
        let spawned = Process::new("playerctl")
            .args(["--follow", "metadata", "--format", FORMAT])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else { return (sink.lock().unwrap())(None) };
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                (sink.lock().unwrap())(parse(&line));
            }
        }
        let _ = child.wait();
        thread::sleep(RESTART);
    }
}

/// One playerctl line; an empty title means no player.
fn parse(line: &str) -> Option<Track> {
    let f: Vec<&str> = line.split('\t').collect();
    let [status, app, title, artist, album, length, position, art_url] = f[..] else { return None };
    let seconds = |micros: &str| micros.parse::<f64>().unwrap_or(0.0) / 1e6;
    (!title.is_empty()).then(|| Track {
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        app: app.into(),
        duration: seconds(length),
        position: seconds(position),
        playing: status == "Playing",
        art: art::load(art_url).map(Arc::new),
    })
}

/// Linux has no per-app output routing to check: the track always shows.
pub fn is_default_output(_endpoint_id: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_a_playerctl_line() {
        let track = super::parse("Playing\tspotify\tSong\tBand\tRecord\t180000000\t1500000\t").unwrap();
        assert_eq!((track.title.as_str(), track.app.as_str(), track.duration, track.position), ("Song", "spotify", 180.0, 1.5));
        assert!(track.playing && track.art.is_none());
        assert!(super::parse("Stopped\tspotify\t\t\t\t\t\t").is_none());
    }
}
