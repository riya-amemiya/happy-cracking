use std::collections::HashSet;
use std::io::{self, Read};
use std::path::Path;

use super::escape::escape;
use super::flags::{LowArgs, Mode, PatternSource, Personality, os_bytes};
use super::read_pattern_file_with_limit;

#[derive(Clone, Debug)]
pub(crate) enum GnuSource {
    Expression(Vec<u8>),
    Operand(Vec<u8>),
    File { name: String, content: Vec<u8> },
}

impl GnuSource {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            GnuSource::Expression(text) | GnuSource::Operand(text) => text,
            GnuSource::File { content, .. } => content,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Patterns {
    pub(crate) list: Vec<Vec<u8>>,
    pub(crate) gnu_empty_source: bool,
    pub(crate) gnu_sources: Vec<GnuSource>,
}

#[derive(Debug)]
pub(crate) struct PatternError(pub(crate) String);

pub(crate) struct StdinState {
    pub(crate) consumed: bool,
}

fn invalid_utf8(bytes: &[u8], valid_up_to: usize) -> String {
    format!(
        "found invalid UTF-8 in pattern at byte offset {valid_up_to}: {} \
         (disable Unicode mode and use hex escape sequences to match \
         arbitrary bytes in a pattern, e.g., '(?-u)\\xFF')",
        escape(bytes)
    )
}

fn split_lines_rg(data: &[u8], origin: &str) -> Result<Vec<Vec<u8>>, PatternError> {
    let body = data.strip_suffix(b"\n").unwrap_or(data);
    if data.is_empty() {
        return Ok(Vec::new());
    }
    body.split(|&b| b == b'\n')
        .enumerate()
        .map(|(index, line)| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            std::str::from_utf8(line)
                .map(|_| line.to_vec())
                .map_err(|err| {
                    PatternError(format!(
                        "{origin}:{}: {}",
                        index + 1,
                        invalid_utf8(line, err.valid_up_to())
                    ))
                })
        })
        .collect()
}

fn read_stdin_all() -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    io::stdin().lock().read_to_end(&mut buf)?;
    Ok(buf)
}

fn rg_patterns(low: &mut LowArgs, stdin: &mut StdinState) -> Result<Patterns, PatternError> {
    if low.patterns.is_empty() {
        if low.positional.is_empty() {
            return Err(PatternError(
                "ripgrep requires at least one pattern to execute a search".into(),
            ));
        }
        let pattern = low.positional.remove(0);
        let Some(text) = pattern.to_str() else {
            return Err(PatternError("pattern given is not valid UTF-8".into()));
        };
        return Ok(Patterns {
            list: vec![text.as_bytes().to_vec()],
            ..Patterns::default()
        });
    }
    let mut seen = HashSet::new();
    let mut list = Vec::with_capacity(low.patterns.len());
    let mut add = |pattern: Vec<u8>| {
        if seen.insert(pattern.clone()) {
            list.push(pattern);
        }
    };
    for source in std::mem::take(&mut low.patterns) {
        match source {
            PatternSource::Regexp(pattern) => add(os_bytes(&pattern).to_vec()),
            PatternSource::File(path) if path == Path::new("-") => {
                if stdin.consumed {
                    return Err(PatternError(
                        "error reading -f/--file from stdin: stdin has already been consumed"
                            .into(),
                    ));
                }
                let data = read_stdin_all().map_err(|e| PatternError(format!("<stdin>:{e}")))?;
                for pattern in split_lines_rg(&data, "<stdin>")? {
                    add(pattern);
                }
                stdin.consumed = true;
            }
            PatternSource::File(path) => {
                let data = read_pattern_file_with_limit(&path, super::MAX_PATTERN_FILE_BYTES)
                    .map_err(|e| PatternError(format!("{}: {e}", path.display())))?;
                for pattern in split_lines_rg(&data, &path.display().to_string())? {
                    add(pattern);
                }
            }
        }
    }
    Ok(Patterns {
        list,
        ..Patterns::default()
    })
}

fn gnu_patterns(
    low: &mut LowArgs,
    stdin: &mut StdinState,
    fixed: bool,
) -> Result<Patterns, PatternError> {
    let explicit = !low.patterns.is_empty();
    let mut keys: Vec<u8> = Vec::new();
    let mut sources = Vec::new();
    if explicit {
        for source in std::mem::take(&mut low.patterns) {
            match source {
                PatternSource::Regexp(pattern) => {
                    keys.extend_from_slice(os_bytes(&pattern));
                    keys.push(b'\n');
                    sources.push(GnuSource::Expression(os_bytes(&pattern).to_vec()));
                }
                PatternSource::File(path) => {
                    let data = if path == Path::new("-") {
                        stdin.consumed = true;
                        read_stdin_all()
                    } else {
                        read_pattern_file_with_limit(&path, super::MAX_PATTERN_FILE_BYTES)
                    }
                    .map_err(|e| {
                        PatternError(format!(
                            "{}: {}",
                            path.display(),
                            super::messages::strerror(&e)
                        ))
                    })?;
                    keys.extend_from_slice(&data);
                    if !data.is_empty() && !data.ends_with(b"\n") {
                        keys.push(b'\n');
                    }
                    sources.push(GnuSource::File {
                        name: path.display().to_string(),
                        content: data,
                    });
                }
            }
        }
    } else {
        if low.positional.is_empty() {
            return Err(PatternError(String::new()));
        }
        let pattern = low.positional.remove(0);
        let bytes = os_bytes(&pattern);
        let skip = !fixed && bytes.starts_with(b"\\-");
        keys.extend_from_slice(&bytes[usize::from(skip)..]);
        keys.push(b'\n');
        sources.push(GnuSource::Operand(bytes.to_vec()));
    }
    if keys.is_empty() {
        return Ok(Patterns {
            list: Vec::new(),
            gnu_empty_source: true,
            gnu_sources: sources,
        });
    }
    keys.pop();
    let mut seen = HashSet::new();
    let list = keys
        .split(|&b| b == b'\n')
        .filter(|p| seen.insert(p.to_vec()))
        .map(<[u8]>::to_vec)
        .collect();
    Ok(Patterns {
        list,
        gnu_empty_source: false,
        gnu_sources: sources,
    })
}

pub(crate) fn from_low_args(
    low: &mut LowArgs,
    personality: Personality,
    gnu_dialect: bool,
    stdin: &mut StdinState,
) -> Result<Patterns, PatternError> {
    if !matches!(low.mode, Mode::Search(_)) {
        return Ok(Patterns::default());
    }
    if personality == Personality::Grep || gnu_dialect {
        gnu_patterns(low, stdin, low.fixed_strings)
    } else {
        rg_patterns(low, stdin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn low_with(patterns: Vec<PatternSource>, positional: &[&str]) -> LowArgs {
        LowArgs {
            patterns,
            positional: positional.iter().map(OsString::from).collect(),
            ..LowArgs::default()
        }
    }

    #[test]
    fn gnu_splits_newlines_and_dedups() {
        let mut low = low_with(Vec::new(), &["a\nb\na", "file"]);
        let mut stdin = StdinState { consumed: false };
        let got = from_low_args(&mut low, Personality::Grep, true, &mut stdin).unwrap();
        assert_eq!(got.list, vec![b"a".to_vec(), b"b".to_vec()]);
        assert_eq!(low.positional, vec![OsString::from("file")]);
    }

    #[test]
    fn gnu_strips_leading_backslash_dash() {
        let mut low = low_with(Vec::new(), &["\\-x"]);
        let mut stdin = StdinState { consumed: false };
        let got = from_low_args(&mut low, Personality::Grep, true, &mut stdin).unwrap();
        assert_eq!(got.list, vec![b"-x".to_vec()]);
    }

    #[test]
    fn rg_requires_a_pattern_and_keeps_order() {
        let mut low = low_with(Vec::new(), &[]);
        let mut stdin = StdinState { consumed: false };
        let err = from_low_args(&mut low, Personality::Rg, false, &mut stdin).unwrap_err();
        assert_eq!(
            err.0,
            "ripgrep requires at least one pattern to execute a search"
        );
        let mut low = low_with(
            vec![
                PatternSource::Regexp("b".into()),
                PatternSource::Regexp("a".into()),
                PatternSource::Regexp("b".into()),
            ],
            &["x"],
        );
        let got = from_low_args(&mut low, Personality::Rg, false, &mut stdin).unwrap();
        assert_eq!(got.list, vec![b"b".to_vec(), b"a".to_vec()]);
    }

    #[test]
    fn rg_pattern_lines_strip_crlf() {
        assert_eq!(
            split_lines_rg(b"a\r\nb\n", "f").unwrap(),
            vec![b"a".to_vec(), b"b".to_vec()]
        );
        assert!(
            split_lines_rg(b"a\xFF\n", "f")
                .unwrap_err()
                .0
                .starts_with("f:1: found invalid UTF-8")
        );
    }
}
