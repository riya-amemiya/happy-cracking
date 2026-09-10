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

    let found = xor::single_byte_xor_bruteforce(&encrypted).find(|(k, _)| *k == key);
    assert!(found.is_some());

    let (_, decrypted) = found.unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn single_byte_xor_bruteforce_returns_256_results() {
    let data = b"test";
    assert_eq!(xor::single_byte_xor_bruteforce(data).count(), 256);
}

#[test]
fn decode_xor_hex_with_limit_rejects_oversized_dump() {
    let err = xor::decode_xor_hex_with_limit("41424344", 2).unwrap_err();
    assert!(err.to_string().contains("Denial of Service"));
}

#[test]
fn decode_xor_hex_with_limit_accepts_dump_at_limit() {
    let data = xor::decode_xor_hex_with_limit("4142", 2).unwrap();
    assert_eq!(data, b"AB");
}

#[test]
fn decode_xor_hex_with_limit_accepts_empty() {
    let data = xor::decode_xor_hex_with_limit("", 16).unwrap();
    assert!(data.is_empty());
}

#[test]
fn decode_xor_hex_with_limit_rejects_invalid_hex() {
    let err = xor::decode_xor_hex_with_limit("zz", 16).unwrap_err();
    assert!(!err.to_string().contains("Denial of Service"));
}

#[test]
fn bruteforce_run_reads_hex_under_limit() {
    let original = b"flag";
    let encrypted: Vec<u8> = original.iter().map(|&b| b ^ 0x42).collect();
    xor::run(xor::XorAction::Bruteforce {
        input: hex::encode(&encrypted),
        printable: true,
    })
    .unwrap();
}

#[test]
fn xor_bytes_empty_key() {
    let data = b"Hello";
    let key = b"";
    let result = xor::xor_bytes(data, key);
    assert_eq!(result, data);
}
