#![allow(clippy::manual_is_multiple_of)]
use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use num_traits::One;

#[derive(Subcommand)]
pub enum MathAction {
    #[command(about = "Calculate greatest common divisor")]
    Gcd {
        #[arg(help = "First number")]
        a: String,
        #[arg(help = "Second number")]
        b: String,
    },
    #[command(about = "Calculate least common multiple")]
    Lcm {
        #[arg(help = "First number")]
        a: String,
        #[arg(help = "Second number")]
        b: String,
    },
    #[command(about = "Calculate modular inverse (a^-1 mod m)")]
    Modinv {
        #[arg(help = "Value")]
        a: String,
        #[arg(help = "Modulus")]
        m: String,
    },
    #[command(about = "Calculate modular exponentiation (base^exp mod m)")]
    Modpow {
        #[arg(help = "Base")]
        base: String,
        #[arg(help = "Exponent")]
        exp: String,
        #[arg(help = "Modulus")]
        m: String,
    },
}

pub fn run(action: MathAction) -> Result<()> {
    match action {
        MathAction::Gcd { a, b } => {
            let a = a.parse::<u128>().context("Invalid number for a")?;
            let b = b.parse::<u128>().context("Invalid number for b")?;
            println!("{}", gcd(a, b));
        }
        MathAction::Lcm { a, b } => {
            let a = a.parse::<u128>().context("Invalid number for a")?;
            let b = b.parse::<u128>().context("Invalid number for b")?;
            println!("{}", lcm(a, b)?);
        }
        MathAction::Modinv { a, m } => {
            let a = a.parse::<u128>().context("Invalid number for a")?;
            let m = m.parse::<u128>().context("Invalid number for m")?;
            println!("{}", modinv(a, m)?);
        }
        MathAction::Modpow { base, exp, m } => {
            let base = base.parse::<u128>().context("Invalid number for base")?;
            let exp = exp.parse::<u128>().context("Invalid number for exp")?;
            let m = m.parse::<u128>().context("Invalid number for m")?;
            println!("{}", modpow(base, exp, m)?);
        }
    }
    Ok(())
}

// Euclidean algorithm for greatest common divisor.
pub fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

// Least common multiple via GCD.
pub fn lcm(a: u128, b: u128) -> Result<u128> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    let g = gcd(a, b);
    (a / g)
        .checked_mul(b)
        .ok_or_else(|| anyhow::anyhow!("LCM overflow"))
}

// Extended Euclidean algorithm for modular inverse.
// Returns a^-1 mod m such that (a * result) mod m == 1.
pub fn modinv(a: u128, m: u128) -> Result<u128> {
    if m == 0 {
        anyhow::bail!("Modulus must be non-zero");
    }
    if m == 1 {
        return Ok(0);
    }

    // Use BigInt to prevent overflow when casting to i128,
    // which happens if a or m > i128::MAX (2^127-1).
    let a_big = BigInt::from(a);
    let m_big = BigInt::from(m);

    let extended = a_big.extended_gcd(&m_big);

    if extended.gcd != BigInt::one() {
        anyhow::bail!(
            "Modular inverse does not exist (gcd({}, {}) = {})",
            a,
            m,
            extended.gcd
        );
    }

    // x might be negative, ensure positive result in [0, m)
    let res = (extended.x % &m_big + &m_big) % &m_big;

    // Result fits in u128 because m fits in u128
    Ok(res.try_into().unwrap())
}

// Binary exponentiation for modular power.
// Computes base^exp mod m.
pub fn modpow(base: u128, exp: u128, m: u128) -> Result<u128> {
    if m == 0 {
        anyhow::bail!("Modulus must be non-zero");
    }
    if m == 1 {
        return Ok(0);
    }

    // Optimization: if m fits in u64, use u128 for intermediate calculations
    // to avoid BigUint allocation overhead.
    if m <= u64::MAX as u128 {
        return Ok(modpow_u64(base, exp, m as u64) as u128);
    }

    // Optimization: if m is odd and fits in u128 (but > u64::MAX), use Montgomery
    // to avoid BigUint allocation overhead.
    if m % 2 != 0 {
        let mont = Montgomery::new(m);
        let base_mont = mont.transform(base);
        let res_mont = mont.pow(base_mont, exp);
        let res = mont.reduce_from(res_mont);
        return Ok(res);
    }

    // Use BigUint to prevent overflow during intermediate calculations (base * base)
    let base_big = BigUint::from(base);
    let exp_big = BigUint::from(exp);
    let m_big = BigUint::from(m);

    let res = base_big.modpow(&exp_big, &m_big);

    // Result fits in u128 because res < m <= u128::MAX
    Ok(res.try_into().unwrap())
}

// Optimized modular exponentiation for u64 modulus using u128 arithmetic.
fn modpow_u64(base: u128, mut exp: u128, m: u64) -> u64 {
    // If m is odd, use Montgomery multiplication to avoid slow division in the loop
    if m % 2 != 0 {
        let mont = Montgomery64::new(m);
        // We need base mod m first
        let base_val = (base % (m as u128)) as u64;
        let base_mont = mont.transform(base_val);
        let res_mont = mont.pow(base_mont, exp);
        return mont.reduce_from(res_mont);
    }

    let m_u128 = m as u128;
    let mut res: u128 = 1;
    let mut base = base % m_u128;

    while exp > 0 {
        if exp & 1 == 1 {
            res = (res * base) % m_u128;
        }
        base = (base * base) % m_u128;
        exp >>= 1;
    }
    res as u64
}

#[derive(Debug, Clone, Copy)]
pub struct Montgomery64 {
    m: u64,
    m_prime: u64, // -m^-1 mod 2^64
    r2: u64,      // R^2 mod m
}

impl Montgomery64 {
    pub fn new(m: u64) -> Self {
        debug_assert!(m % 2 != 0, "Modulus must be odd");

        // Calculate -m^-1 mod 2^64 using Newton's method
        // Start with 1. 2^64 has 64 bits.
        // x_{i+1} = x_i * (2 - m * x_i) mod 2^64
        // 6 iterations: 1 -> 2 -> 4 -> 8 -> 16 -> 32 -> 64
        let mut inv = 1u64;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(m.wrapping_mul(inv)));
        }
        let m_prime = 0u64.wrapping_sub(inv);

        // Calculate R^2 mod m where R = 2^64
        // R % m = (2^64) % m = (u64::MAX % m + 1) % m
        let r_mod_m = (u64::MAX % m).wrapping_add(1) % m;
        let r2 = ((r_mod_m as u128 * r_mod_m as u128) % m as u128) as u64;

        Self { m, m_prime, r2 }
    }

    // Montgomery reduction: computes T * R^-1 mod m
    // T is u128 product.
    #[inline(always)]
    pub fn reduce(&self, t: u128) -> u64 {
        let m = self.m;
        let m_prime = self.m_prime;

        // m_factor = (t mod R) * m_prime mod R
        let m_factor = (t as u64).wrapping_mul(m_prime);

        // t_correction = m_factor * m
        let t_correction = (m_factor as u128) * (m as u128);

        // val = t + t_correction
        // Since t < m*m and t_correction < R*m, sum fits in u128?
        // Actually, t < m*R (if reducing product of two numbers < m, then t < m*m < m*R).
        // But t could be larger if not from strict mul.
        // Assuming strict mul (inputs < m), t < m^2.
        // t_correction < R*m.
        // sum < m^2 + R*m < 2*R*m.
        // We divide by R (shift 64). Result < 2m.

        let (val, overflow) = t.overflowing_add(t_correction);

        // res = val / R = val >> 64
        let mut res = (val >> 64) as u64;

        // If val overflowed u128, it means there is a carry to bit 128.
        // Divided by R (2^64), this carry adds 2^64 to the quotient.
        // So real result is res + 2^64.
        // Since we want result mod m, and m < 2^64, we can subtract m.
        // res + 2^64 - m = res + (2^64 - m).
        // In u64 arithmetic, this is res.wrapping_sub(m).
        if overflow {
            res = res.wrapping_sub(m);
        } else if res >= m {
            res -= m;
        }
        res
    }

    #[inline(always)]
    pub fn mul(&self, a: u64, b: u64) -> u64 {
        let prod = (a as u128) * (b as u128);
        self.reduce(prod)
    }

    pub fn transform(&self, a: u64) -> u64 {
        self.mul(a, self.r2)
    }

    #[allow(dead_code)]
    pub fn reduce_from(&self, a: u64) -> u64 {
        // To reduce from Montgomery form X*R, we compute X*R * R^-1 = X.
        // reduce takes u128 T.
        // We pass a as u128.
        self.reduce(a as u128)
    }

    pub fn pow(&self, mut base: u64, mut exp: u128) -> u64 {
        let mut res = self.transform(1);
        while exp > 0 {
            if exp % 2 == 1 {
                res = self.mul(res, base);
            }
            base = self.mul(base, base);
            exp /= 2;
        }
        res
    }
}

// Helper for 128-bit widening multiplication: (a * b) -> (lo, hi)
fn widening_mul_u128(a: u128, b: u128) -> (u128, u128) {
    let mask = 0xFFFF_FFFF_FFFF_FFFF;
    let al = a & mask;
    let ah = a >> 64;
    let bl = b & mask;
    let bh = b >> 64;

    let t0 = al * bl;
    let t1 = al * bh;
    let t2 = ah * bl;
    let t3 = ah * bh;

    let (mid, carry_mid) = t1.overflowing_add(t2);
    // mid represents bits 64..192
    let mid_lo = mid << 64; // bits 64..128
    let mid_hi = mid >> 64; // bits 128..192

    let (lo, carry_lo) = t0.overflowing_add(mid_lo);

    let mut hi = t3 + mid_hi;
    if carry_mid {
        hi += 1 << 64;
    }
    if carry_lo {
        hi += 1;
    }

    (lo, hi)
}

pub struct Montgomery {
    m: u128,
    m_prime: u128, // -m^-1 mod 2^128
    r2: u128,      // R^2 mod m
}

impl Montgomery {
    pub fn new(m: u128) -> Self {
        if m % 2 == 0 {
            panic!("Modulus must be odd for Montgomery arithmetic");
        }

        // Calculate -m^-1 mod 2^128 using Newton's method
        let mut inv = 1u128;
        for _ in 0..7 {
            inv = inv.wrapping_mul(2u128.wrapping_sub(m.wrapping_mul(inv)));
        }
        let m_prime = 0u128.wrapping_sub(inv);

        // Calculate R^2 mod m where R = 2^128
        // We compute R^2 mod m by doubling R mod m 128 times
        let mut r2 = (u128::MAX % m + 1) % m; // R mod m

        for _ in 0..128 {
            // r2 = (r2 * 2) % m
            let (mut val, overflow) = r2.overflowing_shl(1);
            if overflow {
                // val = (r2 * 2) - 2^128
                // We want (r2 * 2) - m = val + 2^128 - m
                val = val.wrapping_add(0u128.wrapping_sub(m));
            } else if val >= m {
                val -= m;
            }
            r2 = val;
        }

        Self { m, m_prime, r2 }
    }

    // Montgomery reduction: computes T * R^-1 mod m
    pub fn reduce(&self, lo: u128, hi: u128) -> u128 {
        let m = self.m;
        let m_prime = self.m_prime;

        let m_factor = lo.wrapping_mul(m_prime);
        let (prod_lo, prod_hi) = widening_mul_u128(m_factor, m);

        let (_, carry_lo) = lo.overflowing_add(prod_lo);
        let (sum, carry1) = hi.overflowing_add(prod_hi);
        let (mut res, carry2) = sum.overflowing_add(if carry_lo { 1 } else { 0 });
        let carry = carry1 || carry2;

        if carry {
            res = res.wrapping_sub(m);
        } else if res >= m {
            res -= m;
        }
        res
    }

    // Multiplication: a * b * R^-1 mod m
    pub fn mul(&self, a: u128, b: u128) -> u128 {
        let (lo, hi) = widening_mul_u128(a, b);
        self.reduce(lo, hi)
    }

    // Transform to Montgomery form: a * R mod m
    pub fn transform(&self, a: u128) -> u128 {
        self.mul(a, self.r2)
    }

    // Transform from Montgomery form: a * R^-1 mod m
    // Only used for final result, so we can pass 0 for high part
    #[allow(dead_code)]
    pub fn reduce_from(&self, a: u128) -> u128 {
        self.reduce(a, 0)
    }

    pub fn pow(&self, mut base: u128, mut exp: u128) -> u128 {
        let mut res = self.transform(1);
        while exp > 0 {
            if exp % 2 == 1 {
                res = self.mul(res, base);
            }
            base = self.mul(base, base);
            exp /= 2;
        }
        res
    }
}
