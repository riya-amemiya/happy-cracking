use anyhow::Result;
use clap::Subcommand;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::One;

#[derive(Subcommand)]
pub enum PrimesAction {
    #[command(about = "Factorize a number into prime factors")]
    Factorize {
        #[arg(help = "Number to factorize")]
        n: String,
    },
    #[command(about = "Check if a number is prime")]
    Isprime {
        #[arg(help = "Number to test")]
        n: String,
    },
}

pub fn run(action: PrimesAction) -> Result<()> {
    match action {
        PrimesAction::Factorize { n } => {
            let n: u128 = n.parse().map_err(|_| anyhow::anyhow!("Invalid number"))?;
            let factors = factorize(n);
            println!("{}", format_factors(&factors));
        }
        PrimesAction::Isprime { n } => {
            let n: u128 = n.parse().map_err(|_| anyhow::anyhow!("Invalid number"))?;
            if is_prime(n) {
                println!("{} is prime", n);
            } else {
                println!("{} is not prime", n);
            }
        }
    }
    Ok(())
}

// Trial division for prime factorization.
// Returns a list of (prime, exponent) pairs.
pub fn factorize(mut n: u128) -> Vec<(u128, u32)> {
    let mut factors = Vec::new();

    if n <= 1 {
        return factors;
    }

    // Optimization: Check if n is prime immediately.
    // This prevents hanging on large primes (DoS protection).
    if is_prime(n) {
        factors.push((n, 1));
        return factors;
    }

    // Optimization: Handle 2 separately to skip all even numbers in the loop
    if n.is_multiple_of(2) {
        let mut count = 0_u32;
        while n.is_multiple_of(2) {
            count += 1;
            n /= 2;
        }
        factors.push((2, count));
    }

    // Optimization: Handle 3 separately to skip multiples of 3
    if n.is_multiple_of(3) {
        let mut count = 0_u32;
        while n.is_multiple_of(3) {
            count += 1;
            n /= 3;
        }
        factors.push((3, count));
    }

    let mut d = 5_u128;
    let mut step = 2_u128;
    while d * d <= n {
        let mut count = 0_u32;
        while n.is_multiple_of(d) {
            count += 1;
            n /= d;
        }
        if count > 0 {
            factors.push((d, count));
            // If the remaining number is prime, we are done.
            if n > 1 && is_prime(n) {
                factors.push((n, 1));
                return factors;
            }
        }

        d += step;
        step = 6 - step;
    }

    if n > 1 {
        factors.push((n, 1));
    }

    factors
}

// Deterministic Miller-Rabin primality test for u128.
// Bases: 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71.
fn miller_rabin(n: u128) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }

    let bases = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
    ];

    let n_big = BigUint::from(n);
    let one = BigUint::one();
    let n_minus_1 = &n_big - &one;

    // Find d, s such that n - 1 = d * 2^s
    let mut d = n_minus_1.clone();
    let mut s = 0;
    while d.is_even() {
        d >>= 1;
        s += 1;
    }

    'base_loop: for &a in &bases {
        if n <= a {
            break;
        }

        let a_big = BigUint::from(a);
        let mut x = a_big.modpow(&d, &n_big);

        if x == one || x == n_minus_1 {
            continue;
        }

        for _ in 0..s - 1 {
            x = x.modpow(&BigUint::from(2u32), &n_big);
            if x == n_minus_1 {
                continue 'base_loop;
            }
        }

        return false;
    }

    true
}

// Optimized modular exponentiation for u64 using u128 arithmetic.
fn mod_pow_u64(base: u64, mut exp: u64, modulus: u64) -> u64 {
    if modulus == 1 {
        return 0;
    }
    let mut result: u128 = 1;
    let mut base_u128 = (base as u128) % (modulus as u128);
    let modulus_u128 = modulus as u128;

    while exp > 0 {
        if exp % 2 == 1 {
            result = (result * base_u128) % modulus_u128;
        }
        base_u128 = (base_u128 * base_u128) % modulus_u128;
        exp /= 2;
    }
    result as u64
}

// Optimized Miller-Rabin for u64 using u128 arithmetic.
// Bases: 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37.
fn miller_rabin_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }

    let bases = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

    let d_init = n - 1;
    let s = d_init.trailing_zeros();
    let d = d_init >> s;

    for &a in &bases {
        if n <= a {
            break;
        }

        let mut x = mod_pow_u64(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }

        let mut composite = true;
        for _ in 0..s - 1 {
            x = ((x as u128 * x as u128) % (n as u128)) as u64;
            if x == n - 1 {
                composite = false;
                break;
            }
        }
        if composite {
            return false;
        }
    }
    true
}

pub fn is_prime(n: u128) -> bool {
    if n <= u64::MAX as u128 {
        miller_rabin_u64(n as u64)
    } else {
        miller_rabin(n)
    }
}

// Format factorization result as "2^2 × 3 × 7".
pub fn format_factors(factors: &[(u128, u32)]) -> String {
    if factors.is_empty() {
        return "1".to_string();
    }

    factors
        .iter()
        .map(|(p, e)| {
            if *e == 1 {
                p.to_string()
            } else {
                format!("{}^{}", p, e)
            }
        })
        .collect::<Vec<_>>()
        .join(" × ")
}
