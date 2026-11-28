use std::io::{self, Write};

use memchr::memchr;
use regex::bytes::{Regex, RegexBuilder};

use super::super::Flags;
use super::super::matcher::{
    LineMatchKind, LineTerminator, Match, Matcher, NoCaptures, NoError, ParallelMatcher,
};
use super::sink::{Sink, SinkContext, SinkFinish, SinkMatch};
use super::{BinaryDetection, Searcher, SearcherBuilder};

#[derive(Clone, Debug)]
pub(crate) struct RegexMatcher {
    regex: Regex,
    line_term: Option<LineTerminator>,
    every_line_is_candidate: bool,
}

impl RegexMatcher {
    pub(crate) fn new(pattern: &str) -> RegexMatcher {
        let regex = RegexBuilder::new(pattern).multi_line(true).build().unwrap();
        RegexMatcher {
            regex,
            line_term: None,
            every_line_is_candidate: false,
        }
    }

    pub(crate) fn set_line_term(&mut self, line_term: Option<LineTerminator>) -> &mut RegexMatcher {
        self.line_term = line_term;
        self
    }

    pub(crate) fn every_line_is_candidate(&mut self, yes: bool) -> &mut RegexMatcher {
        self.every_line_is_candidate = yes;
        self
    }

    fn candidate(&self, haystack: &[u8]) -> Option<LineMatchKind> {
        if self.every_line_is_candidate {
            assert!(self.line_term.is_some());
            if haystack.is_empty() {
                return None;
            }
            let i =
                memchr(self.line_term.unwrap().as_byte(), haystack).unwrap_or(haystack.len() - 1);
            Some(LineMatchKind::Candidate(i))
        } else {
            self.regex
                .find_at(haystack, 0)
                .map(|m| LineMatchKind::Confirmed(m.end()))
        }
    }
}

impl Matcher for RegexMatcher {
    type Captures = NoCaptures;
    type Error = NoError;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, NoError> {
        Ok(self
            .regex
            .find_at(haystack, at)
            .map(|m| Match::new(m.start(), m.end())))
    }

    fn new_captures(&self) -> Result<NoCaptures, NoError> {
        Ok(NoCaptures::new())
    }

    fn line_terminator(&self) -> Option<LineTerminator> {
        self.line_term
    }

    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, NoError> {
        Ok(self.candidate(haystack))
    }

    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        Some(self)
    }
}

impl ParallelMatcher for RegexMatcher {
    fn par_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        Ok(self.candidate(haystack))
    }

    fn par_is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        Ok(self.regex.find_at(haystack, 0).is_some())
    }

    fn par_find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, String> {
        Ok(self
            .regex
            .find_at(haystack, at)
            .map(|m| Match::new(m.start(), m.end())))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct KitchenSink(Vec<u8>);

impl KitchenSink {
    pub(crate) fn new() -> KitchenSink {
        KitchenSink(vec![])
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Sink for KitchenSink {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        assert_ne!(mat.bytes().len(), 0);
        assert!(mat.lines().count() >= 1);
        let mut line_number = mat.line_number();
        let mut byte_offset = mat.absolute_byte_offset();
        for line in mat.lines() {
            if let Some(ref mut n) = line_number {
                write!(self.0, "{n}:")?;
                *n += 1;
            }
            write!(self.0, "{byte_offset}:")?;
            byte_offset += line.len() as u64;
            self.0.write_all(line)?;
        }
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        context: &SinkContext<'_>,
    ) -> Result<bool, io::Error> {
        assert_ne!(context.bytes().len(), 0);
        assert_eq!(context.lines().count(), 1);
        if let Some(line_number) = context.line_number() {
            write!(self.0, "{line_number}-")?;
        }
        write!(self.0, "{}-", context.absolute_byte_offset)?;
        self.0.write_all(context.bytes())?;
        Ok(true)
    }

    fn context_break(&mut self, _searcher: &Searcher) -> Result<bool, io::Error> {
        self.0.write_all(b"--\n")?;
        Ok(true)
    }

    fn finish(&mut self, _searcher: &Searcher, sink_finish: &SinkFinish) -> Result<(), io::Error> {
        writeln!(self.0)?;
        writeln!(self.0, "byte count:{}", sink_finish.byte_count())?;
        if let Some(offset) = sink_finish.binary_byte_offset() {
            writeln!(self.0, "binary offset:{offset}")?;
        }
        Ok(())
    }
}

const BY_LINE: u32 = 1;
const MULTI_LINE: u32 = 1 << 1;
const INVERT_MATCH: u32 = 1 << 2;
const LINE_NUMBER: u32 = 1 << 3;
const AUTO_HEAP_LIMIT: u32 = 1 << 4;
const PASSTHRU: u32 = 1 << 5;

#[derive(Debug)]
pub(crate) struct SearcherTester {
    haystack: String,
    pattern: String,
    expected_no_line_number: Option<String>,
    expected_with_line_number: Option<String>,
    expected_slice_no_line_number: Option<String>,
    expected_slice_with_line_number: Option<String>,
    flags: Flags,
    binary: BinaryDetection,
    after_context: usize,
    before_context: usize,
}

impl SearcherTester {
    pub(crate) fn new(haystack: &str, pattern: &str) -> SearcherTester {
        SearcherTester {
            haystack: haystack.to_string(),
            pattern: pattern.to_string(),
            expected_no_line_number: None,
            expected_with_line_number: None,
            expected_slice_no_line_number: None,
            expected_slice_with_line_number: None,
            flags: Flags(BY_LINE | MULTI_LINE | LINE_NUMBER | AUTO_HEAP_LIMIT),
            binary: BinaryDetection::none(),
            after_context: 0,
            before_context: 0,
        }
    }

    pub(crate) fn test(&self) {
        assert!(
            self.expected_no_line_number.is_some(),
            "an 'expected' string with NO line numbers must be given"
        );
        assert!(
            !(self.flags.contains(LINE_NUMBER) && self.expected_with_line_number.is_none()),
            "an 'expected' string with line numbers must be given, or disable testing with line numbers"
        );
        let configs = self.configs();
        assert!(
            !configs.is_empty(),
            "test configuration resulted in nothing being tested"
        );
        for config in &configs {
            let label = format!("reader-{}", config.label);
            let got = config.search_reader(&self.haystack);
            assert_eq!(config.expected_reader, got, "{label}");
            let label = format!("slice-{}", config.label);
            let got = config.search_slice(&self.haystack);
            assert_eq!(config.expected_slice, got, "{label}");
        }
    }

    pub(crate) fn expected_no_line_number(&mut self, exp: &str) -> &mut SearcherTester {
        self.expected_no_line_number = Some(exp.to_string());
        self
    }

    pub(crate) fn expected_with_line_number(&mut self, exp: &str) -> &mut SearcherTester {
        self.expected_with_line_number = Some(exp.to_string());
        self
    }

    pub(crate) fn expected_slice_no_line_number(&mut self, exp: &str) -> &mut SearcherTester {
        self.expected_slice_no_line_number = Some(exp.to_string());
        self
    }

    pub(crate) fn line_number(&mut self, yes: bool) -> &mut SearcherTester {
        self.flags.set(LINE_NUMBER, yes);
        self
    }

    pub(crate) fn by_line(&mut self, yes: bool) -> &mut SearcherTester {
        self.flags.set(BY_LINE, yes);
        self
    }

    pub(crate) fn invert_match(&mut self, yes: bool) -> &mut SearcherTester {
        self.flags.set(INVERT_MATCH, yes);
        self
    }

    pub(crate) fn binary_detection(&mut self, detection: BinaryDetection) -> &mut SearcherTester {
        self.binary = detection;
        self
    }

    pub(crate) fn auto_heap_limit(&mut self, yes: bool) -> &mut SearcherTester {
        self.flags.set(AUTO_HEAP_LIMIT, yes);
        self
    }

    pub(crate) fn after_context(&mut self, lines: usize) -> &mut SearcherTester {
        self.after_context = lines;
        self
    }

    pub(crate) fn before_context(&mut self, lines: usize) -> &mut SearcherTester {
        self.before_context = lines;
        self
    }

    pub(crate) fn passthru(&mut self, yes: bool) -> &mut SearcherTester {
        self.flags.set(PASSTHRU, yes);
        self
    }

    fn minimal_heap_limit(&self, multi_line: bool) -> usize {
        if multi_line {
            1 + self.haystack.len()
        } else if self.before_context == 0 && self.after_context == 0 {
            1 + self.haystack.lines().map(str::len).max().unwrap_or(0)
        } else {
            let mut lens: Vec<usize> = self.haystack.lines().map(str::len).collect();
            lens.sort_unstable();
            lens.reverse();
            let context_count = if self.flags.contains(PASSTHRU) {
                self.haystack.lines().count()
            } else {
                2 + self.before_context + self.after_context
            };
            lens.into_iter()
                .take(context_count)
                .map(|len| len + 1)
                .sum::<usize>()
        }
    }

    fn push(
        configs: &mut Vec<TesterConfig>,
        label: &str,
        expected_reader: &str,
        expected_slice: &str,
        builder: &SearcherBuilder,
        matcher: &RegexMatcher,
    ) {
        configs.push(TesterConfig {
            label: label.to_string(),
            expected_reader: expected_reader.to_string(),
            expected_slice: expected_slice.to_string(),
            builder: builder.clone(),
            matcher: matcher.clone(),
        });
    }

    fn configs(&self) -> Vec<TesterConfig> {
        let mut configs = vec![];
        let matcher = RegexMatcher::new(&self.pattern);
        let mut builder = SearcherBuilder::new();
        builder
            .line_number(false)
            .invert_match(self.flags.contains(INVERT_MATCH))
            .binary_detection(self.binary.clone())
            .after_context(self.after_context)
            .before_context(self.before_context)
            .passthru(self.flags.contains(PASSTHRU))
            .parallel(false);

        if self.flags.contains(BY_LINE) {
            let mut matcher = matcher.clone();
            let mut builder = builder.clone();
            let expected_reader = self.expected_no_line_number.clone().unwrap();
            let expected_slice = self
                .expected_slice_no_line_number
                .clone()
                .unwrap_or_else(|| expected_reader.clone());
            let (er, es) = (expected_reader.as_str(), expected_slice.as_str());
            Self::push(
                &mut configs,
                "byline-noterm-nonumber",
                er,
                es,
                &builder,
                &matcher,
            );
            if self.flags.contains(AUTO_HEAP_LIMIT) {
                builder.heap_limit(Some(self.minimal_heap_limit(false)));
                Self::push(
                    &mut configs,
                    "byline-noterm-nonumber-heaplimit",
                    er,
                    es,
                    &builder,
                    &matcher,
                );
                builder.heap_limit(None);
            }
            matcher.set_line_term(Some(LineTerminator::byte(b'\n')));
            Self::push(
                &mut configs,
                "byline-term-nonumber",
                er,
                es,
                &builder,
                &matcher,
            );
            let mut parallel = builder.clone();
            parallel.parallel(true).parallel_threshold(1);
            Self::push(
                &mut configs,
                "byline-term-nonumber-parallel",
                er,
                es,
                &parallel,
                &matcher,
            );
            matcher.every_line_is_candidate(true);
            Self::push(
                &mut configs,
                "byline-term-nonumber-candidates",
                er,
                es,
                &builder,
                &matcher,
            );
            Self::push(
                &mut configs,
                "byline-term-nonumber-candidates-parallel",
                er,
                es,
                &parallel,
                &matcher,
            );
        }
        if self.flags.contains(BY_LINE) && self.flags.contains(LINE_NUMBER) {
            let mut matcher = matcher.clone();
            let mut builder = builder.clone();
            let expected_reader = self.expected_with_line_number.clone().unwrap();
            let expected_slice = self
                .expected_slice_with_line_number
                .clone()
                .unwrap_or_else(|| expected_reader.clone());
            let (er, es) = (expected_reader.as_str(), expected_slice.as_str());
            builder.line_number(true);
            Self::push(
                &mut configs,
                "byline-noterm-number",
                er,
                es,
                &builder,
                &matcher,
            );
            matcher.set_line_term(Some(LineTerminator::byte(b'\n')));
            Self::push(
                &mut configs,
                "byline-term-number",
                er,
                es,
                &builder,
                &matcher,
            );
            let mut parallel = builder.clone();
            parallel.parallel(true).parallel_threshold(1);
            Self::push(
                &mut configs,
                "byline-term-number-parallel",
                er,
                es,
                &parallel,
                &matcher,
            );
            matcher.every_line_is_candidate(true);
            Self::push(
                &mut configs,
                "byline-term-number-candidates",
                er,
                es,
                &builder,
                &matcher,
            );
            Self::push(
                &mut configs,
                "byline-term-number-candidates-parallel",
                er,
                es,
                &parallel,
                &matcher,
            );
        }
        if self.flags.contains(MULTI_LINE) {
            let mut builder = builder.clone();
            let expected_slice = self
                .expected_slice_no_line_number
                .clone()
                .unwrap_or_else(|| self.expected_no_line_number.clone().unwrap());
            builder.multi_line(true);
            Self::push(
                &mut configs,
                "multiline-nonumber",
                &expected_slice,
                &expected_slice,
                &builder,
                &matcher,
            );
            if self.flags.contains(AUTO_HEAP_LIMIT) {
                builder.heap_limit(Some(self.minimal_heap_limit(true)));
                Self::push(
                    &mut configs,
                    "multiline-nonumber-heaplimit",
                    &expected_slice,
                    &expected_slice,
                    &builder,
                    &matcher,
                );
                builder.heap_limit(None);
            }
        }
        if self.flags.contains(MULTI_LINE) && self.flags.contains(LINE_NUMBER) {
            let mut builder = builder.clone();
            let expected_slice = self
                .expected_slice_with_line_number
                .clone()
                .unwrap_or_else(|| self.expected_with_line_number.clone().unwrap());
            builder.multi_line(true);
            builder.line_number(true);
            Self::push(
                &mut configs,
                "multiline-number",
                &expected_slice,
                &expected_slice,
                &builder,
                &matcher,
            );
            builder.heap_limit(Some(self.minimal_heap_limit(true)));
            Self::push(
                &mut configs,
                "multiline-number-heaplimit",
                &expected_slice,
                &expected_slice,
                &builder,
                &matcher,
            );
        }
        configs
    }
}

#[derive(Debug)]
struct TesterConfig {
    label: String,
    expected_reader: String,
    expected_slice: String,
    builder: SearcherBuilder,
    matcher: RegexMatcher,
}

impl TesterConfig {
    fn search_reader(&self, haystack: &str) -> String {
        let mut sink = KitchenSink::new();
        let mut searcher = self.builder.build();
        if let Err(err) = searcher.search_reader(&self.matcher, haystack.as_bytes(), &mut sink) {
            panic!("error running 'reader-{}': {}", self.label, err);
        }
        String::from_utf8(sink.as_bytes().to_vec()).unwrap()
    }

    fn search_slice(&self, haystack: &str) -> String {
        let mut sink = KitchenSink::new();
        let mut searcher = self.builder.build();
        if let Err(err) = searcher.search_slice(&self.matcher, haystack.as_bytes(), &mut sink) {
            panic!("error running 'slice-{}': {}", self.label, err);
        }
        String::from_utf8(sink.as_bytes().to_vec()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(start: usize, end: usize) -> Match {
        Match::new(start, end)
    }

    #[test]
    fn empty_line1() {
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(b"", 0), Ok(Some(m(0, 0))));
    }

    #[test]
    fn empty_line2() {
        let haystack = b"\n";
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(haystack, 0), Ok(Some(m(0, 0))));
        assert_eq!(matcher.find_at(haystack, 1), Ok(Some(m(1, 1))));
    }

    #[test]
    fn empty_line3() {
        let haystack = b"\n\n";
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(haystack, 0), Ok(Some(m(0, 0))));
        assert_eq!(matcher.find_at(haystack, 1), Ok(Some(m(1, 1))));
        assert_eq!(matcher.find_at(haystack, 2), Ok(Some(m(2, 2))));
    }

    #[test]
    fn empty_line4() {
        let haystack = b"a\n\nb\n";
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(haystack, 0), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 1), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 2), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 3), Ok(Some(m(5, 5))));
        assert_eq!(matcher.find_at(haystack, 4), Ok(Some(m(5, 5))));
        assert_eq!(matcher.find_at(haystack, 5), Ok(Some(m(5, 5))));
    }

    #[test]
    fn empty_line5() {
        let haystack = b"a\n\nb\nc";
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(haystack, 0), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 1), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 2), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 3), Ok(None));
        assert_eq!(matcher.find_at(haystack, 4), Ok(None));
        assert_eq!(matcher.find_at(haystack, 5), Ok(None));
        assert_eq!(matcher.find_at(haystack, 6), Ok(None));
    }

    #[test]
    fn empty_line6() {
        let haystack = b"a\n";
        let matcher = RegexMatcher::new(r"^$");
        assert_eq!(matcher.find_at(haystack, 0), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 1), Ok(Some(m(2, 2))));
        assert_eq!(matcher.find_at(haystack, 2), Ok(Some(m(2, 2))));
    }
}
