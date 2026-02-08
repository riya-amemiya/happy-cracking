use happy_cracking::crypto::base85;

#[test]
fn encode_empty_string() {
    assert_eq!(base85::encode(""), "");
}

#[test]
fn encode_hello() {
    assert_eq!(base85::encode("Hello"), "87cURDZ");
}

#[test]
fn encode_four_zeros() {
    assert_eq!(base85::encode("\0\0\0\0"), "z");
}

#[test]
fn encode_single_char() {
    assert_eq!(base85::encode("a"), "@/");
}

#[test]
fn encode_two_chars() {
    assert_eq!(base85::encode("ab"), "@:B");
}

#[test]
fn encode_three_chars() {
    assert_eq!(base85::encode("abc"), "@:E^");
}

#[test]
fn encode_four_chars() {
    assert_eq!(base85::encode("abcd"), "@:E_W");
}

#[test]
fn decode_empty_string() {
    assert_eq!(base85::decode("").unwrap(), "");
}

#[test]
fn decode_hello() {
    assert_eq!(base85::decode("87cURDZ").unwrap(), "Hello");
}

#[test]
fn decode_z_shortcut() {
    assert_eq!(base85::decode("z").unwrap(), "\0\0\0\0");
}

#[test]
fn roundtrip() {
    let original = "flag{base85_encoding}";
    let encoded = base85::encode(original);
    let decoded = base85::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn roundtrip_various_lengths() {
    for s in ["a", "ab", "abc", "abcd", "abcde", "Hello, World!"] {
        let encoded = base85::encode(s);
        let decoded = base85::decode(&encoded).unwrap();
        assert_eq!(decoded, s);
    }
}

#[test]
fn decode_invalid_char() {
    assert!(base85::decode("{").is_err());
}
