use happy_cracking::crypto::hexdump;

#[test]
fn test_dump_hello() {
    let result = hexdump::dump("Hello World!");
    assert!(result.starts_with("00000000: "));
    assert!(result.contains("Hello World!"));
}

#[test]
fn test_dump_empty() {
    let result = hexdump::dump("");
    assert!(result.is_empty());
}

#[test]
fn test_dump_bytes_single_line() {
    let result = hexdump::dump_bytes(b"ABCD");
    assert!(result.starts_with("00000000: "));
    assert!(result.contains("ABCD"));
}

#[test]
fn test_dump_multiline() {
    let data = "This is a longer string that exceeds sixteen bytes";
    let result = hexdump::dump(data);
    let lines: Vec<&str> = result.lines().collect();
    assert!(lines.len() > 1);
    assert!(lines[0].starts_with("00000000: "));
    assert!(lines[1].starts_with("00000010: "));
}

#[test]
fn test_reverse_roundtrip() {
    let original = "Hello World!";
    let dumped = hexdump::dump(original);
    let reversed = hexdump::reverse(&dumped).unwrap();
    assert_eq!(reversed, original.as_bytes());
}

#[test]
fn test_reverse_multiline_roundtrip() {
    let original = "The quick brown fox jumps over the lazy dog";
    let dumped = hexdump::dump(original);
    let reversed = hexdump::reverse(&dumped).unwrap();
    assert_eq!(reversed, original.as_bytes());
}

#[test]
fn test_dump_non_printable() {
    let data: &[u8] = &[0x00, 0x01, 0x41, 0x42, 0x7f, 0xff];
    let result = hexdump::dump_bytes(data);
    // Non-printable characters should appear as '.'
    assert!(result.contains("..AB.."));
}

#[test]
fn test_dump_exactly_16_bytes() {
    let data = "0123456789ABCDEF";
    let result = hexdump::dump(data);
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_dump_17_bytes() {
    let data = "0123456789ABCDEFG";
    let result = hexdump::dump(data);
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines.len(), 2);
}

#[test]
fn test_flag_format_roundtrip() {
    let original = "flag{hexdump_is_useful}";
    let dumped = hexdump::dump(original);
    let reversed = hexdump::reverse(&dumped).unwrap();
    assert_eq!(String::from_utf8(reversed).unwrap(), original);
}
