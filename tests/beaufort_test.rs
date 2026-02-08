use happy_cracking::crypto::beaufort;

#[test]
fn test_encrypt_basic() {
    // Beaufort: (key - plaintext) mod 26
    // H=7, E=4, L=11, L=11, O=14; key K=10, E=4, Y=24
    // (10-7)%26=3=D, (4-4)%26=0=A, (24-11)%26=13=N, (10-11+26)%26=25=Z, (4-14+26)%26=16=Q
    assert_eq!(beaufort::encrypt("HELLO", "KEY").unwrap(), "DANZQ");
}

#[test]
fn test_self_reciprocal() {
    // Beaufort is self-reciprocal: encrypting the ciphertext gives back plaintext
    let plaintext = "HELLO";
    let key = "KEY";
    let encrypted = beaufort::encrypt(plaintext, key).unwrap();
    let decrypted = beaufort::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_cipher_equals_encrypt_and_decrypt() {
    let input = "TESTING";
    let key = "SECRET";
    assert_eq!(
        beaufort::cipher(input, key).unwrap(),
        beaufort::encrypt(input, key).unwrap()
    );
    assert_eq!(
        beaufort::cipher(input, key).unwrap(),
        beaufort::decrypt(input, key).unwrap()
    );
}

#[test]
fn test_preserve_case() {
    let result = beaufort::encrypt("Hello", "KEY").unwrap();
    assert!(result.chars().nth(0).unwrap().is_ascii_uppercase());
    assert!(result.chars().nth(1).unwrap().is_ascii_lowercase());
}

#[test]
fn test_preserve_non_alpha() {
    let result = beaufort::encrypt("HELLO, WORLD!", "KEY").unwrap();
    assert!(result.contains(','));
    assert!(result.contains('!'));
    assert!(result.contains(' '));
}

#[test]
fn test_roundtrip_with_mixed_case() {
    let original = "The quick brown fox";
    let key = "SECRET";
    let encrypted = beaufort::encrypt(original, key).unwrap();
    let decrypted = beaufort::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_empty_key_error() {
    assert!(beaufort::encrypt("HELLO", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(beaufort::encrypt("HELLO", "KEY123").is_err());
}

#[test]
fn test_encrypt_single_char() {
    // A=0 with key A=0: (0-0)%26=0=A
    assert_eq!(beaufort::encrypt("A", "A").unwrap(), "A");
    // Z=25 with key A=0: (0-25+26)%26=1=B
    assert_eq!(beaufort::encrypt("Z", "A").unwrap(), "B");
}

#[test]
fn test_ctf_flag() {
    let original = "FLAGHIDDEN";
    let key = "CTF";
    let encrypted = beaufort::encrypt(original, key).unwrap();
    let decrypted = beaufort::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}
