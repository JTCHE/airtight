//! bplist00 writer. Objects are not shared: each value is written once
//! where it appears, which is valid and keeps the writer simple.

use super::Value;

struct Writer {
    objects: Vec<Vec<u8>>,
    reference_size: usize,
}

/// Serialize `root` as a binary property list.
pub fn serialize(root: &Value) -> Vec<u8> {
    let count = count_objects(root);
    let reference_size = if count < 256 { 1 } else if count < 65536 { 2 } else { 4 };
    let mut writer = Writer { objects: Vec::with_capacity(count), reference_size };
    writer.add(root);

    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(count);
    for object in &writer.objects {
        offsets.push(out.len());
        out.extend_from_slice(object);
    }
    let table_offset = out.len();
    let offset_size = bytes_for(table_offset as u64);
    for offset in offsets {
        put_sized(&mut out, offset as u64, offset_size);
    }
    out.extend_from_slice(&[0; 6]);
    out.push(offset_size as u8);
    out.push(reference_size as u8);
    out.extend_from_slice(&(count as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&(table_offset as u64).to_be_bytes());
    out
}

fn count_objects(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(count_objects).sum::<usize>(),
        Value::Dict(entries) => {
            1 + entries.iter().map(|(_, v)| 1 + count_objects(v)).sum::<usize>()
        }
        _ => 1,
    }
}

fn bytes_for(value: u64) -> usize {
    match value {
        0..=0xff => 1,
        0x100..=0xffff => 2,
        0x1_0000..=0xffff_ffff => 4,
        _ => 8,
    }
}

fn put_sized(out: &mut Vec<u8>, value: u64, size: usize) {
    out.extend_from_slice(&value.to_be_bytes()[8 - size..]);
}

fn header(out: &mut Vec<u8>, kind: u8, length: usize) {
    if length < 15 {
        out.push(kind << 4 | length as u8);
    } else {
        out.push(kind << 4 | 0xf);
        put_int(out, length as i64);
    }
}

fn put_int(out: &mut Vec<u8>, value: i64) {
    let (marker, size) = match value {
        0..=0xff => (0x10, 1),
        0x100..=0xffff => (0x11, 2),
        0x1_0000..=0xffff_ffff => (0x12, 4),
        _ => (0x13, 8),
    };
    out.push(marker);
    put_sized(out, value as u64, size);
}

impl Writer {
    /// Append `value` and its children; returns its object index.
    fn add(&mut self, value: &Value) -> usize {
        let index = self.objects.len();
        self.objects.push(Vec::new());
        let mut out = Vec::new();
        match value {
            Value::Bool(b) => out.push(if *b { 0x09 } else { 0x08 }),
            Value::Int(n) => put_int(&mut out, *n),
            Value::Real(r) => {
                out.push(0x23);
                out.extend_from_slice(&r.to_be_bytes());
            }
            Value::Date(d) => {
                out.push(0x33);
                out.extend_from_slice(&d.to_be_bytes());
            }
            Value::String(s) => put_string(&mut out, s),
            Value::Data(bytes) => {
                header(&mut out, 0x4, bytes.len());
                out.extend_from_slice(bytes);
            }
            Value::Array(items) => {
                let references: Vec<usize> = items.iter().map(|item| self.add(item)).collect();
                header(&mut out, 0xa, items.len());
                self.put_references(&mut out, &references);
            }
            Value::Dict(entries) => {
                let keys: Vec<usize> =
                    entries.iter().map(|(k, _)| self.add(&Value::String(k.clone()))).collect();
                let values: Vec<usize> = entries.iter().map(|(_, v)| self.add(v)).collect();
                header(&mut out, 0xd, entries.len());
                self.put_references(&mut out, &keys);
                self.put_references(&mut out, &values);
            }
        }
        self.objects[index] = out;
        index
    }

    fn put_references(&self, out: &mut Vec<u8>, references: &[usize]) {
        for &reference in references {
            put_sized(out, reference as u64, self.reference_size);
        }
    }
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    if text.is_ascii() {
        header(out, 0x5, text.len());
        out.extend_from_slice(text.as_bytes());
    } else {
        let units: Vec<u16> = text.encode_utf16().collect();
        header(out, 0x6, units.len());
        for unit in units {
            out.extend_from_slice(&unit.to_be_bytes());
        }
    }
}
