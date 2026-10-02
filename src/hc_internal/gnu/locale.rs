use std::sync::OnceLock;

use super::tables;

pub const INVALID_BASE: u32 = 0x11_0000;
pub const MAX_CHAR: u32 = 0x10_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locale {
    pub utf8: bool,
}

impl Locale {
    #[must_use]
    pub fn c() -> Self {
        Self { utf8: false }
    }

    #[must_use]
    pub fn from_env() -> Self {
        let value = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| std::env::var_os(name).filter(|v| !v.is_empty()));
        value.map_or_else(Self::c, |v| Self::from_name(&v.to_string_lossy()))
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        if name == "C" || name == "POSIX" || !locale_exists(name) {
            return Self::c();
        }
        let lower = name.to_ascii_lowercase();
        let codeset = lower.split_once('.').map_or(lower.as_str(), |(_, c)| c);
        let codeset = codeset.split_once('@').map_or(codeset, |(c, _)| c);
        Self {
            utf8: codeset == "utf-8" || codeset == "utf8",
        }
    }
}

#[cfg(target_os = "macos")]
fn locale_exists(name: &str) -> bool {
    !name.contains('/')
        && std::path::Path::new("/usr/share/locale")
            .join(name)
            .is_dir()
}

#[cfg(not(target_os = "macos"))]
fn locale_exists(name: &str) -> bool {
    !name.contains('/')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    Alpha,
    Upper,
    Lower,
    Digit,
    Xdigit,
    Space,
    Punct,
    Alnum,
    Print,
    Graph,
    Cntrl,
    Blank,
}

impl CharClass {
    #[must_use]
    pub fn from_name(name: &[u8]) -> Option<Self> {
        Some(match name {
            b"alpha" => Self::Alpha,
            b"upper" => Self::Upper,
            b"lower" => Self::Lower,
            b"digit" => Self::Digit,
            b"xdigit" => Self::Xdigit,
            b"space" => Self::Space,
            b"punct" => Self::Punct,
            b"alnum" => Self::Alnum,
            b"print" => Self::Print,
            b"graph" => Self::Graph,
            b"cntrl" => Self::Cntrl,
            b"blank" => Self::Blank,
            _ => return None,
        })
    }

    fn utf8_table(self) -> &'static [(u32, u32)] {
        match self {
            Self::Alpha => tables::ALPHA,
            Self::Upper => tables::UPPER,
            Self::Lower => tables::LOWER,
            Self::Digit => tables::DIGIT,
            Self::Xdigit => tables::XDIGIT,
            Self::Space => tables::SPACE,
            Self::Punct => tables::PUNCT,
            Self::Alnum => tables::ALNUM,
            Self::Print => tables::PRINT,
            Self::Graph => tables::GRAPH,
            Self::Cntrl => tables::CNTRL,
            Self::Blank => tables::BLANK,
        }
    }

    fn ascii(self, c: u8) -> bool {
        match self {
            Self::Alpha => c.is_ascii_alphabetic(),
            Self::Upper => c.is_ascii_uppercase(),
            Self::Lower => c.is_ascii_lowercase(),
            Self::Digit => c.is_ascii_digit(),
            Self::Xdigit => c.is_ascii_hexdigit(),
            Self::Space => matches!(c, b'\t'..=b'\r' | b' '),
            Self::Punct => c.is_ascii_punctuation(),
            Self::Alnum => c.is_ascii_alphanumeric(),
            Self::Print => (0x20..0x7f).contains(&c),
            Self::Graph => c.is_ascii_graphic(),
            Self::Cntrl => c < 0x20 || c == 0x7f,
            Self::Blank => c == b' ' || c == b'\t',
        }
    }

    #[must_use]
    pub fn contains(self, utf8: bool, c: u32) -> bool {
        if c < 0x80 {
            return self.ascii(c as u8);
        }
        utf8 && c <= MAX_CHAR && in_ranges(self.utf8_table(), c)
    }

    #[must_use]
    pub fn ranges(self, utf8: bool) -> Vec<(u32, u32)> {
        if utf8 {
            return self.utf8_table().to_vec();
        }
        let mut out: Vec<(u32, u32)> = Vec::new();
        for c in 0..=0x7fu8 {
            if self.ascii(c) {
                match out.last_mut() {
                    Some(last) if last.1 + 1 == u32::from(c) => last.1 = u32::from(c),
                    _ => out.push((u32::from(c), u32::from(c))),
                }
            }
        }
        out
    }
}

#[must_use]
pub fn in_ranges(ranges: &[(u32, u32)], c: u32) -> bool {
    ranges
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[must_use]
#[inline]
pub fn decode(utf8: bool, s: &[u8]) -> (u32, usize) {
    let b0 = s[0];
    if !utf8 || b0 < 0x80 {
        return (u32::from(b0), 1);
    }
    let invalid = (INVALID_BASE + u32::from(b0), 1);
    let cont = |i: usize, lo: u8, hi: u8| s.get(i).is_some_and(|&b| (lo..=hi).contains(&b));
    let (len, lo1, hi1) = match b0 {
        0xc2..=0xdf => (2, 0x80, 0xbf),
        0xe0 => (3, 0xa0, 0xbf),
        0xe1..=0xec | 0xee..=0xef => (3, 0x80, 0xbf),
        0xed => (3, 0x80, 0x9f),
        0xf0 => (4, 0x90, 0xbf),
        0xf1..=0xf3 => (4, 0x80, 0xbf),
        0xf4 => (4, 0x80, 0x8f),
        _ => return invalid,
    };
    if !cont(1, lo1, hi1) {
        return invalid;
    }
    if (2..len).any(|i| !cont(i, 0x80, 0xbf)) {
        return invalid;
    }
    let init = u32::from(b0) & (0x7f >> len);
    let c = s[1..len]
        .iter()
        .fold(init, |acc, &b| (acc << 6) | (u32::from(b) & 0x3f));
    (c, len)
}

#[must_use]
pub fn decode_before(utf8: bool, s: &[u8], end: usize) -> Option<(u32, usize)> {
    if end == 0 {
        return None;
    }
    if !utf8 || s[end - 1] < 0x80 {
        return Some((u32::from(s[end - 1]), 1));
    }
    let lo = end.saturating_sub(4);
    for start in (lo..end).rev() {
        let (c, len) = decode(true, &s[start..end]);
        if start + len == end && c < INVALID_BASE {
            return Some((c, len));
        }
        if s[start] & 0xc0 != 0x80 {
            break;
        }
    }
    Some((INVALID_BASE + u32::from(s[end - 1]), 1))
}

pub fn encode(utf8: bool, unit: u32, out: &mut Vec<u8>) {
    if unit >= INVALID_BASE {
        out.push((unit - INVALID_BASE) as u8);
    } else if !utf8 || unit < 0x80 {
        out.push(unit as u8);
    } else if let Some(c) = char::from_u32(unit) {
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
}

#[must_use]
pub fn has_encoding_error(utf8: bool, s: &[u8]) -> bool {
    utf8 && std::str::from_utf8(s).is_err()
}

fn map_lookup(table: &[(u32, u32)], c: u32) -> u32 {
    table
        .binary_search_by_key(&c, |&(from, _)| from)
        .map_or(c, |i| table[i].1)
}

#[must_use]
pub fn to_upper(utf8: bool, c: u32) -> u32 {
    if c < 0x80 {
        return u32::from((c as u8).to_ascii_uppercase());
    }
    if utf8 && c <= MAX_CHAR {
        map_lookup(tables::TO_UPPER, c)
    } else {
        c
    }
}

#[must_use]
pub fn to_lower(utf8: bool, c: u32) -> u32 {
    if c < 0x80 {
        return u32::from((c as u8).to_ascii_lowercase());
    }
    if utf8 && c <= MAX_CHAR {
        map_lookup(tables::TO_LOWER, c)
    } else {
        c
    }
}

const LONESOME_LOWER: [u32; 19] = [
    0x00B5, 0x0131, 0x017F, 0x01C5, 0x01C8, 0x01CB, 0x01F2, 0x0345, 0x03C2, 0x03D0, 0x03D1, 0x03D5,
    0x03D6, 0x03F0, 0x03F1, 0x03F2, 0x03F5, 0x1E9B, 0x1FBE,
];

#[must_use]
pub fn case_folded_counterparts(utf8: bool, c: u32) -> Vec<u32> {
    let mut out = Vec::new();
    if c >= INVALID_BASE {
        return out;
    }
    let uc = to_upper(utf8, c);
    let lc = to_lower(utf8, uc);
    if uc != c {
        out.push(uc);
    }
    if lc != uc && lc != c && to_upper(utf8, lc) == uc {
        out.push(lc);
    }
    for &li in &LONESOME_LOWER {
        if li != lc && li != uc && li != c && to_upper(utf8, li) == uc {
            out.push(li);
        }
    }
    out
}

fn upper_sources() -> &'static [(u32, u32)] {
    static CELL: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut v: Vec<(u32, u32)> = (0..0x80u32)
            .filter_map(|c| {
                let u = to_upper(true, c);
                (u != c).then_some((c, u))
            })
            .chain(tables::TO_UPPER.iter().copied())
            .collect();
        v.sort_unstable();
        v
    })
}

#[must_use]
pub fn upper_changing(utf8: bool) -> Vec<(u32, u32)> {
    if utf8 {
        upper_sources().to_vec()
    } else {
        (b'a'..=b'z')
            .map(|c| (u32::from(c), u32::from(c.to_ascii_uppercase())))
            .collect()
    }
}

#[must_use]
pub fn is_word_grep(utf8: bool, unit: u32) -> bool {
    unit < INVALID_BASE && (unit == u32::from(b'_') || CharClass::Alnum.contains(utf8, unit))
}

#[must_use]
pub fn is_word_regex(utf8: bool, unit: u32) -> bool {
    if unit >= INVALID_BASE {
        return utf8 && CharClass::Alnum.contains(true, unit - INVALID_BASE);
    }
    unit == u32::from(b'_') || CharClass::Alnum.contains(utf8, unit)
}

#[must_use]
pub fn is_print(utf8: bool, unit: u32) -> bool {
    unit < INVALID_BASE && CharClass::Print.contains(utf8, unit)
}

#[must_use]
pub fn is_space(utf8: bool, unit: u32) -> bool {
    unit < INVALID_BASE && CharClass::Space.contains(utf8, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_invalid_sequences() {
        assert_eq!(decode(true, b"\xc3\xa9"), (0xe9, 2));
        assert_eq!(decode(true, b"\xed\xa0\x80"), (INVALID_BASE + 0xed, 1));
        assert_eq!(decode(true, b"\xc0\x80"), (INVALID_BASE + 0xc0, 1));
        assert_eq!(decode(true, b"\xf4\x90\x80\x80"), (INVALID_BASE + 0xf4, 1));
        assert_eq!(decode(true, b"\xe6\x97"), (INVALID_BASE + 0xe6, 1));
        assert_eq!(decode(false, b"\xe9"), (0xe9, 1));
        assert_eq!(decode_before(true, b"a\xe6\x97\xa5", 4), Some((0x65e5, 3)));
        assert_eq!(
            decode_before(true, b"a\x97\xa5", 3),
            Some((INVALID_BASE + 0xa5, 1))
        );
    }

    #[test]
    fn case_folding_matches_libc() {
        assert_eq!(
            case_folded_counterparts(true, u32::from(b's')),
            vec![0x53, 0x17f]
        );
        assert_eq!(case_folded_counterparts(true, 0x131), vec![0x49, 0x69]);
        assert_eq!(case_folded_counterparts(false, u32::from(b'a')), vec![0x41]);
        assert_eq!(case_folded_counterparts(false, 0xe9), Vec::<u32>::new());
    }

    #[test]
    fn classes_follow_locale() {
        assert!(CharClass::Alpha.contains(true, 0x65e5));
        assert!(!CharClass::Alpha.contains(false, 0xe9));
        assert!(is_word_regex(true, INVALID_BASE + 0xe9));
        assert!(!is_word_grep(true, INVALID_BASE + 0xe9));
        assert_eq!(Locale::from_name("C"), Locale::c());
    }
}
