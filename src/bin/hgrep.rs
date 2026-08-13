use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{ArgAction, Parser};
use memchr::memmem::Finder;
use memchr::{memchr, memchr_iter, memrchr};
use rayon::prelude::*;
use regex::bytes::{Regex, RegexBuilder, RegexSet, RegexSetBuilder};

#[derive(Parser)]
#[command(
    name = "hgrep",
    about = "grep-compatible line matcher",
    disable_help_flag = true
)]
struct Cli {
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    patterns: Vec<OsString>,
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pattern_file: Option<PathBuf>,
    #[arg(short = 'F', long = "fixed-strings")]
    fixed: bool,
    #[arg(short = 'i', long = "ignore-case")]
    ignore_case: bool,
    #[arg(short = 'v', long = "invert-match")]
    invert: bool,
    #[arg(short = 'w', long = "word-regexp")]
    word: bool,
    #[arg(short = 'x', long = "line-regexp")]
    line_regexp: bool,
    #[arg(short = 'c', long = "count")]
    count: bool,
    #[arg(short = 'n', long = "line-number")]
    line_number: bool,
    #[arg(short = 'l', long = "files-with-matches")]
    files_with_matches: bool,
    #[arg(short = 'L', long = "files-without-match")]
    files_without_match: bool,
    #[arg(short = 'o', long = "only-matching")]
    only_matching: bool,
    #[arg(short = 'q', long = "quiet", alias = "silent")]
    quiet: bool,
    #[arg(short = 'r', long = "recursive", visible_short_alias = 'R')]
    recursive: bool,
    #[arg(
        long = "gitignore",
        requires = "recursive",
        help = "Skip paths excluded by .gitignore while walking (requires -r). Reads .gitignore files from the enclosing repository root down to each visited directory, then .git/info/exclude, then core.excludesFile, honors core.ignorecase, and always skips .git"
    )]
    gitignore: bool,
    #[arg(short = 'H', long = "with-filename")]
    with_filename: bool,
    #[arg(short = 'h', long = "no-filename")]
    no_filename: bool,
    #[arg(short = 'a', long = "text")]
    text: bool,
    #[arg(short = 's', long = "no-messages")]
    no_messages: bool,
    #[arg(short = 'm', long = "max-count", value_name = "NUM")]
    max_count: Option<u64>,
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    help: Option<bool>,
    #[arg(value_name = "PATTERN_OR_FILE")]
    operands: Vec<OsString>,
}

const MMAP_THRESHOLD: u64 = 64 * 1024;

struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}

unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr, self.len) };
    }
}

enum Source {
    Map(Mapped),
    Owned(Vec<u8>),
}

impl Source {
    fn bytes(&self) -> &[u8] {
        match self {
            Source::Map(m) => unsafe { std::slice::from_raw_parts(m.ptr as *const u8, m.len) },
            Source::Owned(v) => v,
        }
    }
}

fn open_source(path: &Path) -> io::Result<Source> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < MMAP_THRESHOLD || len > usize::MAX as u64 {
        let mut buf = Vec::with_capacity(len as usize);
        (&file).read_to_end(&mut buf)?;
        return Ok(Source::Owned(buf));
    }
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len as usize,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        let mut buf = Vec::with_capacity(len as usize);
        (&file).read_to_end(&mut buf)?;
        return Ok(Source::Owned(buf));
    }
    unsafe { libc::madvise(ptr, len as usize, libc::MADV_WILLNEED) };
    Ok(Source::Map(Mapped {
        ptr,
        len: len as usize,
    }))
}

enum Matcher {
    Never,
    Literal(Finder<'static>),
    Regex(Regex),
    Word(Regex),
}

impl Matcher {
    fn find_at(&self, hay: &[u8], at: usize) -> Option<(usize, usize)> {
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

    fn is_match(&self, hay: &[u8]) -> bool {
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

fn build_matcher(cli: &Cli, patterns: &[Vec<u8>]) -> Result<Matcher, String> {
    if patterns.is_empty() {
        return Ok(Matcher::Never);
    }
    if patterns.len() == 1
        && !cli.ignore_case
        && !cli.word
        && !cli.line_regexp
        && (cli.fixed || is_plain_literal(&patterns[0]))
    {
        return Ok(Matcher::Literal(Finder::new(&patterns[0]).into_owned()));
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

fn line_bounds(buf: &[u8], start: usize, end: usize) -> (usize, usize) {
    (
        memrchr(b'\n', &buf[..start]).map_or(0, |i| i + 1),
        memchr(b'\n', &buf[end..]).map_or(buf.len(), |i| end + i),
    )
}

struct Job<'a> {
    matcher: &'a Matcher,
    cli: &'a Cli,
    show_name: bool,
    emit_lines: bool,
}

fn write_prefix(out: &mut Vec<u8>, job: &Job, name: &[u8], line_no: u64) {
    if job.show_name {
        out.extend_from_slice(name);
        out.push(b':');
    }
    if job.cli.line_number {
        out.extend_from_slice(line_no.to_string().as_bytes());
        out.push(b':');
    }
}

fn search_forward(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    let limit = job.cli.max_count.unwrap_or(u64::MAX);
    if limit == 0 {
        return 0;
    }
    let trailing = buf.last().is_none_or(|&b| b == b'\n');
    let mut count = 0u64;
    let mut pos = 0usize;
    let mut line_no = base_line + 1;
    let mut counted = 0usize;

    while pos <= buf.len() {
        let Some((s, e)) = job.matcher.find_at(buf, pos) else {
            break;
        };
        if s == buf.len() && trailing {
            break;
        }
        let (ls, le) = line_bounds(buf, s, e);
        count += 1;

        if job.emit_lines {
            if job.cli.line_number {
                line_no += memchr_iter(b'\n', &buf[counted..ls]).count() as u64;
                counted = ls;
            }
            if job.cli.only_matching {
                let mut p = ls;
                while let Some((ms, me)) = job.matcher.find_at(&buf[..le], p) {
                    write_prefix(out, job, name, line_no);
                    out.extend_from_slice(&buf[ms..me]);
                    out.push(b'\n');
                    p = if me > ms { me } else { ms + 1 };
                    if p > le {
                        break;
                    }
                }
            } else {
                write_prefix(out, job, name, line_no);
                out.extend_from_slice(&buf[ls..le]);
                out.push(b'\n');
            }
        }

        if count >= limit || le >= buf.len() {
            break;
        }
        pos = le + 1;
    }
    count
}

fn search_inverted(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    let limit = job.cli.max_count.unwrap_or(u64::MAX);
    if limit == 0 {
        return 0;
    }
    let mut count = 0u64;
    let mut line_no = base_line;
    let mut start = 0usize;

    while start <= buf.len() {
        let end = memchr(b'\n', &buf[start..]).map_or(buf.len(), |i| start + i);
        if start == buf.len() && buf.last().is_none_or(|&b| b == b'\n') {
            break;
        }
        line_no += 1;
        if !job.matcher.is_match(&buf[start..end]) {
            count += 1;
            if job.emit_lines {
                write_prefix(out, job, name, line_no);
                out.extend_from_slice(&buf[start..end]);
                out.push(b'\n');
            }
            if count >= limit {
                break;
            }
        }
        if end >= buf.len() {
            break;
        }
        start = end + 1;
    }
    count
}

fn is_binary(buf: &[u8]) -> bool {
    memchr(b'\0', &buf[..buf.len().min(8192)]).is_some()
}

const PARALLEL_THRESHOLD: usize = 1 << 20;

fn split_chunks(buf: &[u8], parts: usize) -> Vec<(usize, usize)> {
    let target = buf.len() / parts + 1;
    let mut chunks = Vec::with_capacity(parts + 1);
    let mut start = 0usize;
    while start < buf.len() {
        let want = (start + target).min(buf.len());
        let end = match memchr(b'\n', &buf[want..]) {
            Some(i) if want + i + 1 < buf.len() => want + i + 1,
            _ => buf.len(),
        };
        chunks.push((start, end));
        start = end;
    }
    chunks
}

fn search_slice(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    if job.cli.invert {
        search_inverted(buf, job, name, base_line, out)
    } else {
        search_forward(buf, job, name, base_line, out)
    }
}

fn search_split(buf: &[u8], job: &Job, name: &[u8], out: &mut Vec<u8>) -> u64 {
    let chunks = split_chunks(buf, rayon::current_num_threads() * 4);
    let bases = if job.cli.line_number {
        chunks
            .par_iter()
            .map(|&(s, e)| memchr_iter(b'\n', &buf[s..e]).count() as u64)
            .collect::<Vec<_>>()
            .iter()
            .scan(0u64, |acc, n| {
                let base = *acc;
                *acc += n;
                Some(base)
            })
            .collect()
    } else {
        vec![0u64; chunks.len()]
    };

    let parts: Vec<(u64, Vec<u8>)> = chunks
        .par_iter()
        .zip(bases)
        .map(|(&(s, e), base)| {
            let mut body = Vec::new();
            let count = search_slice(&buf[s..e], job, name, base, &mut body);
            (count, body)
        })
        .collect();

    parts.iter().fold(0, |total, (count, body)| {
        out.extend_from_slice(body);
        total + count
    })
}

fn search_buf(buf: &[u8], job: &Job, name: &[u8], out: &mut Vec<u8>) -> u64 {
    let before = out.len();
    let count = if buf.len() >= PARALLEL_THRESHOLD && job.cli.max_count.is_none() {
        search_split(buf, job, name, out)
    } else {
        search_slice(buf, job, name, 0, out)
    };
    if count > 0 && !job.cli.text && is_binary(buf) {
        out.truncate(before);
        if job.emit_lines {
            out.extend_from_slice(b"Binary file ");
            out.extend_from_slice(name);
            out.extend_from_slice(b" matches\n");
        }
    }
    count
}

fn selected(cli: &Cli, count: u64) -> bool {
    if cli.files_without_match && !cli.files_with_matches {
        count == 0
    } else {
        count > 0
    }
}

fn report(job: &Job, name: &[u8], count: u64, body: Vec<u8>, out: &mut Vec<u8>) {
    let cli = job.cli;
    if cli.quiet {
        return;
    }
    if cli.files_with_matches {
        if count > 0 {
            out.extend_from_slice(name);
            out.push(b'\n');
        }
    } else if cli.files_without_match {
        if count == 0 {
            out.extend_from_slice(name);
            out.push(b'\n');
        }
    } else if cli.count {
        if job.show_name {
            out.extend_from_slice(name);
            out.push(b':');
        }
        out.extend_from_slice(count.to_string().as_bytes());
        out.push(b'\n');
    } else {
        out.extend_from_slice(&body);
    }
}

struct Ignore {
    parent: Option<Arc<Ignore>>,
    base: usize,
    set: RegexSet,
    negate: Vec<bool>,
    dir_only: Vec<bool>,
}

impl Ignore {
    fn ignored(&self, rel: &[u8], is_dir: bool) -> bool {
        std::iter::successors(Some(self), |n| n.parent.as_deref())
            .find_map(|n| {
                n.set
                    .matches(&rel[n.base..])
                    .iter()
                    .rev()
                    .find(|&i| is_dir || !n.dir_only[i])
                    .map(|i| !n.negate[i])
            })
            .unwrap_or(false)
    }
}

const POSIX_CLASSES: [&str; 12] = [
    "alnum", "alpha", "blank", "cntrl", "digit", "graph", "lower", "print", "punct", "space",
    "upper", "xdigit",
];

const IGNORE_SIZE_LIMIT: usize = 1 << 28;

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";

fn push_byte(out: &mut String, b: u8) {
    if b.is_ascii_alphanumeric() {
        out.push(b as char);
    } else {
        out.push_str("\\x");
        out.push(HEX_DIGITS[(b >> 4) as usize] as char);
        out.push(HEX_DIGITS[(b & 0x0f) as usize] as char);
    }
}

fn push_range(body: &mut String, lo: u8, hi: u8, fold: bool) {
    if hi < lo {
        return;
    }
    push_byte(body, lo);
    body.push('-');
    push_byte(body, hi);
    let (ulo, uhi) = (lo.max(b'A'), hi.min(b'Z'));
    if fold && ulo <= uhi {
        push_byte(body, ulo + 32);
        body.push('-');
        push_byte(body, uhi + 32);
    }
}

fn push_class(pat: &[u8], open: usize, out: &mut String, fold: bool) -> Option<usize> {
    let mut i = open + 1;
    let negated = matches!(pat.get(i), Some(b'!' | b'^'));
    if negated {
        i += 1;
    }
    let mut body = String::new();
    let mut prev: Option<u8> = None;
    let mut first = true;
    while let Some(&ch) = pat.get(i) {
        if ch == b']' && !first {
            if negated {
                out.push_str("[^\\x2F");
                out.push_str(&body);
                out.push(']');
            } else {
                out.push_str("[[");
                out.push_str(&body);
                out.push_str("]&&[^\\x2F]]");
            }
            return Some(i + 1);
        }
        first = false;
        if ch == b'\\' {
            let &esc = pat.get(i + 1)?;
            push_byte(&mut body, esc);
            prev = Some(esc);
            i += 2;
        } else if let Some(lo) = prev
            && ch == b'-'
            && pat.get(i + 1).is_some_and(|&n| n != b']')
        {
            let escaped = pat[i + 1] == b'\\';
            let hi = if escaped {
                *pat.get(i + 2)?
            } else {
                pat[i + 1]
            };
            push_range(&mut body, lo, hi, fold);
            prev = None;
            i += if escaped { 3 } else { 2 };
        } else if ch == b'[' && pat.get(i + 1) == Some(&b':') {
            let rest = &pat[i + 2..];
            let close = rest.iter().position(|&b| b == b']')?;
            match rest[..close].strip_suffix(b":").filter(|n| !n.is_empty()) {
                Some(name) => {
                    let &text = POSIX_CLASSES.iter().find(|c| c.as_bytes() == name)?;
                    let text = if fold && (text == "upper" || text == "lower") {
                        "lower"
                    } else {
                        text
                    };
                    body.push_str("[:");
                    body.push_str(text);
                    body.push_str(":]");
                    prev = None;
                    i += close + 3;
                }
                None => {
                    push_byte(&mut body, b'[');
                    prev = Some(b'[');
                    i += 1;
                }
            }
        } else {
            push_byte(&mut body, ch);
            prev = Some(ch);
            i += 1;
        }
    }
    None
}

fn glob_to_regex(pat: &[u8], fold: bool) -> Option<String> {
    let mut out = String::with_capacity(pat.len() * 4 + 8);
    let mut i = 0usize;
    while i < pat.len() {
        match pat[i] {
            b'*' => {
                let run = pat[i..].iter().take_while(|&&b| b == b'*').count();
                let starts = i == 0 || pat[i - 1] == b'/';
                let ends = i + run == pat.len() || pat[i + run] == b'/';
                if run >= 2 && starts && ends {
                    if i + run == pat.len() {
                        out.push_str(".*");
                        i += run;
                    } else {
                        out.push_str("(?:.*/)?");
                        i += run + 1;
                    }
                } else {
                    out.push_str("[^/]*");
                    i += run;
                }
            }
            b'?' => {
                out.push_str("[^/]");
                i += 1;
            }
            b'[' => i = push_class(pat, i, &mut out, fold)?,
            b'\\' => {
                push_byte(&mut out, *pat.get(i + 1)?);
                i += 2;
            }
            b => {
                push_byte(&mut out, if fold { b.to_ascii_lowercase() } else { b });
                i += 1;
            }
        }
    }
    Some(out)
}

fn strip_trailing_spaces(line: &[u8]) -> &[u8] {
    let escaped = |at: usize| {
        !line[..at]
            .iter()
            .rev()
            .take_while(|&&b| b == b'\\')
            .count()
            .is_multiple_of(2)
    };
    let keep = (0..=line.len())
        .rev()
        .find(|&end| end == 0 || line[end - 1] != b' ' || escaped(end - 1))
        .unwrap_or(0);
    &line[..keep]
}

fn parse_rule(line: &[u8], fold: bool) -> Option<(String, bool, bool)> {
    if line.first() == Some(&b'#') {
        return None;
    }
    let trimmed = strip_trailing_spaces(line);
    let (negate, rest) = match trimmed.strip_prefix(b"!") {
        Some(r) => (true, r),
        None => (false, trimmed),
    };
    let (dir_only, rest) = match rest.strip_suffix(b"/") {
        Some(r) => (true, r),
        None => (false, rest),
    };
    if rest.is_empty() {
        return None;
    }
    let (anchored, body) = match rest.strip_prefix(b"/") {
        Some(r) => (true, r),
        None => (rest.contains(&b'/'), rest),
    };
    let head = if anchored { "^" } else { "^(?:.*/)?" };
    Some((
        format!("{head}{}$", glob_to_regex(body, fold)?),
        negate,
        dir_only,
    ))
}

fn load_ignore(
    path: &Path,
    base: usize,
    parent: Option<Arc<Ignore>>,
    fold: bool,
    errors: &AtomicBool,
    quiet: bool,
) -> Option<Arc<Ignore>> {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            if e.kind() != io::ErrorKind::NotFound {
                errors.store(true, Ordering::Relaxed);
                if !quiet {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
            }
            return parent;
        }
    };
    let text = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&data);
    let rules: Vec<(String, bool, bool)> = text
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter_map(|line| parse_rule(line, fold))
        .collect();
    if rules.is_empty() {
        return parent;
    }
    match RegexSetBuilder::new(rules.iter().map(|r| &r.0))
        .size_limit(IGNORE_SIZE_LIMIT)
        .unicode(false)
        .dot_matches_new_line(true)
        .build()
    {
        Ok(set) => Some(Arc::new(Ignore {
            parent,
            base,
            negate: rules.iter().map(|r| r.1).collect(),
            dir_only: rules.iter().map(|r| r.2).collect(),
            set,
        })),
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !quiet {
                eprintln!("hgrep: {}: {e}", path.display());
            }
            parent
        }
    }
}

#[derive(Clone, Default)]
struct GitConfig {
    excludes_file: Option<Vec<u8>>,
    ignorecase: bool,
    precompose: bool,
}

#[derive(Clone, Copy, Default)]
struct RepoOpts {
    fold: bool,
    precompose: bool,
}

struct Scope<'a> {
    dir: &'a Path,
    gitdir: Option<&'a Path>,
    depth: u32,
}

const CONFIG_INCLUDE_DEPTH: u32 = 10;

fn skip_line(data: &[u8], from: usize) -> usize {
    memchr(b'\n', &data[from..]).map_or(data.len(), |i| from + i + 1)
}

fn trim_end(text: &[u8]) -> &[u8] {
    let keep = text.iter().rposition(|b| !b.is_ascii_whitespace());
    keep.map_or(&[][..], |i| &text[..=i])
}

fn read_section(data: &[u8], start: usize) -> Option<(Vec<u8>, Vec<u8>, usize)> {
    let mut name = Vec::new();
    let mut i = start + 1;
    loop {
        match *data.get(i)? {
            b']' => return Some((name, Vec::new(), i + 1)),
            b' ' | b'\t' | b'\r' => break,
            c if c.is_ascii_alphanumeric() || c == b'-' || c == b'.' => {
                name.push(c.to_ascii_lowercase());
                i += 1;
            }
            _ => return None,
        }
    }
    while matches!(data.get(i), Some(b' ' | b'\t' | b'\r')) {
        i += 1;
    }
    if data.get(i) != Some(&b'"') {
        return None;
    }
    i += 1;
    let mut sub = Vec::new();
    loop {
        match *data.get(i)? {
            b'"' => {
                i += 1;
                break;
            }
            b'\n' => return None,
            b'\\' => {
                let esc = *data.get(i + 1)?;
                if esc == b'\n' {
                    return None;
                }
                sub.push(esc);
                i += 2;
            }
            c => {
                sub.push(c);
                i += 1;
            }
        }
    }
    if data.get(i) != Some(&b']') {
        return None;
    }
    Some((name, sub, i + 1))
}

fn read_value(data: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut value = Vec::new();
    let mut keep = 0usize;
    let mut quoted = false;
    let mut comment = false;
    let mut i = start;
    while let Some(&c) = data.get(i) {
        let crlf = c == b'\r' && data.get(i + 1) == Some(&b'\n');
        if c == b'\n' || crlf {
            if quoted {
                return None;
            }
            value.truncate(keep);
            return Some((value, i + if crlf { 2 } else { 1 }));
        }
        if comment {
            i += 1;
            continue;
        }
        match c {
            b'#' | b';' if !quoted => {
                comment = true;
                i += 1;
            }
            b' ' | b'\t' | b'\r' if !quoted => {
                if !value.is_empty() {
                    value.push(c);
                }
                i += 1;
            }
            b'"' => {
                quoted = !quoted;
                i += 1;
            }
            b'\\' => {
                let esc = *data.get(i + 1)?;
                let crlf = esc == b'\r' && data.get(i + 2) == Some(&b'\n');
                if esc == b'\n' || crlf {
                    i += if crlf { 3 } else { 2 };
                    continue;
                }
                value.push(match esc {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'\\' | b'"' => esc,
                    _ => return None,
                });
                keep = value.len();
                i += 2;
            }
            _ => {
                value.push(c);
                keep = value.len();
                i += 1;
            }
        }
    }
    if quoted {
        return None;
    }
    value.truncate(keep);
    Some((value, i))
}

fn read_entry(data: &[u8], start: usize) -> Option<(Vec<u8>, Option<Vec<u8>>, usize)> {
    let mut key = Vec::new();
    let mut i = start;
    while let Some(&c) = data.get(i) {
        if c.is_ascii_alphanumeric() || c == b'-' {
            key.push(c.to_ascii_lowercase());
            i += 1;
        } else {
            break;
        }
    }
    while matches!(data.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    match data.get(i) {
        Some(b'=') => {
            let (value, next) = read_value(data, i + 1)?;
            Some((key, Some(value), next))
        }
        None => Some((key, None, i)),
        Some(b'\n') => Some((key, None, i + 1)),
        Some(b'\r') if data.get(i + 1) == Some(&b'\n') => Some((key, None, i + 2)),
        _ => None,
    }
}

fn config_int(text: &[u8]) -> Option<i64> {
    let (negative, rest) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (radix, digits) = match rest
        .strip_prefix(b"0x")
        .or_else(|| rest.strip_prefix(b"0X"))
    {
        Some(hex) => (16u32, hex),
        None if rest.len() > 1 && rest[0] == b'0' => (8, &rest[1..]),
        None => (10, rest),
    };
    let (body, scale) = match digits.last() {
        Some(b'k' | b'K') => (&digits[..digits.len() - 1], 1024i64),
        Some(b'm' | b'M') => (&digits[..digits.len() - 1], 1024 * 1024),
        Some(b'g' | b'G') => (&digits[..digits.len() - 1], 1024 * 1024 * 1024),
        _ => (digits, 1),
    };
    if body.is_empty() {
        return None;
    }
    let magnitude = body.iter().try_fold(0i64, |acc, &b| {
        let digit = (b as char).to_digit(radix)?;
        acc.checked_mul(radix as i64)?.checked_add(digit as i64)
    })?;
    let scaled = magnitude.checked_mul(scale)?;
    Some(if negative { -scaled } else { scaled })
}

fn config_bool(value: &[u8]) -> bool {
    match value.to_ascii_lowercase().as_slice() {
        b"" => false,
        b"true" | b"yes" | b"on" => true,
        b"false" | b"no" | b"off" => false,
        _ => config_int(value).is_some_and(|v| v != 0),
    }
}

fn home_of(user: &[u8]) -> Option<PathBuf> {
    let name = std::ffi::CString::new(user).ok()?;
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut scratch = vec![0 as libc::c_char; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            &mut record,
            scratch.as_mut_ptr(),
            scratch.len(),
            &mut found,
        )
    };
    if rc != 0 || found.is_null() {
        return None;
    }
    let dir = unsafe { (*found).pw_dir };
    if dir.is_null() {
        return None;
    }
    let bytes = unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes().to_vec();
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

fn expand_tilde(value: &[u8]) -> Option<PathBuf> {
    let Some(rest) = value.strip_prefix(b"~") else {
        return Some(PathBuf::from(OsStr::from_bytes(value)));
    };
    let cut = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
    let home = if cut == 0 {
        PathBuf::from(std::env::var_os("HOME")?)
    } else {
        home_of(&rest[..cut])?
    };
    Some(match rest.get(cut + 1..) {
        Some(tail) => home.join(OsStr::from_bytes(tail)),
        None => home,
    })
}

fn path_glob(pattern: &[u8], fold: bool) -> Option<Regex> {
    let body = glob_to_regex(pattern, fold)?;
    RegexBuilder::new(&format!("^{body}$"))
        .unicode(false)
        .dot_matches_new_line(true)
        .build()
        .ok()
}

fn by_gitdir(cond: &[u8], scope: &Scope, fold: bool) -> bool {
    let Some(gitdir) = scope.gitdir else {
        return false;
    };
    let expanded =
        expand_tilde(cond).map_or_else(|| cond.to_vec(), |p| p.into_os_string().into_vec());
    let mut pattern = if expanded.starts_with(b"./") || expanded.starts_with(b"../") {
        let base = scope
            .dir
            .canonicalize()
            .unwrap_or_else(|_| scope.dir.to_path_buf());
        let mut joined = base.into_os_string().into_vec();
        joined.push(b'/');
        joined.extend_from_slice(&expanded);
        joined
    } else if expanded.starts_with(b"/") {
        expanded
    } else {
        let mut prefixed = b"**/".to_vec();
        prefixed.extend_from_slice(&expanded);
        prefixed
    };
    if pattern.last() == Some(&b'/') {
        pattern.extend_from_slice(b"**");
    }
    let Some(glob) = path_glob(&pattern, fold) else {
        return false;
    };
    let real = gitdir.canonicalize().ok();
    [real.as_deref(), Some(gitdir)]
        .into_iter()
        .flatten()
        .any(|candidate| {
            let mut text = candidate.as_os_str().as_bytes().to_vec();
            if fold {
                text.make_ascii_lowercase();
            }
            glob.is_match(&text)
        })
}

fn by_branch(cond: &[u8], scope: &Scope) -> bool {
    let Some(gitdir) = scope.gitdir else {
        return false;
    };
    let Ok(head) = fs::read(gitdir.join("HEAD")) else {
        return false;
    };
    let line = trim_end(head.split(|&b| b == b'\n').next().unwrap_or(&[]));
    let Some(branch) = line.strip_prefix(b"ref: refs/heads/") else {
        return false;
    };
    let mut pattern = cond.to_vec();
    if pattern.last() == Some(&b'/') {
        pattern.extend_from_slice(b"**");
    }
    path_glob(&pattern, false).is_some_and(|glob| glob.is_match(branch))
}

fn condition_holds(cond: &[u8], scope: &Scope) -> bool {
    if let Some(rest) = cond.strip_prefix(b"gitdir:") {
        return by_gitdir(rest, scope, false);
    }
    if let Some(rest) = cond.strip_prefix(b"gitdir/i:") {
        return by_gitdir(rest, scope, true);
    }
    match cond.strip_prefix(b"onbranch:") {
        Some(rest) => by_branch(rest, scope),
        None => false,
    }
}

fn apply_entry(
    section: &[u8],
    sub: &[u8],
    key: &[u8],
    value: Option<Vec<u8>>,
    scope: &Scope,
    cfg: &mut GitConfig,
) -> bool {
    if section == b"core" && sub.is_empty() {
        if key == b"excludesfile" {
            let Some(v) = value else {
                return false;
            };
            cfg.excludes_file = Some(v);
        } else if key == b"ignorecase" {
            cfg.ignorecase = value.is_none_or(|v| config_bool(&v));
        } else if key == b"precomposeunicode" {
            cfg.precompose = value.is_none_or(|v| config_bool(&v));
        }
        return true;
    }
    let conditional = section == b"includeif";
    if (section == b"include" || conditional) && key == b"path" {
        if conditional && !condition_holds(sub, scope) {
            return true;
        }
        let Some(v) = value else {
            return false;
        };
        let Some(raw) = expand_tilde(&v) else {
            return false;
        };
        if scope.depth >= CONFIG_INCLUDE_DEPTH {
            return false;
        }
        let path = if raw.is_absolute() {
            raw
        } else {
            scope.dir.join(raw)
        };
        read_config_file(&path, scope.gitdir, scope.depth + 1, cfg);
    }
    true
}

fn read_config_bytes(data: &[u8], scope: &Scope, cfg: &mut GitConfig) -> bool {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let mut section = Vec::new();
    let mut sub = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        match data[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'#' | b';' => i = skip_line(data, i),
            b'[' => match read_section(data, i) {
                Some((name, quoted, next)) => {
                    section = name;
                    sub = quoted;
                    i = next;
                }
                None => return false,
            },
            c if c.is_ascii_alphabetic() => {
                let Some((key, value, next)) = read_entry(data, i) else {
                    return false;
                };
                i = next;
                if !apply_entry(&section, &sub, &key, value, scope, cfg) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn apply_config(data: &[u8], scope: &Scope, cfg: &mut GitConfig) {
    let mut scratch = cfg.clone();
    if read_config_bytes(data, scope, &mut scratch) {
        *cfg = scratch;
    }
}

fn read_config_file(path: &Path, gitdir: Option<&Path>, depth: u32, cfg: &mut GitConfig) {
    let Ok(data) = fs::read(path) else {
        return;
    };
    let scope = Scope {
        dir: path.parent().unwrap_or(Path::new(".")),
        gitdir,
        depth,
    };
    apply_config(&data, &scope, cfg);
}

fn xdg_config_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => Some(PathBuf::from(x)),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")),
    }
}

struct RawConfig {
    dir: PathBuf,
    data: Vec<u8>,
}

fn push_source(out: &mut Vec<RawConfig>, path: PathBuf) {
    if let Ok(data) = fs::read(&path) {
        out.push(RawConfig {
            dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            data,
        });
    }
}

fn base_sources() -> &'static [RawConfig] {
    static CACHE: OnceLock<Vec<RawConfig>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out = Vec::new();
        let nosystem = std::env::var_os("GIT_CONFIG_NOSYSTEM")
            .is_some_and(|v| config_bool(v.as_os_str().as_bytes()));
        if !nosystem {
            let system = std::env::var_os("GIT_CONFIG_SYSTEM")
                .map_or_else(|| PathBuf::from("/etc/gitconfig"), PathBuf::from);
            push_source(&mut out, system);
        }
        match std::env::var_os("GIT_CONFIG_GLOBAL") {
            Some(g) => push_source(&mut out, PathBuf::from(g)),
            None => {
                if let Some(dir) = xdg_config_home() {
                    push_source(&mut out, dir.join("git").join("config"));
                }
                if let Some(home) = std::env::var_os("HOME") {
                    push_source(&mut out, PathBuf::from(home).join(".gitconfig"));
                }
            }
        }
        out
    })
}

fn repo_config(gitdir: Option<&Path>) -> GitConfig {
    let mut cfg = GitConfig::default();
    for src in base_sources() {
        let scope = Scope {
            dir: &src.dir,
            gitdir,
            depth: 0,
        };
        apply_config(&src.data, &scope, &mut cfg);
    }
    if let Some(dir) = gitdir {
        read_config_file(&dir.join("config"), gitdir, 0, &mut cfg);
    }
    cfg
}

fn excludes_path(cfg: &GitConfig, repo: &Path) -> Option<PathBuf> {
    match &cfg.excludes_file {
        Some(v) if v.is_empty() => None,
        Some(v) => {
            let raw = expand_tilde(v)?;
            Some(if raw.is_absolute() {
                raw
            } else {
                repo.join(raw)
            })
        }
        None => xdg_config_home().map(|d| d.join("git").join("ignore")),
    }
}

fn resolve_gitdir(repo: &Path) -> Option<PathBuf> {
    let dot = repo.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let data = fs::read(&dot).ok()?;
    let line = data.split(|&b| b == b'\n').next()?;
    let raw = trim_end(line.strip_prefix(b"gitdir:")?);
    let target = trim_end(raw.strip_prefix(b" ").unwrap_or(raw));
    if target.is_empty() {
        return None;
    }
    let named = PathBuf::from(OsStr::from_bytes(target));
    let gitdir = if named.is_absolute() {
        named
    } else {
        repo.join(named)
    };
    let Ok(shared) = fs::read(gitdir.join("commondir")) else {
        return Some(gitdir);
    };
    let common = trim_end(shared.split(|&b| b == b'\n').next().unwrap_or(&[]));
    if common.is_empty() {
        return Some(gitdir);
    }
    let named = PathBuf::from(OsStr::from_bytes(common));
    Some(if named.is_absolute() {
        named
    } else {
        gitdir.join(named)
    })
}

fn repo_sources(repo: &Path, errors: &AtomicBool, quiet: bool) -> (Option<Arc<Ignore>>, RepoOpts) {
    let gitdir = resolve_gitdir(repo);
    let cfg = repo_config(gitdir.as_deref());
    let opts = RepoOpts {
        fold: cfg.ignorecase,
        precompose: cfg.precompose,
    };
    let global = excludes_path(&cfg, repo)
        .and_then(|path| load_ignore(&path, 0, None, opts.fold, errors, quiet));
    let stacked = match &gitdir {
        Some(dir) => load_ignore(
            &dir.join("info").join("exclude"),
            0,
            global,
            opts.fold,
            errors,
            quiet,
        ),
        None => global,
    };
    (stacked, opts)
}

#[cfg(target_os = "macos")]
mod nfc {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::os::raw::{c_char, c_int};

    #[link(name = "iconv")]
    unsafe extern "C" {
        fn iconv_open(to: *const c_char, from: *const c_char) -> *mut c_void;
        fn iconv(
            cd: *mut c_void,
            input: *mut *const c_char,
            inleft: *mut usize,
            output: *mut *mut c_char,
            outleft: *mut usize,
        ) -> usize;
        fn iconv_close(cd: *mut c_void) -> c_int;
    }

    struct Conv(Cell<*mut c_void>);

    impl Drop for Conv {
        fn drop(&mut self) {
            let cd = self.0.get();
            if !cd.is_null() && cd as usize != usize::MAX {
                unsafe { iconv_close(cd) };
            }
        }
    }

    thread_local! {
        static CONV: Conv = const { Conv(Cell::new(std::ptr::null_mut())) };
    }

    pub fn precomposed(raw: &[u8]) -> Option<Vec<u8>> {
        if !raw.iter().any(|&b| b >= 0x80) {
            return None;
        }
        CONV.with(|conv| {
            let mut cd = conv.0.get();
            if cd.is_null() {
                cd = unsafe { iconv_open(c"UTF-8".as_ptr(), c"UTF-8-MAC".as_ptr()) };
                conv.0.set(cd);
            }
            if cd as usize == usize::MAX {
                return None;
            }
            let mut out = vec![0u8; raw.len() * 2 + 4];
            let mut input = raw.as_ptr() as *const c_char;
            let mut inleft = raw.len();
            let mut output = out.as_mut_ptr() as *mut c_char;
            let mut outleft = out.len();
            let rc = unsafe { iconv(cd, &mut input, &mut inleft, &mut output, &mut outleft) };
            if rc == usize::MAX || inleft != 0 {
                unsafe {
                    iconv(
                        cd,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                return None;
            }
            let used = out.len() - outleft;
            out.truncate(used);
            Some(out)
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod nfc {
    pub fn precomposed(_raw: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

struct Dir {
    path: PathBuf,
    rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
}

fn child_rel(parent: &[u8], name: &OsStr, opts: RepoOpts) -> Vec<u8> {
    let raw = name.as_bytes();
    let mut rel = Vec::with_capacity(parent.len() + raw.len() + 1);
    rel.extend_from_slice(parent);
    if !rel.is_empty() {
        rel.push(b'/');
    }
    let head = rel.len();
    match opts.precompose.then(|| nfc::precomposed(raw)).flatten() {
        Some(text) => rel.extend_from_slice(&text),
        None => rel.extend_from_slice(raw),
    }
    if opts.fold {
        rel[head..].make_ascii_lowercase();
    }
    rel
}

fn is_dot_git(name: &[u8], fold: bool) -> bool {
    if fold {
        name.eq_ignore_ascii_case(b".git")
    } else {
        name == b".git"
    }
}

fn root_ignore(path: &Path, errors: &AtomicBool, quiet: bool) -> Option<Dir> {
    let loose = |path: &Path| Dir {
        path: path.to_path_buf(),
        rel: Vec::new(),
        ignore: None,
        opts: RepoOpts::default(),
        in_repo: false,
    };
    let Ok(abs) = path.canonicalize() else {
        return Some(loose(path));
    };
    if abs.join(".git").exists() {
        let (seed, opts) = repo_sources(&abs, errors, quiet);
        return Some(Dir {
            path: path.to_path_buf(),
            rel: Vec::new(),
            ignore: seed,
            opts,
            in_repo: true,
        });
    }
    let mut chain = Vec::new();
    let mut repo = None;
    for dir in abs.ancestors().skip(1) {
        chain.push(dir);
        if dir.join(".git").exists() {
            repo = Some(dir);
            break;
        }
    }
    let (Some(root), Some(name)) = (repo, abs.file_name()) else {
        return Some(loose(path));
    };
    let (seed, opts) = repo_sources(root, errors, quiet);
    let mut rel = Vec::new();
    let mut ignore = seed;
    for (idx, dir) in chain.iter().rev().enumerate() {
        if idx > 0 {
            rel = child_rel(&rel, dir.file_name().unwrap_or(OsStr::new("")), opts);
        }
        let base = if rel.is_empty() { 0 } else { rel.len() + 1 };
        ignore = load_ignore(
            &dir.join(".gitignore"),
            base,
            ignore,
            opts.fold,
            errors,
            quiet,
        );
    }
    let rel = child_rel(&rel, name, opts);
    if ignore.as_ref().is_some_and(|ig| ig.ignored(&rel, true)) {
        return None;
    }
    Some(Dir {
        path: path.to_path_buf(),
        rel,
        ignore,
        opts,
        in_repo: true,
    })
}

fn scan_plain(dir: &Dir, errors: &AtomicBool, quiet: bool) -> (Vec<Dir>, Vec<PathBuf>) {
    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    match fs::read_dir(&dir.path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let Ok(t) = entry.file_type() else { continue };
                if t.is_dir() {
                    sub.push(Dir {
                        path: entry.path(),
                        rel: Vec::new(),
                        ignore: None,
                        opts: RepoOpts::default(),
                        in_repo: false,
                    });
                } else if t.is_file() {
                    leaves.push(entry.path());
                }
            }
        }
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !quiet {
                eprintln!("hgrep: {}: {e}", dir.path.display());
            }
        }
    }
    (sub, leaves)
}

fn scan_ignored(dir: &Dir, errors: &AtomicBool, quiet: bool) -> (Vec<Dir>, Vec<PathBuf>) {
    let entries: Vec<fs::DirEntry> = match fs::read_dir(&dir.path) {
        Ok(entries) => entries.flatten().collect(),
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !quiet {
                eprintln!("hgrep: {}: {e}", dir.path.display());
            }
            Vec::new()
        }
    };
    let boundary = entries
        .iter()
        .any(|entry| entry.file_name().as_bytes() == b".git")
        .then(|| repo_sources(&dir.path, errors, quiet));
    let (parent_rel, inherited, opts, in_repo) = match &boundary {
        Some((seed, opts)) => (&[][..], seed.clone(), *opts, true),
        None => (&dir.rel[..], dir.ignore.clone(), dir.opts, dir.in_repo),
    };
    let ignore = if in_repo {
        let base = if parent_rel.is_empty() {
            0
        } else {
            parent_rel.len() + 1
        };
        load_ignore(
            &dir.path.join(".gitignore"),
            base,
            inherited,
            opts.fold,
            errors,
            quiet,
        )
    } else {
        None
    };

    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    for entry in &entries {
        let Ok(t) = entry.file_type() else { continue };
        let is_dir = t.is_dir();
        if !is_dir && !t.is_file() {
            continue;
        }
        let name = entry.file_name();
        if is_dot_git(name.as_bytes(), opts.fold) {
            continue;
        }
        let rel = child_rel(parent_rel, &name, opts);
        if let Some(ig) = &ignore
            && ig.ignored(&rel, is_dir)
        {
            continue;
        }
        if is_dir {
            sub.push(Dir {
                path: entry.path(),
                rel,
                ignore: ignore.clone(),
                opts,
                in_repo,
            });
        } else {
            leaves.push(entry.path());
        }
    }
    (sub, leaves)
}

fn collect_paths(
    roots: &[OsString],
    recursive: bool,
    gitignore: bool,
    errors: &AtomicBool,
    quiet_errors: bool,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut level = Vec::new();

    for root in roots {
        let path = PathBuf::from(root);
        match fs::metadata(&path) {
            Ok(md) if md.is_dir() => {
                if !recursive {
                    errors.store(true, Ordering::Relaxed);
                    if !quiet_errors {
                        eprintln!("hgrep: {}: Is a directory", path.display());
                    }
                } else if gitignore {
                    level.extend(root_ignore(&path, errors, quiet_errors));
                } else {
                    level.push(Dir {
                        path,
                        rel: Vec::new(),
                        ignore: None,
                        opts: RepoOpts::default(),
                        in_repo: false,
                    });
                }
            }
            Ok(_) => files.push(path),
            Err(e) => {
                errors.store(true, Ordering::Relaxed);
                if !quiet_errors {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
            }
        }
    }

    while !level.is_empty() {
        let (dirs, found): (Vec<Vec<Dir>>, Vec<Vec<PathBuf>>) = level
            .par_iter()
            .map(|dir| {
                if gitignore {
                    scan_ignored(dir, errors, quiet_errors)
                } else {
                    scan_plain(dir, errors, quiet_errors)
                }
            })
            .unzip();
        level = dirs.into_iter().flatten().collect();
        files.extend(found.into_iter().flatten());
    }
    files
}

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    let patterns: Vec<Vec<u8>> = if !cli.patterns.is_empty() || cli.pattern_file.is_some() {
        let mut ps: Vec<Vec<u8>> = cli
            .patterns
            .iter()
            .map(|p| p.as_os_str().as_bytes().to_vec())
            .collect();
        if let Some(f) = &cli.pattern_file {
            match fs::read(f) {
                Ok(data) if !data.is_empty() => {
                    let body = data.strip_suffix(b"\n").unwrap_or(&data);
                    ps.extend(
                        body.split(|&b| b == b'\n')
                            .map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec()),
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("hgrep: {}: {e}", f.display());
                    return ExitCode::from(2);
                }
            }
        }
        ps
    } else if cli.operands.is_empty() {
        eprintln!("hgrep: no pattern given");
        return ExitCode::from(2);
    } else {
        vec![cli.operands.remove(0).into_vec()]
    };

    let matcher = match build_matcher(&cli, &patterns) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("hgrep: {e}");
            return ExitCode::from(2);
        }
    };

    let errors = AtomicBool::new(false);
    let found = AtomicBool::new(false);
    let files = collect_paths(
        &cli.operands,
        cli.recursive,
        cli.gitignore,
        &errors,
        cli.no_messages,
    );
    let show_name = if cli.no_filename {
        false
    } else {
        cli.with_filename || files.len() > 1 || cli.recursive
    };
    let job = Job {
        matcher: &matcher,
        cli: &cli,
        show_name,
        emit_lines: !(cli.count || cli.files_with_matches || cli.files_without_match || cli.quiet),
    };

    if cli.operands.is_empty() {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
            eprintln!("hgrep: (standard input): {e}");
            return ExitCode::from(2);
        }
        let mut body = Vec::new();
        let count = search_buf(&buf, &job, b"(standard input)", &mut body);
        let mut out = Vec::new();
        report(&job, b"(standard input)", count, body, &mut out);
        let _ = io::stdout().lock().write_all(&out);
        return exit_code(selected(&cli, count), false);
    }

    let sink = Mutex::new(io::BufWriter::with_capacity(256 * 1024, io::stdout()));
    files.par_iter().for_each(|path| {
        if cli.quiet && found.load(Ordering::Relaxed) {
            return;
        }
        let src = match open_source(path) {
            Ok(s) => s,
            Err(e) => {
                errors.store(true, Ordering::Relaxed);
                if !cli.no_messages {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
                return;
            }
        };
        let name = path.as_os_str().as_bytes();
        let mut body = Vec::new();
        let count = search_buf(src.bytes(), &job, name, &mut body);
        if selected(&cli, count) {
            found.store(true, Ordering::Relaxed);
        }
        let mut out = Vec::new();
        report(&job, name, count, body, &mut out);
        if !out.is_empty()
            && let Ok(mut w) = sink.lock()
        {
            let _ = w.write_all(&out);
        }
    });

    if let Ok(mut w) = sink.lock() {
        let _ = w.flush();
    }
    exit_code(
        found.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    )
}

fn exit_code(found: bool, errored: bool) -> ExitCode {
    if errored {
        ExitCode::from(2)
    } else if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
