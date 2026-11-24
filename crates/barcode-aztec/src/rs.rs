//! Reed-Solomon coding over GF(2^b) for Aztec (ISO/IEC 24778).
//!
//! Same convention as the DM field (roots start at α^1, α = 2), but the
//! element width varies: check words are GF(2^b) codewords where b is the
//! codeword size for the layer count (6/8/10/12 data bits; the mode message
//! uses 4-bit codewords over GF(16)). The generator is
//! g(x) = ∏_{j=1}^{nn} (x − α^j) and a valid codeword is the message
//! (first element = highest degree) followed by the `nn` check elements.
//!
//! Correction capacity: nn check elements correct up to t = ⌊nn/2⌋.

use crate::gf::Gf;

/// Generator polynomial g(x) = ∏_{j=1..nn} (x + α^j), coefficients ascending.
fn generator(gf: &Gf, nn: usize) -> Vec<u16> {
    let mut g = vec![1u16];
    for j in 1..=nn {
        let a = gf.pow(j as i32);
        let mut next = vec![0u16; g.len() + 1];
        for (i, &c) in g.iter().enumerate() {
            next[i + 1] ^= c; // · x
            next[i] ^= gf.mul(c, a); // · α^j
        }
        g = next;
    }
    g
}

/// Systematic encode: the `nn` check elements appended to `data`
/// (data[0] is the first, most significant element of the block).
pub fn rs_encode(gf: &Gf, data: &[u16], nn: usize) -> Vec<u16> {
    // Descending generator: g[0] = 1 (x^nn), g[i] coefficient of x^(nn-i).
    let g_desc: Vec<u16> = generator(gf, nn).into_iter().rev().collect();
    let mut rem = vec![0u16; nn];
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

/// Syndromes S_j = R(α^{j+1}) for j = 0..nn−1 (Horner over the codeword,
/// first element = highest degree).
fn syndromes(gf: &Gf, codewords: &[u16], nn: usize) -> Vec<u16> {
    let mut s = Vec::with_capacity(nn);
    for j in 0..nn {
        let a = gf.pow(j as i32 + 1);
        let mut acc = 0u16;
        for &r in codewords {
            acc = gf.mul(acc, a) ^ r;
        }
        s.push(acc);
    }
    s
}

/// Berlekamp-Massey: the minimal error locator Λ(x) (ascending, Λ(0) = 1)
/// for the syndrome sequence S_0..S_2t−1, or None if it exceeds degree t.
fn error_locator(gf: &Gf, s: &[u16], t: usize) -> Option<Vec<u16>> {
    let mut c = vec![1u16]; // current locator, ascending
    let mut b = vec![1u16]; // locator from the last length change
    let mut b0 = 1u16; // discrepancy that produced `b`
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

/// Correct one block in place. `codewords` = data ++ check. Returns the
/// corrected data elements, or None if uncorrectable.
///
/// Error positions enter the syndromes as S_j = Σ e_i·X_i^{j+1} with
/// X_i = α^{n−1−i}; folding the constant α-factor into the magnitudes
/// (e'_i = e_i·X_i) gives a standard power sum, so BM and Chien match and
/// Forney's magnitude loses its leading X factor.
pub fn rs_correct(gf: &Gf, codewords: &mut [u16], nn: usize) -> Option<()> {
    let n = codewords.len();
    let t = nn / 2;
    let s = syndromes(gf, codewords, nn);
    if s.iter().all(|&x| x == 0) {
        return Some(()); // already valid
    }
    let lambda = error_locator(gf, &s, t)?;

    // Ω(x) = [S(x)·Λ(x)] mod x^nn, ascending.
    let mut omega = vec![0u16; nn];
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
    // Λ(α^-(n-1-i)) = 0. Forney magnitude with the folded X factor:
    // e_i = Ω(X^-1) / Λ'(X^-1), Λ' summing the odd-degree terms.
    let mut errors = 0usize;
    for i in 0..n {
        let x_inv = gf.pow(-((n - 1 - i) as i32));
        let mut lam = 0u16;
        let mut zp = 1u16;
        for &lc in &lambda {
            lam ^= gf.mul(lc, zp);
            zp = gf.mul(zp, x_inv);
        }
        if lam != 0 {
            continue; // no error at this position
        }
        // Λ'(x_inv) = Σ_{k odd} Λ_k·x_inv^(k-1).
        let mut d1 = 0u16;
        let mut zp = 1u16;
        for k in (1..lambda.len()).step_by(2) {
            d1 ^= gf.mul(lambda[k], zp);
            zp = gf.mul(zp, gf.mul(x_inv, x_inv));
        }
        if d1 == 0 {
            return None; // derivative vanished: uncorrectable pattern
        }
        codewords[i] ^= gf.mul(eval(&omega, gf, x_inv), gf.inv(d1));
        errors += 1;
    }
    if errors == 0 || errors > t {
        return None;
    }
    // Decisive check: the corrected word must be a valid codeword.
    syndromes(gf, codewords, nn)
        .iter()
        .all(|&x| x == 0)
        .then_some(())
}

/// Solve `A·x = b` over GF(2^b). `a` is a `count`×`count` row-major
/// matrix, `b` has `count` entries; both are consumed. Returns None when
/// singular (a wrong erasure set can collapse the Vandermonde).
fn solve_linear(gf: &Gf, a: &mut [u16], b: &mut [u16], count: usize) -> Option<Vec<u16>> {
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
/// general RS case for every Aztec field (GF(2^b), b = 6/8/10/12).
///
/// The erasure locator Λ(x) = Π(1 − X_i x) is built from the known
/// positions; the modified syndromes T_k = Σ_m Λ_m S_{k−m} (k = e..nn−1)
/// have the erasures divided out, so Berlekamp–Massey on `T` yields the
/// unknown-error locator σ. The combined positions then go through one
/// Vandermonde solve for the magnitudes. Capacity `e + 2t <= nn`, with at
/// least two syndromes left as checks (`nn - (e + t) >= 2`) — a wrong
/// symbol-size/orientation guess must not "correct" itself into a
/// plausible codeword.
pub fn rs_correct_erasures(
    gf: &Gf,
    codewords: &mut [u16],
    nn: usize,
    erasures: &[usize],
) -> Option<()> {
    let n = codewords.len();
    if nn == 0 || nn > n || n > gf.size() - 1 {
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
    let mut lambda = vec![1u16];
    for &p in &pos {
        let x = gf.pow((n - 1 - p) as i32);
        let mut next = vec![0u16; lambda.len() + 1];
        for (i, &c) in lambda.iter().enumerate() {
            next[i] ^= c;
            next[i + 1] ^= gf.mul(c, x);
        }
        lambda = next;
    }
    // Modified syndromes: T_k = Σ_m λ_m S_{k−m}, k = e..nn−1.
    let mut tseq = Vec::with_capacity(nn - e);
    for k in e..nn {
        let mut t = 0u16;
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
        let mut acc = 0u16;
        let mut zp = 1u16;
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
    // S_j = Σ ẽ_i X_i^j (j = 0..count−1), folded magnitudes ẽ_i = e_i·X_i.
    let xs: Vec<u16> = degs.iter().map(|&d| gf.pow(d as i32)).collect();
    let mut a = vec![0u16; count * count];
    for (i, &x) in xs.iter().enumerate() {
        let mut p = 1u16;
        for j in 0..count {
            a[j * count + i] = p;
            p = gf.mul(p, x);
        }
    }
    let mut b = s[..count].to_vec();
    let vals = solve_linear(gf, &mut a, &mut b, count)?;
    for (i, &d) in degs.iter().enumerate() {
        let mag = gf.mul(vals[i], gf.pow(-(d as i32)));
        codewords[n - 1 - d] ^= mag;
    }
    syndromes(gf, codewords, nn)
        .iter()
        .all(|&x| x == 0)
        .then_some(())
}

/// Evaluate a polynomial with ascending coefficients at the field element `z`.
fn eval(poly: &[u16], gf: &Gf, z: u16) -> u16 {
    let mut acc = 0u16;
    for &c in poly.iter().rev() {
        acc = gf.mul(acc, z) ^ c;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gf::field_for;

    /// GOLDEN: the empirically verified Aztec mode message (STATUS.md).
    /// 'AZTEC' at compact L1: mode word 00000100 (L−1 = 0, D−1 = 4) split
    /// into nibbles [0, 4], check nibbles read off a real zint symbol and
    /// matched exactly.
    #[test]
    fn mode_message_matches_zint_ground_truth() {
        let gf = field_for(4);
        assert_eq!(rs_encode(&gf, &[0, 4], 5), vec![10, 3, 11, 8, 4]);
    }

    #[test]
    fn encode_yields_valid_codeword() {
        for bits in [4u32, 6, 8, 10, 12] {
            let gf = field_for(bits);
            let data: Vec<u16> = (0..9).map(|i| gf.pow((i * 7 + 3) as i32)).collect();
            let nn = 6;
            let mut cw = data.clone();
            cw.extend(rs_encode(&gf, &data, nn));
            assert!(
                syndromes(&gf, &cw, nn).iter().all(|&x| x == 0),
                "not a codeword for {bits} bits"
            );
        }
    }

    #[test]
    fn corrects_up_to_t_errors_in_every_field() {
        for bits in [4u32, 6, 8, 10, 12] {
            let gf = field_for(bits);
            // Block length must stay within q-1 (alpha^i evaluation points);
            // all real Aztec sizes do (max (88+16L)*L/b <= 2^b - 1).
            let nn = 6;
            let data_len = ((1usize << bits) - 1 - nn - 1).min(11);
            let data: Vec<u16> = (0..data_len).map(|i| gf.pow((i * 5 + 1) as i32)).collect();
            let t = nn / 2;
            let mut cw = data.clone();
            cw.extend(rs_encode(&gf, &data, nn));
            // Inject t errors at scattered positions, deterministic values.
            for k in 0..t {
                let pos = (k * 5 + 2) % cw.len();
                cw[pos] ^= gf.pow((k + 3) as i32).max(1);
            }
            assert!(
                rs_correct(&gf, &mut cw, nn).is_some(),
                "correct failed for {bits} bits"
            );
            assert_eq!(
                &cw[..data.len()],
                &data[..],
                "payload wrong for {bits} bits"
            );
        }
    }

    #[test]
    fn rejects_uncorrectable_pattern() {
        let gf = field_for(8);
        let data: Vec<u16> = (0..10).map(|i| gf.pow(i as i32)).collect();
        let nn = 4; // t = 2
        let mut cw = data.clone();
        cw.extend(rs_encode(&gf, &data, nn));
        for i in 0..5 {
            cw[i] ^= 1;
        }
        assert_eq!(rs_correct(&gf, &mut cw, nn), None);
    }

    #[test]
    fn erasures_double_the_capacity_in_every_field() {
        for bits in [4u32, 6, 8, 10, 12] {
            let gf = field_for(bits);
            let nn = 6; // t = 3
            let data_len = ((1usize << bits) - 1 - nn - 1).min(11);
            let data: Vec<u16> = (0..data_len).map(|i| gf.pow((i * 3 + 2) as i32)).collect();
            let mut cw = data.clone();
            cw.extend(rs_encode(&gf, &data, nn));
            let clean = cw.clone();
            // 4 erasures (nn - e = 2 checks) — the error-only capacity is 3.
            let erased = [0usize, 2, 4, 6];
            let mut broken = clean.clone();
            for &i in &erased {
                broken[i] ^= gf.pow(7).max(1);
            }
            assert!(rs_correct(&gf, &mut broken.clone(), nn).is_none());
            assert_eq!(rs_correct_erasures(&gf, &mut broken, nn, &erased), Some(()));
            assert_eq!(broken, clean, "{bits} bits");

            // Mixed: 2 erasures + 2 unknown errors = 2 + 2·2 = 6.
            let mixed = [1usize, 3];
            let mut broken = clean.clone();
            for &i in &mixed {
                broken[i] ^= gf.pow(5).max(1);
            }
            for &i in &[0usize, 5] {
                broken[i] ^= gf.pow(9).max(1);
            }
            assert!(rs_correct_erasures(&gf, &mut broken, nn, &mixed).is_some());
            assert_eq!(broken, clean, "{bits} bits mixed");

            // e + 2t = 4 + 2·2 = 8 > 6 must reject.
            let over = [0usize, 2, 4, 6];
            let mut broken = clean.clone();
            for &i in &over {
                broken[i] ^= 1;
            }
            for &i in &[1usize, 3] {
                broken[i] ^= 2;
            }
            assert_eq!(rs_correct_erasures(&gf, &mut broken, nn, &over), None);
        }
    }

    #[test]
    fn already_valid_returns_some() {
        let gf = field_for(6);
        let data = vec![7, 9, 13];
        let mut cw = data.clone();
        cw.extend(rs_encode(&gf, &data, 4));
        assert!(rs_correct(&gf, &mut cw, 4).is_some());
    }
}
