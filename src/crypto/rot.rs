pub fn rot13(input: &str) -> String {
    rotate(input, 13)
}

// ROT47: shifts ASCII 33('!') through 126('~') by 47 positions
pub fn rot47(input: &str) -> String {
    let mut bytes = input.as_bytes().to_vec();

    for b in &mut bytes {
        if (33..=126).contains(b) {
            *b = (*b - 33 + 47) % 94 + 33;
        }
    }

    // Safety: we only modify printable ASCII characters (33..=126)
    // to other printable ASCII characters within the same range.
    // Therefore, any valid UTF-8 sequences are preserved correctly.
    unsafe { String::from_utf8_unchecked(bytes) }
}

pub fn rotate(input: &str, shift: u8) -> String {
    let mut bytes = input.as_bytes().to_vec();

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_lowercase() { b'a' } else { b'A' };
            *b = (*b - base + shift) % 26 + base;
        }
    }

    // Safety: we only modify ASCII alphabetic characters (which are valid 1-byte UTF-8)
    // to other ASCII alphabetic characters.
    // Therefore, the byte sequence remains valid UTF-8.
    unsafe { String::from_utf8_unchecked(bytes) }
}
