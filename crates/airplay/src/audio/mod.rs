//! The realtime audio stream: ALAC packets to the receiver's data port,
//! anchor packets and retransmits on its control port.

mod drift;
mod ring;

pub use drift::{Correction, Drift};

use airtight_crypto::random;
use airtight_ptp::Clock;
use airtight_wire::{alac, rtp};
use ring::Ring;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const SAMPLE_RATE: u32 = 44100;
pub const CHANNELS: usize = 2;
const SAMPLES_PER_PACKET: usize = rtp::FRAMES_PER_PACKET as usize * CHANNELS;
const SYNC_EVERY_PACKETS: u32 = 100;

/// The sockets, bound before SETUP so their ports can go in it.
pub struct Sockets {
    pub data: UdpSocket,
    pub control: UdpSocket,
}

impl Sockets {
    pub fn bind(local: std::net::Ipv4Addr) -> std::io::Result<Sockets> {
        Ok(Sockets { data: UdpSocket::bind((local, 0))?, control: UdpSocket::bind((local, 0))? })
    }
}

pub struct Stream {
    data: UdpSocket,
    control: UdpSocket,
    receiver_data: SocketAddr,
    receiver_control: SocketAddr,
    key: [u8; 32],
    clock: Clock,
    clock_id: u64,
    sequence: u16,
    /// RTP time of the next frame to send.
    timestamp: u32,
    /// (RTP time, PTP ns): this frame sounds at this instant.
    anchor: Option<(u32, u64)>,
    pending: Vec<i16>,
    packets_since_sync: u32,
    sent_any: bool,
    drift: Drift,
    ring: Arc<Mutex<Ring>>,
    running: Arc<AtomicBool>,
    resender: Option<JoinHandle<()>>,
}

impl Stream {
    pub fn new(
        sockets: Sockets,
        receiver_data: SocketAddr,
        receiver_control: SocketAddr,
        key: [u8; 32],
        clock: Clock,
        clock_id: u64,
    ) -> std::io::Result<Stream> {
        let ring = Arc::new(Mutex::new(Ring::default()));
        let running = Arc::new(AtomicBool::new(true));
        let control = sockets.control.try_clone()?;
        control.set_read_timeout(Some(Duration::from_millis(200)))?;
        let (ring_for_thread, running_for_thread) = (ring.clone(), running.clone());
        let resender = thread::Builder::new()
            .name("rtp-resend".into())
            .spawn(move || resend(&control, &ring_for_thread, &running_for_thread))?;
        Ok(Stream {
            data: sockets.data,
            control: sockets.control,
            receiver_data,
            receiver_control,
            key,
            clock,
            clock_id,
            sequence: random::u32() as u16,
            timestamp: random::u32(),
            anchor: None,
            pending: Vec::with_capacity(SAMPLES_PER_PACKET * 4),
            packets_since_sync: 0,
            sent_any: false,
            drift: Drift::default(),
            ring,
            running,
            resender: Some(resender),
        })
    }

    /// The next frame sent sounds at `play_at_ns` on the PTP timeline.
    pub fn start(&mut self, play_at_ns: u64) {
        self.anchor = Some((self.timestamp, play_at_ns));
        self.send_sync(true);
    }

    /// The frame that sounds at `ns`, by the anchor.
    fn frame_at(&self, ns: u64) -> u32 {
        let (frame, at) = self.anchor.expect("start() comes first");
        let elapsed = ns as i64 - at as i64;
        frame.wrapping_add((elapsed as i128 * SAMPLE_RATE as i128 / 1_000_000_000) as i32 as u32)
    }

    fn send_sync(&mut self, first: bool) {
        let now = self.clock.now_ns();
        let packet = rtp::sync_packet(first, self.frame_at(now), now, self.clock_id);
        let _ = self.control.send_to(&packet, self.receiver_control);
        self.packets_since_sync = 0;
    }

    /// Frames queued ahead of the one that plays now.
    pub fn depth(&self) -> f64 {
        let queued = self.timestamp.wrapping_add((self.pending.len() / CHANNELS) as u32);
        queued.wrapping_sub(self.frame_at(self.clock.now_ns())) as i32 as f64
    }

    /// Queue interleaved stereo samples and send every whole packet.
    pub fn push(&mut self, samples: &[i16]) -> std::io::Result<()> {
        self.pending.extend_from_slice(samples);
        match self.drift.observe(self.depth()) {
            Correction::None => {}
            Correction::Drop(frames) => {
                let drop = (frames * CHANNELS).min(self.pending.len());
                self.pending.drain(..drop);
            }
            Correction::Insert(frames) => {
                // One frame repeats its neighbour; a long gap becomes silence.
                let filler = match self.pending.get(..CHANNELS) {
                    Some(frame) if frames == 1 => frame.to_vec(),
                    _ => vec![0; frames * CHANNELS],
                };
                self.pending.splice(0..0, filler);
            }
        }
        while self.pending.len() >= SAMPLES_PER_PACKET {
            let frame = alac::encode(&self.pending[..SAMPLES_PER_PACKET]);
            self.pending.drain(..SAMPLES_PER_PACKET);
            self.send_packet(&frame)?;
        }
        Ok(())
    }

    fn send_packet(&mut self, frame: &[u8]) -> std::io::Result<()> {
        if self.packets_since_sync >= SYNC_EVERY_PACKETS {
            self.send_sync(false);
        }
        let packet = rtp::audio_packet(&self.key, !self.sent_any, self.sequence, self.timestamp, frame);
        self.sent_any = true;
        self.data.send_to(&packet, self.receiver_data)?;
        self.ring.lock().unwrap().store(self.sequence, packet);
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(rtp::FRAMES_PER_PACKET);
        self.packets_since_sync += 1;
        Ok(())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.resender.take() {
            let _ = thread.join();
        }
    }
}

fn resend(control: &UdpSocket, ring: &Mutex<Ring>, running: &AtomicBool) {
    let mut buffer = [0u8; 2048];
    while running.load(Ordering::Relaxed) {
        let Ok((length, from)) = control.recv_from(&mut buffer) else { continue };
        let Some(request) = rtp::parse_resend_request(&buffer[..length]) else { continue };
        for offset in 0..request.count.min(64) {
            let sequence = request.first.wrapping_add(offset);
            let reply = ring.lock().unwrap().get(sequence).map(|p| rtp::resend_reply(request.request, p));
            if let Some(reply) = reply {
                let _ = control.send_to(&reply, from);
            }
        }
    }
}
