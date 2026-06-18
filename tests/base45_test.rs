use happy_cracking::crypto::base45;

#[test]
fn encode_known_vector_ab() {
    assert_eq!(base45::encode(b"AB"), "BB8");
}

#[test]
fn encode_known_vector_hello() {
    assert_eq!(base45::encode(b"Hello!!"), "%69 VD92EX0");
}

#[test]
fn encode_known_vector_base45() {
    assert_eq!(base45::encode(b"base-45"), "UJCLQE7W581");
}

#[test]
fn decode_known_vector_ab() {
    assert_eq!(base45::decode("BB8").unwrap(), b"AB");
}

#[test]
fn decode_known_vector_hello() {
    assert_eq!(base45::decode("%69 VD92EX0").unwrap(), b"Hello!!");
}

#[test]
fn encode_empty() {
    assert_eq!(base45::encode(b""), "");
}

#[test]
fn decode_empty() {
    assert_eq!(base45::decode("").unwrap(), Vec::<u8>::new());
}

#[test]
fn roundtrip() {
    let original = b"The quick brown fox jumps over the lazy dog";
    let encoded = base45::encode(original);
    let decoded = base45::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_single_byte() {
    let encoded = base45::encode(b"\x00");
    let decoded = base45::decode(&encoded).unwrap();
    assert_eq!(decoded, b"\x00");
}

#[test]
fn decode_invalid_char() {
    assert!(base45::decode("abc").is_err());
}

#[test]
fn decode_overflow_group() {
    assert!(base45::decode(":::").is_err());
}

#[test]
fn decode_bad_length() {
    assert!(base45::decode("BB8B").is_err());
}

#[test]
fn flag_roundtrip() {
    let original = b"flag{base45_rfc9285}";
    let encoded = base45::encode(original);
    let decoded = base45::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
