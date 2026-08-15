use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use regex::bytes::{RegexSet, RegexSetBuilder};

pub(crate) struct Ignore {
    parent: Option<Arc<Ignore>>,
    base: usize,
    set: RegexSet,
    negate: Vec<bool>,
    dir_only: Vec<bool>,
}

impl Ignore {
    pub(crate) fn ignored(&self, rel: &[u8], is_dir: bool) -> bool {
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

const IGNORE_SIZE_LIMIT: usize = if cfg!(test) { 256 } else { 1 << 28 };

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

pub(crate) fn glob_to_regex(pat: &[u8], fold: bool) -> Option<String> {
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

pub(crate) fn load_ignore(
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
                    eprintln!("hfind: {}: {e}", path.display());
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
                eprintln!("hfind: {}: {e}", path.display());
            }
            parent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn quiet_and_empty_rules() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hfind_ignore_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let blocked = dir.join("blocked");
        fs::create_dir(&blocked).unwrap();
        let errors = AtomicBool::new(false);
        let _ = load_ignore(&blocked, 0, None, false, &errors, true);
        let empty = dir.join("empty");
        fs::write(&empty, b"# only\n\n").unwrap();
        let parent = Some(Arc::new(Ignore {
            parent: None,
            base: 0,
            set: RegexSet::empty(),
            negate: Vec::new(),
            dir_only: Vec::new(),
        }));
        let got = load_ignore(&empty, 0, parent.clone(), false, &errors, false);
        assert!(got.is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn huge_rule_reports_regex_error() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_ignore_huge_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ignore");
        fs::write(&path, "a".repeat(4000).as_bytes()).unwrap();
        let errors = AtomicBool::new(false);
        let _ = load_ignore(&path, 0, None, false, &errors, false);
        assert!(errors.load(Ordering::Relaxed));
        let _ = load_ignore(&path, 0, None, false, &errors, true);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn glob_parse_and_match_rules() {
        assert!(glob_to_regex(b"a/**/b", false).unwrap().contains(".*"));
        assert_eq!(glob_to_regex(b"**", false).as_deref(), Some(".*"));
        assert!(glob_to_regex(b"a**b", false).unwrap().contains("[^/]*"));
        assert!(glob_to_regex(b"?", false).unwrap().contains("[^/]"));
        assert!(glob_to_regex(b"[a-z]", false).is_some());
        assert!(glob_to_regex(b"[!a]", false).is_some());
        assert!(glob_to_regex(b"[^a]", false).is_some());
        assert!(glob_to_regex(b"[A-C]", true).is_some());
        assert!(glob_to_regex(b"[z-a]", false).is_some());
        assert!(glob_to_regex(b"[[:digit:]]", false).is_some());
        assert!(glob_to_regex(b"[[:upper:]]", true).is_some());
        assert!(glob_to_regex(b"[[:lower:]]", true).is_some());
        assert!(glob_to_regex(b"[[:nope:]]", false).is_none());
        assert!(glob_to_regex(b"[", false).is_none());
        assert!(glob_to_regex(b"\\", false).is_none());
        assert!(glob_to_regex(b"[a-\\", false).is_none());
        assert!(glob_to_regex(b"a\\b", false).is_some());
        assert!(glob_to_regex(b"[a\\-c]", false).is_some());
        assert!(glob_to_regex(b"[d-\\f]", false).is_some());
        assert!(glob_to_regex(b"[[:", false).is_none());
        assert!(glob_to_regex(b"[:]:x", false).is_some());
        assert!(glob_to_regex(b"[a[b]", false).is_some());
        assert!(parse_rule(b"#c", false).is_none());
        assert!(parse_rule(b"", false).is_none());
        assert!(parse_rule(b"/", false).is_none());
        assert!(parse_rule(b"!keep.log", false).unwrap().1);
        assert!(parse_rule(b"logs/", false).unwrap().2);
        assert!(parse_rule(b"/rooted", false).unwrap().0.starts_with('^'));
        assert!(parse_rule(b"a/b", false).unwrap().0.starts_with('^'));
        assert!(parse_rule(b"*.log  ", false).is_some());
        assert!(parse_rule(b"keep\\  ", false).is_some());
        let parent = Arc::new(Ignore {
            parent: None,
            base: 0,
            set: RegexSetBuilder::new(["^tmp$"])
                .unicode(false)
                .dot_matches_new_line(true)
                .build()
                .unwrap(),
            negate: vec![false],
            dir_only: vec![false],
        });
        let ig = Ignore {
            parent: Some(parent),
            base: 0,
            set: RegexSetBuilder::new(["^a\\.log$", "^keep\\.log$", "^logs$"])
                .unicode(false)
                .dot_matches_new_line(true)
                .build()
                .unwrap(),
            negate: vec![false, true, false],
            dir_only: vec![false, false, true],
        };
        assert!(ig.ignored(b"a.log", false));
        assert!(!ig.ignored(b"keep.log", false));
        assert!(ig.ignored(b"logs", true));
        assert!(!ig.ignored(b"logs", false));
        assert!(ig.ignored(b"tmp", false));
        assert!(!ig.ignored(b"keep.txt", false));
    }
}
