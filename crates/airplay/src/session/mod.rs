//! One streaming session with one receiver, from GET /info to TEARDOWN.

mod keepalive;
mod nowplaying;
mod setup;

use crate::audio::{SAMPLE_RATE, Sockets, Stream};
use crate::channel::Channel;
use crate::events::{Events, OnRemote};
use crate::identity::{Identity, random_uuid};
use crate::{Error, pairing};
use airtight_crypto::random;
use airtight_log::log;
use airtight_ptp::{Clock, Engine};
use airtight_wire::mrp::{Sender, Track};
use airtight_wire::rtsp::PLIST;
use keepalive::Keepalive;
use nowplaying::Publisher;
use setup::Ids;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Config {
    pub receiver: SocketAddrV4,
    pub volume: u8,
    /// Wanted buffer depth; the receiver's window may clamp it.
    pub latency_ms: u32,
}

pub struct Session {
    channel: Arc<Mutex<Channel>>,
    identity: Identity,
    ids: Ids,
    session_url: String,
    receiver: Ipv4Addr,
    ptp: Engine,
    audio: Mutex<Option<(Sockets, SocketAddr, SocketAddr, [u8; 32])>>,
    latency_ms: u32,
    publisher: Mutex<Publisher>,
    keepalive: Mutex<Option<Keepalive>>,
    _events: Option<Events>,
}

impl Session {
    pub fn connect(config: &Config, identity: Identity, on_remote: OnRemote) -> Result<Session, Error> {
        let receiver = *config.receiver.ip();
        let local = local_address(config.receiver)?;
        // The ports first: when another sender holds them, fail before the
        // receiver sees us at all.
        let ptp = Engine::start(Clock::new(), identity.clock_id, local)
            .map_err(|e| Error(format!("Another program uses the PTP ports 319 and 320 ({e})")))?;
        ptp.set_peers(vec![receiver]);
        let mut channel = Channel::connect(SocketAddr::V4(config.receiver), &identity)?;
        channel.expect_ok("GET", "/info", &[], b"")?;
        let keys = pairing::transient(&mut channel)?;
        channel.encrypt(keys.control());
        log!("[AIRPLAY] paired with {receiver}, channel encrypted");

        let ids = Ids { session_id: random::u32(), session_uuid: random_uuid(), group_uuid: random_uuid() };
        let session_url = format!("rtsp://{local}/{}", ids.session_id);
        let headers = [("Content-Type", PLIST)];
        let reply = channel.expect_ok("SETUP", &session_url, &headers, &setup::session(&identity, &ids, local))?;
        let events = match setup::event_port(&reply) {
            Some(port) => open_events(receiver, port, keys.events(), on_remote),
            None => None,
        };
        channel.expect_ok("RECORD", &session_url, &[], b"")?;

        let sockets = Sockets::bind(local)?;
        let (data_port, control_port) = (sockets.data.local_addr()?.port(), sockets.control.local_addr()?.port());
        let body = setup::stream(&ids, &keys.audio(), data_port, control_port);
        let reply = channel.expect_ok("SETUP", &session_url, &headers, &body)?;
        let stream = setup::stream_reply(&reply).ok_or(Error("the stream SETUP reply has no ports".into()))?;
        let latency_ms = clamp_latency(config.latency_ms, &stream);
        channel.expect_ok("SETPEERS", &session_url, &headers, &setup::peers(receiver, local))?;
        log!("[AIRPLAY] session up: data {} control {} buffer {latency_ms} ms", stream.data_port, stream.control_port);

        let to = |port| SocketAddr::V4(SocketAddrV4::new(receiver, port));
        let channel = Arc::new(Mutex::new(channel));
        let session = Session {
            keepalive: Mutex::new(Some(Keepalive::start(channel.clone())?)),
            channel,
            identity,
            ids,
            session_url,
            receiver,
            ptp,
            audio: Mutex::new(Some((sockets, to(stream.data_port), to(stream.control_port), keys.audio()))),
            latency_ms,
            publisher: Mutex::new(Publisher::default()),
            _events: events,
        };
        session.set_volume(config.volume)?;
        Ok(session)
    }

    pub fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    /// True once the receiver follows our clock, so audio can start.
    pub fn clock_ready(&self) -> bool {
        self.ptp.is_ready(self.receiver)
    }

    /// Start the audio: the first frame pushed sounds one buffer from now.
    pub fn start_stream(&self) -> Result<Stream, Error> {
        let taken = self.audio.lock().unwrap().take();
        let (sockets, data, control, key) = taken.ok_or(Error("the stream already started".into()))?;
        let clock = self.ptp.clock();
        let mut stream = Stream::new(sockets, data, control, key, clock, self.ptp.clock_id())?;
        stream.start(clock.now_ns() + u64::from(self.latency_ms) * 1_000_000);
        Ok(stream)
    }

    pub fn set_volume(&self, percent: u8) -> Result<(), Error> {
        let headers = [("Content-Type", "text/parameters")];
        let mut channel = self.channel.lock().unwrap();
        channel.expect_ok("SET_PARAMETER", &self.session_url, &headers, &setup::volume(percent))?;
        Ok(())
    }

    fn sender(&self) -> Sender<'_> {
        Sender {
            dacp_id: &self.identity.dacp_id,
            name: &self.identity.name,
            session_uuid: &self.ids.session_uuid,
            group_uuid: &self.ids.group_uuid,
            device_uuid: &self.identity.device_uuid,
            message_uuid: &self.ids.session_uuid,
        }
    }

    /// Show `track` on the receiver: a new item, or new tags or cover.
    pub fn publish_track(&self, track: &Track) -> Result<(), Error> {
        let mut channel = self.channel.lock().unwrap();
        self.publisher.lock().unwrap().track(&mut channel, &self.sender(), track)
    }

    /// Correct the receiver's timeline: a seek, a pause, a resume.
    pub fn publish_progress(&self, track: &Track) -> Result<(), Error> {
        let mut channel = self.channel.lock().unwrap();
        self.publisher.lock().unwrap().progress(&mut channel, &self.sender(), track)
    }

    /// Why the session died, once it has.
    pub fn failure(&self) -> Option<String> {
        self.keepalive.lock().unwrap().as_ref().and_then(Keepalive::failure)
    }

    /// TEARDOWN, so the receiver takes the next session at once.
    pub fn close(&self) {
        let keepalive = self.keepalive.lock().unwrap().take();
        drop(keepalive);
        let mut channel = self.channel.lock().unwrap();
        match channel.request("TEARDOWN", &self.session_url, &[], b"") {
            Ok(reply) => log!("[AIRPLAY] TEARDOWN answered {:?}", reply.status()),
            Err(e) => log!("[AIRPLAY] TEARDOWN failed: {e}"),
        }
        channel.shutdown();
    }
}

fn open_events(receiver: Ipv4Addr, port: u16, cipher: crate::hap::Cipher, on_remote: OnRemote) -> Option<Events> {
    let address = SocketAddr::V4(SocketAddrV4::new(receiver, port));
    let opened = TcpStream::connect_timeout(&address, Duration::from_secs(3))
        .and_then(|stream| Events::start(stream, cipher, on_remote));
    match opened {
        Ok(events) => Some(events),
        Err(e) => {
            log!("[AIRPLAY] events channel failed: {e}; remote buttons are off");
            None
        }
    }
}

/// The local address Windows routes to the receiver from.
pub(crate) fn local_address(receiver: SocketAddrV4) -> Result<Ipv4Addr, Error> {
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    probe.connect(receiver)?;
    match probe.local_addr()? {
        SocketAddr::V4(local) => Ok(*local.ip()),
        SocketAddr::V6(_) => Err(Error("no IPv4 route to the receiver".into())),
    }
}

fn clamp_latency(wanted_ms: u32, stream: &setup::StreamReply) -> u32 {
    let to_ms = |frames: u32| (u64::from(frames) * 1000 / u64::from(SAMPLE_RATE)) as u32;
    let low = stream.latency_min.map_or(0, to_ms);
    let high = stream.latency_max.map_or(u32::MAX, to_ms);
    wanted_ms.clamp(low, high.max(low))
}
