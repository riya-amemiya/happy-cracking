use happy_cracking::crypto::base62;

#[test]
fn test_encode_basic() {
    let encoded = base62::encode("Hello");
    assert!(!encoded.is_empty());
    assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn test_decode_basic() {
    let encoded = base62::encode("Hello");
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, "Hello");
}

#[test]
fn test_roundtrip() {
    let original = "Hello, World!";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_roundtrip_long() {
    let original = "The quick brown fox jumps over the lazy dog";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_string() {
    assert_eq!(base62::encode(""), "");
    assert_eq!(base62::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let original = "flag{base62_encoding}";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_single_char() {
    let original = "A";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_only_alphanumeric_output() {
    let encoded = base62::encode("test data 123!@#");
    assert!(encoded.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn test_decode_invalid_char() {
    assert!(base62::decode("!!!").is_err());
}

#[test]
fn test_binary_data_roundtrip() {
    let original = "\x01\x02\x03";
    let encoded = base62::encode(original);
    let decoded = base62::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
