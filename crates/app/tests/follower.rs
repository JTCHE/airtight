use airtight_app::link::Gate;
use airtight_app::nowplaying::{Change, Follower, item_id};
use airtight_media::Track;

fn track() -> Track {
    Track {
        title: "a".into(),
        artist: "x".into(),
        album: "y".into(),
        app: "spotify".into(),
        duration: 180.0,
        position: 7.0,
        playing: true,
        art: None,
    }
}

fn at(position: f64, playing: bool) -> Track {
    Track { position, playing, ..track() }
}

#[test]
fn follows_tracks_seeks_and_pauses() {
    let mut f = Follower::default();
    assert_eq!(f.step(None, 0.0), Change::Track, "the placeholder goes out first");
    assert_eq!(f.step(None, 5.0), Change::None);
    assert_eq!(f.step(Some(&track()), 10.0), Change::Track);
    assert_eq!(f.step(Some(&at(9.0, true)), 12.0), Change::None, "normal play is no seek");
    assert_eq!(f.step(Some(&at(60.0, true)), 13.0), Change::Progress, "seek");
    assert_eq!(f.step(Some(&at(61.0, false)), 14.0), Change::Progress, "pause");
    assert_eq!(f.step(Some(&at(61.0, false)), 30.0), Change::None, "paused");
    assert_eq!(f.step(Some(&at(61.0, true)), 31.0), Change::Progress, "resume");
    let covered = Track { art: Some(vec![1u8].into()), ..at(62.0, true) };
    assert_eq!(f.step(Some(&covered), 32.0), Change::Track, "late cover");
    assert_eq!(f.step(Some(&Track { title: "c".into(), ..track() }), 33.0), Change::Track);
    assert_eq!(item_id(&track()), item_id(&Track { album: "z".into(), ..track() }), "album refines in place");
}

#[test]
fn the_gate_fires_once_per_run_of_sound() {
    assert!(Gate::new(true).step(0.5), "audio already playing fires");
    let mut gate = Gate::new(false);
    assert!(!gate.step(0.5), "loud before any silence does not fire");
    for _ in 0..10 {
        assert!(!gate.step(0.0));
    }
    assert!(gate.step(0.5), "sound after a silent run fires");
    assert!(!gate.step(0.5), "once, not per poll");
    assert!(!gate.step(0.0));
    assert!(!gate.step(0.5), "a silent run too short does not re-arm");
}
