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

    // Security: use safe conversion to prevent undefined behavior if
    // future changes accidentally break the ASCII-only invariant.
    String::from_utf8(bytes).expect("rot47: internal error: produced invalid UTF-8")
}

pub fn rotate(input: &str, shift: u8) -> String {
    let mut bytes = input.as_bytes().to_vec();

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_lowercase() { b'a' } else { b'A' };
            *b = (*b - base + shift) % 26 + base;
        }
    }

    // Security: use safe conversion to prevent undefined behavior if
    // future changes accidentally break the ASCII-only invariant.
    String::from_utf8(bytes).expect("rotate: internal error: produced invalid UTF-8")
}
