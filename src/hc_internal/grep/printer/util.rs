use std::borrow::Cow;
use std::cell::OnceCell;
use std::fmt;
use std::io;
use std::path::Path;
use std::sync::OnceLock;
use std::time;

use super::super::matcher::{Captures, LineTerminator, Match, Matcher};
use super::super::searcher::{
    LineIter, Searcher, SinkContext, SinkContextKind, SinkError, SinkMatch,
};
use super::MAX_LOOK_AHEAD;
use super::hyperlink::HyperlinkPath;

pub(crate) struct Replacer<M: Matcher> {
    space: Option<Space<M>>,
}

struct Space<M: Matcher> {
    caps: M::Captures,
    dst: Vec<u8>,
    matches: Vec<Match>,
}

impl<M: Matcher> fmt::Debug for Replacer<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (dst, matches) = self.replacement().unwrap_or((&[], &[]));
        f.debug_struct("Replacer")
            .field("dst", &dst)
            .field("matches", &matches)
            .finish()
    }
}

impl<M: Matcher> Replacer<M> {
    pub(crate) fn new() -> Replacer<M> {
        Replacer { space: None }
    }

    pub(crate) fn replace_all(
        &mut self,
        searcher: &Searcher,
        matcher: &M,
        mut haystack: &[u8],
        range: std::ops::Range<usize>,
        replacement: &[u8],
    ) -> io::Result<()> {
        let is_multi_line = searcher.multi_line_with_matcher(matcher);
        let line_terminator = if is_multi_line {
            if haystack[range.end..].len() >= MAX_LOOK_AHEAD {
                haystack = &haystack[..range.end + MAX_LOOK_AHEAD];
            }
            &[][..]
        } else {
            let mut m = Match::new(0, range.end);
            let line_terminator = trim_line_terminator(searcher, haystack, &mut m);
            haystack = &haystack[..m.end()];
            line_terminator
        };
        let Space { dst, caps, matches } = self.allocate(matcher)?;
        dst.clear();
        matches.clear();
        replace_with_captures_in_context(
            matcher,
            haystack,
            line_terminator,
            range,
            caps,
            dst,
            |caps, dst| {
                let start = dst.len();
                caps.interpolate(
                    |name| matcher.capture_index(name),
                    haystack,
                    replacement,
                    dst,
                );
                let end = dst.len();
                matches.push(Match::new(start, end));
                true
            },
        )
        .map_err(io::Error::error_message)?;
        Ok(())
    }

    pub(crate) fn replacement(&self) -> Option<(&[u8], &[Match])> {
        match self.space {
            Some(ref space) if !space.matches.is_empty() => Some((&space.dst, &space.matches)),
            _ => None,
        }
    }

    pub(crate) fn clear(&mut self) {
        if let Some(ref mut space) = self.space {
            space.dst.clear();
            space.matches.clear();
        }
    }

    fn allocate(&mut self, matcher: &M) -> io::Result<&mut Space<M>> {
        if self.space.is_none() {
            let caps = matcher.new_captures().map_err(io::Error::error_message)?;
            self.space = Some(Space {
                caps,
                dst: vec![],
                matches: vec![],
            });
        }
        Ok(self.space.as_mut().unwrap())
    }
}

#[derive(Debug)]
pub(crate) struct Sunk<'a> {
    bytes: &'a [u8],
    absolute_byte_offset: u64,
    line_number: Option<u64>,
    context_kind: Option<&'a SinkContextKind>,
    matches: &'a [Match],
    original_matches: &'a [Match],
}

impl<'a> Sunk<'a> {
    #[inline]
    pub(crate) fn empty() -> Sunk<'static> {
        Sunk {
            bytes: &[],
            absolute_byte_offset: 0,
            line_number: None,
            context_kind: None,
            matches: &[],
            original_matches: &[],
        }
    }

    #[inline]
    pub(crate) fn from_sink_match(
        sunk: &'a SinkMatch<'a>,
        original_matches: &'a [Match],
        replacement: Option<(&'a [u8], &'a [Match])>,
    ) -> Sunk<'a> {
        let (bytes, matches) = replacement.unwrap_or((sunk.bytes(), original_matches));
        Sunk {
            bytes,
            absolute_byte_offset: sunk.absolute_byte_offset(),
            line_number: sunk.line_number(),
            context_kind: None,
            matches,
            original_matches,
        }
    }

    #[inline]
    pub(crate) fn from_sink_context(
        sunk: &'a SinkContext<'a>,
        original_matches: &'a [Match],
        replacement: Option<(&'a [u8], &'a [Match])>,
    ) -> Sunk<'a> {
        let (bytes, matches) = replacement.unwrap_or((sunk.bytes(), original_matches));
        Sunk {
            bytes,
            absolute_byte_offset: sunk.absolute_byte_offset(),
            line_number: sunk.line_number(),
            context_kind: Some(sunk.kind()),
            matches,
            original_matches,
        }
    }

    #[inline]
    pub(crate) fn context_kind(&self) -> Option<&'a SinkContextKind> {
        self.context_kind
    }

    #[inline]
    pub(crate) fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    #[inline]
    pub(crate) fn matches(&self) -> &'a [Match] {
        self.matches
    }

    #[inline]
    pub(crate) fn original_matches(&self) -> &'a [Match] {
        self.original_matches
    }

    #[inline]
    pub(crate) fn lines(&self, line_term: u8) -> LineIter<'a> {
        LineIter::new(line_term, self.bytes())
    }

    #[inline]
    pub(crate) fn absolute_byte_offset(&self) -> u64 {
        self.absolute_byte_offset
    }

    #[inline]
    pub(crate) fn line_number(&self) -> Option<u64> {
        self.line_number
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PrinterPath<'a> {
    #[cfg(not(unix))]
    path: &'a Path,
    bytes: Cow<'a, [u8]>,
    hyperlink: OnceCell<Option<HyperlinkPath>>,
}

#[cfg(unix)]
pub(crate) fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(path.as_os_str().as_bytes())
}

#[cfg(not(unix))]
pub(crate) fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    match path.to_string_lossy() {
        Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
        Cow::Owned(s) => Cow::Owned(s.into_bytes()),
    }
}

impl<'a> PrinterPath<'a> {
    pub(crate) fn new(path: &'a Path) -> PrinterPath<'a> {
        PrinterPath {
            #[cfg(not(unix))]
            path,
            bytes: path_bytes(path),
            hyperlink: OnceCell::new(),
        }
    }

    pub(crate) fn with_separator(mut self, sep: Option<u8>) -> PrinterPath<'a> {
        let Some(sep) = sep else { return self };
        let mut bytes = self.bytes.to_vec();
        for b in &mut bytes {
            if *b == b'/' || (cfg!(windows) && *b == b'\\') {
                *b = sep;
            }
        }
        self.bytes = Cow::Owned(bytes);
        self
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn as_hyperlink(&self) -> Option<&HyperlinkPath> {
        self.hyperlink
            .get_or_init(|| HyperlinkPath::from_path(self.as_path()))
            .as_ref()
    }

    #[cfg(unix)]
    pub(crate) fn as_path(&self) -> &Path {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        Path::new(OsStr::from_bytes(self.as_bytes()))
    }

    #[cfg(not(unix))]
    pub(crate) fn as_path(&self) -> &Path {
        self.path
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NiceDuration(pub time::Duration);

impl fmt::Display for NiceDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:0.6}s", self.fractional_seconds())
    }
}

impl NiceDuration {
    fn fractional_seconds(self) -> f64 {
        let fractional = f64::from(self.0.subsec_nanos()) / 1_000_000_000.0;
        self.0.as_secs() as f64 + fractional
    }
}

impl serde::Serialize for NiceDuration {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = ser.serialize_struct("Duration", 3)?;
        state.serialize_field("secs", &self.0.as_secs())?;
        state.serialize_field("nanos", &self.0.subsec_nanos())?;
        state.serialize_field("human", &format!("{self}"))?;
        state.end()
    }
}

#[derive(Debug)]
pub(crate) struct DecimalFormatter {
    buf: [u8; Self::MAX_U64_LEN],
    start: usize,
}

impl DecimalFormatter {
    const MAX_U64_LEN: usize = 20;

    pub(crate) fn new(mut n: u64) -> DecimalFormatter {
        let mut buf = [0; Self::MAX_U64_LEN];
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        DecimalFormatter { buf, start: i }
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.buf[self.start..]
    }
}

pub(crate) fn trim_ascii_prefix(line_term: LineTerminator, slice: &[u8], range: Match) -> Match {
    let count = slice[range]
        .iter()
        .take_while(|&&b| {
            matches!(b, b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r' | b' ')
                && !line_term.as_bytes().contains(&b)
        })
        .count();
    range.with_start(range.start() + count)
}

pub(crate) fn find_iter_at_in_context<M, F>(
    searcher: &Searcher,
    matcher: M,
    mut bytes: &[u8],
    range: std::ops::Range<usize>,
    mut on_match: F,
) -> io::Result<()>
where
    M: Matcher,
    F: FnMut(Match) -> bool,
{
    if searcher.multi_line_with_matcher(&matcher) {
        if bytes[range.end..].len() >= MAX_LOOK_AHEAD {
            bytes = &bytes[..range.end + MAX_LOOK_AHEAD];
        }
    } else {
        let mut m = Match::new(0, range.end);
        trim_line_terminator(searcher, bytes, &mut m);
        bytes = &bytes[..m.end()];
    }
    matcher
        .find_iter_at(bytes, range.start, |m| {
            if m.start() >= range.end {
                return false;
            }
            on_match(m)
        })
        .map_err(io::Error::error_message)
}

pub(crate) fn trim_line_terminator<'b>(
    searcher: &Searcher,
    buf: &'b [u8],
    line: &mut Match,
) -> &'b [u8] {
    let lineterm = searcher.line_terminator();
    if lineterm.is_suffix(&buf[*line]) {
        let mut end = line.end() - 1;
        if lineterm.is_crlf() && end > 0 && buf.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        let orig_end = line.end();
        *line = line.with_end(end);
        &buf[end..orig_end]
    } else {
        &[]
    }
}

fn replace_with_captures_in_context<M, F>(
    matcher: &M,
    bytes: &[u8],
    line_terminator: &[u8],
    range: std::ops::Range<usize>,
    caps: &mut M::Captures,
    dst: &mut Vec<u8>,
    mut append: F,
) -> Result<(), M::Error>
where
    M: Matcher,
    F: FnMut(&M::Captures, &mut Vec<u8>) -> bool,
{
    let mut last_match = range.start;
    matcher.captures_iter_at(bytes, range.start, caps, |caps| {
        let m = caps.get(0).unwrap();
        if m.start() >= range.end {
            return false;
        }
        dst.extend_from_slice(&bytes[last_match..m.start()]);
        last_match = m.end();
        append(caps, dst)
    })?;
    let end = if last_match > range.end {
        bytes.len()
    } else {
        std::cmp::min(bytes.len(), range.end)
    };
    dst.extend_from_slice(&bytes[last_match..end]);
    dst.extend_from_slice(line_terminator);
    Ok(())
}

fn grapheme_regex() -> &'static regex::bytes::Regex {
    static RE: OnceLock<regex::bytes::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::bytes::Regex::new(
            r"(?x)
            \A(?:
            \p{gcb=CR} \p{gcb=LF}
            |
            \p{gcb=Control}
            |
            \p{gcb=Prepend}*
            (
              (
                (\p{gcb=L}* (\p{gcb=V}+ | \p{gcb=LV} \p{gcb=V}* | \p{gcb=LVT}) \p{gcb=T}*)
                |
                \p{gcb=L}+
                |
                \p{gcb=T}+
              )
              |
              \p{gcb=RI} \p{gcb=RI}
              |
              \p{Extended_Pictographic} (\p{gcb=Extend}* \p{gcb=ZWJ} \p{Extended_Pictographic})*
              |
              [^\p{gcb=Control} \p{gcb=CR} \p{gcb=LF}]
            )
            [\p{gcb=Extend} \p{gcb=ZWJ} \p{gcb=SpacingMark}]*
            |
            \p{any}
            )",
        )
        .unwrap()
    })
}

pub(crate) fn grapheme_len(bs: &[u8]) -> usize {
    if bs.is_empty() {
        return 0;
    }
    if bs.len() >= 2 && bs[0].is_ascii() && bs[1].is_ascii() && !bs[0].is_ascii_whitespace() {
        return 1;
    }
    match grapheme_regex().find(bs) {
        Some(m) if m.end() > 0 => m.end(),
        _ => super::super::bytes::decode_utf8(bs).1.max(1),
    }
}

pub(crate) fn grapheme_end_offsets(bs: &[u8], limit: usize) -> Option<usize> {
    let mut pos = 0;
    let mut last = None;
    for _ in 0..limit {
        let n = grapheme_len(&bs[pos..]);
        if n == 0 {
            break;
        }
        pos += n;
        last = Some(pos);
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_decimal_format() {
        let fmt = |n: u64| String::from_utf8(DecimalFormatter::new(n).as_bytes().to_vec()).unwrap();
        let std = |n: u64| n.to_string();
        for n in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 100, 123, u64::MAX] {
            assert_eq!(std(n), fmt(n));
        }
    }

    #[test]
    fn graphemes() {
        assert_eq!(grapheme_end_offsets(b"abc", 2), Some(2));
        assert_eq!(grapheme_end_offsets("e\u{301}x".as_bytes(), 1), Some(3));
        assert_eq!(grapheme_end_offsets(b"\r\nx", 1), Some(2));
        assert_eq!(grapheme_end_offsets(b"\xFFab", 1), Some(1));
        assert_eq!(grapheme_end_offsets(b"", 3), None);
    }
}
