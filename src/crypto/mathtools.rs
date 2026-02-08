use anyhow::{Context, Result};
use clap::Subcommand;

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
            println!("{}", lcm(a, b));
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
            println!("{}", modpow(base, exp, m));
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
pub fn lcm(a: u128, b: u128) -> u128 {
    if a == 0 || b == 0 {
        return 0;
    }
    a / gcd(a, b) * b
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

    let (mut old_r, mut r) = (a as i128, m as i128);
    let (mut old_s, mut s) = (1_i128, 0_i128);

    while r != 0 {
        let q = old_r / r;
        let tmp_r = r;
        r = old_r - q * r;
        old_r = tmp_r;
        let tmp_s = s;
        s = old_s - q * s;
        old_s = tmp_s;
    }

    if old_r != 1 {
        anyhow::bail!(
            "Modular inverse does not exist (gcd({}, {}) = {})",
            a,
            m,
            old_r
        );
    }

    Ok(((old_s % m as i128 + m as i128) % m as i128) as u128)
}

// Binary exponentiation for modular power.
// Computes base^exp mod m.
pub fn modpow(mut base: u128, mut exp: u128, m: u128) -> u128 {
    if m == 1 {
        return 0;
    }
    let mut result = 1_u128;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result * base) % m;
        }
        exp >>= 1;
        base = (base * base) % m;
    }
    result
}
