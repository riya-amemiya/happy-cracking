use happy_cracking::crypto::rc4;

#[test]
fn test_rc4_roundtrip() {
    let plaintext = b"Hello, World!";
    let key = b"SecretKey";
    let encrypted = rc4::rc4(plaintext, key).unwrap();
    let decrypted = rc4::rc4(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rc4_known_vector() {
    // RFC 6229 test vector: Key = "Key", Plaintext = "Plaintext"
    let plaintext = b"Plaintext";
    let key = b"Key";
    let encrypted = rc4::rc4(plaintext, key).unwrap();
    assert_eq!(hex::encode(&encrypted), "bbf316e8d940af0ad3");
}

#[test]
fn test_rc4_empty_input() {
    let result = rc4::rc4(b"", b"key").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_rc4_empty_key() {
    assert!(rc4::rc4(b"Hello", b"").is_err());
}

#[test]
fn test_rc4_single_byte() {
    let plaintext = b"A";
    let key = b"K";
    let encrypted = rc4::rc4(plaintext, key).unwrap();
    let decrypted = rc4::rc4(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rc4_sbox_initialized() {
    let key = b"Key";
    let sbox = rc4::rc4_sbox(key).unwrap();
    // S-box must be a permutation of 0..255
    let mut sorted = sbox.to_vec();
    sorted.sort();
    let expected: Vec<u8> = (0..=255).collect();
    assert_eq!(sorted, expected);
}

#[test]
fn test_rc4_flag_format() {
    let plaintext = b"flag{rc4_cipher_works}";
    let key = b"ctf";
    let encrypted = rc4::rc4(plaintext, key).unwrap();
    let decrypted = rc4::rc4(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rc4_long_input() {
    let plaintext =
        b"The quick brown fox jumps over the lazy dog. This is a longer test input for RC4.";
    let key = b"LongKeyForTesting";
    let encrypted = rc4::rc4(plaintext, key).unwrap();
    let decrypted = rc4::rc4(&encrypted, key).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rc4_sbox_empty_key() {
    assert!(rc4::rc4_sbox(b"").is_err());
}

#[test]
fn decode_rc4_hex_with_limit_rejects_oversized_dump() {
    let err = rc4::decode_rc4_hex_with_limit("41424344", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn decode_rc4_hex_with_limit_accepts_dump_at_limit() {
    let data = rc4::decode_rc4_hex_with_limit("4142", 2).unwrap();
    assert_eq!(data, b"AB");
}

#[test]
fn decode_rc4_hex_with_limit_accepts_empty() {
    let data = rc4::decode_rc4_hex_with_limit("", 16).unwrap();
    assert!(data.is_empty());
}

#[test]
fn decode_rc4_hex_with_limit_rejects_invalid_hex() {
    let err = rc4::decode_rc4_hex_with_limit("zz", 16).unwrap_err();
    assert!(!err.to_string().contains("Denial of Service"));
}

#[test]
fn cipher_ascii_with_limit_rejects_oversized_input() {
    let start = std::time::Instant::now();
    let err = rc4::cipher_ascii_with_limit("abcd", "k", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized ASCII input must be rejected before RC4"
    );
}

#[test]
fn cipher_ascii_with_limit_rejects_oversized_key() {
    let start = std::time::Instant::now();
    let err = rc4::cipher_ascii_with_limit("Hi", "abcd", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
    assert!(
        start.elapsed().as_millis() < 100,
        "oversized ASCII key must be rejected before RC4"
    );
}

#[test]
fn cipher_ascii_with_limit_accepts_len_at_limit() {
    let data = rc4::cipher_ascii_with_limit("Hi", "K", 2).unwrap();
    assert_eq!(data, rc4::rc4(b"Hi", b"K").unwrap());
}

#[test]
fn cipher_ascii_with_limit_accepts_empty_input() {
    let data = rc4::cipher_ascii_with_limit("", "Key", 16).unwrap();
    assert!(data.is_empty());
}

#[test]
fn cipher_ascii_with_limit_known_vector() {
    let data = rc4::cipher_ascii_with_limit("Plaintext", "Key", 16).unwrap();
    assert_eq!(hex::encode(&data), "bbf316e8d940af0ad3");
}

fn rc4_hex_matches_decode_rc4_encode(input_hex: &str, key: &[u8]) {
    let (hex_out, plain) = rc4::rc4_hex(input_hex, key).unwrap();
    let decoded = rc4::decode_rc4_hex_with_limit(input_hex, rc4::MAX_RC4_BYTES).unwrap();
    let expected = rc4::rc4(&decoded, key).unwrap();
    assert_eq!(plain, expected);
    assert_eq!(hex_out, hex::encode(&expected));
}

#[test]
fn rc4_hex_plaintext_key() {
    let (hex_out, plain) = rc4::rc4_hex("506c61696e74657874", b"Key").unwrap();
    assert_eq!(hex_out, "bbf316e8d940af0ad3");
    assert_eq!(plain, hex::decode("bbf316e8d940af0ad3").unwrap());
}

#[test]
fn rc4_hex_uppercases_to_lowercase() {
    let (hex_out, _) = rc4::rc4_hex("506C61696E74657874", b"Key").unwrap();
    assert_eq!(hex_out, "bbf316e8d940af0ad3");
}

#[test]
fn rc4_hex_empty_input() {
    let (hex_out, plain) = rc4::rc4_hex("", b"Key").unwrap();
    assert!(hex_out.is_empty());
    assert!(plain.is_empty());
}

#[test]
fn rc4_hex_empty_key() {
    let err = rc4::rc4_hex("506c61696e74657874", b"").unwrap_err();
    assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn rc4_hex_roundtrip() {
    let (ct, _) = rc4::rc4_hex("48656c6c6f", b"Key").unwrap();
    let (pt_hex, pt) = rc4::rc4_hex(&ct, b"Key").unwrap();
    assert_eq!(pt, b"Hello");
    assert_eq!(pt_hex, "48656c6c6f");
}

#[test]
fn rc4_hex_rejects_invalid() {
    let err = rc4::rc4_hex("zz", b"Key").unwrap_err();
    assert!(err.to_string().contains("Failed to decode hex input"));
    assert!(!err.to_string().contains("Denial of Service"));
}

#[test]
fn rc4_hex_rejects_odd_length() {
    let err = rc4::rc4_hex("abc", b"Key").unwrap_err();
    assert!(err.to_string().contains("Failed to decode hex input"));
}

#[test]
fn rc4_hex_rejects_oversized() {
    let err = rc4::rc4_hex_with_limit("41424344", b"Key", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn rc4_hex_4k_matches_library_path() {
    let data: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
    rc4_hex_matches_decode_rc4_encode(&hex::encode(&data), b"Key");
}
