pub fn rot13(input: &str) -> String {
    rotate(input, 13)
}

pub fn rotate(input: &str, shift: u8) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_lowercase() { b'a' } else { b'A' };
                let rotated = (c as u8 - base + shift) % 26 + base;
                rotated as char
            } else {
                c
            }
        })
        .collect()
}
