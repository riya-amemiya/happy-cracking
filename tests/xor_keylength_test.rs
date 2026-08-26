use happy_cracking::crypto::xor;

#[test]
fn detect_key_length_caps_excessive_max() {
    // SECURITY: `--max-len` is a user-controlled loop bound. Without a hard cap,
    // `detect_key_length` is O(n * max_len) Hamming-distance work and can hang.
    let data = vec![0x41u8; 600];
    let results = xor::detect_key_length(&data, usize::MAX);
    assert!(
        results.iter().all(|&(len, _)| len <= xor::MAX_KEY_LENGTH),
        "key lengths must be capped at {}, got {:?}",
        xor::MAX_KEY_LENGTH,
        results.iter().map(|&(len, _)| len).collect::<Vec<_>>()
    );
}

#[test]
fn check_key_length_rejects_zero() {
    let err = xor::check_key_length(0).unwrap_err();
    assert!(err.to_string().contains("at least 1"));
}

#[test]
fn check_key_length_rejects_excessive() {
    let err = xor::check_key_length(xor::MAX_KEY_LENGTH + 1).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn check_key_length_accepts_limit() {
    xor::check_key_length(xor::MAX_KEY_LENGTH).unwrap();
}

#[test]
fn keylength_run_rejects_excessive_max_len() {
    let input = hex::encode(b"HELLO WORLD HELLO WORLD HELLO WORLD HELLO WORLD");
    let err = xor::run(xor::XorAction::Keylength {
        input,
        max_len: xor::MAX_KEY_LENGTH + 1,
        top: 5,
    })
    .unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn detect_key_length_repeating_key() {
    let plaintext = b"HELLO WORLD HELLO WORLD HELLO WORLD HELLO WORLD";
    let key = b"ABC";
    let encrypted = xor::xor_bytes(plaintext, key);

    let results = xor::detect_key_length(&encrypted, 20);
    assert!(!results.is_empty());

    let top_lengths: Vec<usize> = results.iter().take(5).map(|&(len, _)| len).collect();
    // Key length 3 or its multiples (6, 9, ...) should appear
    assert!(
        top_lengths.contains(&3) || top_lengths.contains(&6) || top_lengths.contains(&9),
        "Expected key length 3 or multiple in top results, got {:?}",
        top_lengths
    );
}

#[test]
fn detect_key_length_short_input() {
    let data = vec![0x41, 0x42];
    let results = xor::detect_key_length(&data, 10);
    assert!(results.is_empty());
}

#[test]
fn detect_key_length_returns_sorted() {
    let plaintext = b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps.";
    let key = b"SECRETKEY";
    let encrypted = xor::xor_bytes(plaintext, key);

    let results = xor::detect_key_length(&encrypted, 30);
    for window in results.windows(2) {
        assert!(window[0].1 <= window[1].1);
    }
}

#[test]
fn detect_key_length_single_byte_key() {
    // With a single-byte key, all key lengths produce similar distances
    let plaintext = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let encrypted = xor::xor_bytes(plaintext, &[0x42]);

    let results = xor::detect_key_length(&encrypted, 10);
    assert!(!results.is_empty());
}
