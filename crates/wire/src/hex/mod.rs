//! Lower-case hex text for identifiers and logs.

pub fn encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Bytes joined by `separator`, upper case: "1A:2B:...", as AirPlay ids want.
pub fn upper_joined(bytes: &[u8], separator: &str) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(separator)
}

/// An upper-case UUID string from 16 bytes.
pub fn uuid(bytes: &[u8; 16]) -> String {
    let text = upper_joined(bytes, "");
    format!("{}-{}-{}-{}-{}", &text[..8], &text[8..12], &text[12..16], &text[16..20], &text[20..])
}
