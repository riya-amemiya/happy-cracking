use happy_cracking::crypto::mathtools;

#[test]
fn test_gcd_basic() {
    assert_eq!(mathtools::gcd(12, 8), 4);
    assert_eq!(mathtools::gcd(54, 24), 6);
}

#[test]
fn test_gcd_coprime() {
    assert_eq!(mathtools::gcd(17, 13), 1);
}

#[test]
fn test_gcd_with_zero() {
    assert_eq!(mathtools::gcd(0, 5), 5);
    assert_eq!(mathtools::gcd(5, 0), 5);
    assert_eq!(mathtools::gcd(0, 0), 0);
}

#[test]
fn test_gcd_same() {
    assert_eq!(mathtools::gcd(7, 7), 7);
}

#[test]
fn test_lcm_basic() {
    assert_eq!(mathtools::lcm(4, 6), 12);
    assert_eq!(mathtools::lcm(3, 5), 15);
}

#[test]
fn test_lcm_with_zero() {
    assert_eq!(mathtools::lcm(0, 5), 0);
    assert_eq!(mathtools::lcm(5, 0), 0);
}

#[test]
fn test_lcm_same() {
    assert_eq!(mathtools::lcm(7, 7), 7);
}

#[test]
fn test_modinv_basic() {
    // 3 * 9 = 27 ≡ 1 (mod 26)
    assert_eq!(mathtools::modinv(3, 26).unwrap(), 9);
}

#[test]
fn test_modinv_known_values() {
    // 7^-1 mod 11 = 8 because 7*8=56 ≡ 1 (mod 11)
    assert_eq!(mathtools::modinv(7, 11).unwrap(), 8);
}

#[test]
fn test_modinv_not_exist() {
    // gcd(2, 4) = 2 != 1
    assert!(mathtools::modinv(2, 4).is_err());
}

#[test]
fn test_modinv_mod_one() {
    assert_eq!(mathtools::modinv(5, 1).unwrap(), 0);
}

#[test]
fn test_modpow_basic() {
    // 2^10 mod 1000 = 1024 mod 1000 = 24
    assert_eq!(mathtools::modpow(2, 10, 1000), 24);
}

#[test]
fn test_modpow_large() {
    // 3^13 mod 7 = 1594323 mod 7 = 3
    assert_eq!(mathtools::modpow(3, 13, 7), 3);
}

#[test]
fn test_modpow_zero_exp() {
    assert_eq!(mathtools::modpow(5, 0, 13), 1);
}

#[test]
fn test_modpow_mod_one() {
    assert_eq!(mathtools::modpow(100, 100, 1), 0);
}

#[test]
fn test_modpow_rsa_style() {
    // Simulating RSA: m^e mod n, then c^d mod n
    // Small example: p=61, q=53, n=3233, e=17, d=2753
    let n = 3233_u128;
    let e = 17_u128;
    let d = 2753_u128;
    let message = 65_u128;
    let encrypted = mathtools::modpow(message, e, n);
    let decrypted = mathtools::modpow(encrypted, d, n);
    assert_eq!(decrypted, message);
}
