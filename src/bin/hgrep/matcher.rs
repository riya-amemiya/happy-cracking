use memchr::{memchr, memchr2, memmem::Finder};
use regex::bytes::{Regex, RegexBuilder};

use crate::cli::Cli;

pub(crate) enum Matcher {
    Never,
    Literal {
        finder: Box<Finder<'static>>,
        whole_line: bool,
    },
    ICase {
        needle: Box<[u8]>,
        whole_line: bool,
    },
    Regex(Regex),
    Word(Regex),
}

impl Matcher {
    pub(crate) fn find_at(&self, hay: &[u8], at: usize) -> Option<(usize, usize)> {
        match self {
            Matcher::Never => None,
            Matcher::Literal { finder, whole_line } => find_literal(hay, at, finder, *whole_line),
            Matcher::ICase { needle, whole_line } => find_icase(hay, at, needle, *whole_line),
            Matcher::Regex(r) => r.find_at(hay, at).map(|m| (m.start(), m.end())),
            Matcher::Word(r) => r
                .captures_at(hay, at)
                .and_then(|c| c.get(1))
                .map(|m| (m.start(), m.end())),
        }
    }

    pub(crate) fn is_match(&self, hay: &[u8]) -> bool {
        match self {
            Matcher::Never => false,
            Matcher::Literal {
                finder,
                whole_line: false,
            } => finder.find(hay).is_some(),
            Matcher::ICase {
                needle,
                whole_line: false,
            } => find_ci(hay, needle).is_some(),
            Matcher::Regex(r) | Matcher::Word(r) => r.is_match(hay),
            _ => self.find_at(hay, 0).is_some(),
        }
    }

    pub(crate) fn stream_overlap(&self) -> Option<usize> {
        match self {
            Matcher::Literal {
                finder,
                whole_line: false,
            } => Some(finder.needle().len().saturating_sub(1)),
            Matcher::ICase {
                needle,
                whole_line: false,
            } => Some(needle.len().saturating_sub(1)),
            _ => None,
        }
    }
}

fn whole_line(hay: &[u8], start: usize, end: usize) -> bool {
    (start == 0 || hay[start - 1] == b'\n') && (end == hay.len() || hay[end] == b'\n')
}

fn next_pos(start: usize, end: usize) -> usize {
    if end > start { end } else { start + 1 }
}

fn find_literal(
    hay: &[u8],
    mut at: usize,
    finder: &Finder<'_>,
    require_line: bool,
) -> Option<(usize, usize)> {
    let n = finder.needle().len();
    while let Some(i) = finder.find(&hay[at..]) {
        let start = at + i;
        let end = start + n;
        if !require_line || whole_line(hay, start, end) {
            return Some((start, end));
        }
        at = next_pos(start, end);
        if at > hay.len() {
            break;
        }
    }
    None
}

fn skip_index(needle: &[u8]) -> usize {
    needle
        .iter()
        .position(|b| !b.is_ascii_alphabetic())
        .unwrap_or(needle.len() - 1)
}

fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    let idx = skip_index(needle);
    let b = needle[idx];
    let lo = b.to_ascii_lowercase();
    let hi = b.to_ascii_uppercase();
    let mut from = 0usize;
    loop {
        let rel = if lo == hi {
            memchr(lo, &hay[from..])
        } else {
            memchr2(lo, hi, &hay[from..])
        };
        let rel = rel?;
        let hit = from + rel;
        if hit >= idx {
            let start = hit - idx;
            let end = start + needle.len();
            if end <= hay.len() && hay[start..end].eq_ignore_ascii_case(needle) {
                return Some(start);
            }
        }
        from = hit + 1;
    }
}

fn find_icase(
    hay: &[u8],
    mut at: usize,
    needle: &[u8],
    require_line: bool,
) -> Option<(usize, usize)> {
    let n = needle.len();
    while let Some(i) = find_ci(&hay[at..], needle) {
        let start = at + i;
        let end = start + n;
        if !require_line || whole_line(hay, start, end) {
            return Some((start, end));
        }
        at = next_pos(start, end);
        if at > hay.len() {
            break;
        }
    }
    None
}

fn is_plain_literal(p: &[u8]) -> bool {
    !p.is_empty() && !p.iter().any(|b| br"\.+*?()|[]{}^$".contains(b))
}

pub(crate) fn build_matcher(cli: &Cli, patterns: &[Vec<u8>]) -> Result<Matcher, String> {
    if patterns.is_empty() {
        return Ok(Matcher::Never);
    }
    if patterns.len() == 1
        && !cli.word
        && !patterns[0].is_empty()
        && (cli.fixed || is_plain_literal(&patterns[0]))
    {
        if !cli.ignore_case {
            return Ok(Matcher::Literal {
                finder: Box::new(Finder::new(&patterns[0]).into_owned()),
                whole_line: cli.line_regexp,
            });
        }
        if patterns[0].is_ascii() {
            return Ok(Matcher::ICase {
                needle: patterns[0].clone().into_boxed_slice(),
                whole_line: cli.line_regexp,
            });
        }
    }

    let mut parts = Vec::with_capacity(patterns.len());
    for p in patterns {
        let text = std::str::from_utf8(p).map_err(|_| "pattern is not valid UTF-8".to_string())?;
        parts.push(if cli.fixed {
            format!("(?:{})", regex::escape(text))
        } else {
            format!("(?:{text})")
        });
    }
    let joined = parts.join("|");

    let shaped = if cli.line_regexp {
        format!("^(?:{joined})$")
    } else if cli.word {
        format!(r"(?:^|[^\w])((?:{joined}))(?:[^\w]|$)")
    } else {
        joined
    };

    let wrap = if cli.word && !cli.line_regexp {
        Matcher::Word
    } else {
        Matcher::Regex
    };
    RegexBuilder::new(&shaped)
        .case_insensitive(cli.ignore_case)
        .multi_line(true)
        .build()
        .map(wrap)
        .map_err(|e| e.to_string())
}
