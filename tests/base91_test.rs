use happy_cracking::crypto::base91;

#[test]
fn encode_empty_string() {
    assert_eq!(base91::encode("").unwrap(), "");
}

#[test]
fn encode_hello() {
    let encoded = base91::encode("Hello").unwrap();
    let decoded = base91::decode(&encoded).unwrap();
    assert_eq!(decoded, "Hello");
}

#[test]
fn decode_empty_string() {
    assert_eq!(base91::decode("").unwrap(), "");
}

#[test]
fn roundtrip() {
    let original = "flag{base91_is_compact}";
    let encoded = base91::encode(original).unwrap();
    let decoded = base91::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_single_byte() {
    let original = "A";
    let encoded = base91::encode(original).unwrap();
    let decoded = base91::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_binary_data() {
    let original = "CTF{b4s3_91_f0r_th3_w1n}";
    let encoded = base91::encode(original).unwrap();
    let decoded = base91::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn encode_produces_shorter_output() {
    let input = "This is a test of Base91 encoding";
    let encoded = base91::encode(input).unwrap();
    assert!(encoded.len() <= input.len() * 2);
}

#[test]
fn roundtrip_various_lengths() {
    for len in 1..=20 {
        let original: String = (0..len).map(|i| (b'A' + (i % 26) as u8) as char).collect();
        let encoded = base91::encode(&original).unwrap();
        let decoded = base91::decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
}

#[test]
fn decode_invalid_char() {
    // null byte is not in the alphabet
    assert!(base91::decode("\x00").is_err());
}
