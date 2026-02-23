#![allow(clippy::manual_is_multiple_of)]
use anyhow::Result;
use clap::Subcommand;

use crate::crypto::mathtools::{Montgomery, Montgomery64};

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

    // 1. Remove small factors (2, 3, 5, 7, 11, 13) using trial division
    // This is cheap and effective, and helps Pollard's Rho.
    for &p in &[2, 3, 5, 7, 11, 13] {
        while n % p == 0 {
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
            while temp_n % d == 0 {
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
    if n % 2 == 0 {
        return false;
    }

    let bases = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
    ];

    // Find d, s such that n - 1 = d * 2^s
    let mut d = n - 1;
    let mut s = 0;
    while d % 2 == 0 {
        d >>= 1;
        s += 1;
    }

    let mont = Montgomery::new(n);
    let one_mont = mont.transform(1);
    let n_minus_1_mont = mont.transform(n - 1);

    'base_loop: for &a in &bases {
        if n <= a {
            break;
        }

        let a_mont = mont.transform(a);
        let mut x = mont.pow(a_mont, d);

        if x == one_mont || x == n_minus_1_mont {
            continue;
        }

        for _ in 0..s - 1 {
            x = mont.mul(x, x);
            if x == n_minus_1_mont {
                continue 'base_loop;
            }
        }

        return false;
    }

    true
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
    if n % 2 == 0 {
        return false;
    }

    let bases = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];

    let d_init = n - 1;
    let s = d_init.trailing_zeros();
    let d = d_init >> s;

    let mont = Montgomery64::new(n);
    let one_mont = mont.transform(1);
    let n_minus_1_mont = mont.transform(n - 1);

    for &a in &bases {
        if n <= a {
            break;
        }

        let a_mont = mont.transform(a);
        let mut x = mont.pow(a_mont, d as u128);

        if x == one_mont || x == n_minus_1_mont {
            continue;
        }

        let mut composite = true;
        for _ in 0..s - 1 {
            x = mont.mul(x, x);
            if x == n_minus_1_mont {
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

// Binary GCD algorithm for u128
fn binary_gcd(mut u: u128, mut v: u128) -> u128 {
    if u == 0 {
        return v;
    }
    if v == 0 {
        return u;
    }
    let shift = (u | v).trailing_zeros();
    u >>= u.trailing_zeros();
    loop {
        v >>= v.trailing_zeros();
        if u > v {
            std::mem::swap(&mut u, &mut v);
        }
        v -= u;
        if v == 0 {
            break;
        }
    }
    u << shift
}

// Pollard's Rho algorithm using Brent's cycle detection variant with batch GCD.
pub fn pollard_rho(n: u128) -> u128 {
    if n % 2 == 0 {
        return 2;
    }

    let mont = Montgomery::new(n);
    let one = mont.transform(1);

    // Try different constants c if the first one fails
    for c_val in [1, 3, 5, 7, 2, 4, 6, 8] {
        let c = mont.transform(c_val);

        let f = |x: u128| -> u128 {
            let x2 = mont.mul(x, x);
            // x^2 + c
            let (sum, carry) = x2.overflowing_add(c);
            if carry || sum >= n {
                sum.wrapping_sub(n)
            } else {
                sum
            }
        };

        // Brent's variant: only update y at powers of 2
        let mut y = mont.transform(2);
        let mut x = y;
        let mut q = one;
        let mut r: u128 = 1;
        let mut d: u128 = 1;
        let mut ys = y;

        while d == 1 {
            x = y;
            for _ in 0..r {
                y = f(y);
            }

            let mut k: u128 = 0;
            while k < r && d == 1 {
                ys = y;
                // Batch GCD: accumulate product of differences over ~100 iterations
                let batch_end = (k + 100).min(r);
                for _ in k..batch_end {
                    y = f(y);
                    q = mont.mul(q, x.abs_diff(y));
                }
                d = binary_gcd(q, n);
                k = batch_end;
            }
            r *= 2;
        }

        if d == n {
            // Backtrack: retry without batching from ys
            loop {
                ys = f(ys);
                d = binary_gcd(x.abs_diff(ys), n);
                if d != 1 {
                    break;
                }
            }
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
