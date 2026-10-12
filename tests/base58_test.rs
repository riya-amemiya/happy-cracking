use happy_cracking::crypto::base58;

#[test]
fn test_encode_basic() {
    assert_eq!(base58::encode("Hello"), "9Ajdvzr");
}

#[test]
fn test_decode_basic() {
    assert_eq!(base58::decode("9Ajdvzr").unwrap(), "Hello");
}

#[test]
fn test_roundtrip() {
    let original = "Hello World!";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_string() {
    assert_eq!(base58::encode(""), "");
    assert_eq!(base58::decode("").unwrap(), "");
}

#[test]
fn test_roundtrip_flag() {
    let original = "flag{base58_encoding}";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_leading_zero_bytes() {
    assert_eq!(base58::encode("\0"), "1");
    assert_eq!(base58::encode("\0\0"), "11");
    assert_eq!(base58::decode("1").unwrap(), "\0");
    assert_eq!(base58::decode("11").unwrap(), "\0\0");
}

#[test]
fn test_decode_trims_whitespace() {
    assert_eq!(base58::decode("  9Ajdvzr\n").unwrap(), "Hello");
}

#[test]
fn test_decode_invalid_char() {
    assert!(base58::decode("0OIl").is_err());
    assert!(base58::decode("!!!").is_err());
}

#[test]
fn test_roundtrip_long() {
    let original = "The quick brown fox jumps over the lazy dog";
    let encoded = base58::encode(original);
    let decoded = base58::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
