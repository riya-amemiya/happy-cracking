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
