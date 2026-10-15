//! Montgomery arithmetic modulo an odd number.

use super::Big;

pub struct Montgomery {
    modulus: Vec<u64>,
    inverse: u64, // -modulus^-1 mod 2^64
    r_squared: Vec<u64>,
}

impl Montgomery {
    pub fn new(modulus: &Big) -> Self {
        let n = modulus.limbs().to_vec();
        assert!(n[0] & 1 == 1, "the modulus must be odd");
        // Newton iteration doubles the correct low bits each step.
        let mut inverse = 1u64;
        for _ in 0..6 {
            inverse = inverse.wrapping_mul(2u64.wrapping_sub(n[0].wrapping_mul(inverse)));
        }
        let mut field = Self { modulus: n, inverse: inverse.wrapping_neg(), r_squared: Vec::new() };
        field.r_squared = field.power_of_two(field.modulus.len() * 128);
        field
    }

    /// 2^exponent mod n, by doubling.
    fn power_of_two(&self, exponent: usize) -> Vec<u64> {
        let length = self.modulus.len();
        let mut value = vec![0u64; length];
        value[0] = 1;
        for _ in 0..exponent {
            let mut carry = 0u64;
            for limb in value.iter_mut() {
                let next = *limb >> 63;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            if carry == 1 || !less_than(&value, &self.modulus) {
                subtract_in_place(&mut value, &self.modulus);
            }
        }
        value
    }

    /// a * b * R^-1 mod n, for a and b below n.
    fn multiply(&self, a: &[u64], b: &[u64]) -> Vec<u64> {
        let n = &self.modulus;
        let length = n.len();
        let mut t = vec![0u64; length + 2];
        for &ai in a.iter() {
            let mut carry = 0u128;
            for j in 0..length {
                let sum = u128::from(t[j]) + u128::from(ai) * u128::from(b[j]) + carry;
                t[j] = sum as u64;
                carry = sum >> 64;
            }
            let sum = u128::from(t[length]) + carry;
            t[length] = sum as u64;
            t[length + 1] = (sum >> 64) as u64;
            let m = t[0].wrapping_mul(self.inverse);
            let mut carry = (u128::from(t[0]) + u128::from(m) * u128::from(n[0])) >> 64;
            for j in 1..length {
                let sum = u128::from(t[j]) + u128::from(m) * u128::from(n[j]) + carry;
                t[j - 1] = sum as u64;
                carry = sum >> 64;
            }
            let sum = u128::from(t[length]) + carry;
            t[length - 1] = sum as u64;
            t[length] = t[length + 1] + (sum >> 64) as u64;
            t[length + 1] = 0;
        }
        let mut result = t[..length].to_vec();
        if t[length] != 0 || !less_than(&result, n) {
            subtract_in_place(&mut result, n);
        }
        result
    }

    fn padded(&self, value: &Big) -> Vec<u64> {
        let mut limbs = value.limbs().to_vec();
        limbs.resize(self.modulus.len(), 0);
        limbs
    }

    /// value mod n, for any value: Horner over chunks of the modulus size.
    pub fn reduce(&self, value: &Big) -> Big {
        let length = self.modulus.len();
        let mut acc = vec![0u64; length];
        for chunk in value.limbs().chunks(length).rev() {
            let mut piece = chunk.to_vec();
            piece.resize(length, 0);
            // ponytail: repeated subtraction; quick only while the modulus fills its top limb
            while !less_than(&piece, &self.modulus) {
                subtract_in_place(&mut piece, &self.modulus);
            }
            // multiply() divides by R, so a product with R^2 shifts acc up by R.
            acc = self.multiply(&acc, &self.r_squared);
            add_mod(&mut acc, &piece, &self.modulus);
        }
        Big(acc).trimmed()
    }

    /// base^exponent mod n.
    pub fn power(&self, base: &Big, exponent: &Big) -> Big {
        let base = self.multiply(&self.padded(&self.reduce(base)), &self.r_squared);
        let mut result = self.multiply(&self.padded(&Big::from_u64(1)), &self.r_squared);
        for i in (0..exponent.bits()).rev() {
            result = self.multiply(&result, &result);
            if exponent.bit(i) {
                result = self.multiply(&result, &base);
            }
        }
        let one = self.padded(&Big::from_u64(1));
        Big(self.multiply(&result, &one)).trimmed()
    }

    /// (a * b) mod n.
    pub fn multiply_mod(&self, a: &Big, b: &Big) -> Big {
        let a = self.multiply(&self.padded(&self.reduce(a)), &self.r_squared);
        Big(self.multiply(&a, &self.padded(&self.reduce(b)))).trimmed()
    }

    pub fn modulus(&self) -> Big {
        Big(self.modulus.clone()).trimmed()
    }
}

fn less_than(a: &[u64], b: &[u64]) -> bool {
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    false
}

fn subtract_in_place(a: &mut [u64], b: &[u64]) {
    let mut borrow = 0u64;
    for i in 0..a.len() {
        let (step, under1) = a[i].overflowing_sub(b[i]);
        let (step, under2) = step.overflowing_sub(borrow);
        a[i] = step;
        borrow = u64::from(under1 || under2);
    }
}

fn add_mod(a: &mut [u64], b: &[u64], n: &[u64]) {
    let mut carry = 0u128;
    for i in 0..a.len() {
        let sum = u128::from(a[i]) + u128::from(b[i]) + carry;
        a[i] = sum as u64;
        carry = sum >> 64;
    }
    if carry != 0 || !less_than(a, n) {
        subtract_in_place(a, n);
    }
}
