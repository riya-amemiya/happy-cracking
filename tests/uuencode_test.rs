use happy_cracking::crypto::uuencode;

#[test]
fn encode_known_vector_cat() {
    let encoded = uuencode::encode(b"Cat", "data");
    assert_eq!(encoded, "begin 644 data\n#0V%T\n`\nend\n");
}

#[test]
fn decode_known_vector_cat() {
    let input = "begin 644 data\n#0V%T\n`\nend\n";
    assert_eq!(uuencode::decode(input).unwrap(), b"Cat");
}

#[test]
fn encode_empty() {
    let encoded = uuencode::encode(b"", "data");
    assert_eq!(encoded, "begin 644 data\n`\nend\n");
}

#[test]
fn decode_empty_body() {
    let input = "begin 644 data\n`\nend\n";
    assert_eq!(uuencode::decode(input).unwrap(), Vec::<u8>::new());
}

#[test]
fn roundtrip() {
    let original = b"Hello, World! This is a uuencode roundtrip test.";
    let encoded = uuencode::encode(original, "msg.txt");
    let decoded = uuencode::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_long_input_multiple_lines() {
    let original: Vec<u8> = (0u8..=200).cycle().take(300).collect();
    let encoded = uuencode::encode(&original, "blob");
    let decoded = uuencode::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn decode_accepts_backtick_zero() {
    let space_form = uuencode::encode(b"\x00\x00\x00", "data");
    let decoded = uuencode::decode(&space_form).unwrap();
    assert_eq!(decoded, b"\x00\x00\x00");
}

#[test]
fn decode_missing_begin() {
    assert!(uuencode::decode("#0V%T\n`\nend\n").is_err());
}

#[test]
fn decode_invalid_char() {
    let input = "begin 644 data\n#\x7f\x7f\x7f\x7f\n`\nend\n";
    assert!(uuencode::decode(input).is_err());
}

#[test]
fn flag_roundtrip() {
    let original = b"flag{uuencode_lives_on}";
    let encoded = uuencode::encode(original, "flag.txt");
    let decoded = uuencode::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}
