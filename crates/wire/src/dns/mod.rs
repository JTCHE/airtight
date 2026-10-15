//! The mDNS subset that finds AirPlay receivers: one PTR query, and the
//! PTR, SRV, TXT and A records that come back.

use std::net::Ipv4Addr;

pub const AIRPLAY: &str = "_airplay._tcp.local";

const PTR: u16 = 12;
const TXT: u16 = 16;
const SRV: u16 = 33;
const A: u16 = 1;
/// Class IN with the "unicast response" bit: replies come straight back
/// to our socket, which multicast group joins on busy hosts can miss.
const IN_UNICAST: u16 = 0x8001;

fn put_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

pub fn query(service: &str) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    put_name(&mut out, service);
    out.extend_from_slice(&PTR.to_be_bytes());
    out.extend_from_slice(&IN_UNICAST.to_be_bytes());
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Ptr { name: String, target: String },
    Srv { name: String, target: String, port: u16 },
    Txt { name: String, entries: Vec<String> },
    A { name: String, address: Ipv4Addr },
}

/// A name at `at`, following compression pointers. Returns the name and
/// where the bytes after it start.
fn read_name(packet: &[u8], mut at: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    for _ in 0..64 {
        let length = *packet.get(at)? as usize;
        if length == 0 {
            return Some((labels.join("."), end.unwrap_or(at + 1)));
        }
        if length & 0xc0 == 0xc0 {
            let pointer = (length & 0x3f) << 8 | *packet.get(at + 1)? as usize;
            end.get_or_insert(at + 2);
            at = pointer;
            continue;
        }
        let label = packet.get(at + 1..at + 1 + length)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        at += 1 + length;
    }
    None
}

fn word(packet: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*packet.get(at)?, *packet.get(at + 1)?]))
}

/// Every answer, authority and additional record we understand.
pub fn parse(packet: &[u8]) -> Option<Vec<Record>> {
    let questions = word(packet, 4)?;
    let records = word(packet, 6)? as usize + word(packet, 8)? as usize + word(packet, 10)? as usize;
    let mut at = 12;
    for _ in 0..questions {
        at = read_name(packet, at)?.1 + 4;
    }
    let mut out = Vec::new();
    for _ in 0..records {
        let (name, after) = read_name(packet, at)?;
        let kind = word(packet, after)?;
        let length = word(packet, after + 8)? as usize;
        let data = after + 10;
        packet.get(data..data + length)?;
        if let Some(record) = record(packet, name, kind, data, length) {
            out.push(record);
        }
        at = data + length;
    }
    Some(out)
}

fn record(packet: &[u8], name: String, kind: u16, data: usize, length: usize) -> Option<Record> {
    match kind {
        PTR => Some(Record::Ptr { name, target: read_name(packet, data)?.0 }),
        SRV => {
            let port = word(packet, data + 4)?;
            Some(Record::Srv { name, target: read_name(packet, data + 6)?.0, port })
        }
        A if length == 4 => {
            let b = &packet[data..data + 4];
            Some(Record::A { name, address: Ipv4Addr::new(b[0], b[1], b[2], b[3]) })
        }
        TXT => {
            let mut entries = Vec::new();
            let mut at = data;
            while at < data + length {
                let size = packet[at] as usize;
                let text = packet.get(at + 1..at + 1 + size)?;
                entries.push(String::from_utf8_lossy(text).into_owned());
                at += 1 + size;
            }
            Some(Record::Txt { name, entries })
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    /// The instance label, which is the name the user gave the receiver.
    pub name: String,
    pub address: Ipv4Addr,
    pub port: u16,
    pub model: String,
}

/// Join the records of all replies into services of type `service`.
pub fn services(records: &[Record], service: &str) -> Vec<Service> {
    let suffix = format!(".{service}");
    let mut out: Vec<Service> = Vec::new();
    for record in records {
        let Record::Srv { name, target, port } = record else { continue };
        let Some(label) = name.strip_suffix(&suffix) else { continue };
        let address = records.iter().find_map(|r| match r {
            Record::A { name, address } if name.eq_ignore_ascii_case(target) => Some(*address),
            _ => None,
        });
        let model = records
            .iter()
            .find_map(|r| match r {
                Record::Txt { name: n, entries } if n == name => {
                    entries.iter().find_map(|e| e.strip_prefix("model=").map(String::from))
                }
                _ => None,
            })
            .unwrap_or_default();
        if let Some(address) = address {
            if !out.iter().any(|s| s.name == label) {
                out.push(Service { name: label.to_string(), address, port: *port, model });
            }
        }
    }
    out
}
