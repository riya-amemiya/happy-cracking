mod ast;
mod hir;
mod literal;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use regex::bytes::{CaptureLocations, Regex, RegexBuilder};

use super::Flags;
use super::bytes::debug_bytes;
use super::matcher::{
    ByteSet, Captures, LineMatchKind, LineTerminator, Match, Matcher, NoError, ParallelMatcher,
};
use ast::{
    AstAnalysis, FLAG_I, FLAG_M, FLAG_R, FLAG_S, FLAG_SWAP, FLAG_U, FLAG_X, Parser,
    is_meta_character,
};
use hir::{Hir, Look, Translator};
use literal::FastLine;

#[derive(Clone, Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    Regex(String),
    NotAllowed(String),
    InvalidLineTerminator(u8),
    Banned(u8),
}

impl Error {
    fn new(kind: ErrorKind) -> Error {
        Error { kind }
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ErrorKind::Regex(ref s) => write!(f, "{s}"),
            ErrorKind::NotAllowed(ref lit) => {
                write!(f, "the literal {lit:?} is not allowed in a regex")
            }
            ErrorKind::InvalidLineTerminator(byte) => write!(
                f,
                "line terminators must be ASCII, but {} is not",
                debug_bytes(&[byte])
            ),
            ErrorKind::Banned(byte) => write!(
                f,
                "pattern contains {} but it is impossible to match",
                debug_bytes(&[byte])
            ),
        }
    }
}

const CASE_INSENSITIVE: u32 = 1;
const CASE_SMART: u32 = 1 << 1;
const MULTI_LINE: u32 = 1 << 2;
const DOT_MATCHES_NEW_LINE: u32 = 1 << 3;
const SWAP_GREED: u32 = 1 << 4;
const IGNORE_WHITESPACE: u32 = 1 << 5;
const UNICODE: u32 = 1 << 6;
const OCTAL: u32 = 1 << 7;
const CRLF: u32 = 1 << 8;
const WORD: u32 = 1 << 9;
const FIXED_STRINGS: u32 = 1 << 10;
const WHOLE_LINE: u32 = 1 << 11;

#[derive(Clone, Debug)]
struct Config {
    flags: Flags,
    size_limit: usize,
    dfa_size_limit: usize,
    nest_limit: u32,
    line_terminator: Option<LineTerminator>,
    ban: Option<u8>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            flags: Flags(UNICODE),
            size_limit: 100 * (1 << 20),
            dfa_size_limit: 1000 * (1 << 20),
            nest_limit: 250,
            line_terminator: None,
            ban: None,
        }
    }
}

impl Config {
    fn has_line_terminator(&self, literal: &str) -> bool {
        match self.line_terminator {
            None => false,
            Some(lt) if lt.is_crlf() => literal.bytes().any(|b| b == b'\r' || b == b'\n'),
            Some(lt) => literal.bytes().any(|b| b == lt.as_byte()),
        }
    }

    fn is_fixed_strings<P: AsRef<str>>(&self, patterns: &[P]) -> bool {
        if self.flags.contains(CASE_INSENSITIVE) || self.flags.contains(CASE_SMART) {
            return false;
        }
        if self.flags.contains(FIXED_STRINGS) {
            return !patterns
                .iter()
                .any(|p| self.has_line_terminator(p.as_ref()));
        }
        patterns.iter().all(|p| {
            let p = p.as_ref();
            !p.chars().any(is_meta_character) && !self.has_line_terminator(p)
        })
    }

    fn builder(&self, pattern: &str, case_insensitive: bool) -> RegexBuilder {
        let mut builder = RegexBuilder::new(pattern);
        builder
            .case_insensitive(case_insensitive)
            .multi_line(self.flags.contains(MULTI_LINE))
            .dot_matches_new_line(self.flags.contains(DOT_MATCHES_NEW_LINE))
            .crlf(self.flags.contains(CRLF))
            .swap_greed(self.flags.contains(SWAP_GREED))
            .ignore_whitespace(self.flags.contains(IGNORE_WHITESPACE))
            .unicode(self.flags.contains(UNICODE))
            .octal(self.flags.contains(OCTAL))
            .nest_limit(self.nest_limit)
            .size_limit(self.size_limit)
            .dfa_size_limit(self.dfa_size_limit);
        builder
    }

    fn syntax_error(&self, pattern: &str, case_insensitive: bool) -> Option<Error> {
        match self
            .builder(pattern, case_insensitive)
            .size_limit(0)
            .build()
        {
            Err(regex::Error::Syntax(msg)) => Some(Error::new(ErrorKind::Regex(msg))),
            _ => None,
        }
    }

    fn initial_flags(&self, case_insensitive: bool) -> u8 {
        let mut flags = 0;
        for (on, bit) in [
            (case_insensitive, FLAG_I),
            (self.flags.contains(MULTI_LINE), FLAG_M),
            (self.flags.contains(DOT_MATCHES_NEW_LINE), FLAG_S),
            (self.flags.contains(SWAP_GREED), FLAG_SWAP),
            (self.flags.contains(UNICODE), FLAG_U),
            (self.flags.contains(IGNORE_WHITESPACE), FLAG_X),
            (self.flags.contains(CRLF), FLAG_R),
        ] {
            if on {
                flags |= bit;
            }
        }
        flags
    }

    fn terminators(&self) -> Vec<u8> {
        match self.line_terminator {
            None => vec![],
            Some(lt) if lt.is_crlf() => vec![b'\r', b'\n'],
            Some(lt) => vec![lt.as_byte()],
        }
    }
}

struct Built {
    hir: Hir,
    pattern: String,
    case_insensitive: bool,
    literals: Option<Vec<Vec<u8>>>,
    joined: Option<String>,
}

fn build_fixed<P: AsRef<str>>(patterns: &[P]) -> Built {
    let hir = Hir::alternation(
        patterns
            .iter()
            .map(|p| Hir::literal(p.as_ref().as_bytes().to_vec()))
            .collect(),
    );
    let alts: Vec<String> = patterns.iter().map(|p| regex::escape(p.as_ref())).collect();
    let body = if alts.is_empty() {
        "[a&&b]".to_string()
    } else {
        alts.join("|")
    };
    Built {
        hir,
        pattern: format!("(?u-ix:{body})"),
        case_insensitive: false,
        literals: Some(
            patterns
                .iter()
                .map(|p| p.as_ref().as_bytes().to_vec())
                .collect(),
        ),
        joined: None,
    }
}

fn build_regex<P: AsRef<str>>(config: &Config, patterns: &[P]) -> Result<Built, Error> {
    let joined = patterns
        .iter()
        .map(|p| {
            if config.flags.contains(FIXED_STRINGS) {
                format!("(?:{})", regex::escape(p.as_ref()))
            } else {
                format!("(?:{})", p.as_ref())
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    let Ok(ast) = Parser::new(
        &joined,
        config.flags.contains(OCTAL),
        config.flags.contains(IGNORE_WHITESPACE),
    )
    .parse() else {
        return Err(config
            .syntax_error(&joined, config.flags.contains(CASE_INSENSITIVE))
            .unwrap_or_else(|| {
                Error::new(ErrorKind::Regex(format!("failed to parse {joined:?}")))
            }));
    };
    let analysis = AstAnalysis::from_ast(&ast);
    let case_insensitive = config.flags.contains(CASE_INSENSITIVE)
        || (config.flags.contains(CASE_SMART) && analysis.any_literal && !analysis.any_uppercase);
    let mut translator = Translator::new(&joined, config.terminators());
    let mut hir = translator.translate(&ast, config.initial_flags(case_insensitive));
    let fail = |err: Error| -> Error {
        config
            .syntax_error(&joined, case_insensitive)
            .unwrap_or(err)
    };
    if let Some(byte) = config.ban
        && hir::ban_check(&hir, byte)
    {
        return Err(fail(Error::new(ErrorKind::Banned(byte))));
    }
    for byte in config.terminators() {
        if !byte.is_ascii() {
            return Err(fail(Error::new(ErrorKind::InvalidLineTerminator(byte))));
        }
        hir = hir::strip(hir, byte)
            .map_err(|b| fail(Error::new(ErrorKind::NotAllowed(char::from(b).to_string()))))?;
    }
    let pattern = hir::apply_edits(&joined, std::mem::take(&mut translator.edits));
    Ok(Built {
        hir,
        pattern,
        case_insensitive,
        literals: None,
        joined: Some(joined),
    })
}

#[derive(Clone, Debug)]
pub struct RegexMatcherBuilder {
    config: Config,
}

impl Default for RegexMatcherBuilder {
    fn default() -> RegexMatcherBuilder {
        RegexMatcherBuilder::new()
    }
}

impl RegexMatcherBuilder {
    #[must_use]
    pub fn new() -> RegexMatcherBuilder {
        RegexMatcherBuilder {
            config: Config::default(),
        }
    }

    #[cfg(test)]
    pub fn build(&self, pattern: &str) -> Result<RegexMatcher, Error> {
        self.build_many(&[pattern])
    }

    pub fn build_many<P: AsRef<str>>(&self, patterns: &[P]) -> Result<RegexMatcher, Error> {
        let config = &self.config;
        let fixed = config.is_fixed_strings(patterns);
        let built = if fixed {
            build_fixed(patterns)
        } else {
            build_regex(config, patterns)?
        };
        let (start, end) = if config.flags.contains(CRLF) {
            (Look::StartCrlf, Look::EndCrlf)
        } else {
            (Look::StartLf, Look::EndLf)
        };
        let (hir, pattern) = if config.flags.contains(WHOLE_LINE) {
            (
                Hir::concat(vec![Hir::Look(start), built.hir, Hir::Look(end)]),
                format!("(?m:^)(?:{})(?m:$)", built.pattern),
            )
        } else if config.flags.contains(WORD) {
            (
                Hir::concat(vec![
                    Hir::Look(Look::Word(1)),
                    built.hir,
                    Hir::Look(Look::Word(2)),
                ]),
                format!(r"\b{{start-half}}(?:{})\b{{end-half}}", built.pattern),
            )
        } else {
            (built.hir, built.pattern)
        };
        let mut builder = config.builder(&pattern, built.case_insensitive);
        builder.nest_limit(config.nest_limit.saturating_add(16));
        let regex = match builder.build() {
            Ok(regex) => regex,
            Err(regex::Error::CompiledTooBig(limit)) => {
                return Err(Error::new(ErrorKind::Regex(format!(
                    "compiled regex exceeds size limit of {limit}"
                ))));
            }
            Err(err) => {
                return Err(built
                    .joined
                    .as_deref()
                    .and_then(|j| config.syntax_error(j, built.case_insensitive))
                    .unwrap_or_else(|| Error::new(ErrorKind::Regex(err.to_string()))));
            }
        };
        let non_matching = hir::non_matching_bytes(&hir);
        let line_terminator = if hir::has_anchor_haystack(&hir) {
            None
        } else {
            config.line_terminator
        };
        let names = regex
            .capture_names()
            .enumerate()
            .filter_map(|(i, name)| name.map(|n| (n.to_string(), i)))
            .collect();
        let fast = if line_terminator.is_some()
            && !config.flags.contains(WORD)
            && !config.flags.contains(WHOLE_LINE)
        {
            built
                .literals
                .as_deref()
                .and_then(FastLine::from_literals)
                .map(Arc::new)
        } else {
            None
        };
        Ok(RegexMatcher {
            regex,
            names: Arc::new(names),
            line_terminator,
            non_matching,
            fast,
        })
    }

    pub fn case_insensitive(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(CASE_INSENSITIVE, yes);
        self
    }

    pub fn case_smart(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(CASE_SMART, yes);
        self
    }

    pub fn multi_line(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(MULTI_LINE, yes);
        self
    }

    pub fn dot_matches_new_line(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(DOT_MATCHES_NEW_LINE, yes);
        self
    }

    pub fn unicode(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(UNICODE, yes);
        self
    }

    pub fn octal(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(OCTAL, yes);
        self
    }

    pub fn size_limit(&mut self, bytes: usize) -> &mut RegexMatcherBuilder {
        self.config.size_limit = bytes;
        self
    }

    pub fn dfa_size_limit(&mut self, bytes: usize) -> &mut RegexMatcherBuilder {
        self.config.dfa_size_limit = bytes;
        self
    }

    pub fn line_terminator(&mut self, line_term: Option<u8>) -> &mut RegexMatcherBuilder {
        self.config.line_terminator = line_term.map(LineTerminator::byte);
        self
    }

    pub fn ban_byte(&mut self, byte: Option<u8>) -> &mut RegexMatcherBuilder {
        self.config.ban = byte;
        self
    }

    pub fn crlf(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.line_terminator = if yes {
            Some(LineTerminator::crlf())
        } else {
            None
        };
        self.config.flags.set(CRLF, yes);
        self
    }

    pub fn word(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(WORD, yes);
        self
    }

    pub fn fixed_strings(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(FIXED_STRINGS, yes);
        self
    }

    pub fn whole_line(&mut self, yes: bool) -> &mut RegexMatcherBuilder {
        self.config.flags.set(WHOLE_LINE, yes);
        self
    }
}

#[derive(Clone, Debug)]
pub struct RegexMatcher {
    regex: Regex,
    names: Arc<HashMap<String, usize>>,
    line_terminator: Option<LineTerminator>,
    non_matching: ByteSet,
    fast: Option<Arc<FastLine>>,
}

#[cfg(test)]
impl RegexMatcher {
    pub fn new(pattern: &str) -> Result<RegexMatcher, Error> {
        RegexMatcherBuilder::new().build(pattern)
    }

    pub fn new_line_matcher(pattern: &str) -> Result<RegexMatcher, Error> {
        RegexMatcherBuilder::new()
            .line_terminator(Some(b'\n'))
            .build(pattern)
    }
}

impl Matcher for RegexMatcher {
    type Captures = RegexCaptures;
    type Error = NoError;

    #[inline]
    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, NoError> {
        if let Some(fast) = &self.fast {
            return Ok(fast.find_at(haystack, at));
        }
        Ok(self
            .regex
            .find_at(haystack, at)
            .map(|m| Match::new(m.start(), m.end())))
    }

    #[inline]
    fn new_captures(&self) -> Result<RegexCaptures, NoError> {
        Ok(RegexCaptures(self.regex.capture_locations()))
    }

    #[cfg(test)]
    #[inline]
    fn capture_count(&self) -> usize {
        self.regex.captures_len()
    }

    #[inline]
    fn capture_index(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    #[cfg(test)]
    #[inline]
    fn try_find_iter<F, E>(&self, haystack: &[u8], mut matched: F) -> Result<Result<(), E>, NoError>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        if self.fast.is_some() {
            return self.try_find_iter_at_default(haystack, matched);
        }
        for m in self.regex.find_iter(haystack) {
            match matched(Match::new(m.start(), m.end())) {
                Ok(true) => {}
                Ok(false) => return Ok(Ok(())),
                Err(err) => return Ok(Err(err)),
            }
        }
        Ok(Ok(()))
    }

    #[inline]
    fn captures_at(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut RegexCaptures,
    ) -> Result<bool, NoError> {
        Ok(self
            .regex
            .captures_read_at(&mut caps.0, haystack, at)
            .is_some())
    }

    #[inline]
    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, NoError> {
        if let Some(fast) = &self.fast {
            return Ok(fast.find_at(haystack, at).map(|m| m.end()));
        }
        Ok(self.regex.shortest_match_at(haystack, at))
    }

    #[inline]
    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        Some(&self.non_matching)
    }

    #[inline]
    fn line_terminator(&self) -> Option<LineTerminator> {
        self.line_terminator
    }

    #[inline]
    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, NoError> {
        Ok(self.candidate_line(haystack))
    }

    #[inline]
    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        Some(self)
    }
}

impl ParallelMatcher for RegexMatcher {
    fn par_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        Ok(self.candidate_line(haystack))
    }

    fn par_is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        Ok(match &self.fast {
            Some(fast) => fast.find_at(haystack, 0).is_some(),
            None => self.regex.is_match(haystack),
        })
    }

    fn par_find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, String> {
        Ok(Matcher::find_at(self, haystack, at).unwrap_or(None))
    }

    fn par_fork(&self) -> Option<Box<dyn ParallelMatcher + '_>> {
        Some(Box::new(self.clone()))
    }

    fn par_literal(&self) -> Option<&[u8]> {
        self.fast.as_deref().map(FastLine::needle)
    }
}

impl RegexMatcher {
    #[inline]
    fn candidate_line(&self, haystack: &[u8]) -> Option<LineMatchKind> {
        match &self.fast {
            Some(fast) => fast
                .find_at(haystack, 0)
                .map(|m| LineMatchKind::Confirmed(m.start())),
            None => self
                .regex
                .shortest_match(haystack)
                .map(LineMatchKind::Confirmed),
        }
    }

    #[cfg(test)]
    fn try_find_iter_at_default<F, E>(
        &self,
        haystack: &[u8],
        mut matched: F,
    ) -> Result<Result<(), E>, NoError>
    where
        F: FnMut(Match) -> Result<bool, E>,
    {
        let mut last_end = 0;
        let mut last_match = None;
        loop {
            if last_end > haystack.len() {
                return Ok(Ok(()));
            }
            let Some(m) = self.find_at(haystack, last_end)? else {
                return Ok(Ok(()));
            };
            if m.is_empty() {
                last_end = m.end() + 1;
                if Some(m.end()) == last_match {
                    continue;
                }
            } else {
                last_end = m.end();
            }
            last_match = Some(m.end());
            match matched(m) {
                Ok(true) => {}
                Ok(false) => return Ok(Ok(())),
                Err(err) => return Ok(Err(err)),
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RegexCaptures(CaptureLocations);

impl Captures for RegexCaptures {
    #[inline]
    fn get(&self, i: usize) -> Option<Match> {
        self.0.get(i).map(|(s, e)| Match::new(s, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word() {
        let matcher = RegexMatcherBuilder::new().word(true).build(r"-2").unwrap();
        assert!(matcher.is_match(b"abc -2 foo").unwrap());
        let matcher = RegexMatcherBuilder::new()
            .word(false)
            .build(r"\b-2\b")
            .unwrap();
        assert!(!matcher.is_match(b"abc -2 foo").unwrap());
    }

    #[test]
    fn line_terminator() {
        let matcher = RegexMatcherBuilder::new().build(r"abc\sxyz").unwrap();
        assert!(matcher.is_match(b"abc\nxyz").unwrap());
        let matcher = RegexMatcherBuilder::new()
            .line_terminator(Some(b'\n'))
            .build(r"abc\sxyz")
            .unwrap();
        assert!(!matcher.is_match(b"abc\nxyz").unwrap());
    }

    #[test]
    fn line_terminator_error() {
        assert!(
            RegexMatcherBuilder::new()
                .line_terminator(Some(b'\n'))
                .build(r"a\nz")
                .is_err()
        );
    }

    #[test]
    fn line_terminator_crlf() {
        let matcher = RegexMatcherBuilder::new()
            .multi_line(true)
            .build(r"abc$")
            .unwrap();
        assert!(matcher.is_match(b"abc\n").unwrap());
        let matcher = RegexMatcherBuilder::new()
            .multi_line(true)
            .build(r"abc$")
            .unwrap();
        assert!(!matcher.is_match(b"abc\r\n").unwrap());
        let matcher = RegexMatcherBuilder::new()
            .multi_line(true)
            .crlf(true)
            .build(r"abc$")
            .unwrap();
        assert!(matcher.is_match(b"abc\r\n").unwrap());
    }

    #[test]
    fn case_smart() {
        let matcher = RegexMatcherBuilder::new()
            .case_smart(true)
            .build(r"abc")
            .unwrap();
        assert!(matcher.is_match(b"ABC").unwrap());
        let matcher = RegexMatcherBuilder::new()
            .case_smart(true)
            .build(r"aBc")
            .unwrap();
        assert!(!matcher.is_match(b"ABC").unwrap());
    }

    fn err(builder: &RegexMatcherBuilder, patterns: &[&str]) -> String {
        builder.build_many(patterns).unwrap_err().to_string()
    }

    #[test]
    fn error_messages() {
        let mut b = RegexMatcherBuilder::new();
        b.line_terminator(Some(b'\n'));
        assert_eq!(
            err(&b, &["a\\nb"]),
            "the literal \"\\n\" is not allowed in a regex"
        );
        assert!(err(&b, &["("]).starts_with("regex parse error:\n    (?:()\n"));
        b.ban_byte(Some(0));
        assert_eq!(
            err(&b, &[r"\x00"]),
            "pattern contains \"\\0\" but it is impossible to match"
        );
        let mut c = RegexMatcherBuilder::new();
        c.crlf(true);
        assert_eq!(
            err(&c, &["a\\rb"]),
            "the literal \"\\r\" is not allowed in a regex"
        );
        let mut d = RegexMatcherBuilder::new();
        d.line_terminator(Some(0xFF));
        assert_eq!(
            err(&d, &["a.b"]),
            "line terminators must be ASCII, but \"\\xff\" is not"
        );
    }

    #[test]
    fn stripping() {
        let m = RegexMatcher::new_line_matcher(r"a\sb").unwrap();
        assert!(!m.is_match(b"a\nb").unwrap());
        assert!(m.is_match(b"a b").unwrap());
        let m = RegexMatcher::new_line_matcher(r"a[^x]b").unwrap();
        assert!(!m.is_match(b"a\nb").unwrap());
        let m = RegexMatcherBuilder::new()
            .line_terminator(Some(b'\n'))
            .dot_matches_new_line(true)
            .build("a.b")
            .unwrap();
        assert!(!m.is_match(b"a\nb").unwrap());
        assert!(m.is_match(b"a\rb").unwrap());
        let m = RegexMatcher::new_line_matcher(r"x|\n").unwrap();
        assert!(!m.is_match(b"\n").unwrap());
        assert!(m.is_match(b"x").unwrap());
        assert_eq!(m.line_terminator(), Some(LineTerminator::byte(b'\n')));
        let m = RegexMatcher::new_line_matcher(r"\Afoo").unwrap();
        assert_eq!(m.line_terminator(), None);
    }

    #[test]
    fn fixed_strings_and_whole_line() {
        let m = RegexMatcherBuilder::new()
            .fixed_strings(true)
            .build_many(&["a.b", "c d"])
            .unwrap();
        assert!(m.is_match(b"xa.by").unwrap());
        assert!(!m.is_match(b"axb").unwrap());
        assert!(m.is_match(b"c d").unwrap());
        let m = RegexMatcherBuilder::new()
            .whole_line(true)
            .multi_line(true)
            .build("foo")
            .unwrap();
        assert!(m.is_match(b"x\nfoo\ny").unwrap());
        assert!(!m.is_match(b"x\nfoox\ny").unwrap());
        let none: [&str; 0] = [];
        let m = RegexMatcherBuilder::new().build_many(&none).unwrap();
        assert!(!m.is_match(b"anything").unwrap());
    }

    #[test]
    fn captures() {
        let m = RegexMatcher::new(r"(?P<a>\w+)\s+(?P<b>\w+)").unwrap();
        assert_eq!(m.capture_count(), 3);
        assert_eq!(m.capture_index("b"), Some(2));
        let mut caps = m.new_captures().unwrap();
        assert!(m.captures(b" homer simpson ", &mut caps).unwrap());
        assert_eq!(caps.get(2), Some(Match::new(7, 14)));
    }
}
