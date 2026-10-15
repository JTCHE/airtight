//! What the computer plays: the outputs, a loopback capture of one of
//! them, and its level meter for the sound watch. WASAPI on Windows,
//! PulseAudio or PipeWire on Linux.

#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod endpoint;
#[cfg(windows)]
pub mod meter;
#[cfg(target_os = "linux")]
mod pulse;
#[cfg(target_os = "linux")]
pub use pulse::{capture, com, endpoint, meter, pro_audio_thread};

pub use capture::Loopback;
pub use meter::Meter;

pub const SAMPLE_RATE: u32 = 44100;
pub const CHANNELS: u16 = 2;

/// An output to capture, by id and the name the system settings show.
#[derive(Clone, Debug, PartialEq)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
}

/// Join the multithreaded COM apartment. Every thread that touches an
/// endpoint calls this first; a second call is harmless.
#[cfg(windows)]
pub fn com() {
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

/// Register the calling thread with MMCSS as a pro-audio thread: a boost
/// while it runs, without a time-critical priority that can take a core
/// away from everything else.
#[cfg(windows)]
pub fn pro_audio_thread() -> bool {
    use windows::Win32::System::Threading::{AvSetMmThreadCharacteristicsW, GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
    use windows::core::w;
    let mut task = 0u32;
    if unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) }.is_ok() {
        return true;
    }
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
    false
}
