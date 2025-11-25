use happy_cracking::crypto::rot;

#[test]
fn rot13_hello() {
    assert_eq!(rot::rot13("Hello"), "Uryyb");
}

#[test]
fn rot13_roundtrip() {
    let original = "Hello, World!";
    let encoded = rot::rot13(original);
    let decoded = rot::rot13(&encoded);
    assert_eq!(decoded, original);
}

#[test]
fn rot13_preserves_non_alpha() {
    assert_eq!(rot::rot13("Hello, World! 123"), "Uryyb, Jbeyq! 123");
}

#[test]
fn rot13_uppercase_lowercase() {
    assert_eq!(rot::rot13("ABCxyz"), "NOPklm");
}

#[test]
fn rotate_by_0() {
    assert_eq!(rot::rotate("Hello", 0), "Hello");
}

#[test]
fn rotate_by_1() {
    assert_eq!(rot::rotate("ABC", 1), "BCD");
}

#[test]
fn rotate_by_25() {
    assert_eq!(rot::rotate("B", 25), "A");
}

#[test]
fn rotate_by_26_is_identity() {
    assert_eq!(rot::rotate("Hello", 26), "Hello");
}

#[test]
fn rotate_wraps_around() {
    assert_eq!(rot::rotate("XYZ", 3), "ABC");
}

#[test]
fn caesar_encrypt_decrypt() {
    let plaintext = "The quick brown fox";
    let shift = 7;
    let encrypted = rot::rotate(plaintext, shift);
    let decrypted = rot::rotate(&encrypted, 26 - shift);
    assert_eq!(decrypted, plaintext);
}
