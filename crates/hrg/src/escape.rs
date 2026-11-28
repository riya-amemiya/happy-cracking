use std::fmt::Write;

fn hex_digit(nybble: u8) -> char {
    char::from(if nybble < 10 {
        b'0' + nybble
    } else {
        b'A' + nybble - 10
    })
}

fn push_hex(out: &mut String, byte: u8) {
    out.push_str("\\x");
    out.push(hex_digit(byte >> 4));
    out.push(hex_digit(byte & 0xF));
}

fn utf8_char_len(bytes: &[u8]) -> Option<(char, usize)> {
    let lead = *bytes.first()?;
    let width = match lead {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let chunk = bytes.get(..width)?;
    std::str::from_utf8(chunk)
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| (c, width))
}

#[must_use]
pub fn escape(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some(&byte) = rest.first() {
        if let Some((ch, width)) = utf8_char_len(rest) {
            out.push(ch);
            rest = &rest[width..];
            continue;
        }
        rest = &rest[1..];
        match byte {
            0x21..=0x5B | 0x5D..=0x7E => out.push(char::from(byte)),
            0 => out.push_str("\\0"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'\\' => out.push_str("\\\\"),
            _ => push_hex(&mut out, byte),
        }
    }
    out
}

fn hex_value(ch: char) -> Option<u8> {
    ch.to_digit(16).map(|d| d as u8)
}

fn push_char(out: &mut Vec<u8>, ch: char) {
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
}

#[must_use]
pub fn unescape(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            push_char(&mut out, ch);
            continue;
        }
        let Some(next) = chars.next() else {
            out.push(b'\\');
            break;
        };
        match next {
            '0' => out.push(0),
            '\\' => out.push(b'\\'),
            'r' => out.push(b'\r'),
            'n' => out.push(b'\n'),
            't' => out.push(b'\t'),
            'x' => {
                let Some(first) = chars.next() else {
                    out.extend_from_slice(b"\\x");
                    break;
                };
                let Some(hi) = hex_value(first) else {
                    out.extend_from_slice(b"\\x");
                    push_char(&mut out, first);
                    continue;
                };
                let Some(second) = chars.next() else {
                    out.extend_from_slice(b"\\x");
                    push_char(&mut out, first);
                    break;
                };
                if let Some(lo) = hex_value(second) {
                    out.push(hi << 4 | lo);
                } else {
                    out.extend_from_slice(b"\\x");
                    push_char(&mut out, first);
                    push_char(&mut out, second);
                }
            }
            other => {
                out.push(b'\\');
                push_char(&mut out, other);
            }
        }
    }
    out
}

#[must_use]
pub fn debug_os(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                let _ = write!(out, "{}", text.escape_debug());
                break;
            }
            Err(err) => {
                let (good, bad) = rest.split_at(err.valid_up_to());
                let _ = write!(
                    out,
                    "{}",
                    std::str::from_utf8(good).unwrap_or_default().escape_debug()
                );
                let width = err.error_len().unwrap_or(bad.len());
                for &byte in &bad[..width] {
                    push_hex(&mut out, byte);
                }
                rest = &bad[width..];
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_matches_bstr_rules() {
        assert_eq!(escape(b""), "");
        assert_eq!(escape(b"a\\b"), "a\\\\b");
        assert_eq!(escape(b"\x00"), "\\0");
        assert_eq!(escape(b"\n\r\t"), "\\n\\r\\t");
        assert_eq!(escape(b"a b"), "a\\x20b");
        assert_eq!(escape(b"\xFF\x7F"), "\\xFF\\x7F");
        assert_eq!(escape("\u{2603}".as_bytes()), "\u{2603}");
    }

    #[test]
    fn unescape_matches_bstr_rules() {
        assert_eq!(unescape(""), b"");
        assert_eq!(unescape("\\0"), b"\x00");
        assert_eq!(unescape("\\\\"), b"\\");
        assert_eq!(unescape("a\\x41b"), b"aAb");
        assert_eq!(unescape("\\xZZ"), b"\\xZZ");
        assert_eq!(unescape("\\xA"), b"\\xA");
        assert_eq!(unescape("\\x"), b"\\x");
        assert_eq!(unescape("\\q"), b"\\q");
        assert_eq!(unescape("\\"), b"\\");
        assert_eq!(unescape("\\xFF"), b"\xFF");
        assert_eq!(unescape("\\x4G"), b"\\x4G");
    }

    #[test]
    fn debug_os_matches_rust_debug_for_utf8() {
        assert_eq!(debug_os(b"x"), format!("{:?}", "x"));
        assert_eq!(debug_os(b"a\"b\n"), format!("{:?}", "a\"b\n"));
        assert_eq!(debug_os(b"a\xFFb"), "\"a\\xFFb\"");
    }
}
