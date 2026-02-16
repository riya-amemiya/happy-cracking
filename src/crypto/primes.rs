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

    // 1. Remove small factors (2, 3, 5, 7, 11, 13) using trial division
    // This is cheap and effective, and helps Pollard's Rho.
    for &p in &[2, 3, 5, 7, 11, 13] {
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

struct Montgomery {
    n: u128,
    neg_inv_n: u128, // -n^{-1} mod 2^128
    r2: u128,        // R^2 mod n, where R = 2^128
}

impl Montgomery {
    fn new(n: u128) -> Self {
        // n must be odd
        debug_assert!(!n.is_multiple_of(2));

        // Compute -n^{-1} mod 2^128 using Newton's method
        // x_0 = 1 works because n is odd (n * 1 = 1 mod 2)? No, n must be 1 mod 2.
        // If n is odd, n * x = 1 mod 2 => x=1.
        // Iterate: x = x * (2 - n*x)
        let mut inv = 1u128;
        for _ in 0..7 {
            inv = inv.wrapping_mul(2u128.wrapping_sub(n.wrapping_mul(inv)));
        }
        let neg_inv_n = inv.wrapping_neg();

        // Compute R^2 mod n
        // R = 2^128. R mod n = (2^128 - n) % n = (-n) % n.
        // Wait. 2^128 is 0 in u128 wrapping (mod 2^128).
        // But we want R mod n.
        // R = 2^128.
        // calculating R^2 mod n requires BigUint usually, or double-width arithmetic.
        // R % n = (2^128) % n.
        // = (u128::MAX - n + 1) % n.
        // = ( (u128::MAX % n) + (1 % n) ) % n? No.
        // u128::MAX + 1 = 2^128.
        // 2^128 % n = ((u128::MAX % n) + 1) % n.

        let r_mod_n = (u128::MAX % n).wrapping_add(1) % n;

        // R^2 mod n = (R mod n)^2 mod n
        let r2 = mul_mod(r_mod_n, r_mod_n, n);

        Montgomery { n, neg_inv_n, r2 }
    }

    // Computes a * b * R^-1 mod n
    fn mul(&self, a: u128, b: u128) -> u128 {
        let (hi, lo) = widening_mul_u128(a, b);

        // m = T * n' mod R => lo * neg_inv_n
        let m = lo.wrapping_mul(self.neg_inv_n);

        // t = (T + m*n) / R
        // We only need the high part of (T + m*n)
        // T = hi * 2^128 + lo
        // m*n = m_hi * 2^128 + m_lo
        // lo + m_lo = lo + (lo * neg_inv_n * n)_lo = lo + (lo * -1)_lo = 0 mod 2^128?
        // Yes, m*n = -lo mod 2^128. So lo + m_lo = 0 mod 2^128.
        // So the carry from lo + m_lo is what matters.
        // But since lower 128 bits are zero, we just compute (hi + m_hi + carry).

        // We know lo + m_lo overflows u128 iff lo != 0 (since m_lo = -lo).
        // Actually m_lo = lo.wrapping_mul(self.neg_inv_n).wrapping_mul(self.n) = lo * (-1) = -lo.
        // lo + (-lo) = 0 (mod 2^128).
        // If lo != 0, carry is 1. If lo == 0, carry is 0.
        let carry_lo = if lo != 0 { 1 } else { 0 };

        // Wait, I need high part of m*n too.
        let (m_hi, _) = widening_mul_u128(m, self.n);

        // Result = hi + m_hi + carry_lo
        let (res, carry) = hi.overflowing_add(m_hi);
        let (res, carry2) = res.overflowing_add(carry_lo);

        // If there was an overflow in the high part sum, the result exceeds 2^128?
        // Wait. The true sum (T + m*n)/R can be up to 2n.
        // So res fits in u128 if < 2^128?
        // If carry happened, it means result >= 2^128.
        // Since result < 2n < 2*2^128. The carry is at most 1 bit.

        let mut t = res;
        // If overflow (carry or carry2), we have an extra 2^128.
        // In that case t is definitely >= n (since n < 2^128).
        // Actually, the result is t + carry*2^128.
        // We want (t + carry*2^128) mod n.
        // If t >= n, subtract n.
        // If carry, we definitely subtract n?
        // The result of Redc is bounded by 2n.
        // So if result >= n, subtract n.
        // If carry, result >= 2^128 > n. So subtract n.

        if carry || carry2 {
             t = t.wrapping_sub(self.n);
        }

        if t >= self.n {
            t -= self.n;
        }

        t
    }

    // Converts a to Montgomery form: a * R mod n
    fn transform(&self, a: u128) -> u128 {
        self.mul(a, self.r2)
    }

    // Converts back from Montgomery form: a * R^-1 mod n
    // transform(1) = 1 * R mod n = R mod n
    // mul(a, 1) = a * 1 * R^-1 mod n = a * R^-1 mod n.
    #[allow(dead_code)]
    fn reduce(&self, a: u128) -> u128 {
        self.mul(a, 1)
    }
}

// 128-bit widening multiplication: returns (hi, lo) such that a * b = hi * 2^128 + lo
fn widening_mul_u128(a: u128, b: u128) -> (u128, u128) {
    let al = a as u64;
    let ah = (a >> 64) as u64;
    let bl = b as u64;
    let bh = (b >> 64) as u64;

    let p0 = (al as u128) * (bl as u128);
    let p1 = (al as u128) * (bh as u128);
    let p2 = (ah as u128) * (bl as u128);
    let p3 = (ah as u128) * (bh as u128);

    let (mid, carry_mid) = p1.overflowing_add(p2);

    let (lo, carry_lo) = p0.overflowing_add(mid << 64);

    let mut hi = p3 + (mid >> 64);
    if carry_mid {
        hi += 1u128 << 64;
    }
    if carry_lo {
        hi += 1;
    }

    (hi, lo)
}

// Pollard's Rho algorithm using Brent's cycle detection variant with batch GCD.
fn pollard_rho(n: u128) -> u128 {
    if n.is_multiple_of(2) {
        return 2;
    }

    // Optimize for u128 using Montgomery multiplication
    let mont = if n <= u64::MAX as u128 {
        None // Use standard arithmetic for small n? Or just use mont anyway?
        // For u64, we can use u128 arithmetic directly without Montgomery overhead.
        // But pollard_rho implementation below is unified.
        // Let's use Montgomery only for > u64::MAX to match current optimization level?
        // Actually Montgomery for u128 is fast enough for u64 too, but standard % is faster if hardware supports it.
        // Let's stick to using Montgomery for all u128 inputs in this function for simplicity and speed on large inputs.
    } else {
        Some(Montgomery::new(n))
    };

    let one_mont = if let Some(m) = &mont { m.transform(1) } else { 1 };

    // Try different constants c if the first one fails
    for c_val in [1, 3, 5, 7, 2, 4, 6, 8] {
        let mont_c = if let Some(m) = &mont {
            m.transform(c_val)
        } else {
            c_val
        };

        // If using Montgomery, x, y are in Montgomery form.
        // f(x) = x^2 + c
        let f = |x: u128| -> u128 {
            if let Some(m) = &mont {
                let x2 = m.mul(x, x);
                // (x^2 + c) mod n
                let (sum, overflow) = x2.overflowing_add(mont_c);
                if overflow || sum >= n { sum.wrapping_sub(n) } else { sum }
            } else {
                let x2 = mul_mod(x, x, n); // mul_mod handles u64 fast path
                (x2 + c_val) % n
            }
        };

        let mut y: u128 = mont.as_ref().map_or(2, |m| m.transform(2));
        let mut q: u128 = if mont.is_some() { one_mont } else { 1 };
        let mut r: u128 = 1;
        let mut d: u128 = 1;
        let mut x: u128 = 0; // initialized in loop, but set to 0 to satisfy compiler
        let mut ys: u128 = 0;

        while d == 1 {
            x = y;
            for _ in 0..r {
                y = f(y);
            }

            let mut k: u128 = 0;
            while k < r && d == 1 {
                ys = y;
                let batch_end = (k + 100).min(r);
                for _ in k..batch_end {
                    y = f(y);
                    // q = q * |x - y| mod n
                    let abs_diff = x.abs_diff(y);
                    if let Some(m) = &mont {
                        // abs_diff is in Montgomery form? Yes, x and y are.
                        // Wait. |x - y| is difference of Montgomery forms.
                        // (xR - yR) = (x-y)R.
                        // So abs_diff IS in Montgomery form of (x-y).
                        q = m.mul(q, abs_diff);
                    } else {
                        q = mul_mod(q, abs_diff, n);
                    }
                }

                // q is in Montgomery form. GCD(q, n) works?
                // GCD(qR mod n, n) = GCD(qR, n).
                // If GCD(n, R) = 1 (which it is, since n is odd), then GCD(qR, n) = GCD(q, n).
                // So we don't need to reduce q!
                d = binary_gcd(q, n);
                k = batch_end;
            }
            r *= 2;
        }

        if d == n {
            loop {
                ys = f(ys);
                let abs_diff = x.abs_diff(ys); // Montgomery diff
                // GCD works on Montgomery form directly
                d = binary_gcd(abs_diff, n);
                if d != 1 {
                    break;
                }
            }
        }

        if d != n {
            return d;
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_montgomery() {
        let n = 123456789123456789u128; // Odd number
        let mont = Montgomery::new(n);

        // Test mul: 10 * 20 = 200
        let a = mont.transform(10);
        let b = mont.transform(20);
        let c = mont.mul(a, b);
        let res = mont.reduce(c);
        assert_eq!(res, 200);

        // Test with large numbers
        let a = mont.transform(n - 1);
        let b = mont.transform(2);
        let c = mont.mul(a, b);
        let res = mont.reduce(c);
        assert_eq!(res, n - 2);
    }
}
