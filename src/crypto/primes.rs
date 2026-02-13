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

// Factorize a number into prime factors.
// Uses a hybrid approach: trial division for small factors + Pollard's Rho for large composites.
pub fn factorize(mut n: u128) -> Vec<(u128, u32)> {
    let mut factors_list = Vec::new();

    if n <= 1 {
        return Vec::new();
    }

    // 1. Remove small factors (2, 3, 5) using trial division
    // This is cheap and effective, and helps Pollard's Rho.
    for &p in &[2, 3, 5] {
        while n.is_multiple_of(p) {
            factors_list.push(p);
            n /= p;
        }
    }

    // 2. Recursively factorize the rest
    if n > 1 {
        factor_recursive(n, &mut factors_list);
    }

    factors_list.sort();

    // 3. Group by prime to count exponents
    let mut result = Vec::new();
    if factors_list.is_empty() {
        return result;
    }

    let mut current_p = factors_list[0];
    let mut current_count = 1;

    for &p in &factors_list[1..] {
        if p == current_p {
            current_count += 1;
        } else {
            result.push((current_p, current_count));
            current_p = p;
            current_count = 1;
        }
    }
    result.push((current_p, current_count));

    result
}

fn factor_recursive(n: u128, factors: &mut Vec<u128>) {
    if n == 1 {
        return;
    }

    // For very small numbers, trial division is faster than Miller-Rabin + Pollard's Rho
    if n < 1_000 {
        let mut temp_n = n;
        let mut d = 2;
        while d * d <= temp_n {
            while temp_n.is_multiple_of(d) {
                factors.push(d);
                temp_n /= d;
            }
            d += 1;
        }
        if temp_n > 1 {
            factors.push(temp_n);
        }
        return;
    }

    if is_prime(n) {
        factors.push(n);
        return;
    }

    let divisor = pollard_rho(n);
    if divisor == n {
        // Failed to find a factor. Push n to avoid infinite recursion.
        factors.push(n);
    } else {
        factor_recursive(divisor, factors);
        factor_recursive(n / divisor, factors);
    }
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

// Modular multiplication: (a * b) % m
fn mul_mod(a: u128, b: u128, m: u128) -> u128 {
    if m <= u64::MAX as u128 {
        // Optimization: if m fits in u64, we can use u128 arithmetic directly
        (a * b) % m
    } else {
        // For larger numbers, we use BigUint to avoid overflow
        let a_big = BigUint::from(a);
        let b_big = BigUint::from(b);
        let m_big = BigUint::from(m);
        ((a_big * b_big) % m_big).try_into().unwrap()
    }
}

// Pollard's Rho algorithm for finding a factor of a composite number.
fn pollard_rho(n: u128) -> u128 {
    if n.is_multiple_of(2) {
        return 2;
    }

    // Try different constants c if the first one fails
    for c in [1, 3, 5, 7, 2, 4, 6, 8] {
        let mut x: u128 = 2;
        let mut y: u128 = 2;
        let mut d: u128 = 1;

        let f = |x: u128| -> u128 {
            let x2 = mul_mod(x, x, n);
            (x2 + c) % n
        };

        while d == 1 {
            x = f(x);
            y = f(f(y));

            let abs_diff = x.abs_diff(y);
            d = num_integer::gcd(abs_diff, n);
        }

        if d != n {
            return d;
        }
    }

    // If all fail, return n (factorization failed)
    n
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
