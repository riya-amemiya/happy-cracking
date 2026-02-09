use happy_cracking::crypto::gronsfeld;

#[test]
fn test_encrypt_basic() {
    // G+3=J, R+1=S, O+4=S, N+1=O, S+5=X, F+3=I, E+1=F, L+4=P, D+1=E
    assert_eq!(
        gronsfeld::encrypt("GRONSFELD", "31415").unwrap(),
        "JSSOXIFPE"
    );
}

#[test]
fn test_decrypt_basic() {
    assert_eq!(
        gronsfeld::decrypt("JSSOXIFPE", "31415").unwrap(),
        "GRONSFELD"
    );
}

#[test]
fn test_encrypt_lowercase() {
    assert_eq!(gronsfeld::encrypt("hello", "123").unwrap(), "igomq");
}

#[test]
fn test_decrypt_lowercase() {
    assert_eq!(gronsfeld::decrypt("igomq", "123").unwrap(), "hello");
}

#[test]
fn test_preserve_non_alpha() {
    assert_eq!(
        gronsfeld::encrypt("HELLO, WORLD!", "31415").unwrap(),
        "KFPMT, ZPVMI!"
    );
}

#[test]
fn test_roundtrip() {
    let original = "The quick brown fox";
    let key = "271828";
    let encrypted = gronsfeld::encrypt(original, key).unwrap();
    let decrypted = gronsfeld::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_preserve_case() {
    let original = "HeLLo WoRLd";
    let key = "31415";
    let encrypted = gronsfeld::encrypt(original, key).unwrap();
    let decrypted = gronsfeld::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}

#[test]
fn test_zero_key() {
    // Key of all zeros should produce identical output
    assert_eq!(gronsfeld::encrypt("HELLO", "00000").unwrap(), "HELLO");
}

#[test]
fn test_empty_input() {
    assert_eq!(gronsfeld::encrypt("", "123").unwrap(), "");
}

#[test]
fn test_empty_key_error() {
    assert!(gronsfeld::encrypt("HELLO", "").is_err());
}

#[test]
fn test_non_digit_key_error() {
    assert!(gronsfeld::encrypt("HELLO", "12A45").is_err());
    assert!(gronsfeld::encrypt("HELLO", "abc").is_err());
}

#[test]
fn test_single_digit_key() {
    // Single digit key is like Caesar cipher
    assert_eq!(gronsfeld::encrypt("ABC", "3").unwrap(), "DEF");
    assert_eq!(gronsfeld::decrypt("DEF", "3").unwrap(), "ABC");
}

#[test]
fn test_wrap_around() {
    // Z + 1 = A
    assert_eq!(gronsfeld::encrypt("Z", "1").unwrap(), "A");
    // A - 1 = Z
    assert_eq!(gronsfeld::decrypt("A", "1").unwrap(), "Z");
}

#[test]
fn test_ctf_flag() {
    let original = "flag{gronsfeld}";
    let key = "31415";
    let encrypted = gronsfeld::encrypt(original, key).unwrap();
    let decrypted = gronsfeld::decrypt(&encrypted, key).unwrap();
    assert_eq!(decrypted, original);
}
