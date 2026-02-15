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
