use airtight_airplay::audio::{Correction, Drift};

fn settled(depth: f64) -> Drift {
    let mut drift = Drift::default();
    for _ in 0..200 {
        assert_eq!(drift.observe(depth), Correction::None);
    }
    drift
}

#[test]
fn small_drift_is_left_alone() {
    let mut drift = settled(10_000.0);
    for _ in 0..1000 {
        assert_eq!(drift.observe(10_030.0), Correction::None);
    }
}

#[test]
fn slow_drift_drops_or_repeats_one_frame() {
    let mut drift = settled(10_000.0);
    let deep: Vec<_> = (0..2000).map(|_| drift.observe(10_100.0)).collect();
    assert!(deep.contains(&Correction::Drop(1)));
    let mut drift = settled(10_000.0);
    let shallow: Vec<_> = (0..2000).map(|_| drift.observe(9_900.0)).collect();
    assert!(shallow.contains(&Correction::Insert(1)));
}

#[test]
fn a_stall_is_corrected_at_once() {
    let mut drift = settled(10_000.0);
    let mut seen = None;
    for _ in 0..2000 {
        match drift.observe(1_000.0) {
            Correction::Insert(n) if n > 1 => {
                seen = Some(n);
                break;
            }
            _ => {}
        }
    }
    assert!(seen.unwrap() > 1764);
}
