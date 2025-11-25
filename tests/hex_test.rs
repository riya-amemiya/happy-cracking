use happy_cracking::crypto::hex;

#[test]
fn encode_empty_string() {
    assert_eq!(hex::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(hex::encode("Hello"), "48656c6c6f");
}

#[test]
fn encode_flag() {
    assert_eq!(hex::encode("flag{hex}"), "666c61677b6865787d");
}

#[test]
fn decode_empty_string() {
    assert_eq!(hex::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(hex::decode("48656c6c6f").unwrap(), "Hello");
}

#[test]
fn decode_flag() {
    assert_eq!(hex::decode("666c61677b6865787d").unwrap(), "flag{hex}");
}

#[test]
fn decode_uppercase() {
    assert_eq!(hex::decode("48454C4C4F").unwrap(), "HELLO");
}

#[test]
fn roundtrip() {
    let original = "CTF{h3x_1s_us3ful}";
    let encoded = hex::encode(original);
    let decoded = hex::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_invalid_hex() {
    assert!(hex::decode("GG").is_err());
}

#[test]
fn decode_odd_length() {
    assert!(hex::decode("ABC").is_err());
}
