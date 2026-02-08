use happy_cracking::crypto::polybius;

#[test]
fn test_encrypt_basic() {
    // A=11, B=12, C=13
    assert_eq!(polybius::encrypt("ABC").unwrap(), "11 12 13");
}

#[test]
fn test_decrypt_basic() {
    assert_eq!(polybius::decrypt("11 12 13").unwrap(), "ABC");
}

#[test]
fn test_encrypt_hello() {
    // H=23, E=15, L=31, L=31, O=34
    assert_eq!(polybius::encrypt("HELLO").unwrap(), "23 15 31 31 34");
}

#[test]
fn test_decrypt_hello() {
    assert_eq!(polybius::decrypt("23 15 31 31 34").unwrap(), "HELLO");
}

#[test]
fn test_roundtrip() {
    let original = "THEQUICKBROWNFOX";
    let encrypted = polybius::encrypt(original).unwrap();
    let decrypted = polybius::decrypt(&encrypted).unwrap();
    // J maps to I, but this text has no J
    assert_eq!(decrypted, original);
}

#[test]
fn test_ij_merge() {
    // J is treated as I, both map to position (2,4) = 24
    // Wait, I is at index 8 in grid -> row 8/5+1=2, col 8%5+1=4 -> "24"
    let enc_i = polybius::encrypt("I").unwrap();
    let enc_j = polybius::encrypt("J").unwrap();
    assert_eq!(enc_i, enc_j);
    assert_eq!(enc_i, "24");
}

#[test]
fn test_encrypt_z() {
    // Z is at index 24 in grid -> row 24/5+1=5, col 24%5+1=5 -> "55"
    assert_eq!(polybius::encrypt("Z").unwrap(), "55");
}

#[test]
fn test_encrypt_lowercase() {
    assert_eq!(polybius::encrypt("abc").unwrap(), "11 12 13");
}

#[test]
fn test_non_alpha_ignored() {
    assert_eq!(polybius::encrypt("A B C").unwrap(), "11 12 13");
}

#[test]
fn test_encrypt_empty() {
    assert_eq!(polybius::encrypt("").unwrap(), "");
}

#[test]
fn test_decrypt_empty() {
    assert_eq!(polybius::decrypt("").unwrap(), "");
}

#[test]
fn test_decrypt_invalid_pair() {
    assert!(polybius::decrypt("67").is_err());
}

#[test]
fn test_decrypt_non_numeric() {
    assert!(polybius::decrypt("AB").is_err());
}

#[test]
fn test_decrypt_wrong_length_token() {
    assert!(polybius::decrypt("123").is_err());
}

#[test]
fn test_full_alphabet() {
    let alpha = "ABCDEFGHIKLMNOPQRSTUVWXYZ";
    let encrypted = polybius::encrypt(alpha).unwrap();
    let decrypted = polybius::decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, alpha);
}
