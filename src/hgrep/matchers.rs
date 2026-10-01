use std::collections::HashMap;

use crate::hc_internal::gnu::regex as gnu_regex;
use crate::hc_internal::grep::matcher::{
    Captures, LineMatchKind, LineTerminator, Match, Matcher, NoCaptures, ParallelMatcher,
};
use crate::hc_internal::grep::regex::{RegexMatcher, RegexMatcherBuilder};
use crate::hc_internal::pcre;

use super::flags::{BoundaryMode, CaseMode};
use super::patterns::GnuSource;

#[derive(Clone, Debug)]
pub(crate) enum PatternMatcher {
    Rust(RegexMatcher),
    Pcre(PcreMatcher),
    Gnu(GnuAdapter),
}

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RustSpec<'a> {
    pub(crate) patterns: &'a [Vec<u8>],
    pub(crate) fixed_strings: bool,
    pub(crate) case: CaseMode,
    pub(crate) boundary: Option<BoundaryMode>,
    pub(crate) no_unicode: bool,
    pub(crate) multiline: bool,
    pub(crate) multiline_dotall: bool,
    pub(crate) crlf: bool,
    pub(crate) null_data: bool,
    pub(crate) regex_size_limit: Option<usize>,
    pub(crate) dfa_size_limit: Option<usize>,
    pub(crate) ban_nul: bool,
}

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct PcreSpec<'a> {
    pub(crate) patterns: &'a [Vec<u8>],
    pub(crate) fixed_strings: bool,
    pub(crate) case: CaseMode,
    pub(crate) boundary: Option<BoundaryMode>,
    pub(crate) no_unicode: bool,
    pub(crate) multiline: bool,
    pub(crate) multiline_dotall: bool,
    pub(crate) crlf: bool,
    pub(crate) gnu: bool,
}

pub(crate) struct GnuSpec<'a> {
    pub(crate) sources: &'a [GnuSource],
    pub(crate) dialect: gnu_regex::Dialect,
    pub(crate) case: CaseMode,
    pub(crate) boundary: Option<BoundaryMode>,
    pub(crate) null_data: bool,
    pub(crate) utf8: bool,
}

fn utf8_patterns(patterns: &[Vec<u8>]) -> Result<Vec<String>, String> {
    patterns
        .iter()
        .map(|p| {
            String::from_utf8(p.clone()).map_err(|_| "pattern given is not valid UTF-8".to_string())
        })
        .collect()
}

fn suggest_pcre2(msg: String) -> String {
    if msg.contains("backreferences") || msg.contains("look-around") {
        format!(
            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle backreferences\nand look-around."
        )
    } else {
        msg
    }
}

fn suggest_multiline(msg: String) -> String {
    if msg.contains("the literal") && msg.contains("not allowed") {
        format!(
            "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for short).\nWhen multiline mode is enabled, new line characters can be matched."
        )
    } else {
        msg
    }
}

fn suggest_text(msg: String) -> String {
    if msg.contains("pattern contains \"\\0\"") {
        format!(
            "{msg}\n\nConsider enabling text mode with the --text flag (or -a for short). Otherwise,\nbinary detection is enabled and matching a NUL byte is impossible."
        )
    } else {
        msg
    }
}

pub(crate) fn build_rust_raw(spec: &RustSpec<'_>) -> Result<PatternMatcher, String> {
    let patterns = utf8_patterns(spec.patterns)?;
    let mut builder = RegexMatcherBuilder::new();
    builder
        .multi_line(true)
        .unicode(!spec.no_unicode)
        .octal(false)
        .fixed_strings(spec.fixed_strings);
    match spec.case {
        CaseMode::Sensitive => builder.case_insensitive(false),
        CaseMode::Insensitive => builder.case_insensitive(true),
        CaseMode::Smart => builder.case_smart(true),
    };
    match spec.boundary {
        Some(BoundaryMode::Line) => {
            builder.whole_line(true);
        }
        Some(BoundaryMode::Word) => {
            builder.word(true);
        }
        None => {}
    }
    if spec.multiline {
        builder.dot_matches_new_line(spec.multiline_dotall);
        if spec.crlf {
            builder.crlf(true).line_terminator(None);
        }
    } else {
        builder
            .line_terminator(Some(b'\n'))
            .dot_matches_new_line(false);
        if spec.crlf {
            builder.crlf(true);
        }
        if spec.null_data {
            builder.line_terminator(Some(b'\0'));
        }
    }
    if let Some(limit) = spec.regex_size_limit {
        builder.size_limit(limit);
    }
    if let Some(limit) = spec.dfa_size_limit {
        builder.dfa_size_limit(limit);
    }
    if spec.ban_nul {
        builder.ban_byte(Some(b'\0'));
    }
    builder
        .build_many(&patterns)
        .map(PatternMatcher::Rust)
        .map_err(|err| suggest_text(suggest_multiline(err.to_string())))
}

pub(crate) fn build_rust(spec: &RustSpec<'_>) -> Result<PatternMatcher, String> {
    build_rust_raw(spec).map_err(suggest_pcre2)
}

#[derive(Clone, Debug)]
pub(crate) struct PcreMatcher {
    regex: pcre::Regex,
    utf: bool,
    empty: [bool; 2],
    names: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct PcreCaptures {
    locs: pcre::CaptureLocations,
}

impl Captures for PcreCaptures {
    fn get(&self, i: usize) -> Option<Match> {
        self.locs.get(i).map(|(s, e)| Match::new(s, e))
    }
}

fn has_uppercase_literal(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c.is_uppercase() {
            return true;
        }
    }
    false
}

pub(crate) fn build_pcre2(spec: &PcreSpec<'_>) -> Result<PatternMatcher, String> {
    let patterns = utf8_patterns(spec.patterns)?;
    if spec.gnu && patterns.len() > 1 {
        return Err("the -P option only supports a single pattern".into());
    }
    let joined = if patterns.is_empty() {
        r"[^\S\s]".to_string()
    } else {
        patterns
            .iter()
            .map(|p| {
                if spec.fixed_strings {
                    format!("(?:{})", pcre::escape(p))
                } else {
                    format!("(?:{p})")
                }
            })
            .collect::<Vec<_>>()
            .join("|")
    };
    let mut config = pcre::Config {
        multi_line: !spec.gnu,
        dollar_endonly: spec.gnu,
        ascii_bsd: spec.gnu,
        ..pcre::Config::default()
    };
    config.caseless = match spec.case {
        CaseMode::Sensitive => false,
        CaseMode::Insensitive => true,
        CaseMode::Smart => !has_uppercase_literal(&joined),
    };
    let pattern = match spec.boundary {
        Some(BoundaryMode::Line) if spec.gnu => {
            config.match_line = true;
            joined
        }
        Some(BoundaryMode::Line) => format!(r"(?m:^)(?:{joined})(?m:$)"),
        Some(BoundaryMode::Word) => format!(r"(?<!\w)(?:{joined})(?!\w)"),
        None => joined,
    };
    if !spec.no_unicode {
        config.utf = true;
        config.ucp = true;
    }
    if spec.multiline {
        config.dotall = spec.multiline_dotall;
    }
    if spec.crlf {
        config.crlf = true;
    }
    let regex = pcre::Regex::new(&pattern, config).map_err(|e| e.to_string())?;
    let names = regex
        .capture_names()
        .iter()
        .enumerate()
        .filter_map(|(i, name)| name.as_ref().map(|n| (n.clone(), i)))
        .collect();
    let empty = [false, true].map(|bol| {
        regex
            .find_at_with(
                b"",
                0,
                pcre::MatchOptions {
                    not_bol: !bol,
                    not_eol: false,
                },
            )
            .is_ok_and(|m| m.is_some())
    });
    Ok(PatternMatcher::Pcre(PcreMatcher {
        regex,
        utf: config.utf,
        empty,
        names,
    }))
}

#[derive(Debug)]
pub(crate) struct PcreError(String);

impl std::fmt::Display for PcreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PcreError> for std::io::Error {
    fn from(err: PcreError) -> std::io::Error {
        std::io::Error::other(err.0)
    }
}

pub(crate) fn gnu_match_message(code: i32) -> String {
    match code {
        -46 | -47 => "exceeded PCRE's backtracking limit".to_string(),
        -53 => "exceeded PCRE's nested backtracking limit".to_string(),
        -52 => "PCRE detected recurse loop".to_string(),
        -63 => "exceeded PCRE's heap limit".to_string(),
        -48 => "memory exhausted".to_string(),
        other => format!("internal PCRE error: {other}"),
    }
}

impl PcreMatcher {
    pub(crate) fn utf(&self) -> bool {
        self.utf
    }

    pub(crate) fn empty_match(&self, bol: bool) -> bool {
        self.empty[usize::from(bol)]
    }

    pub(crate) fn find_in(
        &self,
        subject: &[u8],
        offset: usize,
        not_bol: bool,
    ) -> Result<Option<(usize, usize)>, i32> {
        self.regex
            .find_at_with(
                subject,
                offset,
                pcre::MatchOptions {
                    not_bol,
                    not_eol: false,
                },
            )
            .map_err(|e| e.code())
    }
}

impl ParallelMatcher for PcreMatcher {
    fn par_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        self.find_candidate_line(haystack).map_err(|e| e.0)
    }

    fn par_is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        self.is_match(haystack).map_err(|e| e.0)
    }

    fn par_find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, String> {
        self.find_at(haystack, at).map_err(|e| e.0)
    }
}

impl Matcher for PcreMatcher {
    type Captures = PcreCaptures;
    type Error = PcreError;

    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        Some(self)
    }

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, PcreError> {
        self.regex
            .find_at(haystack, at)
            .map(|m| m.map(|(s, e)| Match::new(s, e)))
            .map_err(|e| PcreError(e.to_string()))
    }

    fn new_captures(&self) -> Result<PcreCaptures, PcreError> {
        Ok(PcreCaptures {
            locs: self.regex.capture_locations(),
        })
    }

    #[cfg(test)]
    fn capture_count(&self) -> usize {
        self.regex.captures_len()
    }

    fn capture_index(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    fn captures_at(
        &self,
        haystack: &[u8],
        at: usize,
        caps: &mut PcreCaptures,
    ) -> Result<bool, PcreError> {
        self.regex
            .captures_read_at(&mut caps.locs, haystack, at)
            .map(|m| m.is_some())
            .map_err(|e| PcreError(e.to_string()))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GnuAdapter {
    inner: std::sync::Arc<gnu_regex::Matcher>,
    terminator: u8,
}

impl GnuAdapter {
    pub(crate) fn fork(&self) -> GnuAdapter {
        GnuAdapter {
            inner: std::sync::Arc::new((*self.inner).clone()),
            terminator: self.terminator,
        }
    }

    pub(crate) fn regex(&self) -> &gnu_regex::Matcher {
        &self.inner
    }

    fn strip<'h>(&self, haystack: &'h [u8]) -> &'h [u8] {
        haystack
            .strip_suffix(&[self.terminator])
            .unwrap_or(haystack)
    }
}

pub(crate) fn build_gnu(spec: &GnuSpec<'_>) -> Result<PatternMatcher, String> {
    let mut opts = gnu_regex::Options::new(spec.dialect, spec.utf8);
    opts.ignore_case = match spec.case {
        CaseMode::Sensitive => false,
        CaseMode::Insensitive => true,
        CaseMode::Smart => !spec.sources.iter().any(|source| {
            String::from_utf8_lossy(source.bytes())
                .chars()
                .any(char::is_uppercase)
        }),
    };
    opts.scope = match spec.boundary {
        Some(BoundaryMode::Line) => gnu_regex::Scope::Lines,
        Some(BoundaryMode::Word) => gnu_regex::Scope::Words,
        None => gnu_regex::Scope::Anywhere,
    };
    opts.null_data = spec.null_data;
    let mut set = gnu_regex::PatternSet::new(opts);
    for source in spec.sources {
        match source {
            GnuSource::Expression(text) => set.add_expression(text),
            GnuSource::Operand(text) => set.add_operand(text),
            GnuSource::File { name, content } => set.add_file(name, content),
        }
    }
    let prog = super::messages::prog();
    let compiled = set
        .build()
        .map_err(|failure| failure.messages.join(&format!("\n{prog}: ")))?;
    for warning in &compiled.warnings {
        super::messages::eprint_locked(warning);
    }
    Ok(PatternMatcher::Gnu(GnuAdapter {
        inner: std::sync::Arc::new(compiled.matcher),
        terminator: if spec.null_data { b'\0' } else { b'\n' },
    }))
}

impl ParallelMatcher for GnuAdapter {
    fn par_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        self.find_candidate_line(haystack)
            .map_err(|e| e.to_string())
    }

    fn par_is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        self.is_match(haystack).map_err(|e| e.to_string())
    }

    fn par_find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, String> {
        self.find_at(haystack, at).map_err(|e| e.to_string())
    }
}

impl Matcher for GnuAdapter {
    type Captures = NoCaptures;
    type Error = std::io::Error;

    fn parallel(&self) -> Option<&dyn ParallelMatcher> {
        Some(self)
    }

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, std::io::Error> {
        let line = self.strip(haystack);
        if at > line.len() {
            return Ok(None);
        }
        Ok(self.inner.find_at(line, at).map(|(s, e)| Match::new(s, e)))
    }

    fn new_captures(&self) -> Result<NoCaptures, std::io::Error> {
        Ok(NoCaptures::new())
    }

    fn line_terminator(&self) -> Option<LineTerminator> {
        Some(LineTerminator::byte(self.terminator))
    }

    fn is_match(&self, haystack: &[u8]) -> Result<bool, std::io::Error> {
        Ok(self.inner.is_match(self.strip(haystack)))
    }

    fn find_candidate_line(
        &self,
        haystack: &[u8],
    ) -> Result<Option<LineMatchKind>, std::io::Error> {
        Ok(self
            .inner
            .find_line(haystack, 0)
            .map(|(start, _)| LineMatchKind::Confirmed(start)))
    }
}
