use happy_cracking::crypto::rsa::big_modpow;
use num_bigint::BigUint;
use num_traits::Zero;

#[test]
fn test_modpow_zero_modulus_error() {
    let base = BigUint::from(2u32);
    let exp = BigUint::from(10u32);
    let modulus = BigUint::zero();

    let result = big_modpow(&base, &exp, &modulus);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Modulus must be non-zero");
}
