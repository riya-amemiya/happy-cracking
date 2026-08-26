use happy_cracking::crypto::adfgvx;

#[test]
fn test_encrypt_basic() {
    let result = adfgvx::encrypt("HELLO", "default", "KEY").unwrap();
    assert!(!result.is_empty());
    assert!(result.chars().all(|c| "ADFGVX".contains(c)));
}

#[test]
fn test_roundtrip() {
    let original = "HELLOWORLD";
    let key = "default";
    let tk = "CARGO";
    let encrypted = adfgvx::encrypt(original, key, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, key, tk).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_roundtrip_with_custom_grid() {
    let grid = "PHQGIUMEAYLNOFDXJKRCVSTZWB0123456789";
    let tk = "GERMAN";
    let original = "ATTACK";
    let encrypted = adfgvx::encrypt(original, grid, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, grid, tk).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_roundtrip_with_digits() {
    let original = "HELLO123";
    let key = "default";
    let tk = "SECRET";
    let encrypted = adfgvx::encrypt(original, key, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, key, tk).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_non_alnum_ignored() {
    let enc1 = adfgvx::encrypt("HELLO WORLD!", "default", "KEY").unwrap();
    let enc2 = adfgvx::encrypt("HELLOWORLD", "default", "KEY").unwrap();
    assert_eq!(enc1, enc2);
}

#[test]
fn test_empty_input() {
    assert_eq!(adfgvx::encrypt("", "default", "KEY").unwrap(), "");
    assert_eq!(adfgvx::decrypt("", "default", "KEY").unwrap(), "");
}

#[test]
fn test_empty_transposition_key_error() {
    assert!(adfgvx::encrypt("HELLO", "default", "").is_err());
}

#[test]
fn test_invalid_transposition_key_error() {
    assert!(adfgvx::encrypt("HELLO", "default", "KEY123").is_err());
}

#[test]
fn test_invalid_grid_length_error() {
    assert!(adfgvx::encrypt("HELLO", "ABC", "KEY").is_err());
}

#[test]
fn test_output_contains_only_adfgvx() {
    let result = adfgvx::encrypt("THEQUICKBROWNFOX", "default", "ZEBRA").unwrap();
    assert!(result.chars().all(|c| "ADFGVX".contains(c)));
}

#[test]
fn test_roundtrip_long_text() {
    let original = "THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG0123456789";
    let key = "default";
    let tk = "ZEBRA";
    let encrypted = adfgvx::encrypt(original, key, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, key, tk).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_ctf_flag() {
    let original = "FLAG2024";
    let key = "default";
    let tk = "CTF";
    let encrypted = adfgvx::encrypt(original, key, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, key, tk).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_single_char() {
    let key = "default";
    let tk = "AB";
    let encrypted = adfgvx::encrypt("A", key, tk).unwrap();
    let decrypted = adfgvx::decrypt(&encrypted, key, tk).unwrap();
    assert_eq!(decrypted, "A");
}
