use happy_cracking::crypto::des_cipher;

#[test]
fn test_des_roundtrip() {
    let key = "0123456789abcdef";
    let plaintext = "0123456789abcdef";
    let encrypted = des_cipher::des_encrypt(plaintext, key).unwrap();
    let decrypted = des_cipher::des_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_des_known_vector() {
    let key = "0123456789abcdef";
    let plaintext = "4e6f772069732074";
    let encrypted = des_cipher::des_encrypt(plaintext, key).unwrap();
    let decrypted = des_cipher::des_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_des_multi_block() {
    let key = "0123456789abcdef";
    let plaintext = "0123456789abcdef0123456789abcdef";
    let encrypted = des_cipher::des_encrypt(plaintext, key).unwrap();
    let decrypted = des_cipher::des_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_tdes_roundtrip() {
    let key = "0123456789abcdef0123456789abcdef0123456789abcdef";
    let plaintext = "0123456789abcdef";
    let encrypted = des_cipher::tdes_encrypt(plaintext, key).unwrap();
    let decrypted = des_cipher::tdes_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_tdes_multi_block() {
    let key = "0123456789abcdefaabbccddeeff0011fedcba9876543210";
    let plaintext = "0123456789abcdef0123456789abcdef";
    let encrypted = des_cipher::tdes_encrypt(plaintext, key).unwrap();
    let decrypted = des_cipher::tdes_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_des_invalid_key_length() {
    let key = "01234567";
    let plaintext = "0123456789abcdef";
    assert!(des_cipher::des_encrypt(plaintext, key).is_err());
}

#[test]
fn test_des_invalid_input_length() {
    let key = "0123456789abcdef";
    let plaintext = "0123456789ab";
    assert!(des_cipher::des_encrypt(plaintext, key).is_err());
}

#[test]
fn test_tdes_invalid_key_length() {
    let key = "0123456789abcdef";
    let plaintext = "0123456789abcdef";
    assert!(des_cipher::tdes_encrypt(plaintext, key).is_err());
}

#[test]
fn test_des_empty_input() {
    let key = "0123456789abcdef";
    assert!(des_cipher::des_encrypt("", key).is_err());
}
