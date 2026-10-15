//! One line per event in session.log next to the settings, rolled over to
//! session.1.log past 5 MB. Nothing is written until `open` is called,
//! so tests stay out of the real log.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const ROLL_BYTES: u64 = 5_000_000;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Where the settings and the log live: %APPDATA%\Airtight on Windows,
/// ~/.config/airtight on Linux.
pub fn directory() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Airtight"));
    #[cfg(target_os = "linux")]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("airtight"));
    base.unwrap_or_else(|| PathBuf::from("."))
}

pub fn path() -> PathBuf {
    directory().join("session.log")
}

pub fn open() {
    let _ = fs::create_dir_all(directory());
    let path = path();
    if fs::metadata(&path).is_ok_and(|m| m.len() > ROLL_BYTES) {
        let _ = fs::rename(&path, directory().join("session.1.log"));
    }
    *FILE.lock().unwrap() = OpenOptions::new().create(true).append(true).open(path).ok();
}

/// Append one line, stamped with the local time. Never fails.
pub fn line(text: &str) {
    let mut file = FILE.lock().unwrap();
    let Some(file) = file.as_mut() else { return };
    let (h, m, s, ms) = clock();
    let _ = writeln!(file, "{h:02}:{m:02}:{s:02}.{ms:03} {text}");
}

/// The local time of day: hours, minutes, seconds, milliseconds.
#[cfg(windows)]
fn clock() -> (u16, u16, u16, u16) {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (t.wHour, t.wMinute, t.wSecond, t.wMilliseconds)
}

#[cfg(target_os = "linux")]
fn clock() -> (u16, u16, u16, u16) {
    /// glibc's struct tm: nine ints, then the offset and the zone name.
    #[repr(C)]
    struct Tm([i32; 9], i64, *const u8);
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let mut tm = Tm([0; 9], 0, std::ptr::null());
    unsafe { localtime_r(&(now.as_secs() as i64), &mut tm) };
    (tm.0[2] as u16, tm.0[1] as u16, tm.0[0] as u16, now.subsec_millis() as u16)
}

/// `log!("[TAG] {}", x)` formats and appends one line.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::line(&format!($($arg)*)) };
}
