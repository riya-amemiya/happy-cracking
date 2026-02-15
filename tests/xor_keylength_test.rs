use happy_cracking::crypto::xor;

#[test]
fn detect_key_length_repeating_key() {
    // Encrypt "HELLO WORLD HELLO WORLD HELLO WORLD HELLO WORLD" with key "ABC" (3 bytes)
    let plaintext = b"HELLO WORLD HELLO WORLD HELLO WORLD HELLO WORLD";
    let key = b"ABC";
    let encrypted = xor::xor_bytes(plaintext, key);

    let results = xor::detect_key_length(&encrypted, 20);
    assert!(!results.is_empty());

    // The correct key length (3) should be in the top results
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
    let data = vec![0x41, 0x42]; // Too short
    let results = xor::detect_key_length(&data, 10);
    assert!(results.is_empty());
}

#[test]
fn detect_key_length_returns_sorted() {
    let plaintext = b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps.";
    let key = b"SECRETKEY";
    let encrypted = xor::xor_bytes(plaintext, key);

    let results = xor::detect_key_length(&encrypted, 30);
    // Results should be sorted by normalized distance (ascending)
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
    // Should return results (min key length is 2)
    assert!(!results.is_empty());
}
