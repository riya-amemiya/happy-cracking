use std::time::Instant;

fn is_prime_trial(n: u128) -> bool {
    if n < 2 {
        return false;
    }
    if n < 4 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    let mut i = 5_u128;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

fn main() {
    let large_prime = 18446744073709551557; // 2^64 - 59
    println!("Testing 64-bit prime: {}", large_prime);
    let start = Instant::now();
    let result = is_prime_trial(large_prime);
    let duration = start.elapsed();
    println!("Is prime? {}, Time: {:?}", result, duration);

    // This would hang for hours
    // let larger_prime = 170141183460469231731687303715884105727; // 2^127 - 1
    // println!("Testing 128-bit prime: {}", larger_prime);
    // let start = Instant::now();
    // let result = is_prime_trial(larger_prime);
    // let duration = start.elapsed();
    // println!("Is prime? {}, Time: {:?}", result, duration);
}
