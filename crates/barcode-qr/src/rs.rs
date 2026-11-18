//! Reed-Solomon coding over GF(2^8) (ISO/IEC 18004 annex / appendix I).
//!
//! The field is GF(2^8) with primitive polynomial x^8 + x^4 + x^3 + x^2 + 1
//! (0x11D) and generator α = 2. A QR block is a shortened RS code: `k` data
//! codewords followed by `nn` parity codewords, where the codeword polynomial
//! (first byte = highest-degree coefficient) is divisible by the generator
//! g(x) = ∏_{j=0}^{nn-1} (x − α^j). Equivalently every syndrome
//! S_j = C(α^j), j = 0..nn−1, vanishes on a valid codeword — that is the
//! property the encoder establishes and the corrector exploits.
//!
//! Correction capacity: nn parity symbols correct up to t = ⌊nn/2⌋ erroneous
//! codewords per block.

/// Tables for GF(2^8); α = 2, primitive polynomial 0x11D.
pub struct Gf {
    /// α^i, indexed with i doubled to 0..509 so multiplication needs no mod.
    exp: [u8; 512],
    /// log_α of every nonzero element.
    log: [u8; 256],
}

impl Gf {
    pub fn new() -> Gf {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        for i in 0..255u16 {
            exp[i as usize] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= 0x11d;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Gf { exp, log }
    }

    #[inline]
    pub fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    #[inline]
    pub fn inv(&self, a: u8) -> u8 {
        debug_assert!(a != 0);
        self.exp[(255 - self.log[a as usize] as usize) as usize]
    }

    /// α^e for any integer e (negative via the cycle of length 255).
    #[inline]
    pub fn pow(&self, e: i32) -> u8 {
        self.exp[e.rem_euclid(255) as usize]
    }
}

impl Default for Gf {
    fn default() -> Self {
        Self::new()
    }
}

/// Generator polynomial g(x) = ∏_{j<nn} (x + α^j), coefficients ascending.
fn generator(gf: &Gf, nn: usize) -> Vec<u8> {
    let mut g = vec![1u8];
    for j in 0..nn {
        let a = gf.exp[j];
        let mut next = vec![0u8; g.len() + 1];
        for (i, &c) in g.iter().enumerate() {
            next[i + 1] ^= c; // · x
            next[i] ^= gf.mul(c, a); // · α^j
        }
        g = next;
    }
    g
}

/// Systematic encode: the `nn` parity codewords appended to `data`
/// (data[0] is the first, most significant codeword of the block).
pub fn rs_encode(gf: &Gf, data: &[u8], nn: usize) -> Vec<u8> {
    // Descending generator: g[0] = 1 (x^nn), g[i] coefficient of x^(nn-i).
    let g_desc: Vec<u8> = generator(gf, nn).into_iter().rev().collect();
    let mut rem = vec![0u8; nn];
    for &d in data {
        let factor = d ^ rem[0];
        rem.copy_within(1.., 0);
        *rem.last_mut().unwrap() = 0;
        for (i, &gc) in g_desc.iter().skip(1).enumerate() {
            rem[i] ^= gf.mul(factor, gc);
        }
    }
    rem
}

/// Syndromes S_j = R(α^j) for j = 0..nn−1 (Horner over the codeword,
/// first byte = highest degree).
fn syndromes(gf: &Gf, codewords: &[u8], nn: usize) -> Vec<u8> {
    let mut s = Vec::with_capacity(nn);
    for j in 0..nn {
        let a = gf.exp[j];
        let mut acc = 0u8;
        for &r in codewords {
            acc = gf.mul(acc, a) ^ r;
        }
        s.push(acc);
    }
    s
}

/// Berlekamp-Massey: the minimal error locator Λ(x) (ascending, Λ(0) = 1)
/// for the syndrome sequence S_0..S_2t−1, or None if it exceeds degree t.
fn error_locator(gf: &Gf, s: &[u8], t: usize) -> Option<Vec<u8>> {
    let mut c = vec![1u8]; // current locator, ascending
    let mut b = vec![1u8]; // locator from the last length change
    let mut b0 = 1u8; // discrepancy that produced `b`
    let mut ll = 0usize; // current LFSR length (= deg Λ so far)
    let mut m = 1usize; // gap since the last length change
    for n in 0..s.len() {
        let mut d = s[n];
        for i in 1..=ll.min(c.len() - 1) {
            d ^= gf.mul(c[i], s[n - i]);
        }
        if d == 0 {
            m += 1;
            continue;
        }
        // Snapshot before the update: on a length change, B becomes old C.
        let length_change = 2 * ll <= n;
        let old_c = if length_change { Some(c.clone()) } else { None };
        let coef = gf.mul(d, gf.inv(b0));
        if c.len() < b.len() + m {
            c.resize(b.len() + m, 0);
        }
        for (i, &bc) in b.iter().enumerate() {
            c[i + m] ^= gf.mul(coef, bc);
        }
        if let Some(old_c) = old_c {
            b = old_c;
            b0 = d;
            ll = n + 1 - ll;
            m = 1;
        } else {
            m += 1;
        }
    }
    // Degree of Λ = highest nonzero index.
    let deg = c.iter().rposition(|&x| x != 0)?;
    (deg <= t).then_some(c)
}

/// Correct one block in place. `codewords` = data ++ parity. Returns the
/// corrected data codewords, or None if uncorrectable.
pub fn rs_correct(gf: &Gf, codewords: &mut [u8], nn: usize) -> Option<()> {
    let n = codewords.len();
    let t = nn / 2;
    let s = syndromes(gf, codewords, nn);
    if s.iter().all(|&x| x == 0) {
        return Some(()); // already valid
    }
    let lambda = error_locator(gf, &s, t)?;

    // Ω(x) = [S(x)·Λ(x)] mod x^nn, ascending.
    let mut omega = vec![0u8; nn];
    for (i, &lc) in lambda.iter().enumerate() {
        if lc == 0 {
            continue;
        }
        for (j, &sc) in s.iter().enumerate() {
            if i + j >= nn {
                break;
            }
            omega[i + j] ^= gf.mul(lc, sc);
        }
    }

    // Chien search over codeword positions: an error at index i ⟺
    // Λ(α^-(n-1-i)) = 0. Forney magnitudes with X = α^(n-1-i):
    // e_i = X·Ω(X^-1) / Λ'(X^-1), where Λ' sums the odd-degree terms
    // (in characteristic 2 the even ones vanish).
    let mut errors = 0usize;
    for i in 0..n {
        let x_inv = gf.pow(-((n - 1 - i) as i32));
        let mut lam = 0u8;
        let mut zp = 1u8;
        for &lc in &lambda {
            lam ^= gf.mul(lc, zp);
            zp = gf.mul(zp, x_inv);
        }
        if lam != 0 {
            continue; // no error at this position
        }
        // Λ'(x_inv) = Σ_{k odd} Λ_k·x_inv^(k-1).
        let mut d1 = 0u8;
        let mut zp = 1u8;
        for k in (1..lambda.len()).step_by(2) {
            d1 ^= gf.mul(lambda[k], zp);
            zp = gf.mul(zp, gf.mul(x_inv, x_inv));
        }
        if d1 == 0 {
            return None; // derivative vanished: uncorrectable pattern
        }
        let num = gf.mul(gf.pow((n - 1 - i) as i32), eval(&omega, gf, x_inv));
        codewords[i] ^= gf.mul(num, gf.inv(d1));
        errors += 1;
    }
    if errors == 0 || errors > t {
        return None;
    }
    // Decisive check: the corrected word must be a valid codeword.
    syndromes(gf, codewords, nn).iter().all(|&x| x == 0).then_some(())
}

/// Solve `A·x = b` over GF(2^8). `a` is a `count`×`count` row-major
/// matrix, `b` has `count` entries; both are consumed. Returns None when
/// singular (a wrong erasure set can collapse the Vandermonde).
fn solve_linear(gf: &Gf, a: &mut [u8], b: &mut [u8], count: usize) -> Option<Vec<u8>> {
    for col in 0..count {
        let piv = (col..count).find(|&r| a[r * count + col] != 0)?;
        if piv != col {
            for k in 0..count {
                a.swap(col * count + k, piv * count + k);
            }
            b.swap(col, piv);
        }
        let inv = gf.inv(a[col * count + col]);
        for k in col..count {
            a[col * count + k] = gf.mul(a[col * count + k], inv);
        }
        b[col] = gf.mul(b[col], inv);
        for r in 0..count {
            if r == col {
                continue;
            }
            let f = a[r * count + col];
            if f == 0 {
                continue;
            }
            for k in col..count {
                a[r * count + k] ^= gf.mul(f, a[col * count + k]);
            }
            b[r] ^= gf.mul(f, b[col]);
        }
    }
    Some(b.to_vec())
}

/// Correct a block with known erasure positions plus unknown errors — the
/// general RS case (ISO/IEC 18004).
///
/// Syndromes here start at α^0 (unlike DataMatrix/Aztec's α^1), so the
/// error magnitudes are already a standard power sum: the erasure
/// locator Λ(x) = Π(1 − X_i x) gives modified syndromes
/// T_k = Σ_m Λ_m S_{k−m} (k = e..nn−1), Berlekamp–Massey on `T` gives the
/// unknown-error locator σ, and one Vandermonde solve over the combined
/// positions yields the magnitudes directly (no folded X factor).
///
/// Capacity `e + 2t ≤ nn`, with at least two syndromes left as checks
/// (`nn − (e + t) ≥ 2`) so a wrong grid/labeling cannot "correct" itself
/// into a plausible codeword.
pub fn rs_correct_erasures(
    gf: &Gf,
    codewords: &mut [u8],
    nn: usize,
    erasures: &[usize],
) -> Option<()> {
    let n = codewords.len();
    if nn == 0 || nn > n || n > 255 {
        return None;
    }
    let mut pos: Vec<usize> = erasures.to_vec();
    pos.sort_unstable();
    pos.dedup();
    if pos.iter().any(|&p| p >= n) || pos.len() > nn {
        return None;
    }
    if pos.is_empty() {
        return rs_correct(gf, codewords, nn);
    }
    let s = syndromes(gf, codewords, nn);
    if s.iter().all(|&x| x == 0) {
        return Some(()); // clean: a stale erasure list is harmless
    }
    let e = pos.len();
    // Erasure locator, ascending (λ_0 = 1); roots at X_i^{-1}.
    let mut lambda = vec![1u8];
    for &p in &pos {
        let x = gf.pow((n - 1 - p) as i32);
        let mut next = vec![0u8; lambda.len() + 1];
        for (i, &c) in lambda.iter().enumerate() {
            next[i] ^= c;
            next[i + 1] ^= gf.mul(c, x);
        }
        lambda = next;
    }
    // Modified syndromes: T_k = Σ_m λ_m S_{k−m}, k = e..nn−1.
    let mut tseq = Vec::with_capacity(nn - e);
    for k in e..nn {
        let mut t = 0u8;
        for (m, &c) in lambda.iter().enumerate() {
            if k >= m {
                t ^= gf.mul(c, s[k - m]);
            }
        }
        tseq.push(t);
    }
    let sigma = error_locator(gf, &tseq, (nn - e) / 2)?;
    let t = sigma.iter().rposition(|&x| x != 0).unwrap_or(0);
    if e + 2 * t > nn || nn - (e + t) < 2 {
        return None;
    }
    // Chien over all positions: an error at index i ⇔ σ(α^{−(n−1−i)}) = 0.
    let mut degs: Vec<usize> = Vec::new();
    for d in 0..n {
        let x_inv = gf.pow(-(d as i32));
        let mut acc = 0u8;
        let mut zp = 1u8;
        for &c in &sigma {
            acc ^= gf.mul(c, zp);
            zp = gf.mul(zp, x_inv);
        }
        if acc == 0 {
            degs.push(d);
        }
    }
    if degs.len() != t {
        return None;
    }
    for &p in &pos {
        let d = n - 1 - p;
        if degs.contains(&d) {
            return None;
        }
        degs.push(d);
    }
    let count = degs.len();
    if count != e + t {
        return None;
    }
    // S_j = Σ e_i X_i^j (j = 0..count−1) — QR magnitudes are unfolded.
    let xs: Vec<u8> = degs.iter().map(|&d| gf.pow(d as i32)).collect();
    let mut a = vec![0u8; count * count];
    for (i, &x) in xs.iter().enumerate() {
        let mut p = 1u8;
        for j in 0..count {
            a[j * count + i] = p;
            p = gf.mul(p, x);
        }
    }
    let mut b = s[..count].to_vec();
    let vals = solve_linear(gf, &mut a, &mut b, count)?;
    for (i, &d) in degs.iter().enumerate() {
        codewords[n - 1 - d] ^= vals[i];
    }
    syndromes(gf, codewords, nn).iter().all(|&x| x == 0).then_some(())
}

/// Evaluate a polynomial with ascending coefficients at the field element `z`.
fn eval(poly: &[u8], gf: &Gf, z: u8) -> u8 {
    let mut acc = 0u8;
    for &c in poly.iter().rev() {
        acc = gf.mul(acc, z) ^ c;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handful of published log/antilog anchors (poly 0x11D, α = 2):
    /// α^8 = 29, α^25 = 3, α^42 = 181, α^50 = 5, α^100 = 17, α^255 = 1.
    #[test]
    fn gf_table_matches_published_values() {
        let gf = Gf::new();
        assert_eq!(gf.pow(8), 29);
        assert_eq!(gf.pow(25), 3);
        assert_eq!(gf.pow(42), 181);
        assert_eq!(gf.pow(50), 5);
        assert_eq!(gf.pow(100), 17);
        assert_eq!(gf.pow(255), 1);
        assert_eq!(gf.pow(0), 1);
        // multiplication via logs: 29 · 5 = α^(8+50) = α^58 = 105
        assert_eq!(gf.mul(29, 5), 105);
        assert_eq!(gf.mul(0, 142), 0);
        // 142 = α^254, so its inverse is α^(255-254) = α^1 = 2
        assert_eq!(gf.inv(142), 2);
    }

    #[test]
    fn encode_yields_valid_codeword() {
        let gf = Gf::new();
        let data: Vec<u8> = (0..19u8).map(|i| i.wrapping_mul(37).wrapping_add(11)).collect();
        let mut block = data.clone();
        let parity = rs_encode(&gf, &data, 10);
        block.extend_from_slice(&parity);
        assert!(syndromes(&gf, &block, 10).iter().all(|&x| x == 0));
    }

    #[test]
    fn corrects_up_to_t_errors_and_rejects_beyond() {
        let gf = Gf::new();
        let data: Vec<u8> = (0..16u8).map(|i| i.wrapping_mul(91).wrapping_add(7)).collect();
        let mut block = data.clone();
        block.extend_from_slice(&rs_encode(&gf, &data, 10));
        let clean = block.clone();

        // t = 5 for nn = 10: corrupt 5 codewords (one byte each).
        for &i in &[0usize, 3, 7, 20, 25] {
            block[i] ^= 0xa5;
        }
        assert!(rs_correct(&gf, &mut block, 10).is_some());
        assert_eq!(block, clean, "corrected back to the original");

        // 6 errors exceed capacity: must fail cleanly.
        let mut broken = clean.clone();
        for &i in &[0usize, 3, 7, 20, 25, 24] {
            broken[i] ^= 0x5a;
        }
        assert_eq!(rs_correct(&gf, &mut broken, 10), None);
    }

    #[test]
    fn corrects_whole_and_erased_codewords() {
        let gf = Gf::new();
        let data: Vec<u8> = (0..13u8).map(|i| i.wrapping_mul(13)).collect();
        let mut block = data.clone();
        block.extend_from_slice(&rs_encode(&gf, &data, 22));
        let clean = block.clone();
        // Wipe three full codewords to zero and one to 0xff.
        for i in [0usize, 5, 30] {
            block[i] = 0;
        }
        block[12] = 0xff;
        assert!(rs_correct(&gf, &mut block, 22).is_some());
        assert_eq!(block, clean);
    }
    #[test]
    fn erasures_double_the_capacity() {
        let gf = Gf::new();
        let data: Vec<u8> = (0..16u8).map(|i| i.wrapping_mul(97).wrapping_add(5)).collect();
        let mut block = data.clone();
        block.extend_from_slice(&rs_encode(&gf, &data, 10));
        let clean = block.clone();

        // 8 known erasures (nn - e = 2 checks); error-only capacity is 5.
        let erased = [0usize, 2, 4, 6, 8, 10, 12, 14];
        let mut broken = clean.clone();
        for &i in &erased {
            broken[i] ^= 0xa5;
        }
        assert!(rs_correct(&gf, &mut broken.clone(), 10).is_none());
        assert_eq!(rs_correct_erasures(&gf, &mut broken, 10, &erased), Some(()));
        assert_eq!(broken, clean);

        // Mixed: 4 erasures + 3 unknown errors = 4 + 2·3 = 10.
        let mixed = [1usize, 3, 5, 7];
        let mut broken = clean.clone();
        for &i in &mixed {
            broken[i] ^= 0x5a;
        }
        for &i in &[0usize, 15, 20] {
            broken[i] ^= 0x3c;
        }
        assert!(rs_correct_erasures(&gf, &mut broken, 10, &mixed).is_some());
        assert_eq!(broken, clean);

        // e + 2t = 6 + 2·3 = 12 > 10 must reject.
        let over = [0usize, 2, 4, 6, 8, 10];
        let mut broken = clean.clone();
        for &i in &over {
            broken[i] ^= 0x11;
        }
        for &i in &[1usize, 3, 5] {
            broken[i] ^= 0x22;
        }
        assert_eq!(rs_correct_erasures(&gf, &mut broken, 10, &over), None);
    }

}
