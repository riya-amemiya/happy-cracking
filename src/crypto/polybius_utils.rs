// Shared Polybius square builder used by playfair, bifid, and foursquare ciphers.

pub fn build_polybius_square(key: &str) -> Vec<char> {
    let mut seen = [false; 26];
    // I/J merged: J is treated as I
    seen[(b'J' - b'A') as usize] = true;
    let mut square = Vec::with_capacity(25);

    for c in key
        .to_uppercase()
        .chars()
        .chain('A'..='Z')
        .filter(|c| c.is_ascii_uppercase())
    {
        let c = if c == 'J' { 'I' } else { c };
        let idx = (c as u8 - b'A') as usize;
        if !seen[idx] {
            seen[idx] = true;
            square.push(c);
        }
    }

    square
}

pub fn find_in_square(square: &[char], c: char) -> Option<(usize, usize)> {
    let idx = square.iter().position(|&s| s == c)?;
    Some((idx / 5, idx % 5))
}
