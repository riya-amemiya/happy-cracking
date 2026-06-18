use happy_cracking::crypto::quotedprintable as qp;

#[test]
fn encode_equals_sign() {
    assert_eq!(qp::encode(b"="), "=3D");
}

#[test]
fn encode_plain_ascii() {
    assert_eq!(qp::encode(b"Hello World"), "Hello World");
}

#[test]
fn encode_non_ascii_byte() {
    assert_eq!(qp::encode(&[0xE9]), "=E9");
}

#[test]
fn encode_trailing_space() {
    assert_eq!(qp::encode(b"abc "), "abc=20");
}

#[test]
fn decode_equals_sign() {
    assert_eq!(qp::decode("=3D").unwrap(), b"=");
}

#[test]
fn decode_lowercase_hex() {
    assert_eq!(qp::decode("=e9").unwrap(), &[0xE9]);
}

#[test]
fn decode_soft_line_break() {
    assert_eq!(qp::decode("Hello=\nWorld").unwrap(), b"HelloWorld");
}

#[test]
fn encode_empty() {
    assert_eq!(qp::encode(b""), "");
}

#[test]
fn decode_empty() {
    assert_eq!(qp::decode("").unwrap(), Vec::<u8>::new());
}

#[test]
fn roundtrip() {
    let original = b"Caf\xc3\xa9 = good. Tabs\tand spaces   here.";
    let encoded = qp::encode(original);
    let decoded = qp::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_long_line_soft_break() {
    let original = vec![b'A'; 200];
    let encoded = qp::encode(&original);
    assert!(encoded.contains("=\n"));
    let decoded = qp::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_malformed_escape() {
    assert!(qp::decode("=ZZ").is_err());
}

#[test]
fn decode_truncated_escape() {
    assert!(qp::decode("ab=").is_err());
}

#[test]
fn flag_roundtrip() {
    let original = b"flag{quoted=printable_2045}";
    let encoded = qp::encode(original);
    let decoded = qp::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
