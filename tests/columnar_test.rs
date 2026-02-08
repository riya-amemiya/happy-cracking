use happy_cracking::crypto::columnar;

#[test]
fn test_encrypt_basic() {
    // Key "HACK" -> order H=2, A=0, C=1, K=3
    // "HELLOWORLD" padded to "HELLOWORLDXX"
    // Grid: H E L L
    //        O W O R
    //        L D X X
    // Read by key order (A=col1, C=col2, H=col0, K=col3): EWDLOX HOLLRX
    let result = columnar::encrypt("HELLOWORLD", "HACK").unwrap();
    assert_eq!(result, "EWDLOXHOLLRX");
}

#[test]
fn test_decrypt_basic() {
    let result = columnar::decrypt("EWDLOXHOLLRX", "HACK").unwrap();
    assert_eq!(result, "HELLOWORLDXX");
}

#[test]
fn test_roundtrip() {
    let original = "THEQUICKBROWNFOX";
    let key = "ZEBRA";
    let encrypted = columnar::encrypt(original, key).unwrap();
    let decrypted = columnar::decrypt(&encrypted, key).unwrap();
    // May have trailing padding X's
    assert!(decrypted.starts_with(original));
}

#[test]
fn test_padding() {
    // "HELLO" with key "ABC" (len 3) -> padded to 6 chars with X
    let encrypted = columnar::encrypt("HELLO", "ABC").unwrap();
    assert_eq!(encrypted.len(), 6);
}

#[test]
fn test_exact_fit_no_padding() {
    // "ABCDEF" with key "AB" (len 2) -> exactly 6, no padding needed
    let encrypted = columnar::encrypt("ABCDEF", "AB").unwrap();
    assert_eq!(encrypted.len(), 6);
}

#[test]
fn test_empty_input() {
    assert_eq!(columnar::encrypt("", "KEY").unwrap(), "");
    assert_eq!(columnar::decrypt("", "KEY").unwrap(), "");
}

#[test]
fn test_empty_key_error() {
    assert!(columnar::encrypt("HELLO", "").is_err());
}

#[test]
fn test_invalid_key_error() {
    assert!(columnar::encrypt("HELLO", "KEY123").is_err());
}

#[test]
fn test_decrypt_invalid_length_error() {
    // Ciphertext length 5 is not divisible by key length 3
    assert!(columnar::decrypt("HELLO", "ABC").is_err());
}

#[test]
fn test_ctf_flag() {
    let original = "FLAGCAPTURETHEFLAG";
    let key = "CTF";
    let encrypted = columnar::encrypt(original, key).unwrap();
    let decrypted = columnar::decrypt(&encrypted, key).unwrap();
    assert!(decrypted.starts_with(original));
}
