use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::{BigInt, BigUint, ToBigInt};
use num_integer::Integer;
use num_traits::{One, ToPrimitive, Zero};
use std::collections::HashMap;

use crate::crypto::primes;

#[derive(Subcommand)]
pub enum EcAction {
    #[command(about = "Add two points on an elliptic curve (y^2 = x^3 + ax + b mod p)")]
    Add {
        #[arg(help = "First point as x,y (or 'inf' for point at infinity)")]
        point1: String,
        #[arg(help = "Second point as x,y (or 'inf' for point at infinity)")]
        point2: String,
        #[arg(long, help = "Curve parameter a")]
        a: String,
        #[arg(long, help = "Curve parameter b")]
        b: String,
        #[arg(long, help = "Prime modulus p")]
        p: String,
    },
    #[command(about = "Scalar multiplication of a point (n * P)")]
    Multiply {
        #[arg(help = "Point as x,y")]
        point: String,
        #[arg(long, help = "Scalar multiplier n")]
        n: String,
        #[arg(long, help = "Curve parameter a")]
        a: String,
        #[arg(long, help = "Curve parameter b")]
        b: String,
        #[arg(long, help = "Prime modulus p")]
        p: String,
    },
    #[command(about = "Find the order of a point on the curve")]
    Order {
        #[arg(help = "Point as x,y")]
        point: String,
        #[arg(long, help = "Curve parameter a")]
        a: String,
        #[arg(long, help = "Curve parameter b")]
        b: String,
        #[arg(long, help = "Prime modulus p")]
        p: String,
    },
    #[command(about = "Pohlig-Hellman attack to solve ECDLP (Q = nP) for smooth-order curves")]
    PohligHellman {
        #[arg(help = "Generator point P as x,y")]
        generator: String,
        #[arg(help = "Target point Q as x,y")]
        target: String,
        #[arg(long, help = "Curve parameter a")]
        a: String,
        #[arg(long, help = "Curve parameter b")]
        b: String,
        #[arg(long, help = "Prime modulus p")]
        p: String,
        #[arg(long, help = "Order of the generator point")]
        order: String,
    },
}

pub fn run(action: EcAction) -> Result<()> {
    match action {
        EcAction::Add {
            point1,
            point2,
            a,
            b,
            p,
        } => {
            let p1 = parse_point(&point1)?;
            let p2 = parse_point(&point2)?;
            let a = a.parse::<BigInt>().context("Invalid number for a")?;
            let b = b.parse::<BigInt>().context("Invalid number for b")?;
            let p = p.parse::<BigInt>().context("Invalid number for p")?;
            if p.is_zero() {
                anyhow::bail!("Modulus p must be non-zero");
            }
            validate_point_on_curve(&p1, &a, &b, &p)?;
            validate_point_on_curve(&p2, &a, &b, &p)?;
            let result = point_add(&p1, &p2, &a, &p)?;
            println!("{}", format_point(&result));
        }
        EcAction::Multiply { point, n, a, b, p } => {
            let pt = parse_point(&point)?;
            let n = n.parse::<BigUint>().context("Invalid number for n")?;
            let a = a.parse::<BigInt>().context("Invalid number for a")?;
            let b = b.parse::<BigInt>().context("Invalid number for b")?;
            let p = p.parse::<BigInt>().context("Invalid number for p")?;
            if p.is_zero() {
                anyhow::bail!("Modulus p must be non-zero");
            }
            validate_point_on_curve(&pt, &a, &b, &p)?;
            let result = scalar_multiply(&pt, &n, &a, &p)?;
            println!("{}", format_point(&result));
        }
        EcAction::Order { point, a, b, p } => {
            let pt = parse_point(&point)?;
            let a = a.parse::<BigInt>().context("Invalid number for a")?;
            let b = b.parse::<BigInt>().context("Invalid number for b")?;
            let p = p.parse::<BigInt>().context("Invalid number for p")?;
            if p.is_zero() {
                anyhow::bail!("Modulus p must be non-zero");
            }
            validate_point_on_curve(&pt, &a, &b, &p)?;
            let ord = point_order(&pt, &a, &p)?;
            println!("{}", ord);
        }
        EcAction::PohligHellman {
            generator,
            target,
            a,
            b,
            p,
            order,
        } => {
            let generator_pt = parse_point(&generator)?;
            let tgt = parse_point(&target)?;
            let a = a.parse::<BigInt>().context("Invalid number for a")?;
            let b = b.parse::<BigInt>().context("Invalid number for b")?;
            let p = p.parse::<BigInt>().context("Invalid number for p")?;
            if p.is_zero() {
                anyhow::bail!("Modulus p must be non-zero");
            }
            let order = order
                .parse::<BigUint>()
                .context("Invalid number for order")?;
            validate_point_on_curve(&generator_pt, &a, &b, &p)?;
            validate_point_on_curve(&tgt, &a, &b, &p)?;
            let n = pohlig_hellman(&generator_pt, &tgt, &a, &p, &order)?;
            println!("{}", n);
        }
    }
    Ok(())
}

// A point on an elliptic curve, or the point at infinity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ECPoint {
    Infinity,
    Affine { x: BigInt, y: BigInt },
}

// Parse a point from a string. "inf" for the point at infinity, "x,y" for an affine point.
pub fn parse_point(s: &str) -> Result<ECPoint> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("inf") || s.eq_ignore_ascii_case("infinity") || s == "O" {
        return Ok(ECPoint::Infinity);
    }
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        anyhow::bail!("Point must be 'x,y' or 'inf', got '{}'", s);
    }
    let x = parts[0]
        .trim()
        .parse::<BigInt>()
        .context("Invalid x coordinate")?;
    let y = parts[1]
        .trim()
        .parse::<BigInt>()
        .context("Invalid y coordinate")?;
    Ok(ECPoint::Affine { x, y })
}

// Format a point for display.
pub fn format_point(point: &ECPoint) -> String {
    match point {
        ECPoint::Infinity => "inf".to_string(),
        ECPoint::Affine { x, y } => format!("({}, {})", x, y),
    }
}

// Check whether a point lies on the curve y^2 = x^3 + ax + b (mod p).
pub fn is_on_curve(point: &ECPoint, a: &BigInt, b: &BigInt, p: &BigInt) -> bool {
    if p.is_zero() {
        return false;
    }
    match point {
        ECPoint::Infinity => true,
        ECPoint::Affine { x, y } => {
            let lhs = modp(&(y * y), p);
            let rhs = modp(&(x * x * x + a * x + b), p);
            lhs == rhs
        }
    }
}

fn validate_point_on_curve(point: &ECPoint, a: &BigInt, b: &BigInt, p: &BigInt) -> Result<()> {
    if !is_on_curve(point, a, b, p) {
        anyhow::bail!(
            "Point {} is not on the curve y^2 = x^3 + {}x + {} (mod {})",
            format_point(point),
            a,
            b,
            p
        );
    }
    Ok(())
}

// Modular inverse for BigInt. Returns a^-1 mod p.
fn mod_inverse(a: &BigInt, p: &BigInt) -> Result<BigInt> {
    let ext = a.extended_gcd(p);
    if ext.gcd != BigInt::one() {
        anyhow::bail!("Modular inverse does not exist");
    }
    Ok(((ext.x % p) + p) % p)
}

// Reduce a BigInt modulo p into the range [0, p).
fn modp(a: &BigInt, p: &BigInt) -> BigInt {
    ((a % p) + p) % p
}

// Add two points on the elliptic curve y^2 = x^3 + ax + b (mod p).
// Parameter b is not needed for point addition.
pub fn point_add(p1: &ECPoint, p2: &ECPoint, a: &BigInt, p: &BigInt) -> Result<ECPoint> {
    if p.is_zero() {
        anyhow::bail!("Modulus p must be non-zero");
    }
    match (p1, p2) {
        (ECPoint::Infinity, _) => Ok(p2.clone()),
        (_, ECPoint::Infinity) => Ok(p1.clone()),
        (ECPoint::Affine { x: x1, y: y1 }, ECPoint::Affine { x: x2, y: y2 }) => {
            let x1m = modp(x1, p);
            let y1m = modp(y1, p);
            let x2m = modp(x2, p);
            let y2m = modp(y2, p);

            // If P = -Q (same x, opposite y), result is the point at infinity
            if x1m == x2m && modp(&(&y1m + &y2m), p).is_zero() {
                return Ok(ECPoint::Infinity);
            }

            let lambda = if x1m == x2m && y1m == y2m {
                // Point doubling: lambda = (3*x1^2 + a) / (2*y1) mod p
                let num = modp(&(BigInt::from(3) * &x1m * &x1m + a), p);
                let den = modp(&(BigInt::from(2) * &y1m), p);
                if den.is_zero() {
                    return Ok(ECPoint::Infinity);
                }
                let den_inv = mod_inverse(&den, p)?;
                modp(&(num * den_inv), p)
            } else {
                // Point addition: lambda = (y2 - y1) / (x2 - x1) mod p
                let num = modp(&(&y2m - &y1m), p);
                let den = modp(&(&x2m - &x1m), p);
                let den_inv = mod_inverse(&den, p)?;
                modp(&(num * den_inv), p)
            };

            let x3 = modp(&(&lambda * &lambda - &x1m - &x2m), p);
            let y3 = modp(&(&lambda * &(&x1m - &x3) - &y1m), p);
            Ok(ECPoint::Affine { x: x3, y: y3 })
        }
    }
}

// Scalar multiplication using double-and-add (left-to-right binary method).
pub fn scalar_multiply(point: &ECPoint, n: &BigUint, a: &BigInt, p: &BigInt) -> Result<ECPoint> {
    if p.is_zero() {
        anyhow::bail!("Modulus p must be non-zero");
    }
    if n.is_zero() {
        return Ok(ECPoint::Infinity);
    }
    if *point == ECPoint::Infinity {
        return Ok(ECPoint::Infinity);
    }

    let mut result = ECPoint::Infinity;
    let bit_len = n.bits();
    for i in (0..bit_len).rev() {
        result = point_add(&result, &result, a, p)?;
        if n.bit(i) {
            result = point_add(&result, point, a, p)?;
        }
    }

    Ok(result)
}

// Find the order of a point on the curve by brute force.
// Iterates from 1 until n*P = O. Only practical for small orders.
pub fn point_order(point: &ECPoint, a: &BigInt, p: &BigInt) -> Result<BigUint> {
    if p.is_zero() {
        anyhow::bail!("Modulus p must be non-zero");
    }
    if *point == ECPoint::Infinity {
        return Ok(BigUint::one());
    }

    let p_uint: BigUint = p.to_biguint().context("Prime p must be positive")?;
    // Hasse's theorem: group order is at most p + 1 + 2*sqrt(p)
    let max_order = &p_uint + BigUint::one() + BigUint::from(2u32) * p_uint.sqrt();

    let mut current = point.clone();
    let mut n = BigUint::one();

    loop {
        if n > max_order {
            anyhow::bail!("Could not find point order within Hasse bound");
        }
        if current == ECPoint::Infinity {
            return Ok(n);
        }
        current = point_add(&current, point, a, p)?;
        n += BigUint::one();
    }
}

// Baby-step Giant-step algorithm for ECDLP in a subgroup of known prime order.
// Solves Q = k*P for k, where P has order `order`.
fn bsgs_ecdlp(
    generator: &ECPoint,
    target: &ECPoint,
    a: &BigInt,
    p: &BigInt,
    order: &BigUint,
) -> Result<BigUint> {
    let m_val = order.sqrt() + BigUint::one();

    // Baby step: store j -> j*G for j in [0, m)
    let mut table: HashMap<String, BigUint> = HashMap::new();
    let mut baby = ECPoint::Infinity;
    let mut j = BigUint::zero();
    while j < m_val {
        table.insert(format_point(&baby), j.clone());
        baby = point_add(&baby, generator, a, p)?;
        j += BigUint::one();
    }

    // Giant step: compute Q - i*m*G for i in [0, m)
    // m*G precomputed
    let mg = scalar_multiply(generator, &m_val, a, p)?;
    // Negate m*G for subtraction
    let neg_mg = negate_point(&mg, p);

    let mut gamma = target.clone();
    let mut i = BigUint::zero();
    while i < m_val {
        if let Some(j_val) = table.get(&format_point(&gamma)) {
            // k = i*m + j mod order
            let k = (&i * &m_val + j_val) % order;
            return Ok(k);
        }
        gamma = point_add(&gamma, &neg_mg, a, p)?;
        i += BigUint::one();
    }

    anyhow::bail!("BSGS failed to find discrete log");
}

// Negate a point: -(x, y) = (x, -y mod p).
fn negate_point(point: &ECPoint, p: &BigInt) -> ECPoint {
    match point {
        ECPoint::Infinity => ECPoint::Infinity,
        ECPoint::Affine { x, y } => ECPoint::Affine {
            x: x.clone(),
            y: modp(&(-y), p),
        },
    }
}

// Pohlig-Hellman attack for solving ECDLP when the group order is smooth.
// Given Q = n*P with P having the given order, finds n.
pub fn pohlig_hellman(
    generator: &ECPoint,
    target: &ECPoint,
    a: &BigInt,
    p: &BigInt,
    order: &BigUint,
) -> Result<BigUint> {
    if p.is_zero() {
        anyhow::bail!("Modulus p must be non-zero");
    }
    let factors = factor_biguint(order)?;

    if factors.is_empty() {
        anyhow::bail!("Order must be > 1");
    }

    let mut residues: Vec<BigUint> = Vec::new();
    let mut moduli: Vec<BigUint> = Vec::new();

    for (prime, exp) in &factors {
        let prime_power = prime.pow(*exp);
        // Compute cofactor = order / prime_power
        let cofactor = order / &prime_power;

        // Project to subgroup of order prime_power
        let gen_sub = scalar_multiply(generator, &cofactor, a, p)?;
        let target_sub = scalar_multiply(target, &cofactor, a, p)?;

        // Solve in subgroup of order prime^exp
        // For prime powers, solve digit by digit
        let sub_log = solve_prime_power_ecdlp(&gen_sub, &target_sub, a, p, prime, *exp)?;
        residues.push(sub_log);
        moduli.push(prime_power);
    }

    // Combine using CRT
    crt(&residues, &moduli)
}

// Solve ECDLP in a subgroup of order prime^exp using successive refinement.
fn solve_prime_power_ecdlp(
    generator: &ECPoint,
    target: &ECPoint,
    a: &BigInt,
    p: &BigInt,
    prime: &BigUint,
    exp: u32,
) -> Result<BigUint> {
    if exp == 1 {
        return bsgs_ecdlp(generator, target, a, p, prime);
    }

    // For prime^exp, solve one digit at a time.
    // k = d0 + d1*prime + d2*prime^2 + ... + d_{exp-1}*prime^{exp-1}
    let prime_power = prime.pow(exp);
    let mut k = BigUint::zero();
    let mut remainder = target.clone();

    // g_base = (prime^(exp-1)) * gen, which has order prime
    let base_cofactor = prime.pow(exp - 1);
    let g_base = scalar_multiply(generator, &base_cofactor, a, p)?;

    for i in 0..exp {
        // cofactor = prime^(exp-1-i)
        let cofactor = prime.pow(exp - 1 - i);
        // Project remainder to subgroup of order prime
        let projected = scalar_multiply(&remainder, &cofactor, a, p)?;

        // Solve projected = d_i * g_base in subgroup of order prime
        let d_i = bsgs_ecdlp(&g_base, &projected, a, p, prime)?;

        // k += d_i * prime^i
        let contrib = &d_i * &prime.pow(i);
        k = (&k + &contrib) % &prime_power;

        // Update remainder: remainder = target - k*gen
        let neg_k_gen = negate_point(&scalar_multiply(generator, &k, a, p)?, p);
        remainder = point_add(target, &neg_k_gen, a, p)?;
    }

    Ok(k)
}

// Factor a BigUint into prime factors with exponents.
// Uses optimized Pollard's Rho for numbers fitting in u128, and naive trial division for larger.
fn factor_biguint(n: &BigUint) -> Result<Vec<(BigUint, u32)>> {
    if n <= &BigUint::one() {
        return Ok(Vec::new());
    }

    // Optimization: if n fits in u128, use the much faster implementation from primes module
    // which uses Pollard's Rho algorithm instead of trial division.
    if let Some(n_u128) = n.to_u128() {
        let factors = primes::factorize(n_u128);
        return Ok(factors
            .into_iter()
            .map(|(p, e)| (BigUint::from(p), e))
            .collect());
    }

    // Fallback: Naive trial division for numbers > u128::MAX
    // This is slow but unavoidable without a BigUint Pollard's Rho implementation.
    let mut n = n.clone();
    let mut factors = Vec::new();

    let mut d = BigUint::from(2u32);
    while &d * &d <= n {
        let mut exp = 0u32;
        while (&n % &d).is_zero() {
            n /= &d;
            exp += 1;
        }
        if exp > 0 {
            factors.push((d.clone(), exp));
        }
        d += BigUint::one();
    }
    if n > BigUint::one() {
        factors.push((n, 1));
    }

    Ok(factors)
}

// Chinese Remainder Theorem for BigUint.
// Given residues[i] and pairwise coprime moduli[i], finds x such that
// x ≡ residues[i] (mod moduli[i]) for all i.
fn crt(residues: &[BigUint], moduli: &[BigUint]) -> Result<BigUint> {
    if residues.is_empty() {
        anyhow::bail!("CRT requires at least one congruence");
    }

    let big_m: BigUint = moduli.iter().fold(BigUint::one(), |acc, m| acc * m);

    let mut x = BigInt::zero();
    let big_m_int = big_m.to_bigint().unwrap();

    for (r_i, m_i) in residues.iter().zip(moduli.iter()) {
        let mi_big = &big_m / m_i;
        let mi_int = mi_big.to_bigint().unwrap();
        let m_i_int = m_i.to_bigint().unwrap();
        let r_i_int = r_i.to_bigint().unwrap();

        let ext = mi_int.extended_gcd(&m_i_int);
        if ext.gcd != BigInt::one() {
            anyhow::bail!("Moduli must be pairwise coprime for CRT");
        }
        let yi = ext.x;

        x = (x + r_i_int * mi_int * yi) % &big_m_int;
    }

    Ok(((x + &big_m_int) % &big_m_int).to_biguint().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_add_identity() {
        let p = BigInt::from(23);
        let a = BigInt::from(1);
        let pt = ECPoint::Affine {
            x: BigInt::from(0),
            y: BigInt::from(1),
        };
        let result = point_add(&pt, &ECPoint::Infinity, &a, &p).unwrap();
        assert_eq!(result, pt);
    }

    #[test]
    fn test_parse_point_inf() {
        let pt = parse_point("inf").unwrap();
        assert_eq!(pt, ECPoint::Infinity);
    }

    #[test]
    fn test_factor_biguint_large_prime() {
        // Test with a large 64-bit prime: 18446744073709551557 (largest u64 prime)
        // This would take very long with trial division but should be instant with Pollard's Rho.
        let n = BigUint::from(18446744073709551557u64);
        let factors = factor_biguint(&n).unwrap();
        assert_eq!(factors.len(), 1);
        assert_eq!(factors[0].0, n);
        assert_eq!(factors[0].1, 1);
    }
}
