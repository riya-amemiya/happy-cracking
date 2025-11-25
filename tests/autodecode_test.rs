use happy_cracking::crypto::autodecode;

#[test]
fn test_detect_base64() {
    let results = autodecode::detect_and_decode("SGVsbG8gV29ybGQ=");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Base64" && dec == "Hello World")
    );
}

#[test]
fn test_detect_hex() {
    let results = autodecode::detect_and_decode("48656c6c6f");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Hex" && dec == "Hello")
    );
}

#[test]
fn test_detect_url() {
    let results = autodecode::detect_and_decode("Hello%20World");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "URL" && dec == "Hello World")
    );
}

#[test]
fn test_detect_binary() {
    let results = autodecode::detect_and_decode("01001000 01101001");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Binary" && dec == "Hi")
    );
}

#[test]
fn test_detect_morse() {
    let results = autodecode::detect_and_decode(".... ..");
    assert!(
        results
            .iter()
            .any(|(enc, dec)| *enc == "Morse" && dec == "HI")
    );
}
