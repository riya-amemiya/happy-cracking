use std::fmt::Write as _;

#[must_use]
pub fn decode_utf8(bytes: &[u8]) -> (Option<char>, usize) {
    let Some(&b0) = bytes.first() else {
        return (None, 0);
    };
    if b0 < 0x80 {
        return (Some(char::from(b0)), 1);
    }
    let (need, lo, hi) = match b0 {
        0xC2..=0xDF => (1, 0x80, 0xBF),
        0xE0 => (2, 0xA0, 0xBF),
        0xE1..=0xEC | 0xEE..=0xEF => (2, 0x80, 0xBF),
        0xED => (2, 0x80, 0x9F),
        0xF0 => (3, 0x90, 0xBF),
        0xF1..=0xF3 => (3, 0x80, 0xBF),
        0xF4 => (3, 0x80, 0x8F),
        _ => return (None, 1),
    };
    for i in 1..=need {
        let Some(&b) = bytes.get(i) else {
            return (None, i);
        };
        let ok = if i == 1 {
            (lo..=hi).contains(&b)
        } else {
            (0x80..=0xBF).contains(&b)
        };
        if !ok {
            return (None, i);
        }
    }
    let len = need + 1;
    (
        std::str::from_utf8(&bytes[..len])
            .ok()
            .and_then(|s| s.chars().next()),
        len,
    )
}

#[must_use]
pub fn debug_bytes(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    let mut pos = 0;
    while pos < bytes.len() {
        let (ch, len) = decode_utf8(&bytes[pos..]);
        let chunk = &bytes[pos..pos + len];
        match ch {
            Some('\0') => out.push_str("\\0"),
            Some(c @ '\x01'..='\x7f') => {
                let _ = write!(out, "{}", (c as u8).escape_ascii());
            }
            Some(c) => {
                let _ = write!(out, "{}", c.escape_debug());
            }
            None => {
                for &b in chunk {
                    let _ = write!(out, "\\x{b:02x}");
                }
            }
        }
        pos += len;
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_bytes_matches_bstr() {
        assert_eq!(debug_bytes(b"\x00"), "\"\\0\"");
        assert_eq!(debug_bytes(b"\xff"), "\"\\xff\"");
        assert_eq!(debug_bytes(b"a\tb\n"), "\"a\\tb\\n\"");
        assert_eq!(debug_bytes("\u{2603}".as_bytes()), "\"\u{2603}\"");
        assert_eq!(debug_bytes(b"\x7f\""), "\"\\x7f\\\"\"");
    }

    #[test]
    fn decode_maximal_subpart() {
        assert_eq!(decode_utf8(b"\xE2\x98"), (None, 2));
        assert_eq!(decode_utf8(b"\xE2\x98x"), (None, 2));
        assert_eq!(decode_utf8(b"\xFF"), (None, 1));
        assert_eq!(decode_utf8(b"\xE2\x98\x83"), (Some('\u{2603}'), 3));
    }
}
