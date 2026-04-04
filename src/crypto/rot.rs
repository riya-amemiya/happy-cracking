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

    // Security: avoid unsafe – use safe conversion to prevent undefined behavior
    // if a future refactor accidentally produces invalid UTF-8 bytes.
    String::from_utf8(bytes).expect("rot47: only ASCII bytes were modified")
}

pub fn rotate(input: &str, shift: u8) -> String {
    let mut bytes = input.as_bytes().to_vec();

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_lowercase() { b'a' } else { b'A' };
            *b = (*b - base + shift) % 26 + base;
        }
    }

    // Security: avoid unsafe – use safe conversion to prevent undefined behavior
    // if a future refactor accidentally produces invalid UTF-8 bytes.
    String::from_utf8(bytes).expect("rotate: only ASCII bytes were modified")
}
