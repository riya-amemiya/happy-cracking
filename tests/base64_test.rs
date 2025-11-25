use happy_cracking::crypto::base64;

#[test]
fn encode_empty_string() {
    assert_eq!(base64::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(base64::encode("Hello"), "SGVsbG8=");
}

#[test]
fn encode_hello_world() {
    assert_eq!(base64::encode("Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
}

#[test]
fn encode_japanese() {
    assert_eq!(base64::encode("こんにちは"), "44GT44KT44Gr44Gh44Gv");
}

#[test]
fn decode_empty_string() {
    assert_eq!(base64::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(base64::decode("SGVsbG8=").unwrap(), "Hello");
}

#[test]
fn decode_hello_world() {
    assert_eq!(
        base64::decode("SGVsbG8sIFdvcmxkIQ==").unwrap(),
        "Hello, World!"
    );
}

#[test]
fn decode_japanese() {
    assert_eq!(
        base64::decode("44GT44KT44Gr44Gh44Gv").unwrap(),
        "こんにちは"
    );
}

#[test]
fn roundtrip() {
    let original = "flag{base64_is_not_encryption}";
    let encoded = base64::encode(original);
    let decoded = base64::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_invalid_base64() {
    assert!(base64::decode("!!!invalid!!!").is_err());
}
