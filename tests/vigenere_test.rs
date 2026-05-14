use happy_cracking::crypto::vigenere;

#[test]
fn test_encrypt_basic() {
    assert_eq!(vigenere::encrypt("HELLO", "KEY").unwrap(), "RIJVS");
}

#[test]
fn test_decrypt_basic() {
    assert_eq!(vigenere::decrypt("RIJVS", "KEY").unwrap(), "HELLO");
}

#[test]
fn test_encrypt_lowercase() {
    assert_eq!(vigenere::encrypt("hello", "key").unwrap(), "rijvs");
}

#[test]
fn test_preserve_non_alpha() {
    assert_eq!(
        vigenere::encrypt("HELLO, WORLD!", "KEY").unwrap(),
        "RIJVS, UYVJN!"
    );
}

#[test]
fn test_roundtrip() {
    let original = "The quick brown fox";
    let key = "SECRET";
    let encrypted = vigenere::encrypt(original, key).unwrap();
    let decrypted = vigenere::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_empty_key_error() {
    assert!(vigenere::encrypt("HELLO", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(vigenere::encrypt("HELLO", "KEY123").is_err());
}

// Regression: non-ASCII characters (multi-byte UTF-8) must pass through
// unchanged after the byte-iteration optimization. Key index must only
// advance for ASCII alphabetic input bytes, matching the original char-based
// implementation exactly.
#[test]
fn test_encrypt_preserves_non_ascii() {
    assert_eq!(
        vigenere::encrypt("HELLO, 世界!", "KEY").unwrap(),
        "RIJVS, 世界!"
    );
}

#[test]
fn test_roundtrip_non_ascii() {
    let original = "Привет, world!";
    let key = "KEY";
    let encrypted = vigenere::encrypt(original, key).unwrap();
    let decrypted = vigenere::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}
