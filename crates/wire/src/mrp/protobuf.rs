//! The protobuf writer subset MediaRemote needs: varints and
//! length-delimited fields.

#[derive(Default)]
pub struct Message(Vec<u8>);

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

impl Message {
    pub fn varint(&mut self, field: u32, value: u64) {
        put_varint(&mut self.0, u64::from(field) << 3);
        put_varint(&mut self.0, value);
    }

    pub fn bytes(&mut self, field: u32, value: &[u8]) {
        put_varint(&mut self.0, u64::from(field) << 3 | 2);
        put_varint(&mut self.0, value.len() as u64);
        self.0.extend_from_slice(value);
    }

    pub fn string(&mut self, field: u32, value: &str) {
        self.bytes(field, value.as_bytes());
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// A message with its varint length in front, as MRP frames them.
pub fn length_prefixed(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(message.len() + 3);
    put_varint(&mut out, message.len() as u64);
    out.extend_from_slice(message);
    out
}
