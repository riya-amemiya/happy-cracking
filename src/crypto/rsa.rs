use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::{BigInt, BigUint, ToBigInt};
use num_integer::Integer;
use num_traits::{One, Zero};

#[derive(Subcommand)]
pub enum RsaAction {
    #[command(about = "Compute private exponent d from p, q, e")]
    ComputeD {
        #[arg(long, help = "Prime factor p")]
        p: String,
        #[arg(long, help = "Prime factor q")]
        q: String,
        #[arg(long, help = "Public exponent e")]
        e: String,
    },
    #[command(about = "Decrypt RSA ciphertext (c^d mod n)")]
    Decrypt {
        #[arg(long, help = "Ciphertext (decimal)")]
        c: String,
        #[arg(long, help = "Private exponent d")]
        d: String,
        #[arg(long, help = "Modulus n")]
        n: String,
    },
    #[command(about = "Encrypt with RSA (m^e mod n)")]
    Encrypt {
        #[arg(long, help = "Message (decimal)")]
        m: String,
        #[arg(long, help = "Public exponent e")]
        e: String,
        #[arg(long, help = "Modulus n")]
        n: String,
    },
    #[command(about = "Factor N using Fermat's method (close primes)")]
    FactorizeN {
        #[arg(long, help = "Modulus n")]
        n: String,
    },
    #[command(about = "Wiener's attack (large e relative to n)")]
    Wiener {
        #[arg(long, help = "Modulus n")]
        n: String,
        #[arg(long, help = "Public exponent e")]
        e: String,
    },
    #[command(about = "Small public exponent attack (m^e < n)")]
    SmallE {
        #[arg(long, help = "Ciphertext (decimal)")]
        c: String,
        #[arg(long, help = "Public exponent e")]
        e: String,
    },
    #[command(about = "Hastad's Broadcast Attack (same m, small e, different n)")]
    Hastad {
        #[arg(long, help = "Public exponent e")]
        e: String,
        #[arg(long, help = "Comma-separated ciphertexts")]
        ciphertexts: String,
        #[arg(long, help = "Comma-separated moduli")]
        moduli: String,
    },
    #[command(about = "Common Modulus Attack (same n, two different e values)")]
    CommonModulus {
        #[arg(long, help = "Shared modulus n")]
        n: String,
        #[arg(long, help = "First public exponent")]
        e1: String,
        #[arg(long, help = "Second public exponent")]
        e2: String,
        #[arg(long, help = "First ciphertext")]
        c1: String,
        #[arg(long, help = "Second ciphertext")]
        c2: String,
    },
    #[command(about = "Pollard's p-1 factorization")]
    PollardP1 {
        #[arg(long, help = "Modulus n")]
        n: String,
        #[arg(long, help = "Smoothness bound B", default_value = "100000")]
        b: String,
    },
    #[command(about = "Pollard's Rho factorization (Brent's variant)")]
    PollardRho {
        #[arg(long, help = "Modulus n")]
        n: String,
    },
}

pub fn run(action: RsaAction) -> Result<()> {
    match action {
        RsaAction::ComputeD { p, q, e } => {
            let p = p.parse::<BigUint>().context("Invalid number for p")?;
            let q = q.parse::<BigUint>().context("Invalid number for q")?;
            let e = e.parse::<BigUint>().context("Invalid number for e")?;
            let d = compute_d(&p, &q, &e)?;
            println!("{}", d);
        }
        RsaAction::Decrypt { c, d, n } => {
            let c = c.parse::<BigUint>().context("Invalid number for c")?;
            let d = d.parse::<BigUint>().context("Invalid number for d")?;
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let m = big_modpow(&c, &d, &n)?;
            println!("Decimal: {}", m);
            let ascii = bigint_to_ascii(&m);
            if !ascii.is_empty() {
                println!("ASCII: {}", ascii);
            }
            println!("Hex: {}", hex::encode(m.to_bytes_be()));
        }
        RsaAction::Encrypt { m, e, n } => {
            let m = m.parse::<BigUint>().context("Invalid number for m")?;
            let e = e.parse::<BigUint>().context("Invalid number for e")?;
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let c = big_modpow(&m, &e, &n)?;
            println!("{}", c);
        }
        RsaAction::FactorizeN { n } => {
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let (p, q) = fermat_factor(&n, 1_000_000)?;
            println!("p = {}", p);
            println!("q = {}", q);
        }
        RsaAction::Wiener { n, e } => {
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let e = e.parse::<BigUint>().context("Invalid number for e")?;
            let d = wiener_attack(&e, &n)?;
            println!("{}", d);
        }
        RsaAction::SmallE { c, e } => {
            let c = c.parse::<BigUint>().context("Invalid number for c")?;
            let e: u32 = e.parse().context("Invalid number for e")?;

            if e == 0 {
                anyhow::bail!("Exponent e must be non-zero");
            }

            let m = c.nth_root(e);
            println!("Decimal: {}", m);
            let ascii = bigint_to_ascii(&m);
            if !ascii.is_empty() {
                println!("ASCII: {}", ascii);
            }
            println!("Hex: {}", hex::encode(m.to_bytes_be()));
        }
        RsaAction::Hastad {
            e,
            ciphertexts,
            moduli,
        } => {
            let e: u32 = e.parse().context("Invalid number for e")?;
            let cs: Vec<BigUint> = ciphertexts
                .split(',')
                .map(|s| s.trim().parse::<BigUint>().context("Invalid ciphertext"))
                .collect::<Result<_>>()?;
            let ns: Vec<BigUint> = moduli
                .split(',')
                .map(|s| s.trim().parse::<BigUint>().context("Invalid modulus"))
                .collect::<Result<_>>()?;
            let m = hastad_broadcast(&cs, &ns, e)?;
            println!("Decimal: {}", m);
            let ascii = bigint_to_ascii(&m);
            if !ascii.is_empty() {
                println!("ASCII: {}", ascii);
            }
            println!("Hex: {}", hex::encode(m.to_bytes_be()));
        }
        RsaAction::CommonModulus { n, e1, e2, c1, c2 } => {
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let e1 = e1.parse::<BigUint>().context("Invalid number for e1")?;
            let e2 = e2.parse::<BigUint>().context("Invalid number for e2")?;
            let c1 = c1.parse::<BigUint>().context("Invalid number for c1")?;
            let c2 = c2.parse::<BigUint>().context("Invalid number for c2")?;
            let m = common_modulus_attack(&n, &e1, &e2, &c1, &c2)?;
            println!("Decimal: {}", m);
            let ascii = bigint_to_ascii(&m);
            if !ascii.is_empty() {
                println!("ASCII: {}", ascii);
            }
            println!("Hex: {}", hex::encode(m.to_bytes_be()));
        }
        RsaAction::PollardP1 { n, b } => {
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let b: u64 = b.parse().context("Invalid number for b")?;
            let (p, q) = pollard_p1(&n, b)?;
            println!("p = {}", p);
            println!("q = {}", q);
        }
        RsaAction::PollardRho { n } => {
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let (p, q) = crate::crypto::primes::pollard_rho_biguint(&n)?;
            println!("p = {}", p);
            println!("q = {}", q);
        }
    }
    Ok(())
}

// Computes the RSA private exponent d from primes p, q and public exponent e.
// Rejects p == 0 or q == 0 because BigUint subtraction would panic on the
// resulting negative intermediate.
pub fn compute_d(p: &BigUint, q: &BigUint, e: &BigUint) -> Result<BigUint> {
    if p.is_zero() || q.is_zero() {
        anyhow::bail!("RSA primes p and q must be non-zero");
    }
    let phi = (p - BigUint::one()) * (q - BigUint::one());
    big_modinv(e, &phi)
}

// Modular inverse for BigUint using extended Euclidean algorithm.
pub fn big_modinv(a: &BigUint, m: &BigUint) -> Result<BigUint> {
    if m.is_zero() {
        anyhow::bail!("Modulus must be non-zero");
    }
    if m.is_one() {
        return Ok(BigUint::zero());
    }

    let a_int = a.to_bigint().unwrap();
    let m_int = m.to_bigint().unwrap();

    let (mut old_r, mut r) = (a_int.clone(), m_int.clone());
    let (mut old_s, mut s) = (BigInt::one(), BigInt::zero());

    while !r.is_zero() {
        let q = &old_r / &r;
        let tmp_r = r.clone();
        r = &old_r - &q * &r;
        old_r = tmp_r;
        let tmp_s = s.clone();
        s = &old_s - &q * &s;
        old_s = tmp_s;
    }

    if old_r != BigInt::one() {
        anyhow::bail!("Modular inverse does not exist (gcd is {})", old_r);
    }

    let result = ((old_s % &m_int) + &m_int) % &m_int;
    Ok(result.to_biguint().unwrap())
}

// Modular exponentiation using num-bigint's built-in modpow.
pub fn big_modpow(base: &BigUint, exp: &BigUint, modulus: &BigUint) -> Result<BigUint> {
    if modulus.is_zero() {
        anyhow::bail!("Modulus must be non-zero");
    }
    Ok(base.modpow(exp, modulus))
}

// Integer nth root using the optimized library implementation.
// Returns the largest x such that x^k <= n.
pub fn integer_nth_root(n: &BigUint, k: u32) -> BigUint {
    if k == 0 {
        return BigUint::zero();
    }
    n.nth_root(k)
}

// Fermat's factorization method for N with close prime factors.
pub fn fermat_factor(n: &BigUint, max_iter: u64) -> Result<(BigUint, BigUint)> {
    if n <= &BigUint::one() {
        anyhow::bail!("Cannot factorize n <= 1");
    }

    if n.is_even() {
        let two = BigUint::from(2u32);
        let other = n / &two;
        return Ok((two, other));
    }

    let mut a = n.sqrt();
    if &a * &a < *n {
        a += BigUint::one();
    }

    for _ in 0..max_iter {
        let a_sq = &a * &a;
        if a_sq < *n {
            a += BigUint::one();
            continue;
        }
        let b_sq = &a_sq - n;
        let b = b_sq.sqrt();
        if &b * &b == b_sq {
            let p = &a + &b;
            let q = &a - &b;
            if !q.is_one() && !p.is_one() {
                return Ok((q, p));
            }
        }
        a += BigUint::one();
    }

    anyhow::bail!("Fermat factorization failed after {} iterations", max_iter)
}

// Wiener's attack on RSA when d is small (e is large relative to n).
// Uses continued fraction expansion of e/n to find candidate d values.
pub fn wiener_attack(e: &BigUint, n: &BigUint) -> Result<BigUint> {
    if n.is_zero() {
        anyhow::bail!("Modulus must be non-zero");
    }

    let convergents = continued_fraction_convergents(e, n);

    let one = BigUint::one();
    let three = BigUint::from(3u32);

    for (k, d) in convergents {
        if k.is_zero() || d.is_zero() {
            continue;
        }

        // phi_candidate = (e * d - 1) / k
        let ed = e * &d;
        if ed < one {
            continue;
        }
        let ed_minus_1 = &ed - &one;
        let (phi_candidate, rem) = ed_minus_1.div_rem(&k);
        if !rem.is_zero() {
            continue;
        }

        // Check that phi makes sense: n - phi + 1 should have discriminant that is a perfect square
        // s = n - phi + 1 (= p + q)
        // discriminant = s^2 - 4n = (p - q)^2
        let n_plus_1 = n + &one;
        if phi_candidate >= n_plus_1 {
            continue;
        }
        let s = &n_plus_1 - &phi_candidate;

        if s < three {
            continue;
        }

        let four_n = BigUint::from(4u32) * n;
        let s_sq = &s * &s;
        if s_sq < four_n {
            continue;
        }
        let discriminant = &s_sq - &four_n;
        let sqrt_disc = discriminant.sqrt();
        if &sqrt_disc * &sqrt_disc == discriminant {
            // Verify: encrypt and decrypt a test message
            let test_m = BigUint::from(2u32);
            let test_c = big_modpow(&test_m, e, n)?;
            let test_dec = big_modpow(&test_c, &d, n)?;
            if test_dec == test_m {
                return Ok(d);
            }
        }
    }

    anyhow::bail!("Wiener's attack failed to recover d")
}

// Compute convergents of the continued fraction expansion of a/b.
fn continued_fraction_convergents(a: &BigUint, b: &BigUint) -> Vec<(BigUint, BigUint)> {
    let mut convergents = Vec::new();
    let mut x = a.clone();
    let mut y = b.clone();

    let mut p_prev = BigUint::one();
    let mut p_curr = BigUint::zero();
    let mut q_prev = BigUint::zero();
    let mut q_curr = BigUint::one();

    // First partial quotient
    if y.is_zero() {
        return convergents;
    }

    loop {
        let (quotient, remainder) = x.div_rem(&y);

        // Update convergents: h_n = a_n * h_{n-1} + h_{n-2}
        let p_new = &quotient * &p_prev + &p_curr;
        let q_new = &quotient * &q_prev + &q_curr;

        convergents.push((p_new.clone(), q_new.clone()));

        p_curr = p_prev;
        p_prev = p_new;
        q_curr = q_prev;
        q_prev = q_new;

        if remainder.is_zero() {
            break;
        }

        x = y;
        y = remainder;
    }

    convergents
}

// Hastad's Broadcast Attack.
// When the same message m is encrypted with the same small exponent e
// under e different moduli n_i, use CRT to recover m^e, then take the e-th root.
pub fn hastad_broadcast(ciphertexts: &[BigUint], moduli: &[BigUint], e: u32) -> Result<BigUint> {
    let e_usize = e as usize;
    if ciphertexts.len() < e_usize || moduli.len() < e_usize {
        anyhow::bail!(
            "Hastad's attack requires at least e={} pairs of (ciphertext, modulus)",
            e
        );
    }
    if e == 0 {
        anyhow::bail!("Exponent e must be non-zero");
    }

    let cs = &ciphertexts[..e_usize];
    let ns = &moduli[..e_usize];

    if ns.iter().any(|n| n.is_zero()) {
        anyhow::bail!("All moduli must be non-zero");
    }

    // CRT: find x such that x ≡ c_i (mod n_i) for all i
    let big_n: BigUint = ns.iter().product();
    let mut x = BigUint::zero();

    for i in 0..e_usize {
        let ni_cap = &big_n / &ns[i];
        let yi = big_modinv(&ni_cap, &ns[i])?;
        x += &cs[i] * &ni_cap * &yi;
    }
    let me = x % &big_n;

    let m = me.nth_root(e);

    // Verify
    if m.pow(e) != me {
        anyhow::bail!("Hastad's attack failed: e-th root is not exact");
    }

    Ok(m)
}

// Common Modulus Attack.
// When the same message m is encrypted under the same modulus n with two
// coprime exponents e1, e2, recover m using extended GCD.
// m = c1^s * c2^t mod n where e1*s + e2*t = 1
pub fn common_modulus_attack(
    n: &BigUint,
    e1: &BigUint,
    e2: &BigUint,
    c1: &BigUint,
    c2: &BigUint,
) -> Result<BigUint> {
    if n.is_zero() {
        anyhow::bail!("Modulus must be non-zero");
    }

    let e1_int = e1.to_bigint().unwrap();
    let e2_int = e2.to_bigint().unwrap();

    // Extended GCD to find s, t such that e1*s + e2*t = gcd(e1, e2)
    let (g, s, t) = extended_gcd_bigint(&e1_int, &e2_int);

    if g != BigInt::one() {
        anyhow::bail!("Common modulus attack requires gcd(e1, e2) = 1, got {}", g);
    }

    let n_int = n.to_bigint().unwrap();
    let c1_int = c1.to_bigint().unwrap();
    let c2_int = c2.to_bigint().unwrap();

    // For negative exponents, use modular inverse of the ciphertext
    let part1 = if s < BigInt::zero() {
        let c1_inv = big_modinv_signed(&c1_int, &n_int)?;
        mod_pow_bigint(&c1_inv, &(-&s), &n_int)?
    } else {
        mod_pow_bigint(&c1_int, &s, &n_int)?
    };

    let part2 = if t < BigInt::zero() {
        let c2_inv = big_modinv_signed(&c2_int, &n_int)?;
        mod_pow_bigint(&c2_inv, &(-&t), &n_int)?
    } else {
        mod_pow_bigint(&c2_int, &t, &n_int)?
    };

    let m_int = (&part1 * &part2) % &n_int;
    let m_int = (m_int + &n_int) % &n_int;

    Ok(m_int.to_biguint().unwrap())
}

// Extended GCD for BigInt. Returns (gcd, s, t) where a*s + b*t = gcd.
fn extended_gcd_bigint(a: &BigInt, b: &BigInt) -> (BigInt, BigInt, BigInt) {
    let mut old_r = a.clone();
    let mut r = b.clone();
    let mut old_s = BigInt::one();
    let mut s = BigInt::zero();
    let mut old_t = BigInt::zero();
    let mut t = BigInt::one();

    while !r.is_zero() {
        let q = &old_r / &r;
        let tmp_r = r.clone();
        r = &old_r - &q * &r;
        old_r = tmp_r;
        let tmp_s = s.clone();
        s = &old_s - &q * &s;
        old_s = tmp_s;
        let tmp_t = t.clone();
        t = &old_t - &q * &t;
        old_t = tmp_t;
    }

    (old_r, old_s, old_t)
}

// Modular inverse for BigInt (signed).
fn big_modinv_signed(a: &BigInt, m: &BigInt) -> Result<BigInt> {
    let (g, x, _) = extended_gcd_bigint(a, m);
    if g != BigInt::one() && g != -BigInt::one() {
        anyhow::bail!("Modular inverse does not exist");
    }
    Ok(((x % m) + m) % m)
}

// Modular exponentiation for BigInt.
fn mod_pow_bigint(base: &BigInt, exp: &BigInt, modulus: &BigInt) -> Result<BigInt> {
    if modulus.is_zero() {
        anyhow::bail!("Modulus must be non-zero");
    }
    let base_uint = ((base % modulus) + modulus) % modulus;
    let exp_uint = exp.to_biguint().unwrap();
    let mod_uint = modulus.to_biguint().unwrap();
    let base_uint = base_uint.to_biguint().unwrap();
    Ok(base_uint.modpow(&exp_uint, &mod_uint).to_bigint().unwrap())
}

// Maximum allowed smoothness bound B to prevent CPU Exhaustion DoS attacks.
pub const MAX_POLLARD_P1_BOUND: u64 = 10_000_000;

// Pollard's p-1 factorization.
// Finds a factor of n when p-1 is B-smooth (all prime factors ≤ B).
pub fn pollard_p1(n: &BigUint, b: u64) -> Result<(BigUint, BigUint)> {
    if b > MAX_POLLARD_P1_BOUND {
        anyhow::bail!(
            "Smoothness bound B exceeds the maximum allowed limit of {} to prevent DoS",
            MAX_POLLARD_P1_BOUND
        );
    }

    if n <= &BigUint::one() {
        anyhow::bail!("Cannot factorize n <= 1");
    }

    if n.is_even() {
        let two = BigUint::from(2u32);
        let other = n / &two;
        return Ok((two, other));
    }

    let mut a = BigUint::from(2u32);

    // Compute a = 2^(B!) mod n iteratively: a = a^k mod n for k = 2, 3, ..., B
    for k in 2..=b {
        let k_big = BigUint::from(k);
        a = a.modpow(&k_big, n);

        let a_minus_1 = if a > BigUint::one() {
            &a - BigUint::one()
        } else {
            continue;
        };
        let g = a_minus_1.gcd(n);
        if g > BigUint::one() && &g < n {
            let q = n / &g;
            return Ok((g, q));
        }
        if &g == n {
            anyhow::bail!("Pollard p-1 failed: GCD equals n (try smaller bound)");
        }
    }

    // Final GCD check
    if a > BigUint::one() {
        let a_minus_1 = &a - BigUint::one();
        let g = a_minus_1.gcd(n);
        if g > BigUint::one() && &g < n {
            let q = n / &g;
            return Ok((g, q));
        }
    }

    anyhow::bail!("Pollard p-1 failed to factor n with bound B={}", b)
}

// Convert a BigUint to an ASCII string by interpreting its bytes.
pub fn bigint_to_ascii(n: &BigUint) -> String {
    if n.is_zero() {
        return String::new();
    }
    let bytes = n.to_bytes_be();
    match String::from_utf8(bytes) {
        Ok(s) if s.chars().all(|c| c.is_ascii_graphic() || c == ' ') => s,
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_nth_root_perfect() {
        let n = BigUint::from(27u32);
        assert_eq!(integer_nth_root(&n, 3), BigUint::from(3u32));
    }

    #[test]
    fn test_integer_nth_root_not_perfect() {
        let n = BigUint::from(28u32);
        assert_eq!(integer_nth_root(&n, 3), BigUint::from(3u32));
    }
}
