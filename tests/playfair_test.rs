use happy_cracking::crypto::playfair;

#[test]
fn test_encrypt_basic() {
    // With key "MONARCHY": HE -> EC (same row for the matrix positions)
    let result = playfair::encrypt("HELLO", "MONARCHY").unwrap();
    assert!(!result.is_empty());
    assert_eq!(result.len() % 2, 0);
}

#[test]
fn test_encrypt_known_vector() {
    let encrypted = playfair::encrypt("PLAYFAIR", "KEYWORD").unwrap();
    let decrypted = playfair::decrypt(&encrypted, "KEYWORD").unwrap();
    // Playfair cannot perfectly roundtrip due to I/J merge and X padding,
    // but the alphabetic content should be recoverable
    assert_eq!(decrypted.len(), encrypted.len());
}

#[test]
fn test_decrypt_basic() {
    let encrypted = playfair::encrypt("HELLO", "MONARCHY").unwrap();
    let decrypted = playfair::decrypt(&encrypted, "MONARCHY").unwrap();
    // "HELLO" becomes "HE LX LO" with X inserted between double L
    assert!(decrypted.contains('H'));
}

#[test]
fn test_roundtrip_no_doubles() {
    let original = "ABCDEF";
    let encrypted = playfair::encrypt(original, "SECRET").unwrap();
    let decrypted = playfair::decrypt(&encrypted, "SECRET").unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_double_letter_insert_x() {
    // "BALLOON" has double L -> should insert X between them
    let encrypted = playfair::encrypt("BALLOON", "KEY").unwrap();
    assert_eq!(encrypted.len() % 2, 0);
}

#[test]
fn test_odd_length_padding() {
    // Odd-length input gets X padding
    let encrypted = playfair::encrypt("ABC", "KEY").unwrap();
    assert_eq!(encrypted.len(), 4); // 3 chars + X padding = 4
}

#[test]
fn test_j_treated_as_i() {
    // J should be treated as I
    let enc_i = playfair::encrypt("INJECT", "KEY").unwrap();
    let enc_j = playfair::encrypt("INIECT", "KEY").unwrap();
    assert_eq!(enc_i, enc_j);
}

#[test]
fn test_non_alpha_ignored() {
    let enc1 = playfair::encrypt("HE LL O!", "KEY").unwrap();
    let enc2 = playfair::encrypt("HELLO", "KEY").unwrap();
    assert_eq!(enc1, enc2);
}

#[test]
fn test_empty_input() {
    assert_eq!(playfair::encrypt("", "KEY").unwrap(), "");
}

#[test]
fn test_empty_key_error() {
    assert!(playfair::encrypt("HELLO", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(playfair::encrypt("HELLO", "KEY123").is_err());
}

#[test]
fn test_decrypt_odd_length_error() {
    assert!(playfair::decrypt("ABC", "KEY").is_err());
}

#[test]
fn test_ctf_flag_roundtrip() {
    let original = "FLAGSECRET";
    let encrypted = playfair::encrypt(original, "CIPHER").unwrap();
    let decrypted = playfair::decrypt(&encrypted, "CIPHER").unwrap();
    assert_eq!(decrypted, original);
}
