use happy_cracking::crypto::mathtools;

#[test]
fn test_modpow_zero_modulus() {
    // Should return Err, not panic
    let result = mathtools::modpow(2, 3, 0);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Modulus must be non-zero");
}

#[test]
fn test_lcm_overflow() {
    // Should return Err on overflow
    let max = u128::MAX;
    let max_minus_1 = u128::MAX - 1;
    // gcd(max, max-1) = 1 (since max is odd, max-1 is even, wait gcd(2^128-1, 2^128-2) = gcd(1, ...))
    // Actually gcd(n, n-1) is always 1.
    // So lcm = max * (max-1) which overflows u128.
    let result = mathtools::lcm(max, max_minus_1);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "LCM overflow");
}

#[test]
fn test_montgomery_new_even_modulus() {
    // Should return Err, not panic
    let result = mathtools::Montgomery::new(2);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Modulus must be odd for Montgomery arithmetic"
    );
}
