//! The packets sent last, kept for the receiver's retransmit requests.

const SLOTS: usize = 512; // about 4 s of audio

pub struct Ring {
    slots: Vec<Option<(u16, Vec<u8>)>>,
}

impl Default for Ring {
    fn default() -> Self {
        Self { slots: vec![None; SLOTS] }
    }
}

impl Ring {
    pub fn store(&mut self, sequence: u16, packet: Vec<u8>) {
        self.slots[sequence as usize % SLOTS] = Some((sequence, packet));
    }

    pub fn get(&self, sequence: u16) -> Option<&[u8]> {
        match &self.slots[sequence as usize % SLOTS] {
            Some((stored, packet)) if *stored == sequence => Some(packet),
            _ => None,
        }
    }
}
