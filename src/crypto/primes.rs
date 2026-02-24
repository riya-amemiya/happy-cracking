#![allow(clippy::manual_is_multiple_of)]
use anyhow::Result;
use clap::Subcommand;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::{One, ToPrimitive};

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

    let mont = Montgomery::new(n).expect("n is odd");
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

    let mont = Montgomery::new(n).expect("n is odd");
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

// Factorize a BigUint into prime factors.
// Uses optimized Pollard's Rho for numbers fitting in u128, and Pollard's Rho for BigUint for larger.
pub fn factorize_biguint(n: BigUint) -> Vec<(BigUint, u32)> {
    let mut factors_list = Vec::new();
    if n <= BigUint::one() {
        return Vec::new();
    }

    // Optimization: u128
    if let Some(n_u128) = n.to_u128() {
        let factors = factorize(n_u128);
        return factors.into_iter().map(|(p, e)| (BigUint::from(p), e)).collect();
    }

    // Recursive BigUint factorization
    factor_recursive_biguint(n, &mut factors_list);
    factors_list.sort();

    // Group by prime
    let mut result = Vec::new();
    if factors_list.is_empty() { return result; }
    let mut current_p = factors_list[0].clone();
    let mut current_count = 1;
    for p in factors_list.iter().skip(1) {
        if *p == current_p {
            current_count += 1;
        } else {
            result.push((current_p.clone(), current_count));
            current_p = p.clone();
            current_count = 1;
        }
    }
    result.push((current_p, current_count));
    result
}

fn factor_recursive_biguint(n: BigUint, factors: &mut Vec<BigUint>) {
    if n <= BigUint::one() { return; }

    // Fallback to u128 if small enough
    if let Some(n_u128) = n.to_u128() {
         let sub_factors = factorize(n_u128);
         for (p, e) in sub_factors {
             for _ in 0..e {
                 factors.push(BigUint::from(p));
             }
         }
         return;
    }

    // Try Pollard's Rho
    match pollard_rho_biguint(&n) {
        Ok((d, q)) => {
            // Found a factor d. Recurse on d and n/d (q).
            factor_recursive_biguint(d, factors);
            factor_recursive_biguint(q, factors);
        }
        Err(_) => {
            // Failed to factor. Treat as prime/indivisible.
            factors.push(n);
        }
    }
}

// Pollard's Rho factorization for BigUint using Brent's cycle detection variant.
pub fn pollard_rho_biguint(n: &BigUint) -> Result<(BigUint, BigUint)> {
    if n <= &BigUint::one() {
        anyhow::bail!("Cannot factorize n <= 1");
    }

    if n.is_even() {
        let two = BigUint::from(2u32);
        let other = n / &two;
        return Ok((two, other));
    }

    // Optimization: Use u128 implementation if n fits
    if let Some(n_u128) = n.to_u128() {
        let factor = pollard_rho(n_u128);
        if factor == n_u128 || factor == 1 {
            anyhow::bail!("Pollard's Rho failed to factor n");
        }
        let d = BigUint::from(factor);
        let q = n / &d;
        return Ok((d, q));
    }

    // Try multiple starting values
    for c_val in 1u64..20 {
        let c = BigUint::from(c_val);
        if let Some(d) = pollard_rho_brent(n, &c)
            && d > BigUint::one()
            && &d < n
        {
            let q = n / &d;
            return Ok((d, q));
        }
    }

    anyhow::bail!("Pollard's Rho failed to factor n")
}

// Brent's variant of Pollard's Rho for BigUint. Returns a non-trivial factor or None.
fn pollard_rho_brent(n: &BigUint, c: &BigUint) -> Option<BigUint> {
    let f = |x: &BigUint| -> BigUint { (x * x + c) % n };

    let mut y = BigUint::from(2u32);
    let mut r: u64 = 1;
    let mut q = BigUint::one();

    let mut x = y.clone();
    let mut ys = y.clone();
    let mut g = BigUint::one();

    while g == BigUint::one() {
        x = y.clone();

        for _ in 0..r {
            y = f(&y);
        }

        let mut k: u64 = 0;
        while k < r && g == BigUint::one() {
            ys = y.clone();

            let batch_size = std::cmp::min(128, r - k);
            for _ in 0..batch_size {
                y = f(&y);
                let diff = if x > y { &x - &y } else { &y - &x };
                q = (q * diff) % n;
            }

            g = q.gcd(n);
            k += batch_size;
        }

        r *= 2;

        // Safety limit
        if r > 1_000_000 {
            return None;
        }
    }

    if &g == n {
        // Backtrack
        loop {
            ys = f(&ys);
            let diff = if x > ys { &x - &ys } else { &ys - &x };
            g = diff.gcd(n);
            if g > BigUint::one() {
                break;
            }
        }
    }

    if &g == n { None } else { Some(g) }
}
