//! When a receiver can follow our clock: it probes (Delay_Req or
//! Pdelay_Req) in a streak, and its servo locks some time into the streak.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

/// Silence that ends a streak: a live receiver probes about once a second.
const STREAK_GAP: Duration = Duration::from_secs(3);
/// Full servo lock, counted from the first probe.
const LOCK: Duration = Duration::from_millis(2300);
/// Apple receivers lock sooner: this long after the third probe.
const SETTLE_AFTER_THIRD: Duration = Duration::from_millis(250);

struct Streak {
    first: Instant,
    third: Option<Instant>,
    last: Instant,
    count: u32,
}

#[derive(Default)]
pub struct Probes(Vec<(Ipv4Addr, Streak)>);

impl Probes {
    pub fn record(&mut self, peer: Ipv4Addr, at: Instant) {
        match self.0.iter_mut().find(|(p, _)| *p == peer) {
            Some((_, streak)) if at.duration_since(streak.last) <= STREAK_GAP => {
                streak.count += 1;
                if streak.count == 3 {
                    streak.third = Some(at);
                }
                streak.last = at;
            }
            Some((_, streak)) => *streak = Streak { first: at, third: None, last: at, count: 1 },
            None => self.0.push((peer, Streak { first: at, third: None, last: at, count: 1 })),
        }
    }

    pub fn is_ready(&self, peer: Ipv4Addr, now: Instant) -> bool {
        let Some((_, streak)) = self.0.iter().find(|(p, _)| *p == peer) else { return false };
        if now.duration_since(streak.last) > STREAK_GAP {
            return false;
        }
        let fast = streak.third.is_some_and(|third| now >= third + SETTLE_AFTER_THIRD);
        fast || now >= streak.first + LOCK
    }
}
