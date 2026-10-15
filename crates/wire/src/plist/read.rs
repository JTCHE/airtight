//! bplist00 reader. Rejects malformed input instead of guessing.

use super::Value;

const MAX_DEPTH: usize = 32;

struct Reader<'a> {
    data: &'a [u8],
    offsets: Vec<usize>,
    reference_size: usize,
}

fn read_sized(data: &[u8], at: usize, size: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(size)?)?;
    Some(bytes.iter().fold(0u64, |acc, &b| acc << 8 | u64::from(b)))
}

/// Parse a binary property list. None when the bytes are not one.
pub fn parse(data: &[u8]) -> Option<Value> {
    if data.len() < 40 || !data.starts_with(b"bplist00") {
        return None;
    }
    let trailer = &data[data.len() - 32..];
    let offset_size = trailer[6] as usize;
    let reference_size = trailer[7] as usize;
    let count = read_sized(trailer, 8, 8)? as usize;
    let top = read_sized(trailer, 16, 8)? as usize;
    let table = read_sized(trailer, 24, 8)? as usize;
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&reference_size) {
        return None;
    }
    if count > data.len() {
        return None;
    }
    let offsets = (0..count)
        .map(|i| read_sized(data, table + i * offset_size, offset_size).map(|o| o as usize))
        .collect::<Option<Vec<_>>>()?;
    Reader { data, offsets, reference_size }.object(top, 0)
}

impl Reader<'_> {
    fn object(&self, index: usize, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        let at = *self.offsets.get(index)?;
        let marker = *self.data.get(at)?;
        let (kind, low) = (marker >> 4, marker & 0xf);
        match kind {
            0x0 => match low {
                0x8 => Some(Value::Bool(false)),
                0x9 => Some(Value::Bool(true)),
                _ => None,
            },
            0x1 => self.int(at),
            0x2 => self.real(at + 1, 1 << low).map(Value::Real),
            0x3 => self.real(at + 1, 8).map(Value::Date),
            0x4 => {
                let (start, length) = self.length(at)?;
                Some(Value::Data(self.data.get(start..start + length)?.to_vec()))
            }
            0x5 => {
                let (start, length) = self.length(at)?;
                let bytes = self.data.get(start..start + length)?;
                Some(Value::String(String::from_utf8_lossy(bytes).into_owned()))
            }
            0x6 => {
                let (start, length) = self.length(at)?;
                let bytes = self.data.get(start..start + length * 2)?;
                let units: Vec<u16> =
                    bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                Some(Value::String(String::from_utf16_lossy(&units)))
            }
            0xa => self.array(at, depth),
            0xd => self.dict(at, depth),
            _ => None,
        }
    }

    fn int(&self, at: usize) -> Option<Value> {
        let size = 1usize << (self.data[at] & 0xf);
        let raw = read_sized(self.data, at + 1, size.min(8))?;
        Some(Value::Int(raw as i64))
    }

    fn real(&self, at: usize, size: usize) -> Option<f64> {
        let raw = read_sized(self.data, at, size)?;
        match size {
            4 => Some(f64::from(f32::from_bits(raw as u32))),
            8 => Some(f64::from_bits(raw)),
            _ => None,
        }
    }

    /// Where the payload of a sized object starts, and its element count.
    fn length(&self, at: usize) -> Option<(usize, usize)> {
        let low = self.data[at] & 0xf;
        if low != 0xf {
            return Some((at + 1, low as usize));
        }
        let marker = *self.data.get(at + 1)?;
        if marker >> 4 != 0x1 {
            return None;
        }
        let size = 1usize << (marker & 0xf);
        let length = read_sized(self.data, at + 2, size)? as usize;
        Some((at + 2 + size, length))
    }

    fn references(&self, start: usize, count: usize) -> Option<Vec<usize>> {
        (0..count)
            .map(|i| {
                let at = start + i * self.reference_size;
                read_sized(self.data, at, self.reference_size).map(|r| r as usize)
            })
            .collect()
    }

    fn array(&self, at: usize, depth: usize) -> Option<Value> {
        let (start, count) = self.length(at)?;
        let items = self.references(start, count)?;
        let values = items.into_iter().map(|i| self.object(i, depth + 1));
        Some(Value::Array(values.collect::<Option<Vec<_>>>()?))
    }

    fn dict(&self, at: usize, depth: usize) -> Option<Value> {
        let (start, count) = self.length(at)?;
        let keys = self.references(start, count)?;
        let values = self.references(start + count * self.reference_size, count)?;
        let mut entries = Vec::with_capacity(count);
        for (k, v) in keys.into_iter().zip(values) {
            let Value::String(key) = self.object(k, depth + 1)? else { return None };
            entries.push((key, self.object(v, depth + 1)?));
        }
        Some(Value::Dict(entries))
    }
}
