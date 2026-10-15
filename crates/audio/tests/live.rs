//! Needs a sound card. Run by hand:
//! cargo test -p airtight-audio --test live -- --ignored --nocapture

use airtight_audio::{Loopback, Meter, endpoint};
use std::time::{Duration, Instant};

#[test]
#[ignore]
fn captures_the_default_output() {
    for e in endpoint::list() {
        println!("endpoint: {} ({})", e.name, e.id);
    }
    let target = endpoint::find(None).expect("no render endpoint");
    println!("default: {}", target.name);
    let meter = Meter::open(&target.id).unwrap();
    println!("peak: {}", meter.peak().unwrap());
    let mut capture = Loopback::open(&target.id).unwrap();
    let start = Instant::now();
    let (mut samples, mut reads) = (Vec::new(), 0);
    while start.elapsed() < Duration::from_secs(1) {
        capture.read(&mut samples).unwrap();
        reads += 1;
    }
    let seconds = samples.len() as f64 / 2.0 / 44100.0;
    println!("{reads} reads, {seconds:.3} s of audio, lost {}", capture.lost);
    assert!((0.9..1.2).contains(&seconds));
}
