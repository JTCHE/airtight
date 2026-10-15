//! Random bytes from the system generator.

#[cfg(windows)]
pub fn fill(buffer: &mut [u8]) {
    use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
    // The system generator does not fail with these arguments.
    unsafe { BCryptGenRandom(None, buffer, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }
        .ok()
        .expect("system random generator");
}

#[cfg(target_os = "linux")]
pub fn fill(buffer: &mut [u8]) {
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(buffer)).expect("system random generator");
}

pub fn bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    fill(&mut out);
    out
}

pub fn u32() -> u32 {
    u32::from_le_bytes(bytes())
}

pub fn u64() -> u64 {
    u64::from_le_bytes(bytes())
}
