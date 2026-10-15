//! Message shapes against the sizes and fields iOS senders use.

use airtight_ptp::engine::Probes;
use airtight_ptp::message;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

const CLOCK: u64 = 0x1a2b_3d4e_a1b2_c3d4;

#[test]
fn messages_have_the_ios_sizes() {
    assert_eq!(message::announce(CLOCK, 1).len(), 76);
    assert_eq!(message::signaling(CLOCK, 1).len(), 106);
    assert_eq!(message::sync(CLOCK, 1).len(), 44);
    let follow = message::follow_up(CLOCK, 7, 1_500_000_000_123);
    assert_eq!(follow.len(), 96);
    assert_eq!(message::sequence(&follow), 7);
    assert_eq!(&follow[34..44], &[0, 0, 0, 0, 0x05, 0xdc, 0, 0, 0, 0x7b]);
    assert_eq!(follow[0], 0x18);
    assert_eq!(message::source_clock(&follow), CLOCK);
}

#[test]
fn delay_response_echoes_the_requester() {
    let mut request = vec![0u8; 44];
    request[0] = 0x11;
    request[20..30].copy_from_slice(&[9, 9, 9, 9, 9, 9, 9, 9, 0, 1]);
    request[30..32].copy_from_slice(&[0, 42]);
    let reply = message::response(message::DELAY_RESP, CLOCK, &request, 5);
    assert_eq!(reply.len(), 54);
    assert_eq!(message::kind(&reply), message::DELAY_RESP);
    assert_eq!(message::sequence(&reply), 42);
    assert_eq!(&reply[44..54], &request[20..30]);
}

#[test]
fn unicast_requests_are_granted() {
    let mut request = vec![0u8; 34 + 10 + 10];
    request[0] = 0x1c;
    request[20..28].copy_from_slice(&7u64.to_be_bytes());
    request[44..48].copy_from_slice(&[0, 4, 0, 6]);
    request[48..54].copy_from_slice(&[0xb0, 0xfd, 0, 0, 0, 0]);
    let grant = message::grant(CLOCK, &request).unwrap();
    assert_eq!(grant.len(), 34 + 10 + 12);
    assert_eq!(&grant[44..56], &[0, 5, 0, 8, 0xb0, 0xfd, 0, 0, 1, 0x2c, 0, 1]);
    assert_eq!(message::grant(CLOCK, &request[..44]), None);
}

#[test]
fn readiness_follows_the_probe_streak() {
    let peer = Ipv4Addr::new(10, 0, 0, 2);
    let start = Instant::now();
    let at = |ms| start + Duration::from_millis(ms);
    let mut probes = Probes::default();
    assert!(!probes.is_ready(peer, start));
    probes.record(peer, at(0));
    probes.record(peer, at(500));
    assert!(!probes.is_ready(peer, at(900)));
    probes.record(peer, at(1000));
    assert!(probes.is_ready(peer, at(1300)), "third probe plus settle");
    assert!(!probes.is_ready(peer, at(5000)), "a silent receiver is not ready");
    probes.record(peer, at(6000));
    assert!(!probes.is_ready(peer, at(6100)), "a new streak starts over");
}
