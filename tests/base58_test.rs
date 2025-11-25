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
}
