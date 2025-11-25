use happy_cracking::crypto::base32;

#[test]
fn encode_empty_string() {
    assert_eq!(base32::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(base32::encode("Hello"), "JBSWY3DP");
}

#[test]
fn encode_hello_world() {
    assert_eq!(base32::encode("Hello, World!"), "JBSWY3DPFQQFO33SNRSCC===");
}

#[test]
fn decode_empty_string() {
    assert_eq!(base32::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(base32::decode("JBSWY3DP").unwrap(), "Hello");
}

#[test]
fn decode_hello_world() {
    assert_eq!(
        base32::decode("JBSWY3DPFQQFO33SNRSCC===").unwrap(),
        "Hello, World!"
    );
}

#[test]
fn roundtrip() {
    let original = "flag{base32_ctf_challenge}";
    let encoded = base32::encode(original);
    let decoded = base32::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_invalid_base32() {
    assert!(base32::decode("12345!!!").is_err());
}
