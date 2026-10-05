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

#[test]
fn decode_aes_hex_with_limit_rejects_oversized_dump() {
    let err = aes_cipher::decode_aes_hex_with_limit("41424344", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn decode_aes_hex_with_limit_accepts_dump_at_limit() {
    let data = aes_cipher::decode_aes_hex_with_limit("4142", 2).unwrap();
    assert_eq!(data, b"AB");
}

#[test]
fn decode_aes_hex_with_limit_accepts_empty() {
    let data = aes_cipher::decode_aes_hex_with_limit("", 16).unwrap();
    assert!(data.is_empty());
}

#[test]
fn decode_aes_hex_with_limit_rejects_invalid_hex() {
    let err = aes_cipher::decode_aes_hex_with_limit("zz", 16).unwrap_err();
    assert!(!err.to_string().contains("Denial of Service"));
}

#[test]
fn ecb_encrypt_rejects_oversized_key_hex() {
    let key = "00".repeat(17);
    let plaintext = "00112233445566778899aabbccddeeff";
    let err = aes_cipher::ecb_encrypt(plaintext, &key).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn cbc_encrypt_rejects_oversized_iv_hex() {
    let key = "00112233445566778899aabbccddeeff";
    let iv = "00".repeat(17);
    let plaintext = "00112233445566778899aabbccddeeff";
    let err = aes_cipher::cbc_encrypt(plaintext, key, &iv).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}
