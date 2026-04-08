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

// Performance: build a reverse lookup table mapping ASCII chars to their index
// in the Polybius square. This turns repeated O(25) linear scans into O(1)
// array lookups, which matters when processing many characters per
// encrypt/decrypt call.
pub fn build_reverse_lookup(square: &[char]) -> [u8; 128] {
    let mut table = [0xFF_u8; 128];
    for (i, &ch) in square.iter().enumerate() {
        let code = ch as usize;
        if code < 128 {
            table[code] = i as u8;
        }
    }
    table
}

// Performance: O(1) lookup using the precomputed reverse table instead of
// the O(25) linear scan in find_in_square.
pub fn find_in_square_fast(reverse: &[u8; 128], c: char) -> Option<(usize, usize)> {
    let code = c as usize;
    if code >= 128 {
        return None;
    }
    let idx = reverse[code];
    if idx == 0xFF {
        return None;
    }
    let idx = idx as usize;
    Some((idx / 5, idx % 5))
}
