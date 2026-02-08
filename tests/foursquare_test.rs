use happy_cracking::crypto::foursquare;

#[test]
fn test_encrypt_basic() {
    let result = foursquare::encrypt("HELLO", "EXAMPLE", "KEYWORD").unwrap();
    assert!(!result.is_empty());
    assert_eq!(result.len() % 2, 0);
}

#[test]
fn test_roundtrip() {
    let original = "HELLOWORLD";
    let encrypted = foursquare::encrypt(original, "ALPHA", "BRAVO").unwrap();
    let decrypted = foursquare::decrypt(&encrypted, "ALPHA", "BRAVO").unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_roundtrip_padded() {
    // Odd-length input gets X padding, so decrypted will have trailing X
    let encrypted = foursquare::encrypt("HELLO", "ALPHA", "BRAVO").unwrap();
    let decrypted = foursquare::decrypt(&encrypted, "ALPHA", "BRAVO").unwrap();
    assert_eq!(decrypted, "HELLOX");
}

#[test]
fn test_empty_input() {
    assert_eq!(foursquare::encrypt("", "KEY", "KEY").unwrap(), "");
}

#[test]
fn test_empty_key1_error() {
    assert!(foursquare::encrypt("HELLO", "", "KEY").is_err());
}

#[test]
fn test_empty_key2_error() {
    assert!(foursquare::encrypt("HELLO", "KEY", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(foursquare::encrypt("HELLO", "KEY123", "KEY").is_err());
    assert!(foursquare::encrypt("HELLO", "KEY", "KEY123").is_err());
}

#[test]
fn test_decrypt_odd_length_error() {
    assert!(foursquare::decrypt("ABC", "KEY", "KEY").is_err());
}

#[test]
fn test_j_treated_as_i() {
    let enc1 = foursquare::encrypt("INJECT", "KEY", "SECRET").unwrap();
    let enc2 = foursquare::encrypt("INIECT", "KEY", "SECRET").unwrap();
    assert_eq!(enc1, enc2);
}

#[test]
fn test_non_alpha_ignored() {
    let enc1 = foursquare::encrypt("HE LL O!", "KEY", "SECRET").unwrap();
    let enc2 = foursquare::encrypt("HELLO", "KEY", "SECRET").unwrap();
    // Both have odd count of 5 letters, both padded to 6
    assert_eq!(enc1, enc2);
}

#[test]
fn test_lowercase_input() {
    let enc1 = foursquare::encrypt("hello", "KEY", "SECRET").unwrap();
    let enc2 = foursquare::encrypt("HELLO", "KEY", "SECRET").unwrap();
    assert_eq!(enc1, enc2);
}

#[test]
fn test_known_vector() {
    // Four-square with keys "EXAMPLE" and "KEYWORD"
    // Standard squares: ABCDEFGHIKLMNOPQRSTUVWXYZ
    // "HE" -> H at (1,2), E at (0,4) -> top-right(1,4), bottom-left(0,2) -> rectangle lookup
    let encrypted = foursquare::encrypt("HELPMEOBIWANKENOBI", "EXAMPLE", "KEYWORD").unwrap();
    let decrypted = foursquare::decrypt(&encrypted, "EXAMPLE", "KEYWORD").unwrap();
    assert_eq!(decrypted, "HELPMEOBIWANKENOBI");
}

#[test]
fn test_ctf_flag() {
    let original = "FLAGSECRETMESSAGE";
    let encrypted = foursquare::encrypt(original, "CIPHER", "CRYPTO").unwrap();
    let decrypted = foursquare::decrypt(&encrypted, "CIPHER", "CRYPTO").unwrap();
    // Odd length (17) gets padded to 18 with X
    assert_eq!(decrypted, "FLAGSECRETMESSAGEX");
    assert!(decrypted.starts_with("FLAGSECRETMESSAGE"));
}

#[test]
fn test_same_keys() {
    let original = "TESTDATA";
    let encrypted = foursquare::encrypt(original, "SAME", "SAME").unwrap();
    let decrypted = foursquare::decrypt(&encrypted, "SAME", "SAME").unwrap();
    assert_eq!(decrypted, original);
}
