use happy_cracking::crypto::xor;

#[test]
fn xor_bytes_single_key() {
    let data = b"Hello";
    let key = b"A";
    let result = xor::xor_bytes(data, key);
    assert_eq!(result, vec![0x09, 0x24, 0x2d, 0x2d, 0x2e]);
}

#[test]
fn xor_bytes_single_byte_matches_bytewise_on_long_buffer() {
    let data: Vec<u8> = (0..=255).cycle().take(300).collect();
    let result = xor::xor_bytes(&data, &[0x5a]);
    let expected: Vec<u8> = data.iter().map(|&b| b ^ 0x5a).collect();
    assert_eq!(result, expected);
}

#[test]
fn xor_bytes_single_byte_empty_data() {
    let result = xor::xor_bytes(b"", &[0x41]);
    assert!(result.is_empty());
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
fn single_byte_xor_is_printable_matches_allocated_filter() {
    let data: Vec<u8> = (0u8..=255).cycle().take(64).collect();
    let expected: Vec<u8> = xor::single_byte_xor_bruteforce(&data)
        .filter(|(_, result)| result.iter().all(|&b| b.is_ascii_graphic() || b == b' '))
        .map(|(k, _)| k)
        .collect();
    let got: Vec<u8> = (0u8..=255)
        .filter(|&key| xor::single_byte_xor_is_printable(&data, key))
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn single_byte_xor_is_printable_accepts_known_plaintext() {
    let original = b"flag{xor}";
    let key = 0x42u8;
    let encrypted: Vec<u8> = original.iter().map(|&b| b ^ key).collect();
    assert!(xor::single_byte_xor_is_printable(&encrypted, key));
    assert!(!xor::single_byte_xor_is_printable(&encrypted, key ^ 0xff));
}

#[test]
fn single_byte_xor_is_printable_empty_is_vacuously_true() {
    assert!(xor::single_byte_xor_is_printable(b"", 0x00));
    assert!(xor::single_byte_xor_is_printable(b"", 0xff));
}

#[test]
fn xor_bytes_empty_key() {
    let data = b"Hello";
    let key = b"";
    let result = xor::xor_bytes(data, key);
    assert_eq!(result, data);
}
