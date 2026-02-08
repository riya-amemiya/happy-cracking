pub fn rot13(input: &str) -> String {
    rotate(input, 13)
}

// ROT47: shifts ASCII 33('!') through 126('~') by 47 positions
pub fn rot47(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            let code = c as u8;
            if (33..=126).contains(&code) {
                ((code - 33 + 47) % 94 + 33) as char
            } else {
                c
            }
        })
        .collect()
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
