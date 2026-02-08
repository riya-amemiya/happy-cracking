use happy_cracking::crypto::otp;

#[test]
fn test_encrypt_basic() {
    // "Hi" = 0x48 0x69, key = 0x00 0x00 -> "4869"
    assert_eq!(otp::encrypt("Hi", "0000").unwrap(), "4869");
}

#[test]
fn test_encrypt_xor() {
    // "A" = 0x41, key = 0xFF -> 0x41 ^ 0xFF = 0xBE
    assert_eq!(otp::encrypt("A", "ff").unwrap(), "be");
}

#[test]
fn test_decrypt_basic() {
    assert_eq!(otp::decrypt("4869", "0000").unwrap(), "Hi");
}

#[test]
fn test_roundtrip() {
    let plaintext = "Hello, World!";
    let key = "deadbeefcafe0123456789abcd";
    let encrypted = otp::encrypt(plaintext, key).unwrap();
    let decrypted = otp::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_empty_input() {
    assert_eq!(otp::encrypt("", "abcd").unwrap(), "");
}

#[test]
fn test_empty_ciphertext() {
    assert_eq!(otp::decrypt("", "abcd").unwrap(), "");
}

#[test]
fn test_key_shorter_than_input_error() {
    assert!(otp::encrypt("Hello", "ff").is_err());
}

#[test]
fn test_key_shorter_than_ciphertext_error() {
    assert!(otp::decrypt("48656c6c6f", "ff").is_err());
}

#[test]
fn test_invalid_hex_key_error() {
    assert!(otp::encrypt("Hi", "zzzz").is_err());
}

#[test]
fn test_invalid_hex_ciphertext_error() {
    assert!(otp::decrypt("gggg", "0000").is_err());
}

#[test]
fn test_key_longer_than_input() {
    // Extra key bytes are simply unused
    let result = otp::encrypt("A", "ff00ff00ff00").unwrap();
    assert_eq!(result, "be");
}

#[test]
fn test_ctf_flag() {
    let plaintext = "flag{otp_unbreakable}";
    // Key must be at least 21 bytes = 42 hex chars
    let key = "0102030405060708090a0b0c0d0e0f1011121314151617181920";
    let encrypted = otp::encrypt(plaintext, key).unwrap();
    let decrypted = otp::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_known_xor_values() {
    // "AB" = 0x41 0x42, key = 0x41 0x42 -> 0x00 0x00
    assert_eq!(otp::encrypt("AB", "4142").unwrap(), "0000");
    assert_eq!(otp::decrypt("0000", "4142").unwrap(), "AB");
}
