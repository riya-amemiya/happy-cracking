use happy_cracking::crypto::binary;

#[test]
fn test_encode_basic() {
    assert_eq!(binary::encode("A"), "01000001");
}

#[test]
fn test_encode_hello() {
    assert_eq!(binary::encode("Hi"), "01001000 01101001");
}

#[test]
fn test_decode_basic() {
    assert_eq!(binary::decode("01000001").unwrap(), "A");
}

#[test]
fn test_decode_with_spaces() {
    assert_eq!(binary::decode("01001000 01101001").unwrap(), "Hi");
}

#[test]
fn test_roundtrip() {
    let original = "Hello";
    let encoded = binary::encode(original);
    let decoded = binary::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_invalid_length() {
    assert!(binary::decode("0100").is_err());
}
