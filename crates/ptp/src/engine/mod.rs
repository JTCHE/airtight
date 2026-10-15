//! Sockets and threads: one thread sends Announce, Signaling and Sync on
//! schedule, one per port answers what receivers send.

mod readiness;

pub use readiness::Probes;

use crate::clock::Clock;
use crate::message;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const EVENT_PORT: u16 = 319;
pub const GENERAL_PORT: u16 = 320;
const MULTICAST: Ipv4Addr = Ipv4Addr::new(224, 0, 1, 129);
const ANNOUNCE_EVERY: Duration = Duration::from_secs(1);
const SYNC_EVERY: Duration = Duration::from_millis(125);
const POLL: Duration = Duration::from_millis(100);

struct Shared {
    clock: Clock,
    clock_id: u64,
    running: AtomicBool,
    peers: Mutex<Vec<Ipv4Addr>>,
    probes: Mutex<Probes>,
    event: UdpSocket,
    general: UdpSocket,
}

pub struct Engine {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

fn open(port: u16, interface: Ipv4Addr) -> io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
    // A failed join leaves unicast working, which is what receivers use.
    let _ = socket.join_multicast_v4(&MULTICAST, &interface);
    socket.set_multicast_ttl_v4(1)?;
    socket.set_multicast_loop_v4(false)?;
    socket.set_read_timeout(Some(POLL))?;
    Ok(socket)
}

impl Engine {
    /// Bind UDP 319 and 320 and start serving `clock` as grandmaster.
    pub fn start(clock: Clock, clock_id: u64, interface: Ipv4Addr) -> io::Result<Engine> {
        let shared = Arc::new(Shared {
            clock,
            clock_id,
            running: AtomicBool::new(true),
            peers: Mutex::new(Vec::new()),
            probes: Mutex::new(Probes::default()),
            event: open(EVENT_PORT, interface)?,
            general: open(GENERAL_PORT, interface)?,
        });
        let mut threads = Vec::new();
        for (name, run) in [
            ("ptp-send", schedule as fn(&Shared)),
            ("ptp-event", |s: &Shared| receive(s, &s.event)),
            ("ptp-general", |s: &Shared| receive(s, &s.general)),
        ] {
            let shared = shared.clone();
            threads.push(thread::Builder::new().name(name.into()).spawn(move || run(&shared))?);
        }
        Ok(Engine { shared, threads })
    }

    /// Serve these receivers by unicast. With none, the multicast group gets it.
    pub fn set_peers(&self, peers: Vec<Ipv4Addr>) {
        *self.shared.peers.lock().unwrap() = peers;
    }

    pub fn clock(&self) -> Clock {
        self.shared.clock
    }

    pub fn clock_id(&self) -> u64 {
        self.shared.clock_id
    }

    /// True once `peer` has probed our clock long enough to follow it.
    pub fn is_ready(&self, peer: Ipv4Addr) -> bool {
        self.shared.probes.lock().unwrap().is_ready(peer, Instant::now())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn send(shared: &Shared, socket: &UdpSocket, port: u16, packet: &[u8]) {
    let peers = shared.peers.lock().unwrap().clone();
    if peers.is_empty() {
        let _ = socket.send_to(packet, SocketAddrV4::new(MULTICAST, port));
    }
    for peer in peers {
        let _ = socket.send_to(packet, SocketAddrV4::new(peer, port));
    }
}

fn schedule(shared: &Shared) {
    let mut sequences = [0u16; 3]; // announce, signaling, sync
    let mut next_announce = Instant::now();
    let mut next_sync = Instant::now();
    while shared.running.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= next_announce {
            send(shared, &shared.general, GENERAL_PORT, &message::announce(shared.clock_id, sequences[0]));
            send(shared, &shared.general, GENERAL_PORT, &message::signaling(shared.clock_id, sequences[1]));
            sequences[0] = sequences[0].wrapping_add(1);
            sequences[1] = sequences[1].wrapping_add(1);
            next_announce = (next_announce + ANNOUNCE_EVERY).max(now);
        }
        if now >= next_sync {
            let sequence = sequences[2];
            send(shared, &shared.event, EVENT_PORT, &message::sync(shared.clock_id, sequence));
            let egress = shared.clock.now_ns();
            let follow_up = message::follow_up(shared.clock_id, sequence, egress);
            send(shared, &shared.general, GENERAL_PORT, &follow_up);
            sequences[2] = sequence.wrapping_add(1);
            next_sync = (next_sync + SYNC_EVERY).max(now);
        }
        let wake = next_announce.min(next_sync);
        thread::sleep(wake.saturating_duration_since(Instant::now()));
    }
}

fn receive(shared: &Shared, socket: &UdpSocket) {
    let mut buffer = [0u8; 512];
    while shared.running.load(Ordering::Relaxed) {
        // Errors include Windows' ICMP "connection reset" on UDP: skip them.
        let Ok((length, SocketAddr::V4(from))) = socket.recv_from(&mut buffer) else { continue };
        let received = Instant::now();
        let rx_ns = shared.clock.at(received);
        let packet = &buffer[..length];
        if length < message::HEADER || message::source_clock(packet) == shared.clock_id {
            continue;
        }
        let reply_to = |port| SocketAddrV4::new(*from.ip(), port);
        match message::kind(packet) {
            message::DELAY_REQ => {
                shared.probes.lock().unwrap().record(*from.ip(), received);
                let reply = message::response(message::DELAY_RESP, shared.clock_id, packet, rx_ns);
                let _ = shared.general.send_to(&reply, reply_to(GENERAL_PORT));
            }
            message::PDELAY_REQ => {
                shared.probes.lock().unwrap().record(*from.ip(), received);
                let reply = message::response(message::PDELAY_RESP, shared.clock_id, packet, rx_ns);
                let _ = shared.event.send_to(&reply, reply_to(EVENT_PORT));
                let egress = shared.clock.now_ns();
                let follow = message::response(message::PDELAY_RESP_FOLLOW_UP, shared.clock_id, packet, egress);
                let _ = shared.general.send_to(&follow, reply_to(GENERAL_PORT));
            }
            message::SIGNALING => {
                if let Some(reply) = message::grant(shared.clock_id, packet) {
                    let _ = shared.general.send_to(&reply, reply_to(GENERAL_PORT));
                }
            }
            _ => {}
        }
    }
}
