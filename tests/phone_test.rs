use happy_cracking::crypto::phone;

#[test]
fn test_encode_basic() {
    // A is key 2, first letter -> "2"
    assert_eq!(phone::encode("A"), "2");
}

#[test]
fn test_encode_abc() {
    // A, B, C all on key 2 -> same key, separated by dashes
    assert_eq!(phone::encode("ABC"), "2-22-222");
}

#[test]
fn test_encode_hello() {
    // H=44, E=33, L=555, L=555, O=666
    // H(4) E(3) different keys -> space. L(5) L(5) same key -> dash. L(5) O(6) different -> space.
    assert_eq!(phone::encode("HELLO"), "44 33 555-555 666");
}

#[test]
fn test_decode_basic() {
    assert_eq!(phone::decode("2").unwrap(), "A");
}

#[test]
fn test_decode_abc() {
    assert_eq!(phone::decode("2-22-222").unwrap(), "ABC");
}

#[test]
fn test_decode_hello() {
    assert_eq!(phone::decode("44 33 555-555 666").unwrap(), "HELLO");
}

#[test]
fn test_encode_with_space() {
    // space -> "0"
    assert_eq!(phone::encode("HI THERE"), "44-444 0 8 44 33 777 33");
}

#[test]
fn test_decode_with_space() {
    assert_eq!(
        phone::decode("44-444 0 8 44 33 777 33").unwrap(),
        "HI THERE"
    );
}

#[test]
fn test_encode_s_and_z() {
    // S=7777, Z=9999
    assert_eq!(phone::encode("SZ"), "7777 9999");
}

#[test]
fn test_decode_s_and_z() {
    assert_eq!(phone::decode("7777 9999").unwrap(), "SZ");
}

#[test]
fn test_roundtrip() {
    let original = "HELLO WORLD";
    let encoded = phone::encode(original);
    let decoded = phone::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_encode_lowercase() {
    assert_eq!(phone::encode("hello"), phone::encode("HELLO"));
}

#[test]
fn test_encode_empty() {
    assert_eq!(phone::encode(""), "");
}

#[test]
fn test_decode_empty() {
    assert_eq!(phone::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = phone::encode("FLAG");
    let decoded = phone::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_decode_invalid_digit() {
    assert!(phone::decode("1").is_err());
}

#[test]
fn test_encode_ad() {
    // A(key 2) D(key 3) different keys -> space
    assert_eq!(phone::encode("AD"), "2 3");
}
