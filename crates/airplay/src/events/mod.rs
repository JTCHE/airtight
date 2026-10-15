//! The events channel: a TCP connection we open to the receiver, on which
//! it sends us requests, such as the remote presses from a Watch.

use crate::hap::Cipher;
use airtight_log::log;
use airtight_wire::mrp::{self, RemoteCommand};
use airtight_wire::rtsp::{self, Parsed};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub type OnRemote = Box<dyn Fn(RemoteCommand) + Send>;

pub struct Events {
    stream: TcpStream,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Events {
    pub fn start(stream: TcpStream, cipher: Cipher, on_remote: OnRemote) -> std::io::Result<Events> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        let running = Arc::new(AtomicBool::new(true));
        let (reader, flag) = (stream.try_clone()?, running.clone());
        let thread = thread::Builder::new()
            .name("airplay-events".into())
            .spawn(move || serve(reader, cipher, &on_remote, &flag))?;
        Ok(Events { stream, running, thread: Some(thread) })
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, mut cipher: Cipher, on_remote: &OnRemote, running: &AtomicBool) {
    let mut plain = Vec::new();
    let mut buffer = [0u8; 8192];
    while running.load(Ordering::Relaxed) {
        let count = match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => count,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(_) => return,
        };
        let Some(opened) = cipher.open(&buffer[..count]) else {
            log!("[EVENTS] a frame failed to decrypt; closing the channel");
            return;
        };
        plain.extend_from_slice(&opened);
        while let Parsed::Done(request, used) = rtsp::parse(&plain) {
            plain.drain(..used);
            let reply = cipher.seal(&rtsp::ok_reply(&request));
            if stream.write_all(&reply).is_err() {
                return;
            }
            if let Some(command) = mrp::parse_remote_command(&request.body) {
                log!("[REMOTE] {command:?}");
                on_remote(command);
            }
        }
    }
}
