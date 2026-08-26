use happy_cracking::crypto::morse;

#[test]
fn test_encode_sos() {
    assert_eq!(morse::encode("SOS"), "... --- ...");
}

#[test]
fn test_encode_hello() {
    assert_eq!(morse::encode("HELLO"), ".... . .-.. .-.. ---");
}

#[test]
fn test_encode_with_space() {
    assert_eq!(morse::encode("HI THERE"), ".... .. / - .... . .-. .");
}

#[test]
fn test_decode_sos() {
    assert_eq!(morse::decode("... --- ...").unwrap(), "SOS");
}

#[test]
fn test_decode_with_space() {
    assert_eq!(
        morse::decode(".... .. / - .... . .-. .").unwrap(),
        "HI THERE"
    );
}

#[test]
fn test_roundtrip() {
    let original = "HELLO WORLD";
    let encoded = morse::encode(original);
    let decoded = morse::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_empty_input() {
    assert_eq!(morse::encode(""), "");
    assert_eq!(morse::decode("").unwrap(), "");
}

#[test]
fn test_ctf_flag() {
    let encoded = morse::encode("FLAG");
    let decoded = morse::decode(&encoded).unwrap();
    assert_eq!(decoded, "FLAG");
}

#[test]
fn test_roundtrip_digits_and_punctuation() {
    let original = "SOS 123 ...";
    let encoded = morse::encode(original);
    let decoded = morse::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn test_decode_unknown_token() {
    let err = morse::decode("......").unwrap_err().to_string();
    assert!(err.contains("Unknown Morse code"));
}

#[test]
fn test_decode_rejects_non_morse_chars() {
    assert!(morse::decode("abc").is_err());
}
