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
}

pub fn run(action: RsaAction) -> Result<()> {
    match action {
        RsaAction::ComputeD { p, q, e } => {
            let p = p.parse::<BigUint>().context("Invalid number for p")?;
            let q = q.parse::<BigUint>().context("Invalid number for q")?;
            let e = e.parse::<BigUint>().context("Invalid number for e")?;
            let phi = (&p - BigUint::one()) * (&q - BigUint::one());
            let d = big_modinv(&e, &phi)?;
            println!("{}", d);
        }
        RsaAction::Decrypt { c, d, n } => {
            let c = c.parse::<BigUint>().context("Invalid number for c")?;
            let d = d.parse::<BigUint>().context("Invalid number for d")?;
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let m = big_modpow(&c, &d, &n);
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
            let c = big_modpow(&m, &e, &n);
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
            let m = integer_nth_root(&c, e);
            println!("Decimal: {}", m);
            let ascii = bigint_to_ascii(&m);
            if !ascii.is_empty() {
                println!("ASCII: {}", ascii);
            }
            println!("Hex: {}", hex::encode(m.to_bytes_be()));
        }
    }
    Ok(())
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
pub fn big_modpow(base: &BigUint, exp: &BigUint, modulus: &BigUint) -> BigUint {
    base.modpow(exp, modulus)
}

// Integer nth root using Newton's method.
// Returns the largest x such that x^k <= n.
pub fn integer_nth_root(n: &BigUint, k: u32) -> BigUint {
    if n.is_zero() {
        return BigUint::zero();
    }
    if k == 1 {
        return n.clone();
    }

    let k_big = BigUint::from(k);
    let k_minus_1 = BigUint::from(k - 1);

    // Initial guess: 2^(ceil(bit_length / k))
    let bit_len = n.bits();
    let shift = bit_len.div_ceil(u64::from(k));
    let mut x = BigUint::one() << shift;

    loop {
        // x_new = ((k-1) * x + n / x^(k-1)) / k
        let x_pow = x.pow(k - 1);
        let x_new = (&k_minus_1 * &x + n / &x_pow) / &k_big;

        if x_new >= x {
            break;
        }
        x = x_new;
    }

    // Verify and adjust downward if needed
    while x.pow(k) > *n {
        x -= BigUint::one();
    }

    x
}

// Fermat's factorization method for N with close prime factors.
pub fn fermat_factor(n: &BigUint, max_iter: u64) -> Result<(BigUint, BigUint)> {
    if n.is_even() {
        let two = BigUint::from(2u32);
        let other = n / &two;
        return Ok((two, other));
    }

    let mut a = integer_nth_root(n, 2);
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
        let b = integer_nth_root(&b_sq, 2);
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
        let sqrt_disc = integer_nth_root(&discriminant, 2);
        if &sqrt_disc * &sqrt_disc == discriminant {
            // Verify: encrypt and decrypt a test message
            let test_m = BigUint::from(2u32);
            let test_c = big_modpow(&test_m, e, n);
            let test_dec = big_modpow(&test_c, &d, n);
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
