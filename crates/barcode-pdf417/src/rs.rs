//! Reed-Solomon error detection over GF(929) (spec 2.6.3).
//!
//! The message polynomial's coefficients are the codeword values in
//! stream order (first codeword = highest degree): the length descriptor,
//! data codewords and pad codewords — row indicators excluded. The ECC
//! codewords are the complement of the remainder of d(x)·x^k ÷ g(x),
//! with g(x) = (x−3)(x−3²)⋯(x−3^k). All arithmetic mod 929 (prime).
//!
//! Decoding only needs syndromes: the full codeword polynomial
//! T(x) = d(x)·x^k + ecc is divisible by g(x), so T(3^i) = 0 for
//! i = 1..=k on an undamaged symbol.

/// ECC codeword count for an error correction level (spec Table 6).
pub fn k_for_level(level: u8) -> usize {
    1 << (level + 1)
}

/// 3^i mod 929, i >= 1 (running product; no table needed).
fn pow3(i: usize) -> u32 {
    let mut acc = 1u32;
    for _ in 0..i {
        acc = acc * 3 % 929;
    }
    acc
}

/// Generator polynomial coefficients (monic, highest degree first),
/// excluding the leading 1: g(x) = x^k + g[0]x^(k-1) + ... + g[k-1].
fn generator(k: usize) -> Vec<u32> {
    let mut gen = vec![1u32]; // highest-first, monic
    for i in 1..=k {
        let r = pow3(i);
        let mut next = vec![0u32; gen.len() + 1];
        for (j, &g) in gen.iter().enumerate() {
            next[j] = (next[j] + g) % 929;
            next[j + 1] = (next[j + 1] + 929 - (g * r) % 929) % 929;
        }
        gen = next;
    }
    gen[1..].to_vec()
}

/// Encode: ECC codewords for `message` (descriptor+data+pads) with `k`
/// ecc codewords. Complement of the remainder of d(x)·x^k ÷ g(x).
pub fn rs_encode(message: &[u16], k: usize) -> Vec<u16> {
    assert!(k > 0 && k <= 512);
    let gen = generator(k);
    let mut buf = vec![0u32; message.len() + k];
    for (i, &m) in message.iter().enumerate() {
        buf[i] = m as u32;
    }
    for i in 0..message.len() {
        let c = buf[i];
        if c == 0 {
            continue;
        }
        for (j, &g) in gen.iter().enumerate() {
            buf[i + 1 + j] = (buf[i + 1 + j] + 929 - (c * g) % 929) % 929;
        }
    }
    buf[message.len()..]
        .iter()
        .map(|&x| ((929 - x) % 929) as u16)
        .collect()
}

/// Syndromes S_i = T(3^i), i = 1..=k, of the full codeword sequence
/// (message ++ ecc, stream order). All zero <=> undamaged.
pub fn rs_syndromes(full: &[u16], k: usize) -> Vec<u32> {
    (1..=k)
        .map(|i| {
            let root = pow3(i);
            full.iter()
                .fold(0u32, |acc, &c| (acc * root + c as u32) % 929)
        })
        .collect()
}

/// True if the full codeword sequence (message ++ ecc) has zero syndromes.
pub fn rs_valid(full: &[u16], k: usize) -> bool {
    full.iter().all(|&c| (c as u32) < 929) && rs_syndromes(full, k).iter().all(|&s| s == 0)
}

/// a^e mod m, square-and-multiply.
fn pow_mod(mut a: u32, mut e: u32, m: u32) -> u32 {
    let mut acc = 1u32;
    while e > 0 {
        if e & 1 == 1 {
            acc = acc * a % m;
        }
        a = a * a % m;
        e >>= 1;
    }
    acc
}

/// Multiplicative inverse mod the prime 929 (Fermat).
fn inv(a: u32) -> u32 {
    pow_mod(a, 927, 929)
}

/// Minimal connection polynomial C (lowest-degree-first, C[0] = 1) with
/// sum_i C[i] * S[n-i] == 0 for n >= L (Massey 1969 over GF(p)).
/// Returns (C, L), or None on internal inconsistency.
fn berlekamp_massey(s: &[u32]) -> Option<(Vec<u32>, usize)> {
    let mut c: Vec<u32> = vec![1];
    let mut b: Vec<u32> = vec![1];
    let (mut l, mut m, mut b_coef) = (0usize, 1usize, 1u32);
    for n in 0..s.len() {
        let mut d = s[n];
        for i in 1..=l.min(c.len() - 1) {
            d = (d + c[i] * s[n - i]) % 929;
        }
        if d == 0 {
            m += 1;
            continue;
        }
        let coef = d * inv(b_coef) % 929;
        let need = b.len() + m;
        if c.len() < need {
            c.resize(need, 0);
        }
        if 2 * l <= n {
            let t = c.clone();
            for (i, &bc) in b.iter().enumerate() {
                c[i + m] = (c[i + m] + 929 - coef * bc % 929) % 929;
            }
            l = n + 1 - l;
            b = t;
            b_coef = d;
            m = 1;
        } else {
            for (i, &bc) in b.iter().enumerate() {
                c[i + m] = (c[i + m] + 929 - coef * bc % 929) % 929;
            }
            m += 1;
        }
    }
    Some((c, l))
}

/// Solve mat·x = rhs over GF(929) (n x n). None if singular.
fn solve_linear(mat: &[Vec<u32>], rhs: &[u32]) -> Option<Vec<u32>> {
    let n = rhs.len();
    let mut a: Vec<Vec<u32>> = mat
        .iter()
        .zip(rhs)
        .map(|(row, &r)| row.iter().chain(std::iter::once(&r)).map(|&v| v % 929).collect())
        .collect();
    for col in 0..n {
        let piv = (col..n).find(|&r| a[r][col] != 0)?;
        a.swap(col, piv);
        let iv = inv(a[col][col]);
        for v in a[col].iter_mut() {
            *v = *v * iv % 929;
        }
        for r in 0..n {
            if r != col && a[r][col] != 0 {
                let f = a[r][col];
                for i in 0..=n {
                    a[r][i] = (a[r][i] + 929 - f * a[col][i] % 929) % 929;
                }
            }
        }
    }
    Some(a.iter().map(|row| row[n]).collect())
}

/// Correct a codeword sequence when the damaged positions are KNOWN
/// (erasures). Each erasure costs one slot, so up to `k` codewords are
/// recoverable — twice the error-only budget of `(k-2)/2`.
///
/// The detector knows these for free: a PDF417 cell whose 17-module
/// pattern matches no table entry, or a whole row that never voted, was
/// emitted as a zero placeholder. This solves the value system directly:
/// S_j = sum_l e_l * X_l^j for j = 1..=e (the first `e` syndromes are
/// enough for `e` known positions), then verifies against all `k`.
///
/// Pure erasures only: if unknown errors are also present the solved
/// values generally fail the full verification, and the caller falls back
/// to [`rs_correct`].
pub fn rs_correct_erasures(
    full: &[u16],
    k: usize,
    erasures: &[usize],
) -> Option<(Vec<u16>, Vec<(usize, u16, u16)>)> {
    if k == 0 || k > 512 || !full.iter().all(|&c| (c as u32) < 929) {
        return None;
    }
    let n = full.len();
    let mut pos: Vec<usize> = erasures.to_vec();
    pos.sort_unstable();
    pos.dedup();
    if pos.is_empty() || pos.len() > k || pos.iter().any(|&p| p >= n) {
        return None;
    }
    let s = rs_syndromes(full, k);
    if s.iter().all(|&x| x == 0) {
        return Some((full.to_vec(), Vec::new()));
    }
    // stream position p (highest degree first) -> degree n-1-p, X = 3^d
    let xs: Vec<u32> = pos.iter().map(|&p| pow3(n - 1 - p)).collect();
    let e = pos.len();
    let mat: Vec<Vec<u32>> = (1..=e)
        .map(|j| xs.iter().map(|&x| pow_mod(x, j as u32, 929)).collect())
        .collect();
    let vals = solve_linear(&mat, &s[..e])?;
    let mut out = full.to_vec();
    let mut fixes = Vec::new();
    for (&p, &val) in pos.iter().zip(&vals) {
        let old = out[p];
        let new = ((old as u32 + 929 - val) % 929) as u16;
        out[p] = new;
        fixes.push((p, old, new));
    }
    if !rs_valid(&out, k) {
        return None;
    }
    Some((out, fixes))
}

/// Errors AND erasures: known positions plus unknown errors, the general
/// RS case. Erasure locator Λ(x) = Π(1 − X_i x); the modified syndromes
/// T(x) = S(x)·Λ(x) (truncated to x^(k−1)) satisfy the error-only
/// recurrence, so Berlekamp–Massey on `T_{e+1..k}` yields the unknown
/// error locator σ(x). The combined positions (erasures ∪ roots of σ)
/// then go through the same value solve as the pure cases. Capacity:
/// `e + 2t <= k`, verified against all `k` syndromes.
pub fn rs_correct_mixed(
    full: &[u16],
    k: usize,
    erasures: &[usize],
) -> Option<(Vec<u16>, Vec<(usize, u16, u16)>)> {
    if k == 0 || k > 512 || !full.iter().all(|&c| (c as u32) < 929) {
        return None;
    }
    let n = full.len();
    let mut pos: Vec<usize> = erasures.to_vec();
    pos.sort_unstable();
    pos.dedup();
    if pos.iter().any(|&p| p >= n) || pos.len() > k {
        return None;
    }
    if pos.is_empty() {
        return rs_correct(full, k);
    }
    let s = rs_syndromes(full, k);
    if s.iter().all(|&x| x == 0) {
        return Some((full.to_vec(), Vec::new()));
    }
    let e = pos.len();
    // Erasure locator, lowest-degree first (λ_0 = 1).
    let mut nu: Vec<u32> = vec![1];
    for &p in &pos {
        let x = pow3(n - 1 - p);
        let mut next = vec![0u32; nu.len() + 1];
        for (i, &c) in nu.iter().enumerate() {
            next[i] = (next[i] + c) % 929;
            next[i + 1] = (next[i + 1] + 929 - (c * x) % 929) % 929;
        }
        nu = next;
    }
    // Modified syndromes T_n = Σ_m λ_m S_{n-m}, n = e+1 ..= k.
    let mut tseq = Vec::with_capacity(k.saturating_sub(e));
    for nn in (e + 1)..=k {
        let mut t = 0u32;
        for (m, &lam) in nu.iter().enumerate() {
            if nn >= m + 1 {
                t = (t + lam * s[nn - m - 1]) % 929;
            }
        }
        tseq.push(t);
    }
    let (c, l) = berlekamp_massey(&tseq)?;
    if e + 2 * l > k {
        return None;
    }
    // Unknown-error positions: roots of σ over the same Chien mapping.
    let c_hi: Vec<u32> = c.iter().rev().copied().collect();
    let mut degrees: Vec<usize> = (0..n)
        .filter(|&d| {
            let x = pow3(928 - d);
            c_hi.iter().fold(0u32, |acc, &ci| (acc * x + ci) % 929) == 0
        })
        .collect();
    if degrees.len() != l {
        return None;
    }
    // Combined = unknown errors + erasures (positions must be disjoint).
    for &p in &pos {
        let d = n - 1 - p;
        if degrees.contains(&d) {
            return None;
        }
        degrees.push(d);
    }
    let count = degrees.len();
    if count != e + l {
        return None;
    }
    let xs: Vec<u32> = degrees.iter().map(|&d| pow3(d)).collect();
    let mat: Vec<Vec<u32>> = (1..=count)
        .map(|j| xs.iter().map(|&x| pow_mod(x, j as u32, 929)).collect())
        .collect();
    let vals = solve_linear(&mat, &s[..count])?;
    let mut out = full.to_vec();
    let mut fixes = Vec::new();
    for (i, &d) in degrees.iter().enumerate() {
        let p = n - 1 - d;
        let old = out[p];
        let new = ((old as u32 + 929 - vals[i]) % 929) as u16;
        out[p] = new;
        fixes.push((p, old, new));
    }
    if !rs_valid(&out, k) {
        return None;
    }
    Some((out, fixes))
}

/// Correct up to (k-2)/2 errors anywhere in the full codeword sequence
/// (message ++ ecc, stream order = highest degree first). Erasures may
/// be pre-zeroed by the caller: they become errors with unknown
/// positions (capacity 2 per slot instead of 1; see
/// [`rs_correct_erasures`] for known positions).
///
/// Returns (corrected, (position, old, new) fixes) or None if the
/// damage exceeds capacity or the result fails verification.
pub fn rs_correct(full: &[u16], k: usize) -> Option<(Vec<u16>, Vec<(usize, u16, u16)>)> {
    if k == 0 || k > 512 || !full.iter().all(|&c| (c as u32) < 929) {
        return None;
    }
    let n = full.len();
    let s = rs_syndromes(full, k);
    if s.iter().all(|&x| x == 0) {
        return Some((full.to_vec(), Vec::new()));
    }

    let (c, l) = berlekamp_massey(&s)?;
    if l == 0 || l > (k - 2) / 2 {
        return None;
    }

    // Chien: error at degree d <=> C(3^-d) == 0 (C lowest-first; the
    // reversed slice makes poly-evaluation highest-first).
    let c_hi: Vec<u32> = c.iter().rev().copied().collect();
    let degrees: Vec<usize> = (0..n)
        .filter(|&d| {
            let x = pow3(928 - d);
            c_hi.iter().fold(0u32, |acc, &ci| (acc * x + ci) % 929) == 0
        })
        .collect();
    if degrees.len() != l {
        return None; // locator inconsistent with the codeword count
    }

    // error values: sum_l e_l * X_l^j = S_j (j = 1..nu), X_l = 3^d_l
    let xs: Vec<u32> = degrees.iter().map(|&d| pow3(d)).collect();
    let mat: Vec<Vec<u32>> = (1..=degrees.len())
        .map(|j| xs.iter().map(|&x| pow_mod(x, j as u32, 929)).collect())
        .collect();
    let vals = solve_linear(&mat, &s[..degrees.len()])?;

    let mut out = full.to_vec();
    let mut fixes = Vec::new();
    for (&d, &e) in degrees.iter().zip(&vals) {
        let pos = n - 1 - d; // stream index of the coefficient at degree d
        let old = out[pos];
        let new = ((old as u32 + 929 - e) % 929) as u16;
        out[pos] = new;
        fixes.push((pos, old, new));
    }
    if !rs_valid(&out, k) {
        return None;
    }
    Some((out, fixes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec-anchored example: 'A1' level 0 -> ecc [208, 633].
    #[test]
    fn a1_level0() {
        let message = [3u16, 28, 59]; // descriptor + text data
        assert_eq!(rs_encode(&message, 2), vec![208, 633]);
        let mut full = message.to_vec();
        full.extend([208, 633]);
        assert!(rs_valid(&full, 2));
    }

    #[test]
    fn corrupted_codeword_detected() {
        let mut full = vec![3u16, 28, 59, 208, 633];
        assert!(rs_valid(&full, 2));
        full[1] = (full[1] + 1) % 929;
        assert!(!rs_valid(&full, 2));
    }

    #[test]
    fn generator_roots_annihilate_message() {
        // remainder of message * x^k / g(x) must be zero for a full
        // sequence we built ourselves
        let message = [3u16, 28, 59];
        let ecc = rs_encode(&message, 8);
        let mut full = message.to_vec();
        full.extend(&ecc);
        assert!(rs_valid(&full, 8));
    }

    /// Known positions (erasures) cost one slot each: all `k` parity
    /// codewords can be erased, while the error-only pass stops at
    /// (k-2)/2 = 3 for k=8.
    #[test]
    fn known_erasures_reach_the_parity_budget() {
        let message = [10u16, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let ecc = rs_encode(&message, 8);
        let good: Vec<u16> = message.iter().chain(ecc.iter()).copied().collect();
        let mut damaged = good.clone();
        // eight known positions, deliberately spread over data and parity
        let erased: Vec<usize> = (2..18).step_by(2).take(8).collect();
        for &p in &erased {
            damaged[p] = 0;
        }
        assert!(!rs_valid(&damaged, 8));
        assert!(
            rs_correct(&damaged, 8).is_none(),
            "8 unknown errors must exceed (k-2)/2 = 3"
        );
        let (fixed, fixes) = rs_correct_erasures(&damaged, 8, &erased).expect("8 known erasures");
        assert_eq!(fixed, good);
        assert_eq!(fixes.len(), 8);
    }

    /// Known erasures AND unknown errors together (the general case).
    /// Error-only capacity is 3; 2 erasures + 2 errors uses 2 + 4 = 6.
    #[test]
    fn mixed_errors_and_erasures_decode() {
        let message = [10u16, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let ecc = rs_encode(&message, 8);
        let good: Vec<u16> = message.iter().chain(ecc.iter()).copied().collect();
        let mut damaged = good.clone();
        let erased = [2usize, 15];
        for &p in &erased {
            damaged[p] = 0;
        }
        for &p in &[5usize, 11] {
            damaged[p] = (damaged[p] + 7) % 929;
        }
        assert!(!rs_valid(&damaged, 8));
        assert!(rs_correct(&damaged, 8).is_none(), "4 unknown errors exceed (k-2)/2 = 3");
        let (fixed, fixes) = rs_correct_mixed(&damaged, 8, &erased).expect("2 erasures + 2 errors");
        assert_eq!(fixed, good);
        assert_eq!(fixes.len(), 4);
    }

    /// The mixed capacity is e + 2t <= k.
    #[test]
    fn mixed_capacity_bound() {
        let message = [10u16, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let ecc = rs_encode(&message, 8);
        let good: Vec<u16> = message.iter().chain(ecc.iter()).copied().collect();

        let mut at_capacity = good.clone();
        let erased = [1usize, 2, 3, 12, 13, 14];
        for &p in &erased {
            at_capacity[p] = 0;
        }
        at_capacity[16] = (at_capacity[16] + 3) % 929;
        let (fixed, _) = rs_correct_mixed(&at_capacity, 8, &erased).expect("6 + 2*1 = 8");
        assert_eq!(fixed, good);

        let mut over = good.clone();
        let erased = [1usize, 2, 3, 4, 12, 13, 14];
        for &p in &erased {
            over[p] = 0;
        }
        over[16] = (over[16] + 3) % 929;
        assert!(rs_correct_mixed(&over, 8, &erased).is_none(), "7 + 2*1 > 8");
    }

    /// A clean stream with a stale erasure list must not be "corrected".
    #[test]
    fn erasures_on_a_clean_stream_are_a_noop() {
        let message = [4u16, 1, 2, 3];
        let ecc = rs_encode(&message, 4);
        let good: Vec<u16> = message.iter().chain(ecc.iter()).copied().collect();
        let (fixed, fixes) = rs_correct_erasures(&good, 4, &[0, 5]).expect("clean stream");
        assert_eq!(fixed, good);
        assert!(fixes.is_empty());
    }

    #[test]
    fn k_doubles_per_level() {
        assert_eq!((0..=8).map(k_for_level).collect::<Vec<_>>(), vec![2, 4, 8, 16, 32, 64, 128, 256, 512]);
    }
}
