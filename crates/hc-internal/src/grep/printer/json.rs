use std::borrow::Cow;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use serde::ser::{Serialize, SerializeStruct, Serializer};

use super::super::matcher::{Match, Matcher};
use super::super::searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use super::counter::CounterWriter;
use super::stats::Stats;
use super::util::{Replacer, find_iter_at_in_context};

#[derive(Debug, Clone, Default)]
struct Config {
    pretty: bool,
    always_begin_end: bool,
    replacement: Arc<Option<Vec<u8>>>,
}

#[derive(Clone, Debug, Default)]
pub struct JSONBuilder {
    config: Config,
}

impl JSONBuilder {
    #[must_use]
    pub fn new() -> JSONBuilder {
        JSONBuilder::default()
    }

    pub fn build<W: io::Write>(&self, wtr: W) -> Json<W> {
        Json {
            config: self.config.clone(),
            wtr: CounterWriter::new(wtr),
            matches: vec![],
        }
    }

    pub fn pretty(&mut self, yes: bool) -> &mut JSONBuilder {
        self.config.pretty = yes;
        self
    }

    pub fn always_begin_end(&mut self, yes: bool) -> &mut JSONBuilder {
        self.config.always_begin_end = yes;
        self
    }

    pub fn replacement(&mut self, replacement: Option<Vec<u8>>) -> &mut JSONBuilder {
        self.config.replacement = Arc::new(replacement);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Json<W> {
    config: Config,
    wtr: CounterWriter<W>,
    matches: Vec<Match>,
}

impl<W: io::Write> Json<W> {
    #[cfg(test)]
    pub fn sink<M: Matcher>(&mut self, matcher: M) -> JSONSink<'static, '_, M, W> {
        JSONSink {
            matcher,
            replacer: Replacer::new(),
            json: self,
            path: None,
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            begin_printed: false,
            stats: Stats::new(),
        }
    }

    pub fn sink_with_path<'p, 's, M, P>(
        &'s mut self,
        matcher: M,
        path: &'p P,
    ) -> JSONSink<'p, 's, M, W>
    where
        M: Matcher,
        P: ?Sized + AsRef<Path>,
    {
        JSONSink {
            matcher,
            replacer: Replacer::new(),
            json: self,
            path: Some(path.as_ref()),
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            begin_printed: false,
            stats: Stats::new(),
        }
    }

    fn write_message(&mut self, message: &Message<'_>) -> io::Result<()> {
        if self.config.pretty {
            serde_json::to_writer_pretty(&mut self.wtr, message)?;
        } else {
            serde_json::to_writer(&mut self.wtr, message)?;
        }
        self.wtr.write_all(b"\n")
    }
}

impl<W> Json<W> {
    pub fn get_mut(&mut self) -> &mut W {
        self.wtr.get_mut()
    }
}

#[derive(Debug)]
pub struct JSONSink<'p, 's, M: Matcher, W> {
    matcher: M,
    replacer: Replacer<M>,
    json: &'s mut Json<W>,
    path: Option<&'p Path>,
    start_time: Instant,
    match_count: u64,
    binary_byte_offset: Option<u64>,
    begin_printed: bool,
    stats: Stats,
}

impl<M: Matcher, W: io::Write> JSONSink<'_, '_, M, W> {
    pub fn has_match(&self) -> bool {
        self.match_count > 0
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    fn record_matches(
        &mut self,
        searcher: &Searcher,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> io::Result<()> {
        self.json.matches.clear();
        let matches = &mut self.json.matches;
        find_iter_at_in_context(searcher, &self.matcher, bytes, range.clone(), |m| {
            matches.push(Match::new(m.start() - range.start, m.end() - range.start));
            true
        })?;
        if matches
            .last()
            .is_some_and(|m| m.is_empty() && m.start() >= bytes.len())
        {
            matches.pop();
        }
        Ok(())
    }

    fn replace(
        &mut self,
        searcher: &Searcher,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> io::Result<()> {
        self.replacer.clear();
        if let Some(replacement) = self.json.config.replacement.as_ref().as_ref() {
            self.replacer
                .replace_all(searcher, &self.matcher, bytes, range, replacement)?;
        }
        Ok(())
    }

    fn write_begin_message(&mut self) -> io::Result<()> {
        if self.begin_printed {
            return Ok(());
        }
        self.json
            .write_message(&Message::Begin(Begin { path: self.path }))?;
        self.begin_printed = true;
        Ok(())
    }
}

impl<M: Matcher, W: io::Write> Sink for JSONSink<'_, '_, M, W> {
    type Error = io::Error;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        self.match_count += 1;
        self.write_begin_message()?;
        self.record_matches(searcher, mat.buffer(), mat.bytes_range_in_buffer())?;
        self.replace(searcher, mat.buffer(), mat.bytes_range_in_buffer())?;
        self.stats.add_matches(self.json.matches.len() as u64);
        self.stats.add_matched_lines(mat.lines().count() as u64);
        let submatches = sub_matches(mat.bytes(), &self.json.matches, self.replacer.replacement());
        let msg = Message::Match(Payload {
            path: self.path,
            lines: mat.bytes(),
            line_number: mat.line_number(),
            absolute_offset: mat.absolute_byte_offset(),
            submatches: &submatches,
        });
        self.json.write_message(&msg)?;
        Ok(true)
    }

    fn context(&mut self, searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, io::Error> {
        self.write_begin_message()?;
        self.json.matches.clear();
        let submatches = if searcher.invert_match() {
            self.record_matches(searcher, ctx.bytes(), 0..ctx.bytes().len())?;
            self.replace(searcher, ctx.bytes(), 0..ctx.bytes().len())?;
            sub_matches(ctx.bytes(), &self.json.matches, self.replacer.replacement())
        } else {
            vec![]
        };
        let msg = Message::Context(Payload {
            path: self.path,
            lines: ctx.bytes(),
            line_number: ctx.line_number(),
            absolute_offset: ctx.absolute_byte_offset(),
            submatches: &submatches,
        });
        self.json.write_message(&msg)?;
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        _binary_byte_offset: u64,
    ) -> Result<bool, io::Error> {
        Ok(true)
    }

    fn begin(&mut self, _searcher: &Searcher) -> Result<bool, io::Error> {
        self.json.wtr.reset_count();
        self.start_time = Instant::now();
        self.match_count = 0;
        self.binary_byte_offset = None;
        if !self.json.config.always_begin_end {
            return Ok(true);
        }
        self.write_begin_message()?;
        Ok(true)
    }

    fn finish(&mut self, _searcher: &Searcher, finish: &SinkFinish) -> Result<(), io::Error> {
        self.binary_byte_offset = finish.binary_byte_offset();
        self.stats.add_elapsed(self.start_time.elapsed());
        self.stats.add_searches(1);
        if self.match_count > 0 {
            self.stats.add_searches_with_match(1);
        }
        self.stats.add_bytes_searched(finish.byte_count());
        self.stats.add_bytes_printed(self.json.wtr.count());
        if !self.begin_printed {
            return Ok(());
        }
        let msg = Message::End(End {
            path: self.path,
            binary_offset: finish.binary_byte_offset(),
            stats: self.stats.clone(),
        });
        self.json.write_message(&msg)?;
        Ok(())
    }
}

fn sub_matches<'a>(
    bytes: &'a [u8],
    matches: &[Match],
    replacement: Option<(&'a [u8], &'a [Match])>,
) -> Vec<SubMatch<'a>> {
    matches
        .iter()
        .enumerate()
        .map(|(i, &mat)| SubMatch {
            m: &bytes[mat],
            replacement: replacement.map(|(rbuf, rmatches)| &rbuf[rmatches[i]]),
            start: mat.start(),
            end: mat.end(),
        })
        .collect()
}

enum Message<'a> {
    Begin(Begin<'a>),
    End(End<'a>),
    Match(Payload<'a>),
    Context(Payload<'a>),
}

impl Serialize for Message<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("Message", 2)?;
        match *self {
            Message::Begin(ref msg) => {
                state.serialize_field("type", &"begin")?;
                state.serialize_field("data", msg)?;
            }
            Message::End(ref msg) => {
                state.serialize_field("type", &"end")?;
                state.serialize_field("data", msg)?;
            }
            Message::Match(ref msg) => {
                state.serialize_field("type", &"match")?;
                state.serialize_field("data", msg)?;
            }
            Message::Context(ref msg) => {
                state.serialize_field("type", &"context")?;
                state.serialize_field("data", msg)?;
            }
        }
        state.end()
    }
}

struct Begin<'a> {
    path: Option<&'a Path>,
}

impl Serialize for Begin<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("Begin", 1)?;
        state.serialize_field("path", &self.path.map(Data::from_path))?;
        state.end()
    }
}

struct End<'a> {
    path: Option<&'a Path>,
    binary_offset: Option<u64>,
    stats: Stats,
}

impl Serialize for End<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("End", 3)?;
        state.serialize_field("path", &self.path.map(Data::from_path))?;
        state.serialize_field("binary_offset", &self.binary_offset)?;
        state.serialize_field("stats", &self.stats)?;
        state.end()
    }
}

struct Payload<'a> {
    path: Option<&'a Path>,
    lines: &'a [u8],
    line_number: Option<u64>,
    absolute_offset: u64,
    submatches: &'a [SubMatch<'a>],
}

impl Serialize for Payload<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("Lines", 5)?;
        state.serialize_field("path", &self.path.map(Data::from_path))?;
        state.serialize_field("lines", &Data::from_bytes(self.lines))?;
        state.serialize_field("line_number", &self.line_number)?;
        state.serialize_field("absolute_offset", &self.absolute_offset)?;
        state.serialize_field("submatches", &self.submatches)?;
        state.end()
    }
}

struct SubMatch<'a> {
    m: &'a [u8],
    replacement: Option<&'a [u8]>,
    start: usize,
    end: usize,
}

impl Serialize for SubMatch<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("SubMatch", 3)?;
        state.serialize_field("match", &Data::from_bytes(self.m))?;
        if let Some(r) = self.replacement {
            state.serialize_field("replacement", &Data::from_bytes(r))?;
        }
        state.serialize_field("start", &self.start)?;
        state.serialize_field("end", &self.end)?;
        state.end()
    }
}

enum Data<'a> {
    Text { text: Cow<'a, str> },
    Bytes { bytes: &'a [u8] },
}

impl<'a> Data<'a> {
    fn from_bytes(bytes: &'a [u8]) -> Data<'a> {
        match std::str::from_utf8(bytes) {
            Ok(text) => Data::Text {
                text: Cow::Borrowed(text),
            },
            Err(_) => Data::Bytes { bytes },
        }
    }

    #[cfg(unix)]
    fn from_path(path: &Path) -> Data<'_> {
        use std::os::unix::ffi::OsStrExt;
        match path.to_str() {
            Some(text) => Data::Text {
                text: Cow::Borrowed(text),
            },
            None => Data::Bytes {
                bytes: path.as_os_str().as_bytes(),
            },
        }
    }

    #[cfg(not(unix))]
    fn from_path(path: &Path) -> Data<'_> {
        Data::Text {
            text: path.to_string_lossy(),
        }
    }
}

impl Serialize for Data<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("Data", 1)?;
        match *self {
            Data::Text { ref text } => state.serialize_field("text", text)?,
            Data::Bytes { bytes } => state.serialize_field("bytes", &base64_standard(bytes))?,
        }
        state.end()
    }
}

fn base64_standard(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let (chunks, remainder) = bytes.as_chunks::<3>();
    for chunk in chunks {
        let group =
            (usize::from(chunk[0]) << 16) | (usize::from(chunk[1]) << 8) | usize::from(chunk[2]);
        for shift in [18, 12, 6, 0] {
            out.push(char::from(ALPHABET[(group >> shift) & 0b11_1111]));
        }
    }
    match *remainder {
        [] => {}
        [b0] => {
            let group = usize::from(b0);
            out.push(char::from(ALPHABET[(group >> 2) & 0b11_1111]));
            out.push(char::from(ALPHABET[(group << 4) & 0b11_1111]));
            out.push_str("==");
        }
        [b0, b1] => {
            let group = (usize::from(b0) << 8) | usize::from(b1);
            out.push(char::from(ALPHABET[(group >> 10) & 0b11_1111]));
            out.push(char::from(ALPHABET[(group >> 4) & 0b11_1111]));
            out.push(char::from(ALPHABET[(group << 2) & 0b11_1111]));
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

#[cfg(test)]
mod base64_tests {
    use super::base64_standard;

    #[test]
    fn base64_basic() {
        let b64 = |s: &str| base64_standard(s.as_bytes());
        assert_eq!(b64(""), "");
        assert_eq!(b64("f"), "Zg==");
        assert_eq!(b64("fo"), "Zm8=");
        assert_eq!(b64("foo"), "Zm9v");
        assert_eq!(b64("foob"), "Zm9vYg==");
        assert_eq!(b64("fooba"), "Zm9vYmE=");
        assert_eq!(b64("foobar"), "Zm9vYmFy");
    }
}
#[cfg(test)]
mod tests {
    use super::super::super::matcher::LineTerminator;
    use super::super::super::regex::{RegexMatcher, RegexMatcherBuilder};
    use super::super::super::searcher::SearcherBuilder;

    use super::{JSONBuilder, Json};

    const SHERLOCK: &[u8] = b"\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";

    fn printer_contents(printer: &mut Json<Vec<u8>>) -> String {
        String::from_utf8(printer.get_mut().to_owned()).unwrap()
    }

    #[test]
    fn binary_detection() {
        use super::super::super::searcher::BinaryDetection;

        const BINARY: &[u8] = b"\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew \x00 from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.\
";

        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .heap_limit(Some(80))
            .build()
            .search_reader(&matcher, BINARY, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);

        assert_eq!(got.lines().count(), 3);
        let last = got.lines().last().unwrap();
        assert!(last.contains(r#""binary_offset":212,"#));
    }

    #[test]
    fn max_matches() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, SHERLOCK, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);

        assert_eq!(got.lines().count(), 3);
    }

    #[test]
    fn max_matches_after_context() {
        let haystack = "\
a
b
c
d
e
d
e
d
e
d
e
";
        let matcher = RegexMatcher::new(r"d").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .after_context(2)
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, haystack.as_bytes(), printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);

        assert_eq!(got.lines().count(), 5);
    }

    #[test]
    fn no_match() {
        let matcher = RegexMatcher::new(r"DOES NOT MATCH").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(&matcher, SHERLOCK, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);

        assert_eq!(got.len(), 0);
    }

    #[test]
    fn always_begin_end_no_match() {
        let matcher = RegexMatcher::new(r"DOES NOT MATCH").unwrap();
        let mut printer = JSONBuilder::new().always_begin_end(true).build(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(&matcher, SHERLOCK, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);

        assert_eq!(got.lines().count(), 2);
        assert!(got.contains("begin") && got.contains("end"));
    }

    #[test]
    fn missing_crlf() {
        let haystack = "test\r\n".as_bytes();

        let matcher = RegexMatcherBuilder::new().build("test").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(&matcher, haystack, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);
        assert_eq!(got.lines().count(), 3);
        assert!(
            got.lines().nth(1).unwrap().contains(r"test\r\n"),
            r"missing 'test\r\n' in '{}'",
            got.lines().nth(1).unwrap(),
        );

        let matcher = RegexMatcherBuilder::new().crlf(true).build("test").unwrap();
        let mut printer = JSONBuilder::new().build(vec![]);
        SearcherBuilder::new()
            .line_terminator(LineTerminator::crlf())
            .build()
            .search_reader(&matcher, haystack, printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);
        assert_eq!(got.lines().count(), 3);
        assert!(
            got.lines().nth(1).unwrap().contains(r"test\r\n"),
            r"missing 'test\r\n' in '{}'",
            got.lines().nth(1).unwrap(),
        );
    }
}
