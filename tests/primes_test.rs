use happy_cracking::crypto::primes;

#[test]
fn test_factorize_prime() {
    assert_eq!(primes::factorize(7), vec![(7, 1)]);
}

#[test]
fn test_factorize_composite() {
    // 12 = 2^2 * 3
    assert_eq!(primes::factorize(12), vec![(2, 2), (3, 1)]);
}

#[test]
fn test_factorize_power_of_two() {
    assert_eq!(primes::factorize(64), vec![(2, 6)]);
}

#[test]
fn test_factorize_large() {
    // 2310 = 2 * 3 * 5 * 7 * 11
    assert_eq!(
        primes::factorize(2310),
        vec![(2, 1), (3, 1), (5, 1), (7, 1), (11, 1)]
    );
}

#[test]
fn test_factorize_one() {
    assert_eq!(primes::factorize(1), vec![]);
}

#[test]
fn test_factorize_zero() {
    assert_eq!(primes::factorize(0), vec![]);
}

#[test]
fn test_factorize_two() {
    assert_eq!(primes::factorize(2), vec![(2, 1)]);
}

#[test]
fn test_is_prime_small() {
    assert!(!primes::is_prime(0));
    assert!(!primes::is_prime(1));
    assert!(primes::is_prime(2));
    assert!(primes::is_prime(3));
    assert!(!primes::is_prime(4));
    assert!(primes::is_prime(5));
}

#[test]
fn test_is_prime_known() {
    assert!(primes::is_prime(97));
    assert!(primes::is_prime(101));
    assert!(primes::is_prime(7919));
}

#[test]
fn test_is_prime_composite() {
    assert!(!primes::is_prime(100));
    assert!(!primes::is_prime(1000));
    assert!(!primes::is_prime(9));
}

#[test]
fn test_format_factors_single() {
    assert_eq!(primes::format_factors(&[(7, 1)]), "7");
}

#[test]
fn test_format_factors_with_exponents() {
    assert_eq!(
        primes::format_factors(&[(2, 2), (3, 1), (7, 1)]),
        "2^2 × 3 × 7"
    );
}

#[test]
fn test_format_factors_empty() {
    assert_eq!(primes::format_factors(&[]), "1");
}

#[test]
fn test_factorize_roundtrip() {
    let n = 360_u128; // 2^3 * 3^2 * 5
    let factors = primes::factorize(n);
    let product: u128 = factors.iter().map(|(p, e)| p.pow(*e)).product();
    assert_eq!(product, n);
}
