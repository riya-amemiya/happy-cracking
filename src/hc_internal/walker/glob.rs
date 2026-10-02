use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::hash::{BuildHasherDefault, Hasher};
use std::path::{Path, is_separator};

use regex::bytes::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};

const REGEX_SIZE_LIMIT: usize = 10 * (1 << 20);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    glob: Option<String>,
    kind: ErrorKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    UnclosedClass,
    InvalidRange(char, char),
    UnopenedAlternates,
    UnclosedAlternates,
    DanglingEscape,
    Regex(String),
}

impl Error {
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl std::error::Error for Error {}

impl ErrorKind {
    fn description(&self) -> &str {
        match self {
            ErrorKind::UnclosedClass => "unclosed character class; missing ']'",
            ErrorKind::InvalidRange(_, _) => "invalid character range",
            ErrorKind::UnopenedAlternates => {
                "unopened alternate group; missing '{' (maybe escape '}' with '[}]'?)"
            }
            ErrorKind::UnclosedAlternates => {
                "unclosed alternate group; missing '}' (maybe escape '{' with '[{]'?)"
            }
            ErrorKind::DanglingEscape => "dangling '\\'",
            ErrorKind::Regex(err) => err,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.glob {
            None => self.kind.fmt(f),
            Some(glob) => write!(f, "error parsing glob '{glob}': {}", self.kind),
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::InvalidRange(s, e) => write!(f, "invalid range; '{s}' > '{e}'"),
            _ => f.write_str(self.description()),
        }
    }
}

fn new_regex(pat: &str) -> Result<Regex, Error> {
    RegexBuilder::new(pat)
        .dot_matches_new_line(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|err| Error {
            glob: Some(pat.to_string()),
            kind: ErrorKind::Regex(err.to_string()),
        })
}

fn new_regex_set(pats: &[String]) -> Result<RegexSet, Error> {
    RegexSetBuilder::new(pats)
        .dot_matches_new_line(true)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|err| Error {
            glob: None,
            kind: ErrorKind::Regex(err.to_string()),
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MatchStrategy {
    Literal(String),
    BasenameLiteral(String),
    Extension(String),
    Prefix(String),
    Suffix { suffix: String, component: bool },
    RequiredExtension(String),
    Regex,
}

impl MatchStrategy {
    pub(crate) fn new(pat: &Glob) -> MatchStrategy {
        if let Some(lit) = pat.basename_literal() {
            MatchStrategy::BasenameLiteral(lit)
        } else if let Some(lit) = pat.literal() {
            MatchStrategy::Literal(lit)
        } else if let Some(ext) = pat.ext() {
            MatchStrategy::Extension(ext)
        } else if let Some(prefix) = pat.prefix() {
            MatchStrategy::Prefix(prefix)
        } else if let Some((suffix, component)) = pat.suffix() {
            MatchStrategy::Suffix { suffix, component }
        } else if let Some(ext) = pat.required_ext() {
            MatchStrategy::RequiredExtension(ext)
        } else {
            MatchStrategy::Regex
        }
    }
}

#[derive(Clone, Eq)]
pub struct Glob {
    pattern: String,
    re: String,
    opts: GlobOptions,
    tokens: Tokens,
}

impl AsRef<Glob> for Glob {
    fn as_ref(&self) -> &Glob {
        self
    }
}

impl PartialEq for Glob {
    fn eq(&self, other: &Glob) -> bool {
        self.pattern == other.pattern && self.opts == other.opts
    }
}

impl std::hash::Hash for Glob {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pattern.hash(state);
        self.opts.hash(state);
    }
}

impl fmt::Debug for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            f.debug_struct("Glob")
                .field("glob", &self.pattern)
                .field("re", &self.re)
                .field("opts", &self.opts)
                .field("tokens", &self.tokens)
                .finish()
        } else {
            f.debug_tuple("Glob").field(&self.pattern).finish()
        }
    }
}

impl fmt::Display for Glob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.pattern.fmt(f)
    }
}

impl std::str::FromStr for Glob {
    type Err = Error;

    fn from_str(glob: &str) -> Result<Self, Self::Err> {
        Self::new(glob)
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct GlobMatcher {
    re: Regex,
}

#[cfg(test)]
impl GlobMatcher {
    pub fn is_match<P: AsRef<Path>>(&self, path: P) -> bool {
        self.is_match_candidate(&Candidate::new(path.as_ref()))
    }

    pub fn is_match_candidate(&self, path: &Candidate<'_>) -> bool {
        self.re.is_match(&path.path)
    }
}

#[derive(Clone, Debug)]
pub struct GlobBuilder<'a> {
    glob: &'a str,
    opts: GlobOptions,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GlobOptions {
    case_insensitive: bool,
    literal_separator: bool,
    backslash_escape: bool,
    empty_alternates: bool,
    allow_unclosed_class: bool,
}

impl Default for GlobOptions {
    fn default() -> GlobOptions {
        GlobOptions {
            case_insensitive: false,
            literal_separator: false,
            backslash_escape: !is_separator('\\'),
            empty_alternates: false,
            allow_unclosed_class: false,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Tokens(Vec<Token>);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Literal(char),
    Any,
    ZeroOrMore,
    RecursivePrefix,
    RecursiveSuffix,
    RecursiveZeroOrMore,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Alternates(Vec<Tokens>),
}

impl Glob {
    pub fn new(glob: &str) -> Result<Glob, Error> {
        GlobBuilder::new(glob).build()
    }

    #[cfg(test)]
    pub fn compile_matcher(&self) -> GlobMatcher {
        let re = new_regex(&self.re).expect("regex compilation shouldn't fail");
        GlobMatcher { re }
    }

    pub fn regex(&self) -> &str {
        &self.re
    }

    fn literal(&self) -> Option<String> {
        if self.opts.case_insensitive {
            return None;
        }
        let mut lit = String::new();
        for t in &self.tokens.0 {
            let Token::Literal(c) = *t else { return None };
            lit.push(c);
        }
        if lit.is_empty() { None } else { Some(lit) }
    }

    fn ext(&self) -> Option<String> {
        if self.opts.case_insensitive {
            return None;
        }
        let toks = &self.tokens.0;
        let start = usize::from(matches!(toks.first()?, Token::RecursivePrefix));
        match toks.get(start)? {
            Token::ZeroOrMore => {
                if start == 0 && self.opts.literal_separator {
                    return None;
                }
            }
            _ => return None,
        }
        match toks.get(start + 1)? {
            Token::Literal('.') => {}
            _ => return None,
        }
        let mut lit = ".".to_string();
        for t in &toks[start + 2..] {
            match *t {
                Token::Literal('.' | '/') => return None,
                Token::Literal(c) => lit.push(c),
                _ => return None,
            }
        }
        Some(lit)
    }

    fn required_ext(&self) -> Option<String> {
        if self.opts.case_insensitive {
            return None;
        }
        let mut ext: Vec<char> = vec![];
        for t in self.tokens.0.iter().rev() {
            match *t {
                Token::Literal('/') => return None,
                Token::Literal(c) => {
                    ext.push(c);
                    if c == '.' {
                        break;
                    }
                }
                _ => return None,
            }
        }
        if ext.last() == Some(&'.') {
            Some(ext.into_iter().rev().collect())
        } else {
            None
        }
    }

    fn prefix(&self) -> Option<String> {
        if self.opts.case_insensitive {
            return None;
        }
        let toks = &self.tokens.0;
        let (end, need_sep) = match toks.last()? {
            Token::ZeroOrMore => {
                if self.opts.literal_separator {
                    return None;
                }
                (toks.len() - 1, false)
            }
            Token::RecursiveSuffix => (toks.len() - 1, true),
            _ => (toks.len(), false),
        };
        let mut lit = String::new();
        for t in &toks[0..end] {
            let Token::Literal(c) = *t else { return None };
            lit.push(c);
        }
        if need_sep {
            lit.push('/');
        }
        if lit.is_empty() { None } else { Some(lit) }
    }

    fn suffix(&self) -> Option<(String, bool)> {
        if self.opts.case_insensitive {
            return None;
        }
        let toks = &self.tokens.0;
        let mut lit = String::new();
        let (start, entire) = match toks.first()? {
            Token::RecursivePrefix => {
                if let Some(&Token::Literal(_)) = toks.get(1) {
                    lit.push('/');
                    (1, true)
                } else {
                    (1, false)
                }
            }
            _ => (0, false),
        };
        let start = match toks.get(start)? {
            Token::ZeroOrMore => {
                if self.opts.literal_separator {
                    return None;
                }
                start + 1
            }
            _ => start,
        };
        for t in &toks[start..] {
            let Token::Literal(c) = *t else { return None };
            lit.push(c);
        }
        if lit.is_empty() || lit == "/" {
            None
        } else {
            Some((lit, entire))
        }
    }

    fn basename_tokens(&self) -> Option<&[Token]> {
        if self.opts.case_insensitive {
            return None;
        }
        let toks = &self.tokens.0;
        let start = match toks.first()? {
            Token::RecursivePrefix => 1,
            _ => return None,
        };
        if toks[start..].is_empty() {
            return None;
        }
        for t in &toks[start..] {
            match *t {
                Token::Literal(_) if *t != Token::Literal('/') => {}
                Token::Any | Token::ZeroOrMore => {
                    if !self.opts.literal_separator {
                        return None;
                    }
                }
                Token::Literal(_)
                | Token::RecursivePrefix
                | Token::RecursiveSuffix
                | Token::RecursiveZeroOrMore
                | Token::Class { .. }
                | Token::Alternates(..) => return None,
            }
        }
        Some(&toks[start..])
    }

    fn basename_literal(&self) -> Option<String> {
        let tokens = self.basename_tokens()?;
        let mut lit = String::new();
        for t in tokens {
            let Token::Literal(c) = *t else { return None };
            lit.push(c);
        }
        Some(lit)
    }
}

impl<'a> GlobBuilder<'a> {
    pub fn new(glob: &'a str) -> GlobBuilder<'a> {
        GlobBuilder {
            glob,
            opts: GlobOptions::default(),
        }
    }

    pub fn build(&self) -> Result<Glob, Error> {
        let mut p = Parser {
            glob: self.glob,
            alternates_stack: Vec::new(),
            branches: vec![Tokens::default()],
            chars: self.glob.chars().peekable(),
            prev: None,
            cur: None,
            found_unclosed_class: false,
            opts: &self.opts,
        };
        p.parse()?;
        if p.branches.len() > 1 {
            return Err(Error {
                glob: Some(self.glob.to_string()),
                kind: ErrorKind::UnclosedAlternates,
            });
        }
        let tokens = p.branches.pop().expect("parser always keeps one branch");
        Ok(Glob {
            pattern: self.glob.to_string(),
            re: tokens.to_regex_with(self.opts),
            opts: self.opts,
            tokens,
        })
    }

    pub fn case_insensitive(&mut self, yes: bool) -> &mut GlobBuilder<'a> {
        self.opts.case_insensitive = yes;
        self
    }

    pub fn literal_separator(&mut self, yes: bool) -> &mut GlobBuilder<'a> {
        self.opts.literal_separator = yes;
        self
    }

    pub fn backslash_escape(&mut self, yes: bool) -> &mut GlobBuilder<'a> {
        self.opts.backslash_escape = yes;
        self
    }

    #[cfg(test)]
    pub fn empty_alternates(&mut self, yes: bool) -> &mut GlobBuilder<'a> {
        self.opts.empty_alternates = yes;
        self
    }

    pub fn allow_unclosed_class(&mut self, yes: bool) -> &mut GlobBuilder<'a> {
        self.opts.allow_unclosed_class = yes;
        self
    }
}

impl Tokens {
    fn to_regex_with(&self, options: GlobOptions) -> String {
        let mut re = String::new();
        re.push_str("(?-u)");
        if options.case_insensitive {
            re.push_str("(?i)");
        }
        re.push('^');
        if self.0.len() == 1 && self.0[0] == Token::RecursivePrefix {
            re.push_str(".*$");
            return re;
        }
        tokens_to_regex(options, &self.0, &mut re);
        re.push('$');
        re
    }
}

fn tokens_to_regex(options: GlobOptions, tokens: &[Token], re: &mut String) {
    for tok in tokens {
        match tok {
            Token::Literal(c) => push_escaped_char(re, *c),
            Token::Any => re.push_str(if options.literal_separator {
                "[^/]"
            } else {
                "."
            }),
            Token::ZeroOrMore => {
                re.push_str(if options.literal_separator {
                    "[^/]*"
                } else {
                    ".*"
                });
            }
            Token::RecursivePrefix => re.push_str("(?:/?|.*/)"),
            Token::RecursiveSuffix => re.push_str("/.*"),
            Token::RecursiveZeroOrMore => re.push_str("(?:/|/.*/)"),
            Token::Class { negated, ranges } => {
                re.push('[');
                if *negated {
                    re.push('^');
                }
                for &(lo, hi) in ranges {
                    push_escaped_char(re, lo);
                    if lo != hi {
                        re.push('-');
                        push_escaped_char(re, hi);
                    }
                }
                re.push(']');
            }
            Token::Alternates(patterns) => {
                let mut parts = vec![];
                for pat in patterns {
                    let mut altre = String::new();
                    tokens_to_regex(options, &pat.0, &mut altre);
                    if !altre.is_empty() || options.empty_alternates {
                        parts.push(altre);
                    }
                }
                if !parts.is_empty() {
                    re.push_str("(?:");
                    re.push_str(&parts.join("|"));
                    re.push(')');
                }
            }
        }
    }
}

fn is_regex_meta(b: u8) -> bool {
    matches!(
        b,
        b'\\'
            | b'.'
            | b'+'
            | b'*'
            | b'?'
            | b'('
            | b')'
            | b'|'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'^'
            | b'$'
            | b'#'
            | b'&'
            | b'-'
            | b'~'
    )
}

fn push_escaped_char(re: &mut String, c: char) {
    let mut buf = [0; 4];
    for &b in c.encode_utf8(&mut buf).as_bytes() {
        if b <= 0x7F {
            if is_regex_meta(b) {
                re.push('\\');
            }
            re.push(char::from(b));
        } else {
            let _ = write!(re, "\\x{b:02x}");
        }
    }
}

struct Parser<'a> {
    glob: &'a str,
    alternates_stack: Vec<usize>,
    branches: Vec<Tokens>,
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    prev: Option<char>,
    cur: Option<char>,
    found_unclosed_class: bool,
    opts: &'a GlobOptions,
}

impl Parser<'_> {
    fn error(&self, kind: ErrorKind) -> Error {
        Error {
            glob: Some(self.glob.to_string()),
            kind,
        }
    }

    fn parse(&mut self) -> Result<(), Error> {
        while let Some(c) = self.bump() {
            match c {
                '?' => self.push_token(Token::Any)?,
                '*' => self.parse_star()?,
                '[' if !self.found_unclosed_class => self.parse_class()?,
                '{' => self.push_alternate(),
                '}' => self.pop_alternate()?,
                ',' => self.parse_comma()?,
                '\\' => self.parse_backslash()?,
                c => self.push_token(Token::Literal(c))?,
            }
        }
        Ok(())
    }

    fn push_alternate(&mut self) {
        self.alternates_stack.push(self.branches.len());
        self.branches.push(Tokens::default());
    }

    fn pop_alternate(&mut self) -> Result<(), Error> {
        let Some(start) = self.alternates_stack.pop() else {
            return Err(self.error(ErrorKind::UnopenedAlternates));
        };
        let alts = Token::Alternates(self.branches.drain(start..).collect());
        self.push_token(alts)
    }

    fn push_token(&mut self, tok: Token) -> Result<(), Error> {
        if let Some(pat) = self.branches.last_mut() {
            pat.0.push(tok);
            return Ok(());
        }
        Err(self.error(ErrorKind::UnopenedAlternates))
    }

    fn pop_token(&mut self) -> Result<Token, Error> {
        if let Some(pat) = self.branches.last_mut() {
            return Ok(pat.0.pop().expect("a token was pushed before"));
        }
        Err(self.error(ErrorKind::UnopenedAlternates))
    }

    fn have_tokens(&self) -> Result<bool, Error> {
        match self.branches.last() {
            None => Err(self.error(ErrorKind::UnopenedAlternates)),
            Some(pat) => Ok(!pat.0.is_empty()),
        }
    }

    fn parse_comma(&mut self) -> Result<(), Error> {
        if self.alternates_stack.is_empty() {
            self.push_token(Token::Literal(','))
        } else {
            self.branches.push(Tokens::default());
            Ok(())
        }
    }

    fn parse_backslash(&mut self) -> Result<(), Error> {
        if self.opts.backslash_escape {
            match self.bump() {
                None => Err(self.error(ErrorKind::DanglingEscape)),
                Some(c) => self.push_token(Token::Literal(c)),
            }
        } else if is_separator('\\') {
            self.push_token(Token::Literal('/'))
        } else {
            self.push_token(Token::Literal('\\'))
        }
    }

    fn push_two_stars(&mut self) -> Result<(), Error> {
        self.push_token(Token::ZeroOrMore)?;
        self.push_token(Token::ZeroOrMore)
    }

    fn parse_star(&mut self) -> Result<(), Error> {
        let prev = self.prev;
        if self.peek() != Some('*') {
            return self.push_token(Token::ZeroOrMore);
        }
        self.bump();
        if !self.have_tokens()? {
            if self.peek().is_some_and(|c| !is_separator(c)) {
                self.push_two_stars()?;
            } else {
                self.push_token(Token::RecursivePrefix)?;
                self.bump();
            }
            return Ok(());
        }
        if !prev.is_some_and(is_separator)
            && (self.branches.len() <= 1 || (prev != Some(',') && prev != Some('{')))
        {
            return self.push_two_stars();
        }
        let is_suffix = match self.peek() {
            None => {
                self.bump();
                true
            }
            Some(',' | '}') if self.branches.len() >= 2 => true,
            Some(c) if is_separator(c) => {
                self.bump();
                false
            }
            _ => return self.push_two_stars(),
        };
        match self.pop_token()? {
            Token::RecursivePrefix => self.push_token(Token::RecursivePrefix),
            Token::RecursiveSuffix => self.push_token(Token::RecursiveSuffix),
            _ if is_suffix => self.push_token(Token::RecursiveSuffix),
            _ => self.push_token(Token::RecursiveZeroOrMore),
        }
    }

    fn parse_class(&mut self) -> Result<(), Error> {
        let saved_chars = self.chars.clone();
        let saved_prev = self.prev;
        let saved_cur = self.cur;
        let mut ranges: Vec<(char, char)> = vec![];
        let negated = if matches!(self.chars.peek(), Some('!' | '^')) {
            self.bump();
            true
        } else {
            false
        };
        let mut first = true;
        let mut in_range = false;
        loop {
            let Some(c) = self.bump() else {
                if self.opts.allow_unclosed_class {
                    self.chars = saved_chars;
                    self.cur = saved_cur;
                    self.prev = saved_prev;
                    self.found_unclosed_class = true;
                    return self.push_token(Token::Literal('['));
                }
                return Err(self.error(ErrorKind::UnclosedClass));
            };
            match c {
                ']' => {
                    if first {
                        ranges.push((']', ']'));
                    } else {
                        break;
                    }
                }
                '-' => {
                    if first {
                        ranges.push(('-', '-'));
                    } else if in_range {
                        self.add_to_last_range(&mut ranges, '-')?;
                        in_range = false;
                    } else {
                        in_range = true;
                    }
                }
                c => {
                    if in_range {
                        self.add_to_last_range(&mut ranges, c)?;
                    } else {
                        ranges.push((c, c));
                    }
                    in_range = false;
                }
            }
            first = false;
        }
        if in_range {
            ranges.push(('-', '-'));
        }
        self.push_token(Token::Class { negated, ranges })
    }

    fn add_to_last_range(&self, ranges: &mut [(char, char)], add: char) -> Result<(), Error> {
        let r = ranges.last_mut().expect("a range start was seen");
        r.1 = add;
        if r.1 < r.0 {
            Err(self.error(ErrorKind::InvalidRange(r.0, r.1)))
        } else {
            Ok(())
        }
    }

    fn bump(&mut self) -> Option<char> {
        self.prev = self.cur;
        self.cur = self.chars.next();
        self.cur
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
}

pub(crate) struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

pub(crate) type FnvMap<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;

type LitMap = FnvMap<Vec<u8>, Vec<usize>>;

#[derive(Clone)]
pub struct Candidate<'a> {
    path: Cow<'a, [u8]>,
    base: usize,
    ext: usize,
    has_base: bool,
}

impl fmt::Debug for Candidate<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Candidate")
            .field("path", &String::from_utf8_lossy(&self.path))
            .field("basename", &String::from_utf8_lossy(self.basename()))
            .field("ext", &String::from_utf8_lossy(self.ext()))
            .finish()
    }
}

impl<'a> Candidate<'a> {
    pub fn new<P: AsRef<Path> + ?Sized>(path: &'a P) -> Candidate<'a> {
        Self::from_cow(path_bytes(path.as_ref()))
    }

    fn from_cow(path: Cow<'a, [u8]>) -> Candidate<'a> {
        let base = memchr::memrchr(b'/', &path).map_or(0, |i| i + 1);
        let has_base = !path.is_empty() && &path[base..] != b"..";
        let ext = if has_base {
            memchr::memrchr(b'.', &path[base..]).map_or(path.len(), |i| base + i)
        } else {
            path.len()
        };
        Candidate {
            path,
            base,
            ext,
            has_base,
        }
    }

    pub(crate) fn path(&self) -> &[u8] {
        &self.path
    }

    pub(crate) fn basename(&self) -> &[u8] {
        if self.has_base {
            &self.path[self.base..]
        } else {
            b""
        }
    }

    pub(crate) fn ext(&self) -> &[u8] {
        &self.path[self.ext..]
    }
}

#[cfg(unix)]
pub(crate) fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(path.as_os_str().as_bytes())
}

#[cfg(not(unix))]
pub(crate) fn path_bytes(path: &Path) -> Cow<'_, [u8]> {
    let bytes = match path.to_string_lossy() {
        Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
        Cow::Owned(s) => Cow::Owned(s.into_bytes()),
    };
    normalize_bytes(bytes)
}

#[cfg(not(unix))]
fn normalize_bytes(mut path: Cow<'_, [u8]>) -> Cow<'_, [u8]> {
    for i in 0..path.len() {
        if path[i] != b'/' && is_separator(char::from(path[i])) {
            path.to_mut()[i] = b'/';
        }
    }
    path
}

#[derive(Clone, Debug, Default)]
struct IndexMap {
    map: LitMap,
    lens: u64,
}

fn len_bit(len: usize) -> u64 {
    1 << len.min(63)
}

impl IndexMap {
    fn add(&mut self, index: usize, key: String) {
        self.lens |= len_bit(key.len());
        self.map.entry(key.into_bytes()).or_default().push(index);
    }

    fn get(&self, key: &[u8]) -> Option<&[usize]> {
        if key.is_empty() || self.lens & len_bit(key.len()) == 0 {
            return None;
        }
        self.map.get(key).map(Vec::as_slice)
    }
}

#[derive(Clone, Debug, Default)]
struct Affix {
    by_len: Vec<(usize, LitMap)>,
}

impl Affix {
    fn add(&mut self, index: usize, lit: String) {
        let len = lit.len();
        let pos = match self.by_len.binary_search_by_key(&len, |(l, _)| *l) {
            Ok(pos) => pos,
            Err(pos) => {
                self.by_len.insert(pos, (len, FnvMap::default()));
                pos
            }
        };
        self.by_len[pos]
            .1
            .entry(lit.into_bytes())
            .or_default()
            .push(index);
    }

    fn hits<'s>(&'s self, path: &'s [u8], suffix: bool) -> impl Iterator<Item = &'s [usize]> + 's {
        self.by_len
            .iter()
            .take_while(move |(len, _)| *len <= path.len())
            .filter_map(move |(len, map)| {
                let key = if suffix {
                    &path[path.len() - len..]
                } else {
                    &path[..*len]
                };
                map.get(key).map(Vec::as_slice)
            })
    }
}

#[derive(Clone, Debug)]
struct RegexStrategy {
    set: RegexSet,
    map: Vec<usize>,
}

#[derive(Clone, Debug)]
enum Strategy {
    Literal(IndexMap),
    BasenameLiteral(IndexMap),
    Extension(IndexMap),
    Prefix(Affix),
    Suffix(Affix),
    RequiredExtension(FnvMap<Vec<u8>, Vec<(usize, Regex)>>),
    Regex(RegexStrategy),
}

impl Strategy {
    #[cfg(test)]
    fn is_match(&self, c: &Candidate<'_>) -> bool {
        match self {
            Strategy::Literal(m) => m.get(c.path()).is_some(),
            Strategy::BasenameLiteral(m) => m.get(c.basename()).is_some(),
            Strategy::Extension(m) => m.get(c.ext()).is_some(),
            Strategy::Prefix(a) => a.hits(c.path(), false).next().is_some(),
            Strategy::Suffix(a) => a.hits(c.path(), true).next().is_some(),
            Strategy::RequiredExtension(m) => {
                let ext = c.ext();
                !ext.is_empty()
                    && m.get(ext)
                        .is_some_and(|res| res.iter().any(|(_, re)| re.is_match(c.path())))
            }
            Strategy::Regex(r) => r.set.is_match(c.path()),
        }
    }

    fn for_each_match(&self, c: &Candidate<'_>, f: &mut dyn FnMut(usize)) {
        let mut each = |hits: &[usize]| hits.iter().for_each(|&i| f(i));
        match self {
            Strategy::Literal(m) => m.get(c.path()).into_iter().for_each(&mut each),
            Strategy::BasenameLiteral(m) => m.get(c.basename()).into_iter().for_each(&mut each),
            Strategy::Extension(m) => m.get(c.ext()).into_iter().for_each(&mut each),
            Strategy::Prefix(a) => a.hits(c.path(), false).for_each(&mut each),
            Strategy::Suffix(a) => a.hits(c.path(), true).for_each(&mut each),
            Strategy::RequiredExtension(m) => {
                let ext = c.ext();
                if ext.is_empty() {
                    return;
                }
                for (i, re) in m.get(ext).into_iter().flatten() {
                    if re.is_match(c.path()) {
                        f(*i);
                    }
                }
            }
            Strategy::Regex(r) => {
                if r.map.len() == 1 {
                    if r.set.is_match(c.path()) {
                        f(r.map[0]);
                    }
                } else if r.set.is_match(c.path()) {
                    for i in &r.set.matches(c.path()) {
                        f(r.map[i]);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GlobSet {
    len: usize,
    strats: Vec<(usize, Strategy)>,
}

impl GlobSet {
    pub const fn empty() -> GlobSet {
        GlobSet {
            len: 0,
            strats: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[cfg(test)]
    pub fn is_match<P: AsRef<Path>>(&self, path: P) -> bool {
        self.is_match_candidate(&Candidate::new(path.as_ref()))
    }

    #[cfg(test)]
    pub fn is_match_candidate(&self, path: &Candidate<'_>) -> bool {
        self.strats.iter().any(|(_, s)| s.is_match(path))
    }

    pub(crate) fn last_match(
        &self,
        path: &Candidate<'_>,
        accept: &dyn Fn(usize) -> bool,
    ) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (max, s) in &self.strats {
            if best.is_some_and(|b| b >= *max) {
                continue;
            }
            s.for_each_match(path, &mut |i| {
                if best.is_none_or(|b| i > b) && accept(i) {
                    best = Some(i);
                }
            });
        }
        best
    }

    pub fn new<I, G>(globs: I) -> Result<GlobSet, Error>
    where
        I: IntoIterator<Item = G>,
        G: AsRef<Glob>,
    {
        let mut len = 0;
        let mut lits = IndexMap::default();
        let mut base_lits = IndexMap::default();
        let mut exts = IndexMap::default();
        let mut prefixes = Affix::default();
        let mut suffixes = Affix::default();
        let mut req_exts: FnvMap<Vec<u8>, Vec<(usize, String)>> = FnvMap::default();
        let mut regexes: Vec<String> = vec![];
        let mut regex_map: Vec<usize> = vec![];
        let mut maxes = [0usize; 7];
        let mut seen = [false; 7];
        for (i, p) in globs.into_iter().enumerate() {
            len += 1;
            let p = p.as_ref();
            let slot = match MatchStrategy::new(p) {
                MatchStrategy::Literal(lit) => {
                    lits.add(i, lit);
                    2
                }
                MatchStrategy::BasenameLiteral(lit) => {
                    base_lits.add(i, lit);
                    1
                }
                MatchStrategy::Extension(ext) => {
                    exts.add(i, ext);
                    0
                }
                MatchStrategy::Prefix(prefix) => {
                    prefixes.add(i, prefix);
                    4
                }
                MatchStrategy::Suffix { suffix, component } => {
                    if component {
                        lits.add(i, suffix[1..].to_string());
                        maxes[2] = i;
                        seen[2] = true;
                    }
                    suffixes.add(i, suffix);
                    3
                }
                MatchStrategy::RequiredExtension(ext) => {
                    req_exts
                        .entry(ext.into_bytes())
                        .or_default()
                        .push((i, p.regex().to_owned()));
                    5
                }
                MatchStrategy::Regex => {
                    regexes.push(p.regex().to_owned());
                    regex_map.push(i);
                    6
                }
            };
            maxes[slot] = i;
            seen[slot] = true;
        }
        let mut strats = Vec::with_capacity(7);
        if seen[0] {
            strats.push((maxes[0], Strategy::Extension(exts)));
        }
        if seen[1] {
            strats.push((maxes[1], Strategy::BasenameLiteral(base_lits)));
        }
        if seen[2] {
            strats.push((maxes[2], Strategy::Literal(lits)));
        }
        if seen[3] {
            strats.push((maxes[3], Strategy::Suffix(suffixes)));
        }
        if seen[4] {
            strats.push((maxes[4], Strategy::Prefix(prefixes)));
        }
        if seen[5] {
            let mut compiled = FnvMap::default();
            for (ext, list) in req_exts {
                let mut res = Vec::with_capacity(list.len());
                for (i, re) in list {
                    res.push((i, new_regex(&re)?));
                }
                compiled.insert(ext, res);
            }
            strats.push((maxes[5], Strategy::RequiredExtension(compiled)));
        }
        if seen[6] {
            let set = new_regex_set(&regexes)?;
            strats.push((
                maxes[6],
                Strategy::Regex(RegexStrategy {
                    set,
                    map: regex_map,
                }),
            ));
        }
        Ok(GlobSet { len, strats })
    }
}

#[derive(Clone, Debug, Default)]
pub struct GlobSetBuilder {
    pats: Vec<Glob>,
}

impl GlobSetBuilder {
    pub fn new() -> GlobSetBuilder {
        GlobSetBuilder { pats: vec![] }
    }

    pub fn build(&self) -> Result<GlobSet, Error> {
        GlobSet::new(self.pats.iter())
    }

    pub fn add(&mut self, pat: Glob) -> &mut GlobSetBuilder {
        self.pats.push(pat);
        self
    }
}
#[cfg(test)]
mod tests {
    use super::Token::*;
    use super::{ErrorKind, Glob, GlobBuilder, GlobSetBuilder, Token};

    #[derive(Clone, Copy, Debug, Default)]
    struct Options {
        casei: Option<bool>,
        litsep: Option<bool>,
        bsesc: Option<bool>,
        ealtre: Option<bool>,
        unccls: Option<bool>,
    }

    macro_rules! syntax {
        ($name:ident, $pat:expr, $tokens:expr) => {
            #[test]
            fn $name() {
                let pat = Glob::new($pat).unwrap();
                assert_eq!($tokens, pat.tokens.0);
            }
        };
    }

    macro_rules! syntaxerr {
        ($name:ident, $pat:expr, $err:expr) => {
            #[test]
            fn $name() {
                let err = Glob::new($pat).unwrap_err();
                assert_eq!(&$err, err.kind());
            }
        };
    }

    macro_rules! toregex {
        ($name:ident, $pat:expr, $re:expr) => {
            toregex!($name, $pat, $re, Options::default());
        };
        ($name:ident, $pat:expr, $re:expr, $options:expr) => {
            #[test]
            fn $name() {
                let mut builder = GlobBuilder::new($pat);
                if let Some(casei) = $options.casei {
                    builder.case_insensitive(casei);
                }
                if let Some(litsep) = $options.litsep {
                    builder.literal_separator(litsep);
                }
                if let Some(bsesc) = $options.bsesc {
                    builder.backslash_escape(bsesc);
                }
                if let Some(ealtre) = $options.ealtre {
                    builder.empty_alternates(ealtre);
                }
                if let Some(unccls) = $options.unccls {
                    builder.allow_unclosed_class(unccls);
                }

                let pat = builder.build().unwrap();
                assert_eq!(format!("(?-u){}", $re), pat.regex());
            }
        };
    }

    macro_rules! matches {
        ($name:ident, $pat:expr, $path:expr) => {
            matches!($name, $pat, $path, Options::default());
        };
        ($name:ident, $pat:expr, $path:expr, $options:expr) => {
            #[test]
            fn $name() {
                let mut builder = GlobBuilder::new($pat);
                if let Some(casei) = $options.casei {
                    builder.case_insensitive(casei);
                }
                if let Some(litsep) = $options.litsep {
                    builder.literal_separator(litsep);
                }
                if let Some(bsesc) = $options.bsesc {
                    builder.backslash_escape(bsesc);
                }
                if let Some(ealtre) = $options.ealtre {
                    builder.empty_alternates(ealtre);
                }
                let pat = builder.build().unwrap();
                let matcher = pat.compile_matcher();
                let set = GlobSetBuilder::new().add(pat).build().unwrap();
                assert!(matcher.is_match($path));
                assert!(set.is_match($path));
            }
        };
    }

    macro_rules! nmatches {
        ($name:ident, $pat:expr, $path:expr) => {
            nmatches!($name, $pat, $path, Options::default());
        };
        ($name:ident, $pat:expr, $path:expr, $options:expr) => {
            #[test]
            fn $name() {
                let mut builder = GlobBuilder::new($pat);
                if let Some(casei) = $options.casei {
                    builder.case_insensitive(casei);
                }
                if let Some(litsep) = $options.litsep {
                    builder.literal_separator(litsep);
                }
                if let Some(bsesc) = $options.bsesc {
                    builder.backslash_escape(bsesc);
                }
                if let Some(ealtre) = $options.ealtre {
                    builder.empty_alternates(ealtre);
                }
                let pat = builder.build().unwrap();
                let matcher = pat.compile_matcher();
                let set = GlobSetBuilder::new().add(pat).build().unwrap();
                assert!(!matcher.is_match($path));
                assert!(!set.is_match($path));
            }
        };
    }

    fn s(string: &str) -> String {
        string.to_string()
    }

    fn class(s: char, e: char) -> Token {
        Class {
            negated: false,
            ranges: vec![(s, e)],
        }
    }

    fn classn(s: char, e: char) -> Token {
        Class {
            negated: true,
            ranges: vec![(s, e)],
        }
    }

    fn rclass(ranges: &[(char, char)]) -> Token {
        Class {
            negated: false,
            ranges: ranges.to_vec(),
        }
    }

    fn rclassn(ranges: &[(char, char)]) -> Token {
        Class {
            negated: true,
            ranges: ranges.to_vec(),
        }
    }

    syntax!(literal1, "a", vec![Literal('a')]);
    syntax!(literal2, "ab", vec![Literal('a'), Literal('b')]);
    syntax!(any1, "?", vec![Any]);
    syntax!(any2, "a?b", vec![Literal('a'), Any, Literal('b')]);
    syntax!(seq1, "*", vec![ZeroOrMore]);
    syntax!(seq2, "a*b", vec![Literal('a'), ZeroOrMore, Literal('b')]);
    syntax!(
        seq3,
        "*a*b*",
        vec![
            ZeroOrMore,
            Literal('a'),
            ZeroOrMore,
            Literal('b'),
            ZeroOrMore,
        ]
    );
    syntax!(rseq1, "**", vec![RecursivePrefix]);
    syntax!(rseq2, "**/", vec![RecursivePrefix]);
    syntax!(rseq3, "/**", vec![RecursiveSuffix]);
    syntax!(rseq4, "/**/", vec![RecursiveZeroOrMore]);
    syntax!(
        rseq5,
        "a/**/b",
        vec![Literal('a'), RecursiveZeroOrMore, Literal('b'),]
    );
    syntax!(cls1, "[a]", vec![class('a', 'a')]);
    syntax!(cls2, "[!a]", vec![classn('a', 'a')]);
    syntax!(cls3, "[a-z]", vec![class('a', 'z')]);
    syntax!(cls4, "[!a-z]", vec![classn('a', 'z')]);
    syntax!(cls5, "[-]", vec![class('-', '-')]);
    syntax!(cls6, "[]]", vec![class(']', ']')]);
    syntax!(cls7, "[*]", vec![class('*', '*')]);
    syntax!(cls8, "[!!]", vec![classn('!', '!')]);
    syntax!(cls9, "[a-]", vec![rclass(&[('a', 'a'), ('-', '-')])]);
    syntax!(cls10, "[-a-z]", vec![rclass(&[('-', '-'), ('a', 'z')])]);
    syntax!(cls11, "[a-z-]", vec![rclass(&[('a', 'z'), ('-', '-')])]);
    syntax!(
        cls12,
        "[-a-z-]",
        vec![rclass(&[('-', '-'), ('a', 'z'), ('-', '-')]),]
    );
    syntax!(cls13, "[]-z]", vec![class(']', 'z')]);
    syntax!(cls14, "[--z]", vec![class('-', 'z')]);
    syntax!(cls15, "[ --]", vec![class(' ', '-')]);
    syntax!(cls16, "[0-9a-z]", vec![rclass(&[('0', '9'), ('a', 'z')])]);
    syntax!(cls17, "[a-z0-9]", vec![rclass(&[('a', 'z'), ('0', '9')])]);
    syntax!(cls18, "[!0-9a-z]", vec![rclassn(&[('0', '9'), ('a', 'z')])]);
    syntax!(cls19, "[!a-z0-9]", vec![rclassn(&[('a', 'z'), ('0', '9')])]);
    syntax!(cls20, "[^a]", vec![classn('a', 'a')]);
    syntax!(cls21, "[^a-z]", vec![classn('a', 'z')]);

    syntaxerr!(err_unclosed1, "[", ErrorKind::UnclosedClass);
    syntaxerr!(err_unclosed2, "[]", ErrorKind::UnclosedClass);
    syntaxerr!(err_unclosed3, "[!", ErrorKind::UnclosedClass);
    syntaxerr!(err_unclosed4, "[!]", ErrorKind::UnclosedClass);
    syntaxerr!(err_range1, "[z-a]", ErrorKind::InvalidRange('z', 'a'));
    syntaxerr!(err_range2, "[z--]", ErrorKind::InvalidRange('z', '-'));
    syntaxerr!(err_alt1, "{a,b", ErrorKind::UnclosedAlternates);
    syntaxerr!(err_alt2, "{a,{b,c}", ErrorKind::UnclosedAlternates);
    syntaxerr!(err_alt3, "a,b}", ErrorKind::UnopenedAlternates);
    syntaxerr!(err_alt4, "{a,b}}", ErrorKind::UnopenedAlternates);

    const CASEI: Options = Options {
        casei: Some(true),
        litsep: None,
        bsesc: None,
        ealtre: None,
        unccls: None,
    };
    const SLASHLIT: Options = Options {
        casei: None,
        litsep: Some(true),
        bsesc: None,
        ealtre: None,
        unccls: None,
    };
    const NOBSESC: Options = Options {
        casei: None,
        litsep: None,
        bsesc: Some(false),
        ealtre: None,
        unccls: None,
    };
    const BSESC: Options = Options {
        casei: None,
        litsep: None,
        bsesc: Some(true),
        ealtre: None,
        unccls: None,
    };
    const EALTRE: Options = Options {
        casei: None,
        litsep: None,
        bsesc: Some(true),
        ealtre: Some(true),
        unccls: None,
    };
    const UNCCLS: Options = Options {
        casei: None,
        litsep: None,
        bsesc: None,
        ealtre: None,
        unccls: Some(true),
    };

    toregex!(allow_unclosed_class_single, r"[", r"^\[$", &UNCCLS);
    toregex!(allow_unclosed_class_many, r"[abc", r"^\[abc$", &UNCCLS);
    toregex!(allow_unclosed_class_empty1, r"[]", r"^\[\]$", &UNCCLS);
    toregex!(allow_unclosed_class_empty2, r"[][", r"^\[\]\[$", &UNCCLS);
    toregex!(
        allow_unclosed_class_negated_unclosed,
        r"[!",
        r"^\[!$",
        &UNCCLS
    );
    toregex!(
        allow_unclosed_class_negated_empty,
        r"[!]",
        r"^\[!\]$",
        &UNCCLS
    );
    toregex!(
        allow_unclosed_class_brace1,
        r"{[abc,xyz}",
        r"^(?:\[abc|xyz)$",
        &UNCCLS
    );
    toregex!(
        allow_unclosed_class_brace2,
        r"{[abc,[xyz}",
        r"^(?:\[abc|\[xyz)$",
        &UNCCLS
    );
    toregex!(
        allow_unclosed_class_brace3,
        r"{[abc],[xyz}",
        r"^(?:[abc]|\[xyz)$",
        &UNCCLS
    );

    toregex!(re_empty, "", "^$");

    toregex!(re_casei, "a", "(?i)^a$", &CASEI);

    toregex!(re_slash1, "?", r"^[^/]$", SLASHLIT);
    toregex!(re_slash2, "*", r"^[^/]*$", SLASHLIT);

    toregex!(re1, "a", "^a$");
    toregex!(re2, "?", "^.$");
    toregex!(re3, "*", "^.*$");
    toregex!(re4, "a?", "^a.$");
    toregex!(re5, "?a", "^.a$");
    toregex!(re6, "a*", "^a.*$");
    toregex!(re7, "*a", "^.*a$");
    toregex!(re8, "[*]", r"^[\*]$");
    toregex!(re9, "[+]", r"^[\+]$");
    toregex!(re10, "+", r"^\+$");
    toregex!(re11, "\u{2603}", r"^\xe2\x98\x83$");
    toregex!(re12, "**", r"^.*$");
    toregex!(re13, "**/", r"^.*$");
    toregex!(re14, "**/*", r"^(?:/?|.*/).*$");
    toregex!(re15, "**/**", r"^.*$");
    toregex!(re16, "**/**/*", r"^(?:/?|.*/).*$");
    toregex!(re17, "**/**/**", r"^.*$");
    toregex!(re18, "**/**/**/*", r"^(?:/?|.*/).*$");
    toregex!(re19, "a/**", r"^a/.*$");
    toregex!(re20, "a/**/**", r"^a/.*$");
    toregex!(re21, "a/**/**/**", r"^a/.*$");
    toregex!(re22, "a/**/b", r"^a(?:/|/.*/)b$");
    toregex!(re23, "a/**/**/b", r"^a(?:/|/.*/)b$");
    toregex!(re24, "a/**/**/**/b", r"^a(?:/|/.*/)b$");
    toregex!(re25, "**/b", r"^(?:/?|.*/)b$");
    toregex!(re26, "**/**/b", r"^(?:/?|.*/)b$");
    toregex!(re27, "**/**/**/b", r"^(?:/?|.*/)b$");
    toregex!(re28, "a**", r"^a.*.*$");
    toregex!(re29, "**a", r"^.*.*a$");
    toregex!(re30, "a**b", r"^a.*.*b$");
    toregex!(re31, "***", r"^.*.*.*$");
    toregex!(re32, "/a**", r"^/a.*.*$");
    toregex!(re33, "/**a", r"^/.*.*a$");
    toregex!(re34, "/a**b", r"^/a.*.*b$");
    toregex!(re35, "{a,b}", r"^(?:a|b)$");
    toregex!(re36, "{a,{b,c}}", r"^(?:a|(?:b|c))$");
    toregex!(re37, "{{a,b},{c,d}}", r"^(?:(?:a|b)|(?:c|d))$");

    matches!(match1, "a", "a");
    matches!(match2, "a*b", "a_b");
    matches!(match3, "a*b*c", "abc");
    matches!(match4, "a*b*c", "a_b_c");
    matches!(match5, "a*b*c", "a___b___c");
    matches!(match6, "abc*abc*abc", "abcabcabcabcabcabcabc");
    matches!(
        match7,
        "a*a*a*a*a*a*a*a*a",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    matches!(match8, "a*b[xyz]c*d", "abxcdbxcddd");
    matches!(match9, "*.rs", ".rs");
    matches!(match10, "\u{2603}", "\u{2603}");

    matches!(matchrec1, "some/**/needle.txt", "some/needle.txt");
    matches!(matchrec2, "some/**/needle.txt", "some/one/needle.txt");
    matches!(matchrec3, "some/**/needle.txt", "some/one/two/needle.txt");
    matches!(matchrec4, "some/**/needle.txt", "some/other/needle.txt");
    matches!(matchrec5, "**", "abcde");
    matches!(matchrec6, "**", "");
    matches!(matchrec7, "**", ".asdf");
    matches!(matchrec8, "**", "/x/.asdf");
    matches!(matchrec9, "some/**/**/needle.txt", "some/needle.txt");
    matches!(matchrec10, "some/**/**/needle.txt", "some/one/needle.txt");
    matches!(
        matchrec11,
        "some/**/**/needle.txt",
        "some/one/two/needle.txt"
    );
    matches!(matchrec12, "some/**/**/needle.txt", "some/other/needle.txt");
    matches!(matchrec13, "**/test", "one/two/test");
    matches!(matchrec14, "**/test", "one/test");
    matches!(matchrec15, "**/test", "test");
    matches!(matchrec16, "/**/test", "/one/two/test");
    matches!(matchrec17, "/**/test", "/one/test");
    matches!(matchrec18, "/**/test", "/test");
    matches!(matchrec19, "**/.*", ".abc");
    matches!(matchrec20, "**/.*", "abc/.abc");
    matches!(matchrec21, "**/foo/bar", "foo/bar");
    matches!(matchrec22, ".*/**", ".abc/abc");
    matches!(matchrec23, "test/**", "test/");
    matches!(matchrec24, "test/**", "test/one");
    matches!(matchrec25, "test/**", "test/one/two");
    matches!(matchrec26, "some/*/needle.txt", "some/one/needle.txt");

    matches!(matchrange1, "a[0-9]b", "a0b");
    matches!(matchrange2, "a[0-9]b", "a9b");
    matches!(matchrange3, "a[!0-9]b", "a_b");
    matches!(matchrange4, "[a-z123]", "1");
    matches!(matchrange5, "[1a-z23]", "1");
    matches!(matchrange6, "[123a-z]", "1");
    matches!(matchrange7, "[abc-]", "-");
    matches!(matchrange8, "[-abc]", "-");
    matches!(matchrange9, "[-a-c]", "b");
    matches!(matchrange10, "[a-c-]", "b");
    matches!(matchrange11, "[-]", "-");
    matches!(matchrange12, "a[^0-9]b", "a_b");

    matches!(matchpat1, "*hello.txt", "hello.txt");
    matches!(matchpat2, "*hello.txt", "gareth_says_hello.txt");
    matches!(matchpat3, "*hello.txt", "some/path/to/hello.txt");
    matches!(matchpat4, "*hello.txt", "some\\path\\to\\hello.txt");
    matches!(matchpat5, "*hello.txt", "/an/absolute/path/to/hello.txt");
    matches!(
        matchpat6,
        "*some/path/to/hello.txt",
        "some/path/to/hello.txt"
    );
    matches!(
        matchpat7,
        "*some/path/to/hello.txt",
        "a/bigger/some/path/to/hello.txt"
    );

    matches!(matchescape, "_[[]_[]]_[?]_[*]_!_", "_[_]_?_*_!_");

    matches!(matchcasei1, "aBcDeFg", "aBcDeFg", CASEI);
    matches!(matchcasei2, "aBcDeFg", "abcdefg", CASEI);
    matches!(matchcasei3, "aBcDeFg", "ABCDEFG", CASEI);
    matches!(matchcasei4, "aBcDeFg", "AbCdEfG", CASEI);

    matches!(matchalt1, "a,b", "a,b");
    matches!(matchalt2, ",", ",");
    matches!(matchalt3, "{a,b}", "a");
    matches!(matchalt4, "{a,b}", "b");
    matches!(matchalt5, "{**/src/**,foo}", "abc/src/bar");
    matches!(matchalt6, "{**/src/**,foo}", "foo");
    matches!(matchalt7, "{[}],foo}", "}");
    matches!(matchalt8, "{foo}", "foo");
    matches!(matchalt9, "{}", "");
    matches!(matchalt10, "{,}", "");
    matches!(matchalt11, "{*.foo,*.bar,*.wat}", "test.foo");
    matches!(matchalt12, "{*.foo,*.bar,*.wat}", "test.bar");
    matches!(matchalt13, "{*.foo,*.bar,*.wat}", "test.wat");
    matches!(matchalt14, "foo{,.txt}", "foo.txt");
    nmatches!(matchalt15, "foo{,.txt}", "foo");
    matches!(matchalt16, "foo{,.txt}", "foo", EALTRE);
    matches!(matchalt17, "{a,b{c,d}}", "bc");
    matches!(matchalt18, "{a,b{c,d}}", "bd");
    matches!(matchalt19, "{a,b{c,d}}", "a");

    matches!(matchslash1, "abc/def", "abc/def", SLASHLIT);
    #[cfg(unix)]
    nmatches!(matchslash2, "abc?def", "abc/def", SLASHLIT);
    #[cfg(not(unix))]
    nmatches!(matchslash2, "abc?def", "abc\\def", SLASHLIT);
    nmatches!(matchslash3, "abc*def", "abc/def", SLASHLIT);
    matches!(matchslash4, "abc[/]def", "abc/def", SLASHLIT);
    #[cfg(unix)]
    nmatches!(matchslash5, "abc\\def", "abc/def", SLASHLIT);
    #[cfg(not(unix))]
    matches!(matchslash5, "abc\\def", "abc/def", SLASHLIT);

    matches!(matchbackslash1, "\\[", "[", BSESC);
    matches!(matchbackslash2, "\\?", "?", BSESC);
    matches!(matchbackslash3, "\\*", "*", BSESC);
    matches!(matchbackslash4, "\\[a-z]", "\\a", NOBSESC);
    matches!(matchbackslash5, "\\?", "\\a", NOBSESC);
    matches!(matchbackslash6, "\\*", "\\\\", NOBSESC);
    #[cfg(unix)]
    matches!(matchbackslash7, "\\a", "a");
    #[cfg(not(unix))]
    matches!(matchbackslash8, "\\a", "/a");

    nmatches!(matchnot1, "a*b*c", "abcd");
    nmatches!(matchnot2, "abc*abc*abc", "abcabcabcabcabcabcabca");
    nmatches!(matchnot3, "some/**/needle.txt", "some/other/notthis.txt");
    nmatches!(matchnot4, "some/**/**/needle.txt", "some/other/notthis.txt");
    nmatches!(matchnot5, "/**/test", "test");
    nmatches!(matchnot6, "/**/test", "/one/notthis");
    nmatches!(matchnot7, "/**/test", "/notthis");
    nmatches!(matchnot8, "**/.*", "ab.c");
    nmatches!(matchnot9, "**/.*", "abc/ab.c");
    nmatches!(matchnot10, ".*/**", "a.bc");
    nmatches!(matchnot11, ".*/**", "abc/a.bc");
    nmatches!(matchnot12, "a[0-9]b", "a_b");
    nmatches!(matchnot13, "a[!0-9]b", "a0b");
    nmatches!(matchnot14, "a[!0-9]b", "a9b");
    nmatches!(matchnot15, "[!-]", "-");
    nmatches!(matchnot16, "*hello.txt", "hello.txt-and-then-some");
    nmatches!(matchnot17, "*hello.txt", "goodbye.txt");
    nmatches!(
        matchnot18,
        "*some/path/to/hello.txt",
        "some/path/to/hello.txt-and-then-some"
    );
    nmatches!(
        matchnot19,
        "*some/path/to/hello.txt",
        "some/other/path/to/hello.txt"
    );
    nmatches!(matchnot20, "a", "foo/a");
    nmatches!(matchnot21, "./foo", "foo");
    nmatches!(matchnot22, "**/foo", "foofoo");
    nmatches!(matchnot23, "**/foo/bar", "foofoo/bar");
    nmatches!(matchnot24, "/*.c", "mozilla-sha1/sha1.c");
    nmatches!(matchnot25, "*.c", "mozilla-sha1/sha1.c", SLASHLIT);
    nmatches!(
        matchnot26,
        "**/m4/ltoptions.m4",
        "csharp/src/packages/repositories.config",
        SLASHLIT
    );
    nmatches!(matchnot27, "a[^0-9]b", "a0b");
    nmatches!(matchnot28, "a[^0-9]b", "a9b");
    nmatches!(matchnot29, "[^-]", "-");
    nmatches!(matchnot30, "some/*/needle.txt", "some/needle.txt");
    nmatches!(
        matchrec31,
        "some/*/needle.txt",
        "some/one/two/needle.txt",
        SLASHLIT
    );
    nmatches!(
        matchrec32,
        "some/*/needle.txt",
        "some/one/two/three/needle.txt",
        SLASHLIT
    );
    nmatches!(matchrec33, ".*/**", ".abc");
    nmatches!(matchrec34, "foo/**", "foo");

    macro_rules! extract {
        ($which:ident, $name:ident, $pat:expr, $expect:expr) => {
            extract!($which, $name, $pat, $expect, Options::default());
        };
        ($which:ident, $name:ident, $pat:expr, $expect:expr, $options:expr) => {
            #[test]
            fn $name() {
                let mut builder = GlobBuilder::new($pat);
                if let Some(casei) = $options.casei {
                    builder.case_insensitive(casei);
                }
                if let Some(litsep) = $options.litsep {
                    builder.literal_separator(litsep);
                }
                if let Some(bsesc) = $options.bsesc {
                    builder.backslash_escape(bsesc);
                }
                if let Some(ealtre) = $options.ealtre {
                    builder.empty_alternates(ealtre);
                }
                let pat = builder.build().unwrap();
                assert_eq!($expect, pat.$which());
            }
        };
    }

    macro_rules! literal {
        ($($tt:tt)*) => { extract!(literal, $($tt)*); }
    }

    macro_rules! basetokens {
        ($($tt:tt)*) => { extract!(basename_tokens, $($tt)*); }
    }

    macro_rules! ext {
        ($($tt:tt)*) => { extract!(ext, $($tt)*); }
    }

    macro_rules! required_ext {
        ($($tt:tt)*) => { extract!(required_ext, $($tt)*); }
    }

    macro_rules! prefix {
        ($($tt:tt)*) => { extract!(prefix, $($tt)*); }
    }

    macro_rules! suffix {
        ($($tt:tt)*) => { extract!(suffix, $($tt)*); }
    }

    macro_rules! baseliteral {
        ($($tt:tt)*) => { extract!(basename_literal, $($tt)*); }
    }

    literal!(extract_lit1, "foo", Some(s("foo")));
    literal!(extract_lit2, "foo", None, CASEI);
    literal!(extract_lit3, "/foo", Some(s("/foo")));
    literal!(extract_lit4, "/foo/", Some(s("/foo/")));
    literal!(extract_lit5, "/foo/bar", Some(s("/foo/bar")));
    literal!(extract_lit6, "*.foo", None);
    literal!(extract_lit7, "foo/bar", Some(s("foo/bar")));
    literal!(extract_lit8, "**/foo/bar", None);

    basetokens!(
        extract_basetoks1,
        "**/foo",
        Some(&*vec![Literal('f'), Literal('o'), Literal('o'),])
    );
    basetokens!(extract_basetoks2, "**/foo", None, CASEI);
    basetokens!(
        extract_basetoks3,
        "**/foo",
        Some(&*vec![Literal('f'), Literal('o'), Literal('o'),]),
        SLASHLIT
    );
    basetokens!(extract_basetoks4, "*foo", None, SLASHLIT);
    basetokens!(extract_basetoks5, "*foo", None);
    basetokens!(extract_basetoks6, "**/fo*o", None);
    basetokens!(
        extract_basetoks7,
        "**/fo*o",
        Some(&*vec![Literal('f'), Literal('o'), ZeroOrMore, Literal('o'),]),
        SLASHLIT
    );

    ext!(extract_ext1, "**/*.rs", Some(s(".rs")));
    ext!(extract_ext2, "**/*.rs.bak", None);
    ext!(extract_ext3, "*.rs", Some(s(".rs")));
    ext!(extract_ext4, "a*.rs", None);
    ext!(extract_ext5, "/*.c", None);
    ext!(extract_ext6, "*.c", None, SLASHLIT);
    ext!(extract_ext7, "*.c", Some(s(".c")));

    required_ext!(extract_req_ext1, "*.rs", Some(s(".rs")));
    required_ext!(extract_req_ext2, "/foo/bar/*.rs", Some(s(".rs")));
    required_ext!(extract_req_ext3, "/foo/bar/*.rs", Some(s(".rs")));
    required_ext!(extract_req_ext4, "/foo/bar/.rs", Some(s(".rs")));
    required_ext!(extract_req_ext5, ".rs", Some(s(".rs")));
    required_ext!(extract_req_ext6, "./rs", None);
    required_ext!(extract_req_ext7, "foo", None);
    required_ext!(extract_req_ext8, ".foo/", None);
    required_ext!(extract_req_ext9, "foo/", None);

    prefix!(extract_prefix1, "/foo", Some(s("/foo")));
    prefix!(extract_prefix2, "/foo/*", Some(s("/foo/")));
    prefix!(extract_prefix3, "**/foo", None);
    prefix!(extract_prefix4, "foo/**", Some(s("foo/")));

    suffix!(extract_suffix1, "**/foo/bar", Some((s("/foo/bar"), true)));
    suffix!(extract_suffix2, "*/foo/bar", Some((s("/foo/bar"), false)));
    suffix!(extract_suffix3, "*/foo/bar", None, SLASHLIT);
    suffix!(extract_suffix4, "foo/bar", Some((s("foo/bar"), false)));
    suffix!(extract_suffix5, "*.foo", Some((s(".foo"), false)));
    suffix!(extract_suffix6, "*.foo", None, SLASHLIT);
    suffix!(extract_suffix7, "**/*_test", Some((s("_test"), false)));

    baseliteral!(extract_baselit1, "**/foo", Some(s("foo")));
    baseliteral!(extract_baselit2, "foo", None);
    baseliteral!(extract_baselit3, "*foo", None);
    baseliteral!(extract_baselit4, "*/foo", None);
}
