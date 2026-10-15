//! Start with Windows: the per-user Run key names this exe.

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW};
use windows::core::{HSTRING, w};

const KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: windows::core::PCWSTR = w!("Airtight");

pub fn enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, KEY, VALUE, RRF_RT_REG_SZ, None, None, None) }.is_ok()
}

pub fn set(on: bool) {
    if !on {
        let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, KEY, VALUE) };
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let command = HSTRING::from(format!("\"{}\"", exe.display()));
    let bytes = unsafe { std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), (command.len() + 1) * 2) };
    let _ = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, KEY, VALUE, REG_SZ.0, Some(bytes.as_ptr().cast()), bytes.len() as u32) };
}
