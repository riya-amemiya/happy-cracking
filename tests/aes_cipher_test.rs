use happy_cracking::crypto::aes_cipher;

#[test]
fn test_ecb_roundtrip() {
    let key = "00112233445566778899aabbccddeeff";
    let plaintext = "00112233445566778899aabbccddeeff";
    let encrypted = aes_cipher::ecb_encrypt(plaintext, key).unwrap();
    let decrypted = aes_cipher::ecb_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_ecb_known_vector() {
    // NIST AES-128 ECB test vector
    let key = "2b7e151628aed2a6abf7158809cf4f3c";
    let plaintext = "6bc1bee22e409f96e93d7e117393172a";
    let expected = "3ad77bb40d7a3660a89ecaf32466ef97";
    let result = aes_cipher::ecb_encrypt(plaintext, key).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_ecb_decrypt_known_vector() {
    let key = "2b7e151628aed2a6abf7158809cf4f3c";
    let ciphertext = "3ad77bb40d7a3660a89ecaf32466ef97";
    let expected = "6bc1bee22e409f96e93d7e117393172a";
    let result = aes_cipher::ecb_decrypt(ciphertext, key).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_ecb_multi_block() {
    let key = "00112233445566778899aabbccddeeff";
    let plaintext = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let encrypted = aes_cipher::ecb_encrypt(plaintext, key).unwrap();
    let decrypted = aes_cipher::ecb_decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_cbc_roundtrip() {
    let key = "00112233445566778899aabbccddeeff";
    let iv = "00000000000000000000000000000000";
    let plaintext = "00112233445566778899aabbccddeeff";
    let encrypted = aes_cipher::cbc_encrypt(plaintext, key, iv).unwrap();
    let decrypted = aes_cipher::cbc_decrypt(&encrypted, key, iv).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_cbc_multi_block_roundtrip() {
    let key = "2b7e151628aed2a6abf7158809cf4f3c";
    let iv = "000102030405060708090a0b0c0d0e0f";
    let plaintext = "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51";
    let encrypted = aes_cipher::cbc_encrypt(plaintext, key, iv).unwrap();
    let decrypted = aes_cipher::cbc_decrypt(&encrypted, key, iv).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_ecb_detect_positive() {
    // Two identical blocks will produce identical ciphertext blocks in ECB mode
    let key = "00112233445566778899aabbccddeeff";
    let plaintext = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let encrypted = aes_cipher::ecb_encrypt(plaintext, key).unwrap();
    let (detected, count) = aes_cipher::ecb_detect(&encrypted).unwrap();
    assert!(detected);
    assert_eq!(count, 1);
}

#[test]
fn test_ecb_detect_negative() {
    // CBC mode with non-zero IV should not produce duplicate blocks
    let key = "00112233445566778899aabbccddeeff";
    let iv = "0102030405060708090a0b0c0d0e0f10";
    let plaintext = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    let encrypted = aes_cipher::cbc_encrypt(plaintext, key, iv).unwrap();
    let (detected, _) = aes_cipher::ecb_detect(&encrypted).unwrap();
    assert!(!detected);
}

#[test]
fn test_invalid_key_length() {
    let key = "0011223344556677";
    let plaintext = "00112233445566778899aabbccddeeff";
    assert!(aes_cipher::ecb_encrypt(plaintext, key).is_err());
}

#[test]
fn test_invalid_input_length() {
    let key = "00112233445566778899aabbccddeeff";
    let plaintext = "001122334455";
    assert!(aes_cipher::ecb_encrypt(plaintext, key).is_err());
}

#[test]
fn test_invalid_iv_length() {
    let key = "00112233445566778899aabbccddeeff";
    let iv = "0011223344556677";
    let plaintext = "00112233445566778899aabbccddeeff";
    assert!(aes_cipher::cbc_encrypt(plaintext, key, iv).is_err());
}
