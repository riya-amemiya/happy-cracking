use happy_cracking::crypto::binary;

#[test]
fn test_encode_basic() {
    assert_eq!(binary::encode("A"), "01000001");
}

#[test]
fn test_encode_hello() {
    assert_eq!(binary::encode("Hi"), "01001000 01101001");
}

#[test]
fn test_encode_empty() {
    assert_eq!(binary::encode(""), "");
}

#[test]
fn test_decode_basic() {
    assert_eq!(binary::decode("01000001").unwrap(), "A");
}

#[test]
fn test_decode_with_spaces() {
    assert_eq!(binary::decode("01001000 01101001").unwrap(), "Hi");
}

#[test]
fn test_decode_compact_no_spaces() {
    assert_eq!(binary::decode("0100100001101001").unwrap(), "Hi");
}

#[test]
fn test_decode_ascii_bytes_grouped_and_compact() {
    for b in 0x20u8..=0x7e {
        let ch = b as char;
        let text = ch.to_string();
        let grouped = binary::encode(&text);
        assert_eq!(binary::decode(&grouped).unwrap(), text);
        let compact: String = grouped.chars().filter(|c| *c != ' ').collect();
        assert_eq!(binary::decode(&compact).unwrap(), text);
    }
}

#[test]
fn test_roundtrip() {
    let original = "Hello";
    let encoded = binary::encode(original);
    let decoded = binary::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_invalid_length() {
    assert!(binary::decode("0100").is_err());
}

#[test]
fn test_decode_empty() {
    assert_eq!(binary::decode("").unwrap(), "");
}

#[test]
fn test_decode_skips_non_bits() {
    assert_eq!(binary::decode("01001000\n01101001").unwrap(), "Hi");
}

#[test]
fn test_ctf_flag_roundtrip() {
    let original = "flag{binary_decode}";
    let encoded = binary::encode(original);
    assert_eq!(binary::decode(&encoded).unwrap(), original);
}

#[test]
fn test_decode_compact_roundtrip() {
    for original in ["Hello", "flag{binary_decode}", "こんにちは"] {
        let compact: String = binary::encode(original)
            .chars()
            .filter(|c| *c != ' ')
            .collect();
        assert_eq!(binary::decode(&compact).unwrap(), original);
    }
}
