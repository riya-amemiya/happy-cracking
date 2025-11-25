use happy_cracking::crypto::xor;

#[test]
fn xor_bytes_single_key() {
    let data = b"Hello";
    let key = b"A";
    let result = xor::xor_bytes(data, key);
    assert_eq!(result, vec![0x09, 0x24, 0x2d, 0x2d, 0x2e]);
}

#[test]
fn xor_bytes_roundtrip() {
    let data = b"Secret message";
    let key = b"KEY";
    let encrypted = xor::xor_bytes(data, key);
    let decrypted = xor::xor_bytes(&encrypted, key);
    assert_eq!(decrypted, data);
}

#[test]
fn xor_bytes_repeating_key() {
    let data = b"AAAA";
    let key = b"AB";
    let result = xor::xor_bytes(data, key);
    assert_eq!(result, vec![0x00, 0x03, 0x00, 0x03]);
}

#[test]
fn xor_bytes_empty_data() {
    let data = b"";
    let key = b"key";
    let result = xor::xor_bytes(data, key);
    assert!(result.is_empty());
}

#[test]
fn single_byte_xor_bruteforce_contains_original() {
    let original = b"flag";
    let key = 0x42u8;
    let encrypted: Vec<u8> = original.iter().map(|&b| b ^ key).collect();

    let results = xor::single_byte_xor_bruteforce(&encrypted);

    let found = results.iter().find(|(k, _)| *k == key);
    assert!(found.is_some());

    let (_, decrypted) = found.unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn single_byte_xor_bruteforce_returns_256_results() {
    let data = b"test";
    let results = xor::single_byte_xor_bruteforce(data);
    assert_eq!(results.len(), 256);
}
