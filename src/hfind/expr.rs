use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use regex::bytes::{Regex, RegexBuilder};

use super::walk::{Follow, Item, Kind};

#[derive(Clone, Copy)]
pub(crate) enum Cmp {
    Eq,
    Lt,
    Gt,
}

pub(crate) enum Expr {
    True,
    False,
    Glob {
        pat: Vec<u8>,
        fold: bool,
        whole: bool,
    },
    Regex(Regex),
    Type(Kind),
    Size {
        cmp: Cmp,
        n: u64,
        unit: u64,
    },
    Empty,
    Age {
        cmp: Cmp,
        n: i64,
        unit: u64,
    },
    Newer(SystemTime),
    Print {
        nul: bool,
    },
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

struct Parser<'a> {
    tokens: &'a [OsString],
    i: usize,
    follow: Follow,
    has_action: bool,
    mindepth: usize,
    maxdepth: Option<usize>,
}

pub(crate) fn parse(
    tokens: &[OsString],
    follow: Follow,
) -> Result<(Expr, usize, Option<usize>), String> {
    if tokens.is_empty() {
        return Ok((Expr::Print { nul: false }, 0, None));
    }
    let mut p = Parser {
        tokens,
        i: 0,
        follow,
        has_action: false,
        mindepth: 0,
        maxdepth: None,
    };
    let inner = p.parse_or()?;
    if p.i != tokens.len() {
        return Err(format!("unexpected `{}'", tokens[p.i].to_string_lossy()));
    }
    let expr = if p.has_action {
        inner
    } else {
        Expr::And(vec![inner, Expr::Print { nul: false }])
    };
    Ok((expr, p.mindepth, p.maxdepth))
}

impl Parser<'_> {
    fn peek(&self) -> Option<&[u8]> {
        self.tokens.get(self.i).map(|t| t.as_bytes())
    }

    fn take_arg(&mut self, flag: &str) -> Result<OsString, String> {
        let v = self
            .tokens
            .get(self.i)
            .cloned()
            .ok_or_else(|| format!("missing argument to `{flag}'"))?;
        self.i += 1;
        Ok(v)
    }

    fn at_or(&self) -> bool {
        matches!(self.peek(), Some(b"-o" | b"-or"))
    }

    fn at_not(&self) -> bool {
        matches!(self.peek(), Some(b"!" | b"-not"))
    }

    fn has_term(&self) -> bool {
        match self.peek() {
            None | Some(b")") | Some(b"-o") | Some(b"-or") => false,
            Some(_) => true,
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut items = vec![self.parse_and()?];
        while self.at_or() {
            self.i += 1;
            items.push(self.parse_and()?);
        }
        Ok(if items.len() == 1 {
            items.swap_remove(0)
        } else {
            Expr::Or(items)
        })
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut items = vec![self.parse_not()?];
        while self.has_term() {
            if matches!(self.peek(), Some(b"-a" | b"-and")) {
                self.i += 1;
            }
            items.push(self.parse_not()?);
        }
        Ok(if items.len() == 1 {
            items.swap_remove(0)
        } else {
            Expr::And(items)
        })
    }

    fn parse_not(&mut self) -> Result<Expr, String> {
        let mut n = 0usize;
        while self.at_not() {
            self.i += 1;
            n += 1;
        }
        let inner = self.parse_primary()?;
        if n % 2 == 1 {
            Ok(Expr::Not(Box::new(inner)))
        } else {
            Ok(inner)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(b"(") {
            self.i += 1;
            if self.peek() == Some(b")") {
                return Err("empty parentheses".into());
            }
            let inner = self.parse_or()?;
            if self.peek() != Some(b")") {
                return Err("expected `)'".into());
            }
            self.i += 1;
            return Ok(inner);
        }
        self.parse_pred()
    }

    fn parse_pred(&mut self) -> Result<Expr, String> {
        let tok = self
            .tokens
            .get(self.i)
            .cloned()
            .ok_or_else(|| "expected an expression".to_string())?;
        self.i += 1;
        match tok.as_bytes() {
            b"," => Err("the comma operator is not supported".into()),
            b"-true" => Ok(Expr::True),
            b"-false" => Ok(Expr::False),
            b"-print" | b"-print0" => {
                self.has_action = true;
                Ok(Expr::Print {
                    nul: tok.as_bytes() == b"-print0",
                })
            }
            b"-empty" => Ok(Expr::Empty),
            b"-name" => self.glob_arg("-name", false, false),
            b"-iname" => self.glob_arg("-iname", true, false),
            b"-path" => self.glob_arg("-path", false, true),
            b"-ipath" => self.glob_arg("-ipath", true, true),
            b"-regex" => self.regex_arg("-regex", false),
            b"-iregex" => self.regex_arg("-iregex", true),
            b"-type" => self.type_arg(),
            b"-size" => self.size_arg(),
            b"-mtime" => self.age_arg("-mtime", 86400),
            b"-mmin" => self.age_arg("-mmin", 60),
            b"-newer" => self.newer_arg(),
            b"-maxdepth" => {
                self.maxdepth = Some(self.nat_arg("-maxdepth")?);
                Ok(Expr::True)
            }
            b"-mindepth" => {
                self.mindepth = self.nat_arg("-mindepth")?;
                Ok(Expr::True)
            }
            b"-a" | b"-and" | b"-o" | b"-or" | b"-not" | b"!" | b")" | b"(" => {
                Err(format!("unexpected `{}'", tok.to_string_lossy()))
            }
            _ => Err(format!("unknown predicate `{}'", tok.to_string_lossy())),
        }
    }

    fn glob_arg(&mut self, flag: &str, fold: bool, whole: bool) -> Result<Expr, String> {
        let raw = self.take_arg(flag)?;
        Ok(Expr::Glob {
            pat: raw.as_bytes().to_vec(),
            fold,
            whole,
        })
    }

    fn regex_arg(&mut self, flag: &str, fold: bool) -> Result<Expr, String> {
        let raw = self.take_arg(flag)?;
        let text = raw
            .to_str()
            .ok_or_else(|| format!("invalid regular expression `{}'", raw.to_string_lossy()))?;
        let wrapped = format!("^(?:{text})$");
        let re = RegexBuilder::new(&wrapped)
            .unicode(false)
            .case_insensitive(fold)
            .dot_matches_new_line(true)
            .build()
            .map_err(|e| format!("{e}"))?;
        Ok(Expr::Regex(re))
    }

    fn type_arg(&mut self) -> Result<Expr, String> {
        let raw = self.take_arg("-type")?;
        let kind = match raw.as_bytes() {
            b"f" => Kind::File,
            b"d" => Kind::Dir,
            b"l" => Kind::Link,
            _ => {
                return Err(format!(
                    "unknown argument to -type: {}",
                    raw.to_string_lossy()
                ));
            }
        };
        Ok(Expr::Type(kind))
    }

    fn size_arg(&mut self) -> Result<Expr, String> {
        let raw = self.take_arg("-size")?;
        let (cmp, n, unit) = parse_size(raw.as_bytes())?;
        Ok(Expr::Size { cmp, n, unit })
    }

    fn age_arg(&mut self, flag: &str, unit: u64) -> Result<Expr, String> {
        let raw = self.take_arg(flag)?;
        let (cmp, n) = parse_signed(raw.as_bytes())
            .ok_or_else(|| format!("invalid argument `{}' to `{flag}'", raw.to_string_lossy()))?;
        Ok(Expr::Age { cmp, n, unit })
    }

    fn newer_arg(&mut self) -> Result<Expr, String> {
        let raw = self.take_arg("-newer")?;
        let path = Path::new(&raw);
        let follow = matches!(self.follow, Follow::Cli | Follow::Always);
        let meta = if follow {
            fs::metadata(path).or_else(|_| fs::symlink_metadata(path))
        } else {
            fs::symlink_metadata(path)
        };
        match meta.and_then(|m| m.modified()) {
            Ok(t) => Ok(Expr::Newer(t)),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    fn nat_arg(&mut self, flag: &str) -> Result<usize, String> {
        let raw = self.take_arg(flag)?;
        std::str::from_utf8(raw.as_bytes())
            .ok()
            .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("invalid argument `{}' to `{flag}'", raw.to_string_lossy()))
    }
}

fn split_cmp(raw: &[u8]) -> (Cmp, &[u8]) {
    match raw.first() {
        Some(b'+') => (Cmp::Gt, &raw[1..]),
        Some(b'-') => (Cmp::Lt, &raw[1..]),
        _ => (Cmp::Eq, raw),
    }
}

fn parse_size(raw: &[u8]) -> Result<(Cmp, u64, u64), String> {
    let err = || {
        format!(
            "invalid argument `{}' to `-size'",
            String::from_utf8_lossy(raw)
        )
    };
    let (cmp, rest) = split_cmp(raw);
    if rest.is_empty() {
        return Err(err());
    }
    let (digits, unit) = match rest.last() {
        Some(b'c') => (&rest[..rest.len() - 1], 1u64),
        Some(b'w') => (&rest[..rest.len() - 1], 2),
        Some(b'b') => (&rest[..rest.len() - 1], 512),
        Some(b'k') => (&rest[..rest.len() - 1], 1024),
        Some(b'M') => (&rest[..rest.len() - 1], 1024 * 1024),
        Some(b'G') => (&rest[..rest.len() - 1], 1024 * 1024 * 1024),
        Some(b) if b.is_ascii_digit() => (rest, 512),
        _ => return Err(err()),
    };
    if digits.is_empty() || !digits.iter().all(|b| b.is_ascii_digit()) {
        return Err(err());
    }
    let n = std::str::from_utf8(digits)
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(err)?;
    Ok((cmp, n, unit))
}

fn parse_signed(raw: &[u8]) -> Option<(Cmp, i64)> {
    let (cmp, rest) = split_cmp(raw);
    if rest.is_empty() || !rest.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = std::str::from_utf8(rest).ok()?.parse().ok()?;
    Some((cmp, n))
}

pub(crate) fn needs_meta(expr: &Expr) -> bool {
    let mut stack = vec![expr];
    while let Some(e) = stack.pop() {
        match e {
            Expr::Size { .. } | Expr::Empty | Expr::Age { .. } | Expr::Newer(_) => return true,
            Expr::Not(inner) => stack.push(inner),
            Expr::And(items) | Expr::Or(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    false
}

pub(crate) fn eval(
    expr: &Expr,
    item: &Item,
    now: SystemTime,
    errors: &AtomicBool,
    emit: &mut impl FnMut(&[u8], bool),
) -> bool {
    match expr {
        Expr::True => true,
        Expr::False => false,
        Expr::Print { nul } => {
            emit(item.path.as_os_str().as_bytes(), *nul);
            true
        }
        Expr::Not(e) => !eval(e, item, now, errors, emit),
        Expr::And(items) => items.iter().all(|e| eval(e, item, now, errors, emit)),
        Expr::Or(items) => items.iter().any(|e| eval(e, item, now, errors, emit)),
        Expr::Glob { pat, fold, whole } => {
            let text = if *whole {
                item.path.as_os_str().as_bytes()
            } else {
                base_name(item.path)
            };
            glob_match(pat, text, *fold)
        }
        Expr::Regex(re) => re.is_match(item.path.as_os_str().as_bytes()),
        Expr::Type(k) => item.kind == *k,
        Expr::Size { cmp, n, unit } => item
            .meta
            .is_some_and(|m| cmp_ord(*cmp, rounded_units(m.len(), *unit), *n)),
        Expr::Empty => match item.kind {
            Kind::File => item.meta.is_some_and(|m| m.len() == 0),
            Kind::Dir => match fs::read_dir(item.path) {
                Ok(mut rd) => rd.next().is_none(),
                Err(e) => {
                    super::walk::report(item.path, e, errors);
                    false
                }
            },
            _ => false,
        },
        Expr::Age { cmp, n, unit } => item
            .meta
            .and_then(|m| m.modified().ok())
            .is_some_and(|mtime| cmp_ord(*cmp, age_units(now, mtime, *unit), *n)),
        Expr::Newer(t) => item
            .meta
            .and_then(|m| m.modified().ok())
            .is_some_and(|mtime| mtime > *t),
    }
}

fn base_name(path: &Path) -> &[u8] {
    let bytes = path.as_os_str().as_bytes();
    let mut end = bytes.len();
    while end > 1 && bytes[end - 1] == b'/' {
        end -= 1;
    }
    if end == 1 && bytes.first() == Some(&b'/') {
        return &bytes[..1];
    }
    match bytes[..end].iter().rposition(|&b| b == b'/') {
        Some(i) => &bytes[i + 1..end],
        None => &bytes[..end],
    }
}

fn cmp_ord<T: Ord>(cmp: Cmp, got: T, n: T) -> bool {
    match cmp {
        Cmp::Eq => got == n,
        Cmp::Lt => got < n,
        Cmp::Gt => got > n,
    }
}

fn rounded_units(bytes: u64, unit: u64) -> u64 {
    bytes.div_ceil(unit)
}

fn age_units(now: SystemTime, mtime: SystemTime, unit: u64) -> i64 {
    match now.duration_since(mtime) {
        Ok(d) => (d.as_secs() / unit) as i64,
        Err(e) => -((e.duration().as_secs() / unit) as i64),
    }
}

fn byte_eq(a: u8, b: u8, fold: bool) -> bool {
    if fold {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

fn posix_close(text: &[u8]) -> Option<usize> {
    text.windows(2).position(|w| w == b":]")
}

fn class_close(body: &[u8]) -> Option<usize> {
    let mut i = usize::from(body.first() == Some(&b']'));
    while i < body.len() {
        if body[i] == b'['
            && body.get(i + 1) == Some(&b':')
            && let Some(rel) = posix_close(&body[i + 2..])
        {
            i += 2 + rel + 2;
            continue;
        }
        if body[i] == b'\\' {
            i += 2;
            continue;
        }
        if body[i] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn split_class(pat: &[u8]) -> Option<(&[u8], &[u8], bool)> {
    let body = pat.get(1..)?;
    let (pos, body) = match body.first() {
        Some(b'!' | b'^') => (false, &body[1..]),
        _ => (true, body),
    };
    let end = class_close(body)?;
    let rest_off = 1 + usize::from(!pos) + end + 1;
    Some((&body[..end], pat.get(rest_off..)?, pos))
}

fn class_range(got: u8, lo: u8, hi: u8, fold: bool) -> bool {
    if fold {
        let g = got.to_ascii_lowercase();
        let a = lo.to_ascii_lowercase();
        let b = hi.to_ascii_lowercase();
        a <= b && g >= a && g <= b
    } else {
        got >= lo && got <= hi
    }
}

fn posix_has(name: &[u8], got: u8, fold: bool) -> bool {
    match name {
        b"alnum" => got.is_ascii_alphanumeric(),
        b"alpha" => got.is_ascii_alphabetic(),
        b"blank" => got == b' ' || got == b'\t',
        b"cntrl" => got.is_ascii_control(),
        b"digit" => got.is_ascii_digit(),
        b"graph" => got.is_ascii_graphic(),
        b"lower" => {
            if fold {
                got.is_ascii_alphabetic()
            } else {
                got.is_ascii_lowercase()
            }
        }
        b"print" => got.is_ascii() && !got.is_ascii_control(),
        b"punct" => got.is_ascii_punctuation(),
        b"space" => got.is_ascii_whitespace(),
        b"upper" => {
            if fold {
                got.is_ascii_alphabetic()
            } else {
                got.is_ascii_uppercase()
            }
        }
        b"xdigit" => got.is_ascii_hexdigit(),
        _ => false,
    }
}

fn class_hit(inner: &[u8], got: u8, fold: bool) -> bool {
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'['
            && inner.get(i + 1) == Some(&b':')
            && let Some(rel) = posix_close(&inner[i + 2..])
        {
            if posix_has(&inner[i + 2..i + 2 + rel], got, fold) {
                return true;
            }
            i += 2 + rel + 2;
            continue;
        }
        let (lo, next) = if inner[i] == b'\\' {
            let Some(&esc) = inner.get(i + 1) else {
                return false;
            };
            (esc, i + 2)
        } else {
            (inner[i], i + 1)
        };
        if inner.get(next) == Some(&b'-') && next + 1 < inner.len() {
            let (hi, after) = if inner[next + 1] == b'\\' {
                let Some(&esc) = inner.get(next + 2) else {
                    return false;
                };
                (esc, next + 3)
            } else {
                (inner[next + 1], next + 2)
            };
            if class_range(got, lo, hi, fold) {
                return true;
            }
            i = after;
            continue;
        }
        if byte_eq(got, lo, fold) {
            return true;
        }
        i = next;
    }
    false
}

fn match_atom(pat: &[u8], got: u8, fold: bool) -> Option<usize> {
    match pat.first()? {
        b'\\' => {
            let want = *pat.get(1)?;
            byte_eq(got, want, fold).then_some(2)
        }
        b'?' => Some(1),
        b'[' => match split_class(pat) {
            Some((inner, rest, pos)) => {
                (class_hit(inner, got, fold) == pos).then_some(pat.len() - rest.len())
            }
            None => byte_eq(got, b'[', fold).then_some(1),
        },
        b'*' => None,
        &want => byte_eq(got, want, fold).then_some(1),
    }
}

fn glob_match(pat: &[u8], text: &[u8], fold: bool) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star = None;
    while ti < text.len() {
        if pat.get(pi) == Some(&b'*') {
            while pat.get(pi) == Some(&b'*') {
                pi += 1;
            }
            star = Some((pi, ti));
            continue;
        }
        if let Some(n) = match_atom(&pat[pi..], text[ti], fold) {
            pi += n;
            ti += 1;
            continue;
        }
        let Some((spi, sti)) = star else {
            return false;
        };
        let next = sti + 1;
        star = Some((spi, next));
        pi = spi;
        ti = next;
    }
    while pat.get(pi) == Some(&b'*') {
        pi += 1;
    }
    pi == pat.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn glob_and_age_edges() {
        assert!(!glob_match(b"\\", b"a", false));
        assert!(!glob_match(b"\\a", b"", false));
        assert!(!glob_match(b"\\a", b"b", false));
        assert!(!glob_match(b"?", b"", false));
        assert!(glob_match(b"?", b"/", false));
        assert!(glob_match(b"*", b"/", false));
        assert!(!glob_match(b"a*b", b"a", false));
        assert!(!glob_match(b"[", b"", false));
        assert!(!glob_match(b"[a]", b"", false));
        assert!(glob_match(b"[/]", b"/", false));
        assert!(glob_match(br"[\a-\z]", b"m", false));
        assert!(glob_match(b"[[:digit:]]", b"7", false));
        assert!(!glob_match(b"[[:digit:]]", b"a", false));
        assert!(glob_match(b"[[:alnum:]]", b"A", false));
        assert!(glob_match(b"[[:alpha:]]", b"Q", false));
        assert!(glob_match(b"[[:blank:]]", b" ", false));
        assert!(glob_match(b"[[:blank:]]", b"\t", false));
        assert!(glob_match(b"[[:cntrl:]]", b"\x01", false));
        assert!(glob_match(b"[[:graph:]]", b"!", false));
        assert!(glob_match(b"[[:lower:]]", b"Q", true));
        assert!(glob_match(b"[[:lower:]]", b"q", false));
        assert!(!glob_match(b"[[:lower:]]", b"Q", false));
        assert!(glob_match(b"[[:print:]]", b" ", false));
        assert!(glob_match(b"[[:punct:]]", b"!", false));
        assert!(glob_match(b"[[:space:]]", b"\t", false));
        assert!(glob_match(b"[[:upper:]]", b"q", true));
        assert!(glob_match(b"[[:upper:]]", b"Q", false));
        assert!(!glob_match(b"[[:upper:]]", b"q", false));
        assert!(glob_match(b"[[:xdigit:]]", b"f", false));
        assert!(!glob_match(b"[[:nope:]]", b"a", false));
        assert_eq!(match_atom(b"*", b'x', false), None);
        assert_eq!(match_atom(b"*", b'*', false), None);
        assert!(!class_hit(b"\\", b'a', false));
        assert!(!class_hit(b"a-\\", b'b', false));
        assert!(cmp_ord(Cmp::Eq, 2i64, 2));
        let now = SystemTime::now();
        let future = now + std::time::Duration::from_secs(90);
        assert!(age_units(now, future, 60) < 0);
    }

    #[test]
    fn glob_star_is_linear() {
        let pat = b"*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a";
        let hit = [b'a'; 40];
        let mut miss = hit;
        miss[39] = b'b';
        assert!(glob_match(pat, &hit, false));
        assert!(!glob_match(pat, &miss, false));
    }

    #[test]
    fn base_name_strips_trailing_slashes() {
        assert_eq!(base_name(Path::new("/")), b"/");
        assert_eq!(base_name(Path::new("./")), b".");
        assert_eq!(base_name(Path::new("../")), b"..");
        assert_eq!(base_name(Path::new("foo/")), b"foo");
        assert_eq!(base_name(Path::new("foo/bar")), b"bar");
    }

    #[test]
    fn regex_alternation_matches_longer_alternative() {
        let (expr, _, _) = parse(
            &[
                OsString::from("-regex"),
                OsString::from("a|ab"),
                OsString::from("-print"),
            ],
            Follow::Never,
        )
        .unwrap();
        let item = Item {
            path: Path::new("ab"),
            kind: Kind::File,
            meta: None,
        };
        let errors = AtomicBool::new(false);
        let now = SystemTime::now();
        let mut printed = false;
        assert!(eval(&expr, &item, now, &errors, &mut |_, _| {
            printed = true;
        }));
        assert!(printed);
    }

    #[test]
    fn long_and_and_not_parse() {
        let mut tokens = Vec::new();
        for _ in 0..4000 {
            tokens.push(OsString::from("-true"));
            tokens.push(OsString::from("-a"));
        }
        tokens.push(OsString::from("-true"));
        tokens.push(OsString::from("-print"));
        assert!(parse(&tokens, Follow::Never).is_ok());
        let mut nots = Vec::new();
        for _ in 0..4000 {
            nots.push(OsString::from("!"));
        }
        nots.push(OsString::from("-true"));
        nots.push(OsString::from("-print"));
        assert!(parse(&nots, Follow::Never).is_ok());
    }

    fn item<'a>(path: &'a str, kind: Kind, meta: Option<&'a fs::Metadata>) -> Item<'a> {
        Item {
            path: Path::new(path),
            kind,
            meta,
        }
    }

    #[test]
    fn parse_and_eval_cover_predicates() {
        assert!(matches!(
            parse(&[], Follow::Never).unwrap().0,
            Expr::Print { nul: false }
        ));
        assert!(
            parse(
                &[OsString::from("-true"), OsString::from(")")],
                Follow::Never
            )
            .is_err()
        );
        assert!(parse(&[OsString::from(",")], Follow::Never).is_err());
        let (even, _, _) = parse(
            &[
                OsString::from("!"),
                OsString::from("!"),
                OsString::from("-true"),
                OsString::from("-print"),
            ],
            Follow::Never,
        )
        .unwrap();
        let (odd, _, _) = parse(
            &[
                OsString::from("-not"),
                OsString::from("-true"),
                OsString::from("-print"),
            ],
            Follow::Never,
        )
        .unwrap();
        let (typed, _, _) = parse(
            &[
                OsString::from("-type"),
                OsString::from("f"),
                OsString::from("-print0"),
            ],
            Follow::Never,
        )
        .unwrap();
        let (sized, _, _) = parse(
            &[
                OsString::from("-size"),
                OsString::from("+0c"),
                OsString::from("-empty"),
                OsString::from("-mtime"),
                OsString::from("0"),
                OsString::from("-print"),
            ],
            Follow::Never,
        )
        .unwrap();
        assert!(needs_meta(&sized));
        assert!(needs_meta(&Expr::Not(Box::new(Expr::Empty))));
        assert!(!needs_meta(&Expr::True));
        let errors = AtomicBool::new(false);
        let now = SystemTime::now();
        let file = item("ab", Kind::File, None);
        fn noop(_: &[u8], _: bool) {}
        noop(b"", false);
        let mut saw = false;
        assert!(eval(&even, &file, now, &errors, &mut |_, _| {
            saw = true;
        }));
        assert!(saw);
        assert!(!eval(&odd, &file, now, &errors, &mut noop));
        let mut saw_nul = false;
        assert!(eval(&typed, &file, now, &errors, &mut |_, nul| {
            saw_nul = nul;
        }));
        assert!(saw_nul);
        assert!(!eval(
            &Expr::Size {
                cmp: Cmp::Eq,
                n: 1,
                unit: 1,
            },
            &file,
            now,
            &errors,
            &mut noop
        ));
        assert!(!eval(
            &Expr::Age {
                cmp: Cmp::Eq,
                n: 0,
                unit: 60,
            },
            &file,
            now,
            &errors,
            &mut noop
        ));
        assert!(!eval(&Expr::Newer(now), &file, now, &errors, &mut noop));
        assert!(eval(
            &Expr::Type(Kind::File),
            &file,
            now,
            &errors,
            &mut noop
        ));
        assert!(eval(
            &Expr::Glob {
                pat: b"ab".to_vec(),
                fold: false,
                whole: false,
            },
            &file,
            now,
            &errors,
            &mut noop
        ));
        assert!(eval(
            &Expr::Glob {
                pat: b"ab".to_vec(),
                fold: false,
                whole: true,
            },
            &file,
            now,
            &errors,
            &mut noop
        ));
        assert!(!eval(
            &Expr::Empty,
            &item("l", Kind::Link, None),
            now,
            &errors,
            &mut noop
        ));
        assert!(parse_size(b"18446744073709551616c").is_err());
        assert!(parse_signed(b"9223372036854775808").is_none());
        assert!(cmp_ord(Cmp::Eq, 1u64, 1));
        assert!(cmp_ord(Cmp::Lt, 0u64, 1));
        assert!(cmp_ord(Cmp::Gt, 2u64, 1));
        assert_eq!(rounded_units(513, 512), 2);
        let dir = std::env::temp_dir().join(format!(
            "hfind_expr_{}_{}",
            std::process::id(),
            now.duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("e");
        fs::create_dir(&empty).unwrap();
        let locked = dir.join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let newer = dir.join("n");
        fs::write(&newer, b"").unwrap();
        let (age, _, _) = parse(
            &[
                OsString::from("-newer"),
                newer.clone().into_os_string(),
                OsString::from("-print"),
            ],
            Follow::Never,
        )
        .unwrap();
        let (age_h, _, _) = parse(
            &[
                OsString::from("-newer"),
                newer.clone().into_os_string(),
                OsString::from("-print"),
            ],
            Follow::Cli,
        )
        .unwrap();
        let meta = fs::symlink_metadata(&newer).unwrap();
        let _ = eval(
            &age,
            &item(newer.to_str().unwrap(), Kind::File, Some(&meta)),
            now,
            &errors,
            &mut noop,
        );
        let _ = eval(
            &age_h,
            &item(newer.to_str().unwrap(), Kind::File, Some(&meta)),
            now,
            &errors,
            &mut noop,
        );
        assert!(eval(
            &Expr::Empty,
            &item(empty.to_str().unwrap(), Kind::Dir, None),
            now,
            &errors,
            &mut noop
        ));
        assert!(!eval(
            &Expr::Empty,
            &item(locked.to_str().unwrap(), Kind::Dir, None),
            now,
            &errors,
            &mut noop
        ));
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }
}
