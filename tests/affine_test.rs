use happy_cracking::crypto::affine;

#[test]
fn test_encrypt_basic() {
    // a=5, b=8: E(x) = (5x + 8) mod 26
    assert_eq!(affine::encrypt("HELLO", 5, 8).unwrap(), "RCLLA");
}

#[test]
fn test_decrypt_basic() {
    assert_eq!(affine::decrypt("RCLLA", 5, 8).unwrap(), "HELLO");
}

#[test]
fn test_encrypt_lowercase() {
    assert_eq!(affine::encrypt("hello", 5, 8).unwrap(), "rclla");
}

#[test]
fn test_preserve_non_alpha() {
    assert_eq!(
        affine::encrypt("HELLO, WORLD!", 5, 8).unwrap(),
        "RCLLA, OAPLX!"
    );
}

#[test]
fn test_roundtrip() {
    let original = "The quick brown fox";
    let encrypted = affine::encrypt(original, 7, 3).unwrap();
    let decrypted = affine::decrypt(&encrypted, 7, 3).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_invalid_a_error() {
    // a=2 is not coprime with 26
    assert!(affine::encrypt("HELLO", 2, 5).is_err());
}

#[test]
fn test_encrypt_equivalent_a_mod_26() {
    let got = affine::encrypt("HELLO", i32::MAX, 8).unwrap();
    let expected = affine::encrypt("HELLO", 23, 8).unwrap();
    assert_eq!(got, expected);
}

#[test]
fn test_decrypt_equivalent_b_mod_26() {
    let ciphertext = affine::encrypt("HELLO", 5, 2).unwrap();
    let got = affine::decrypt(&ciphertext, 5, i32::MIN).unwrap();
    assert_eq!(got, "HELLO");
}

#[test]
fn test_roundtrip_extreme_coefficients() {
    let original = "FLAG";
    let encrypted = affine::encrypt(original, i32::MAX, i32::MAX).unwrap();
    let decrypted = affine::decrypt(&encrypted, i32::MAX, i32::MAX).unwrap();
    assert_eq!(decrypted, original);
}
