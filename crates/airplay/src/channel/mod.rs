//! The RTSP control connection: plain until pairing, HAP-encrypted after.
//! One request at a time; the caller serializes access.

use crate::hap::Cipher;
use crate::identity::Identity;
use crate::Error;
use airtight_wire::rtsp::{self, Message, Parsed};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Channel {
    stream: TcpStream,
    cipher: Option<Cipher>,
    received: Vec<u8>,
    cseq: u32,
    dacp_id: String,
    active_remote: String,
}

impl Channel {
    pub fn connect(address: SocketAddr, identity: &Identity) -> Result<Channel, Error> {
        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
        stream.set_write_timeout(Some(REPLY_TIMEOUT))?;
        Ok(Channel {
            stream,
            cipher: None,
            received: Vec::new(),
            cseq: 0,
            dacp_id: identity.dacp_id.clone(),
            active_remote: identity.active_remote.clone(),
        })
    }

    /// Encrypt everything from here on.
    pub fn encrypt(&mut self, cipher: Cipher) {
        self.cipher = Some(cipher);
    }

    /// Send one request and wait for its reply.
    pub fn request(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<Message, Error> {
        self.cseq += 1;
        let identity = rtsp::Identity { dacp_id: &self.dacp_id, active_remote: &self.active_remote };
        let plain = rtsp::request(method, uri, self.cseq, &identity, headers, body);
        let wire = match &mut self.cipher {
            Some(cipher) => cipher.seal(&plain),
            None => plain,
        };
        self.stream.write_all(&wire)?;
        loop {
            let reply = self.next_message()?;
            // A late reply to an earlier, timed-out request: skip it.
            if reply.cseq().is_none_or(|c| c == self.cseq) {
                return Ok(reply);
            }
        }
    }

    /// A request whose reply must be 200; returns the reply body.
    pub fn expect_ok(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let reply = self.request(method, uri, headers, body)?;
        match reply.status() {
            Some(200) => Ok(reply.body),
            status => Err(Error(format!("{method} {uri} answered {status:?}"))),
        }
    }

    fn next_message(&mut self) -> Result<Message, Error> {
        loop {
            match rtsp::parse(&self.received) {
                Parsed::Done(message, used) => {
                    self.received.drain(..used);
                    return Ok(message);
                }
                Parsed::Invalid => return Err(Error("malformed RTSP reply".into())),
                Parsed::NeedMore => {}
            }
            let mut buffer = [0u8; 16 * 1024];
            let count = self.stream.read(&mut buffer)?;
            if count == 0 {
                return Err(Error("the receiver closed the connection".into()));
            }
            let plain = match &mut self.cipher {
                Some(cipher) => cipher.open(&buffer[..count]).ok_or(Error("bad frame from receiver".into()))?,
                None => buffer[..count].to_vec(),
            };
            self.received.extend_from_slice(&plain);
        }
    }

    /// Close both directions so a reader blocked in another thread wakes.
    pub fn shutdown(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
