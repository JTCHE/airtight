//! Unsigned big integers, just enough for SRP: add, multiply, and
//! modular power through Montgomery multiplication.

mod montgomery;

pub use montgomery::Montgomery;

use std::cmp::Ordering;

/// Little-endian 64-bit limbs, no high zero limbs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big(Vec<u64>);

impl Big {
    pub fn from_be(bytes: &[u8]) -> Self {
        let mut limbs = Vec::with_capacity(bytes.len() / 8 + 1);
        for chunk in bytes.rchunks(8) {
            let mut word = [0u8; 8];
            word[8 - chunk.len()..].copy_from_slice(chunk);
            limbs.push(u64::from_be_bytes(word));
        }
        Self(limbs).trimmed()
    }

    pub fn from_u64(value: u64) -> Self {
        Self(vec![value]).trimmed()
    }

    fn trimmed(mut self) -> Self {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
        self
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// Big-endian bytes, left-padded with zeros to `length`.
    pub fn to_be_padded(&self, length: usize) -> Vec<u8> {
        let mut out = vec![0u8; length.max(self.0.len() * 8)];
        let end = out.len();
        for (i, limb) in self.0.iter().enumerate() {
            out[end - 8 * (i + 1)..end - 8 * i].copy_from_slice(&limb.to_be_bytes());
        }
        let extra = out.len() - length.max(self.minimal_length());
        out.drain(..extra);
        out
    }

    /// Big-endian bytes with no leading zeros.
    pub fn to_be(&self) -> Vec<u8> {
        self.to_be_padded(0)
    }

    fn minimal_length(&self) -> usize {
        match self.0.last() {
            None => 0,
            Some(top) => self.0.len() * 8 - top.leading_zeros() as usize / 8,
        }
    }

    pub fn bits(&self) -> usize {
        match self.0.last() {
            None => 0,
            Some(top) => self.0.len() * 64 - top.leading_zeros() as usize,
        }
    }

    pub fn bit(&self, index: usize) -> bool {
        self.0.get(index / 64).is_some_and(|limb| limb >> (index % 64) & 1 == 1)
    }

    pub fn add(&self, other: &Big) -> Big {
        let length = self.0.len().max(other.0.len());
        let mut out = Vec::with_capacity(length + 1);
        let mut carry = 0u128;
        for i in 0..length {
            let sum = u128::from(*self.0.get(i).unwrap_or(&0))
                + u128::from(*other.0.get(i).unwrap_or(&0))
                + carry;
            out.push(sum as u64);
            carry = sum >> 64;
        }
        out.push(carry as u64);
        Big(out).trimmed()
    }

    /// `self - other`; the caller makes sure that `self >= other`.
    pub fn sub(&self, other: &Big) -> Big {
        let mut out = Vec::with_capacity(self.0.len());
        let mut borrow = 0u64;
        for i in 0..self.0.len() {
            let (step, under1) = self.0[i].overflowing_sub(*other.0.get(i).unwrap_or(&0));
            let (step, under2) = step.overflowing_sub(borrow);
            out.push(step);
            borrow = u64::from(under1 || under2);
        }
        Big(out).trimmed()
    }

    pub fn mul(&self, other: &Big) -> Big {
        let mut out = vec![0u64; self.0.len() + other.0.len() + 1];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &b) in other.0.iter().enumerate() {
                let t = u128::from(a) * u128::from(b) + u128::from(out[i + j]) + carry;
                out[i + j] = t as u64;
                carry = t >> 64;
            }
            out[i + other.0.len()] = carry as u64;
        }
        Big(out).trimmed()
    }

    pub fn limbs(&self) -> &[u64] {
        &self.0
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.len().cmp(&other.0.len()).then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}
