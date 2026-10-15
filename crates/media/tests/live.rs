//! Reads the real media session. Run by hand:
//! cargo test -p airtight-media --test live -- --ignored --nocapture

use airtight_media::{Watcher, art};
use std::sync::mpsc::channel;
use std::time::Duration;

#[test]
#[ignore]
fn reads_what_plays() {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let _watcher = Watcher::start(Box::new(move |track| {
        let _ = tx.lock().unwrap().send(track);
    }));
    let track = rx.recv_timeout(Duration::from_secs(10)).expect("no read within 10 s");
    match track {
        None => println!("nothing plays"),
        Some(t) => {
            println!("{} - {} [{}] {:.0}/{:.0} s playing={} app={}", t.artist, t.title, t.album, t.position, t.duration, t.playing, t.app);
            if let Some(cover) = t.art {
                let jpeg = art::to_jpeg(&cover).expect("the cover did not convert");
                println!("cover {} bytes, jpeg {} bytes", cover.len(), jpeg.len());
                assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
            }
        }
    }
}
