use happy_cracking::crypto::strings;

#[test]
fn test_extract_flag_from_noise() {
    let mut data = vec![0x00, 0x01, 0x02, 0xFF, 0xFE];
    data.extend_from_slice(b"flag{strings_are_useful}");
    data.extend_from_slice(&[0x00, 0x80, 0x90]);
    let found = strings::extract_ascii(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "flag{strings_are_useful}"));
}

#[test]
fn test_min_len_filtering() {
    let mut data = vec![0x00];
    data.extend_from_slice(b"abc");
    data.push(0x00);
    data.extend_from_slice(b"abcd");
    data.push(0x00);
    let found = strings::extract_ascii(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "abcd"));
    assert!(!found.iter().any(|s| s == "abc"));
}

#[test]
fn test_empty_input() {
    let found = strings::extract_ascii(&[], 4).unwrap();
    assert!(found.is_empty());
}

#[test]
fn test_utf16le_extraction() {
    let data = [0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F, 0x00];
    let found = strings::extract_utf16le(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "Hello"));
}

#[test]
fn test_utf16le_flag() {
    let mut data = vec![0xFF, 0xFE];
    for &b in b"flag{utf16}" {
        data.push(b);
        data.push(0x00);
    }
    let found = strings::extract_utf16le(&data, 4).unwrap();
    assert!(found.iter().any(|s| s == "flag{utf16}"));
}

#[test]
fn test_ascii_min_len_zero_errors() {
    assert!(strings::extract_ascii(b"hello", 0).is_err());
}

#[test]
fn test_utf16le_min_len_zero_errors() {
    assert!(strings::extract_utf16le(b"hello", 0).is_err());
}
