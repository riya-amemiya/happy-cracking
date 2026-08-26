use happy_cracking::crypto::a1z26;

#[test]
fn encode_empty_string() {
    assert_eq!(a1z26::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(a1z26::encode("HELLO"), "8-5-12-12-15");
}

#[test]
fn encode_lowercase() {
    assert_eq!(a1z26::encode("hello"), "8-5-12-12-15");
}

#[test]
fn encode_with_spaces() {
    assert_eq!(a1z26::encode("HI THERE"), "8-9 20-8-5-18-5");
}

#[test]
fn encode_with_numbers() {
    assert_eq!(a1z26::encode("A1B2"), "1122");
}

#[test]
fn encode_single_char() {
    assert_eq!(a1z26::encode("A"), "1");
    assert_eq!(a1z26::encode("Z"), "26");
}

#[test]
fn decode_empty_string() {
    assert_eq!(a1z26::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(a1z26::decode("8-5-12-12-15").unwrap(), "HELLO");
}

#[test]
fn decode_with_spaces() {
    assert_eq!(a1z26::decode("8 5 12 12 15").unwrap(), "HELLO");
}

#[test]
fn decode_with_commas() {
    assert_eq!(a1z26::decode("8,5,12,12,15").unwrap(), "HELLO");
}

#[test]
fn decode_single_number() {
    assert_eq!(a1z26::decode("1").unwrap(), "A");
    assert_eq!(a1z26::decode("26").unwrap(), "Z");
}

#[test]
fn decode_out_of_range() {
    assert!(a1z26::decode("27").is_err());
    assert!(a1z26::decode("0").is_err());
}

#[test]
fn roundtrip() {
    let original = "HELLO";
    let encoded = a1z26::encode(original);
    let decoded = a1z26::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_flag() {
    let original = "FLAG";
    let encoded = a1z26::encode(original);
    assert_eq!(encoded, "6-12-1-7");
    let decoded = a1z26::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
