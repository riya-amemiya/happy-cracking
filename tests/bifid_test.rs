use happy_cracking::crypto::bifid;

#[test]
fn test_encrypt_basic() {
    // With key "A" (square: ABCDEFGHIKLMNOPQRSTUVWXYZ)
    // "HELLO" -> H=(1,2), E=(0,4), L=(2,0), L=(2,0), O=(2,3)
    // rows: 1,0,2,2,2  cols: 2,4,0,0,3
    // combined: [1,0,2,2,2,2,4,0,0,3]
    // pairs: (1,0)->F, (2,2)->N, (2,2)->N, (4,0)->V, (0,3)->D
    let result = bifid::encrypt("HELLO", "A").unwrap();
    assert_eq!(result, "FNNVD");
}

#[test]
fn test_decrypt_basic() {
    let result = bifid::decrypt("FNNVD", "A").unwrap();
    assert_eq!(result, "HELLO");
}

#[test]
fn test_roundtrip() {
    let original = "ATTACKATDAWN";
    let key = "SECRET";
    let encrypted = bifid::encrypt(original, key).unwrap();
    let decrypted = bifid::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_roundtrip_with_keyword() {
    let original = "THEQUICKBROWNFOX";
    let key = "KEYWORD";
    let encrypted = bifid::encrypt(original, key).unwrap();
    let decrypted = bifid::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, "THEQUICKBROWNFOX");
}

#[test]
fn test_j_treated_as_i() {
    let enc_i = bifid::encrypt("INJECT", "KEY").unwrap();
    let enc_j = bifid::encrypt("INIECT", "KEY").unwrap();
    assert_eq!(enc_i, enc_j);
}

#[test]
fn test_non_alpha_ignored() {
    let enc1 = bifid::encrypt("HE LL O!", "KEY").unwrap();
    let enc2 = bifid::encrypt("HELLO", "KEY").unwrap();
    assert_eq!(enc1, enc2);
}

#[test]
fn test_empty_input() {
    assert_eq!(bifid::encrypt("", "KEY").unwrap(), "");
    assert_eq!(bifid::decrypt("", "KEY").unwrap(), "");
}

#[test]
fn test_empty_key_error() {
    assert!(bifid::encrypt("HELLO", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(bifid::encrypt("HELLO", "KEY123").is_err());
}

#[test]
fn test_single_char() {
    let key = "A";
    let encrypted = bifid::encrypt("A", key).unwrap();
    let decrypted = bifid::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, "A");
}

#[test]
fn test_ctf_flag() {
    let key = "CIPHER";
    let original = "FLAGSECRET";
    let encrypted = bifid::encrypt(original, key).unwrap();
    let decrypted = bifid::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}
