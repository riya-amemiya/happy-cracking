use happy_cracking::crypto::baconian;

#[test]
fn test_encode_basic() {
    // A=AAAAA, B=AAAAB, C=AAABA
    assert_eq!(baconian::encode("ABC").unwrap(), "AAAAA AAAAB AAABA");
}

#[test]
fn test_decode_basic() {
    assert_eq!(baconian::decode("AAAAA AAAAB AAABA").unwrap(), "ABC");
}

#[test]
fn test_encode_hello() {
    let result = baconian::encode("HELLO").unwrap();
    // H=AABBB(7), E=AABAA(4), L=ABABA(10), L=ABABA(10), O=ABBAB(13)
    assert_eq!(result, "AABBB AABAA ABABA ABABA ABBAB");
}

#[test]
fn test_decode_hello() {
    assert_eq!(
        baconian::decode("AABBB AABAA ABABA ABABA ABBAB").unwrap(),
        "HELLO"
    );
}

#[test]
fn test_roundtrip() {
    let original = "THEQUICKBROWNFOX";
    let encoded = baconian::encode(original).unwrap();
    let decoded = baconian::decode(&encoded).unwrap();
    // I/J and U/V are merged, so we compare with that in mind
    assert_eq!(decoded, original);
}

#[test]
fn test_ij_merge() {
    // I and J both encode to ABAAA
    let enc_i = baconian::encode("I").unwrap();
    let enc_j = baconian::encode("J").unwrap();
    assert_eq!(enc_i, enc_j);
}

#[test]
fn test_uv_merge() {
    // U and V both encode to BAABB
    let enc_u = baconian::encode("U").unwrap();
    let enc_v = baconian::encode("V").unwrap();
    assert_eq!(enc_u, enc_v);
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(baconian::encode("abc").unwrap(), "AAAAA AAAAB AAABA");
}

#[test]
fn test_non_alpha_ignored() {
    assert_eq!(baconian::encode("A B C").unwrap(), "AAAAA AAAAB AAABA");
}

#[test]
fn test_encode_empty() {
    assert_eq!(baconian::encode("").unwrap(), "");
}

#[test]
fn test_decode_empty() {
    assert_eq!(baconian::decode("").unwrap(), "");
}

#[test]
fn test_decode_invalid_pattern() {
    assert!(baconian::decode("AAXAA").is_err());
}

#[test]
fn test_decode_wrong_length_pattern() {
    assert!(baconian::decode("AAA").is_err());
}

#[test]
fn test_encode_z() {
    // Z = index 23 = 10111 = BABBB
    assert_eq!(baconian::encode("Z").unwrap(), "BABBB");
}
