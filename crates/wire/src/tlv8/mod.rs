//! TLV8, the HomeKit pairing body: type byte, length byte, value. Values
//! longer than 255 bytes go out as consecutive fragments of one type.

pub const METHOD: u8 = 0x00;
pub const SALT: u8 = 0x02;
pub const PUBLIC_KEY: u8 = 0x03;
pub const PROOF: u8 = 0x04;
pub const STATE: u8 = 0x06;
pub const ERROR: u8 = 0x07;
pub const FLAGS: u8 = 0x13;

pub fn encode(items: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(kind, value) in items {
        if value.is_empty() {
            out.extend_from_slice(&[kind, 0]);
        }
        for chunk in value.chunks(255) {
            out.push(kind);
            out.push(chunk.len() as u8);
            out.extend_from_slice(chunk);
        }
    }
    out
}

/// Items in order, with adjacent fragments of one type joined. None when
/// the input ends inside an item.
pub fn decode(mut data: &[u8]) -> Option<Vec<(u8, Vec<u8>)>> {
    let mut items: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut previous_full = false;
    while !data.is_empty() {
        let kind = data[0];
        let length = *data.get(1)? as usize;
        let value = data.get(2..2 + length)?;
        match items.last_mut() {
            Some((last, joined)) if previous_full && *last == kind => joined.extend_from_slice(value),
            _ => items.push((kind, value.to_vec())),
        }
        previous_full = length == 255;
        data = &data[2 + length..];
    }
    Some(items)
}

pub fn find(items: &[(u8, Vec<u8>)], kind: u8) -> Option<&[u8]> {
    items.iter().find(|(k, _)| *k == kind).map(|(_, v)| v.as_slice())
}
