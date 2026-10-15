//! Event-driven WASAPI loopback capture of one render endpoint, as 16-bit
//! stereo at 44.1 kHz. Windows signals an event once per device period
//! (10 ms) and the thread sleeps in between.
//!
//! The buffer is half a second on purpose: a shorter one overflowed on any
//! hiccup, the receiver ran dry and playback slid later for good.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::Result;

use crate::{CHANNELS, SAMPLE_RATE};
const BUFFER: Duration = Duration::from_millis(500);
/// No packet for this long: the endpoint is idle, so emit silence.
const SILENCE_GAP: Duration = Duration::from_millis(40);
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const SILENT: u32 = AUDCLNT_BUFFERFLAGS_SILENT.0 as u32;
const DISCONTINUITY: u32 = AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32;

pub struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    /// The end of the audio handed out so far, on the wall clock.
    clock: Instant,
    last_packet: Option<Instant>,
    /// Overflows while audio was flowing: audio was lost.
    pub lost: u32,
}

impl Loopback {
    pub fn open(endpoint_id: &str) -> Result<Loopback> {
        let device = crate::endpoint::device(endpoint_id)?;
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
        let block = CHANNELS * 4;
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: CHANNELS,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * u32::from(block),
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
            | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
            | AUDCLNT_STREAMFLAGS_NOPERSIST;
        let hundred_ns = (BUFFER.as_nanos() / 100) as i64;
        unsafe {
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, hundred_ns, 0, &format, None)?;
            let event = CreateEventW(None, false, false, None)?;
            let capture = client.SetEventHandle(event).and_then(|_| client.GetService::<IAudioCaptureClient>());
            let capture = match capture {
                Ok(capture) => capture,
                Err(e) => {
                    let _ = CloseHandle(event);
                    return Err(e);
                }
            };
            client.Start()?;
            Ok(Loopback { client, capture, event, clock: Instant::now(), last_packet: None, lost: 0 })
        }
    }

    /// Block until audio arrives or a silence gap passes, then append what
    /// there is to `out` as interleaved 16-bit samples.
    pub fn read(&mut self, out: &mut Vec<i16>) -> Result<()> {
        loop {
            let signalled = unsafe { WaitForSingleObject(self.event, SILENCE_GAP.as_millis() as u32) } == WAIT_OBJECT_0;
            let before = out.len();
            if signalled {
                self.drain(out)?;
            }
            let now = Instant::now();
            if out.len() > before {
                self.clock = now;
                return Ok(());
            }
            let gap = now - self.clock;
            if gap >= SILENCE_GAP {
                let frames = (gap.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize;
                self.clock += Duration::from_secs_f64(frames as f64 / f64::from(SAMPLE_RATE));
                out.resize(out.len() + frames * usize::from(CHANNELS), 0);
                return Ok(());
            }
        }
    }

    fn drain(&mut self, out: &mut Vec<i16>) -> Result<()> {
        loop {
            if unsafe { self.capture.GetNextPacketSize()? } == 0 {
                return Ok(());
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
            unsafe { self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)? };
            let count = frames as usize * usize::from(CHANNELS);
            if flags & SILENT != 0 || data.is_null() {
                out.resize(out.len() + count, 0);
            } else {
                let samples = unsafe { std::slice::from_raw_parts(data as *const f32, count) };
                out.extend(samples.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
            }
            unsafe { self.capture.ReleaseBuffer(frames)? };
            let now = Instant::now();
            // The flag also marks a restart after idle, which is harmless.
            // Mid-stream it means the buffer overflowed.
            let flowing = self.last_packet.is_some_and(|t| now - t < Duration::from_millis(200));
            if flags & DISCONTINUITY != 0 && flowing {
                self.lost += 1;
            }
            self.last_packet = Some(now);
        }
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}
