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
