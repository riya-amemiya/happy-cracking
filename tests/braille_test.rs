use happy_cracking::crypto::braille;

#[test]
fn test_encode_basic() {
    assert_eq!(braille::encode("A"), "\u{2801}");
}

#[test]
fn test_encode_hello() {
    assert_eq!(
        braille::encode("HELLO"),
        "\u{2813}\u{2811}\u{2807}\u{2807}\u{2815}"
    );
}

#[test]
fn test_decode_basic() {
    assert_eq!(braille::decode("\u{2801}").unwrap(), "A");
}

#[test]
fn test_decode_hello() {
    assert_eq!(
        braille::decode("\u{2813}\u{2811}\u{2807}\u{2807}\u{2815}").unwrap(),
        "HELLO"
    );
}

#[test]
fn test_encode_with_space() {
    assert_eq!(
        braille::encode("HI THERE"),
        "\u{2813}\u{280A}\u{2800}\u{281E}\u{2813}\u{2811}\u{2817}\u{2811}"
    );
}

#[test]
fn test_decode_with_space() {
    assert_eq!(
        braille::decode("\u{2813}\u{280A}\u{2800}\u{281E}\u{2813}\u{2811}\u{2817}\u{2811}")
            .unwrap(),
        "HI THERE"
    );
}

#[test]
fn test_roundtrip() {
    let original = "HELLO WORLD";
    let encoded = braille::encode(original);
    let decoded = braille::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(braille::encode("hello"), braille::encode("HELLO"));
}

#[test]
fn test_encode_numbers() {
    // 1 is encoded as number prefix + A pattern
    assert_eq!(braille::encode("1"), "\u{283C}\u{2801}");
}

#[test]
fn test_decode_numbers() {
    assert_eq!(braille::decode("\u{283C}\u{2801}").unwrap(), "1");
}

#[test]
fn test_number_roundtrip() {
    let original = "A1B2";
    let encoded = braille::encode(original);
    let decoded = braille::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_encode_empty() {
    assert_eq!(braille::encode(""), "");
}

#[test]
fn test_decode_empty() {
    assert_eq!(braille::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = braille::encode("FLAG");
    let decoded = braille::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_full_alphabet_roundtrip() {
    let alpha = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let encoded = braille::encode(alpha);
    let decoded = braille::decode(&encoded).unwrap();
    assert_eq!(decoded, alpha);
}

#[test]
fn test_encode_zero() {
    // 0 uses J pattern
    assert_eq!(braille::encode("0"), "\u{283C}\u{281A}");
}

#[test]
fn test_decode_zero() {
    assert_eq!(braille::decode("\u{283C}\u{281A}").unwrap(), "0");
}
