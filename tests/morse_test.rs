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
