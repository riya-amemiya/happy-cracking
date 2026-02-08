use happy_cracking::crypto::nato;

#[test]
fn test_encode_basic() {
    assert_eq!(nato::encode("A"), "ALFA");
}

#[test]
fn test_encode_hello() {
    assert_eq!(nato::encode("HELLO"), "HOTEL ECHO LIMA LIMA OSCAR");
}

#[test]
fn test_decode_basic() {
    assert_eq!(nato::decode("ALFA").unwrap(), "A");
}

#[test]
fn test_decode_hello() {
    assert_eq!(nato::decode("HOTEL ECHO LIMA LIMA OSCAR").unwrap(), "HELLO");
}

#[test]
fn test_encode_with_numbers() {
    assert_eq!(nato::encode("A1"), "ALFA ONE");
}

#[test]
fn test_decode_with_numbers() {
    assert_eq!(nato::decode("ALFA ONE").unwrap(), "A1");
}

#[test]
fn test_roundtrip() {
    let original = "HELLOWORLD";
    let encoded = nato::encode(original);
    let decoded = nato::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(nato::encode("hello"), nato::encode("HELLO"));
}

#[test]
fn test_decode_case_insensitive() {
    assert_eq!(nato::decode("alfa").unwrap(), "A");
    assert_eq!(nato::decode("Alfa").unwrap(), "A");
    assert_eq!(nato::decode("ALFA").unwrap(), "A");
}

#[test]
fn test_encode_empty() {
    assert_eq!(nato::encode(""), "");
}

#[test]
fn test_decode_empty() {
    assert_eq!(nato::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = nato::encode("FLAG");
    assert_eq!(encoded, "FOXTROT LIMA ALFA GOLF");
    let decoded = nato::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_decode_unknown_word() {
    assert!(nato::decode("NOTAWORD").is_err());
}

#[test]
fn test_full_alphabet() {
    let alpha = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let encoded = nato::encode(alpha);
    let decoded = nato::decode(&encoded).unwrap();
    assert_eq!(decoded, alpha);
}

#[test]
fn test_all_digits() {
    let digits = "0123456789";
    let encoded = nato::encode(digits);
    let decoded = nato::decode(&encoded).unwrap();
    assert_eq!(decoded, digits);
}

#[test]
fn test_encode_spaces_ignored() {
    // Spaces are not in the NATO table, so they are filtered out
    assert_eq!(nato::encode("SOS"), "SIERRA OSCAR SIERRA");
}
