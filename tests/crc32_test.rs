use happy_cracking::crypto::crc32;

#[test]
fn test_known_vector() {
    assert_eq!(crc32::compute("123456789"), "cbf43926");
}

#[test]
fn test_compute_bytes() {
    assert_eq!(crc32::compute_bytes(b"123456789"), 0xCBF43926);
}

#[test]
fn test_empty_string() {
    assert_eq!(crc32::compute(""), "00000000");
}

#[test]
fn test_hello() {
    let result = crc32::compute("Hello");
    assert_eq!(result.len(), 8);
    assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_single_char() {
    let result = crc32::compute("a");
    assert_eq!(result.len(), 8);
}

#[test]
fn test_flag_format() {
    let checksum = crc32::compute("flag{crc32_test}");
    assert_eq!(checksum.len(), 8);
    assert_eq!(crc32::compute("flag{crc32_test}"), checksum);
}

#[test]
fn test_deterministic() {
    let a = crc32::compute("test data");
    let b = crc32::compute("test data");
    assert_eq!(a, b);
}

#[test]
fn test_different_inputs_differ() {
    assert_ne!(crc32::compute("abc"), crc32::compute("abd"));
}
