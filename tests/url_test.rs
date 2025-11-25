use happy_cracking::crypto::url;

#[test]
fn test_encode_basic() {
    assert_eq!(url::encode("hello world"), "hello%20world");
}

#[test]
fn test_encode_special_chars() {
    assert_eq!(url::encode("a=1&b=2"), "a%3D1%26b%3D2");
}

#[test]
fn test_decode_basic() {
    assert_eq!(url::decode("hello%20world").unwrap(), "hello world");
}

#[test]
fn test_decode_special_chars() {
    assert_eq!(url::decode("a%3D1%26b%3D2").unwrap(), "a=1&b=2");
}

#[test]
fn test_roundtrip() {
    let original = "Hello World! @#$%";
    let encoded = url::encode(original);
    let decoded = url::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
