use memchr::memmem::Finder;
use regex::bytes::{Regex, RegexBuilder};

use crate::cli::Cli;

pub(crate) enum Matcher {
    Never,
    Literal(Box<Finder<'static>>),
    Regex(Regex),
    Word(Regex),
}

impl Matcher {
    pub(crate) fn find_at(&self, hay: &[u8], at: usize) -> Option<(usize, usize)> {
        match self {
            Matcher::Never => None,
            Matcher::Literal(f) => f
                .find(&hay[at..])
                .map(|i| (at + i, at + i + f.needle().len())),
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
            Matcher::Literal(f) => f.find(hay).is_some(),
            Matcher::Regex(r) | Matcher::Word(r) => r.is_match(hay),
        }
    }
}

fn is_plain_literal(p: &[u8]) -> bool {
    !p.is_empty() && !p.iter().any(|b| br"\.+*?()|[]{}^$".contains(b))
}

pub(crate) fn build_matcher(cli: &Cli, patterns: &[Vec<u8>]) -> Result<Matcher, String> {
    if patterns.is_empty() {
        return Ok(Matcher::Never);
    }
    if patterns.len() == 1
        && !cli.ignore_case
        && !cli.word
        && !cli.line_regexp
        && (cli.fixed || is_plain_literal(&patterns[0]))
    {
        return Ok(Matcher::Literal(Box::new(
            Finder::new(&patterns[0]).into_owned(),
        )));
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
