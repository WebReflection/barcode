//! Generic GF(2^b) fields for Aztec (ISO/IEC 24778).
//!
//! Aztec uses five field sizes: GF(16) for the mode message check words,
//! and GF(2^b) for data codewords with b = 6, 8, 10, 12 depending on the
//! layer count. All use α = 2 with the primitive polynomials from the spec.
//! Elements are `u16` (max 4095).

/// A GF(2^b) field, α = 2.
pub struct Gf {
    bits: u32,
    /// Primitive polynomial, e.g. 0x13 = x^4 + x + 1.
    poly: u16,
    /// α^i, indexed with i doubled (0..2*(size-2)) so mul needs no mod.
    exp: Vec<u16>,
    /// log_α of every nonzero element.
    log: Vec<u16>,
}

impl Gf {
    /// Build the field for `bits` with primitive polynomial `poly`
    /// (must satisfy 2^bits < poly < 2^(bits+1)).
    pub fn new(bits: u32, poly: u16) -> Gf {
        let size = 1usize << bits;
        let max = (1u32 << (bits + 1)) as u16;
        assert!(
            poly > size as u16 && poly < max,
            "poly {poly:#x} not of degree {bits}"
        );
        let order = size - 1; // cycle length of α, i.e. 2^b - 1
        let mut exp = vec![0u16; 2 * order];
        let mut log = vec![0u16; size];
        let mut x: u16 = 1;
        for i in 0..order {
            exp[i] = x;
            log[x as usize] = i as u16;
            x <<= 1;
            if x & (size as u16) != 0 {
                x ^= poly;
            }
        }
        // α must be primitive: full cycle before returning to 1.
        assert_eq!(x, 1, "alpha is not primitive for poly {poly:#x}");
        for i in order..2 * order {
            exp[i] = exp[i - order];
        }
        Gf {
            bits,
            poly,
            exp,
            log,
        }
    }

    pub fn bits(&self) -> u32 {
        self.bits
    }

    pub fn poly(&self) -> u16 {
        self.poly
    }

    /// Number of elements (2^b).
    pub fn size(&self) -> usize {
        1usize << self.bits
    }

    #[inline]
    pub fn mul(&self, a: u16, b: u16) -> u16 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    #[inline]
    pub fn inv(&self, a: u16) -> u16 {
        debug_assert!(a != 0);
        self.exp[((self.size() - 1 - self.log[a as usize] as usize) as u16)
            .rem_euclid(2 * (self.size() - 1) as u16) as usize]
    }

    /// α^e for any integer e (negative via the cycle of length 2^b−1).
    #[inline]
    pub fn pow(&self, e: i32) -> u16 {
        let order = (self.size() - 1) as i32;
        self.exp[e.rem_euclid(order) as usize]
    }
}

/// Aztec's five fields, keyed by codeword/mode bit width b.
pub fn field_for(bits: u32) -> Gf {
    match bits {
        4 => Gf::new(4, 0x13),     // x^4 + x + 1 (mode message)
        6 => Gf::new(6, 0x43),     // x^6 + x + 1
        8 => Gf::new(8, 0x12d),    // x^8 + x^5 + x^3 + x^2 + 1 (same as DM)
        10 => Gf::new(10, 0x409),  // x^10 + x^3 + 1
        12 => Gf::new(12, 0x1069), // x^12 + x^6 + x^5 + x^3 + 1
        _ => panic!("no Aztec field for {bits} bits"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// α must cycle with full order 2^b − 1 for every Aztec polynomial.
    #[test]
    fn alpha_is_primitive() {
        for bits in [4u32, 6, 8, 10, 12] {
            let gf = field_for(bits);
            assert_eq!(gf.pow((1usize << bits) as i32 - 1), 1);
            // and not earlier
            let mut x = 1u16;
            for _ in 1..(1usize << bits) - 1 {
                x = gf.mul(x, 2);
                assert_ne!(x, 1, "alpha order too small for {bits} bits");
            }
        }
    }

    /// Published/computed anchors: α^b = x^b mod poly.
    #[test]
    fn alpha_power_b_anchors() {
        assert_eq!(field_for(4).pow(4), 0x3); // x^4 mod (x^4+x+1) = x+1
        assert_eq!(field_for(6).pow(6), 0x3); // x^6 mod (x^6+x+1) = x+1
        assert_eq!(field_for(8).pow(8), 45); // x^8 mod 0x12d = 0x2d (DM anchor)
        assert_eq!(field_for(10).pow(10), 0x9); // x^10 mod (x^10+x^3+1) = x^3+1
        assert_eq!(field_for(12).pow(12), 0x69); // x^12 mod 0x1069
    }

    #[test]
    fn mul_inv_pow_consistency() {
        for bits in [4u32, 6, 8, 10, 12] {
            let gf = field_for(bits);
            let order = (1usize << bits) - 1;
            for i in [0i32, 1, 7, 100, order as i32 - 1] {
                let a = gf.pow(i);
                assert_eq!(gf.mul(a, gf.inv(a)), 1);
                assert_eq!(gf.mul(a, gf.pow(-i)), 1);
            }
            // α^i · α^j = α^(i+j)
            let (i, j) = (123i32, 456i32);
            assert_eq!(gf.mul(gf.pow(i), gf.pow(j)), gf.pow(i + j));
            assert_eq!(gf.mul(0, gf.pow(3)), 0);
        }
    }
}
