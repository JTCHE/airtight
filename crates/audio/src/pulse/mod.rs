//! PulseAudio, or PipeWire's Pulse server, through libpulse-simple. The
//! library is loaded at run time: nothing to install to build, and every
//! Linux desktop ships it.

pub mod capture;
pub mod endpoint;
pub mod meter;

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::io::{Error, ErrorKind, Result};
use std::ptr::null;
use std::sync::OnceLock;

const RTLD_NOW: c_int = 2;
const STREAM_RECORD: c_int = 2;
const SAMPLE_S16LE: c_int = 3;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

unsafe extern "C" {
    fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

type New = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const SampleSpec,
    *const c_void,
    *const BufferAttr,
    *mut c_int,
) -> *mut c_void;
type Read = unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);
type StrError = unsafe extern "C" fn(c_int) -> *const c_char;

struct Api {
    new: New,
    read: Read,
    free: Free,
    strerror: StrError,
}

fn load() -> Option<Api> {
    unsafe {
        let library = dlopen(c"libpulse-simple.so.0".as_ptr(), RTLD_NOW);
        if library.is_null() {
            return None;
        }
        // pa_strerror lives in libpulse, which dlsym finds through the
        // dependencies of libpulse-simple.
        let symbol = |name: &CStr| Some(dlsym(library, name.as_ptr())).filter(|p| !p.is_null());
        Some(Api {
            new: std::mem::transmute::<*mut c_void, New>(symbol(c"pa_simple_new")?),
            read: std::mem::transmute::<*mut c_void, Read>(symbol(c"pa_simple_read")?),
            free: std::mem::transmute::<*mut c_void, Free>(symbol(c"pa_simple_free")?),
            strerror: std::mem::transmute::<*mut c_void, StrError>(symbol(c"pa_strerror")?),
        })
    }
}

fn api() -> Result<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    let missing = || Error::new(ErrorKind::NotFound, "libpulse-simple.so.0 is missing: install PulseAudio or pipewire-pulse");
    API.get_or_init(load).as_ref().ok_or_else(missing)
}

fn failure(api: &Api, code: c_int) -> Error {
    Error::other(unsafe { CStr::from_ptr((api.strerror)(code)) }.to_string_lossy().into_owned())
}

/// One record stream of 16-bit samples from a Pulse source.
struct Record {
    handle: *mut c_void,
}

// The handle belongs to whichever thread holds the Record.
unsafe impl Send for Record {}

impl Record {
    /// `fragment` is the bytes per delivery; at most `most` bytes wait
    /// unread before the server drops the oldest.
    fn open(source: &str, rate: u32, channels: u8, fragment: u32, most: u32) -> Result<Record> {
        let api = api()?;
        let spec = SampleSpec { format: SAMPLE_S16LE, rate, channels };
        let attr = BufferAttr { maxlength: most, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: fragment };
        let source = CString::new(source).map_err(|_| Error::from(ErrorKind::InvalidInput))?;
        let name = c"Airtight".as_ptr();
        let mut code = 0;
        let handle = unsafe { (api.new)(null(), name, STREAM_RECORD, source.as_ptr(), name, &spec, null(), &attr, &mut code) };
        if handle.is_null() {
            return Err(failure(api, code));
        }
        Ok(Record { handle })
    }

    /// Fill `out`, waiting for the audio as it comes.
    fn read(&mut self, out: &mut [u8]) -> Result<()> {
        let api = api()?;
        let mut code = 0;
        if unsafe { (api.read)(self.handle, out.as_mut_ptr().cast(), out.len(), &mut code) } < 0 {
            return Err(failure(api, code));
        }
        Ok(())
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        if let Ok(api) = api() {
            unsafe { (api.free)(self.handle) };
        }
    }
}

/// Windows needs COM on each audio thread; Pulse needs nothing.
pub fn com() {}

/// The Pulse server runs the realtime part; the reader needs no boost.
pub fn pro_audio_thread() -> bool {
    false
}
