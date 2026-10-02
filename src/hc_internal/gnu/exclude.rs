use super::locale::{self, CharClass};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegKind {
    Hash,
    Pattern,
}

#[derive(Clone, Debug)]
struct Segment {
    kind: SegKind,
    include: bool,
    patterns: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
struct ExcludeList {
    segments: Vec<Segment>,
}

impl ExcludeList {
    fn add(&mut self, pattern: &[u8], include: bool) {
        let kind = if has_wildcards(pattern) {
            SegKind::Pattern
        } else {
            SegKind::Hash
        };
        let stored = if kind == SegKind::Hash {
            unescape(pattern)
        } else {
            pattern.to_vec()
        };
        match self.segments.last_mut() {
            Some(seg) if seg.kind == kind && seg.include == include => seg.patterns.push(stored),
            _ => self.segments.push(Segment {
                kind,
                include,
                patterns: vec![stored],
            }),
        }
    }

    fn excluded(&self, name: &[u8], anchored: bool, utf8: bool) -> bool {
        let Some(oldest) = self.segments.first() else {
            return false;
        };
        for seg in self.segments.iter().rev() {
            let hit = match seg.kind {
                SegKind::Hash => hash_matches(&seg.patterns, name, anchored),
                SegKind::Pattern => seg
                    .patterns
                    .iter()
                    .any(|p| exclude_fnmatch(p, name, anchored, utf8)),
            };
            if hit {
                return !seg.include;
            }
        }
        oldest.include
    }
}

fn has_wildcards(p: &[u8]) -> bool {
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            b'?' | b'*' | b'[' => return true,
            b'\\' if i + 1 < p.len() => i += 1,
            _ => {}
        }
        i += 1;
    }
    false
}

fn unescape(p: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(p.len());
    let mut i = 0;
    while i < p.len() {
        if p[i] == b'\\' && i + 1 < p.len() {
            i += 1;
        }
        out.push(p[i]);
        i += 1;
    }
    out
}

fn hash_matches(patterns: &[Vec<u8>], name: &[u8], anchored: bool) -> bool {
    let mut f = name;
    loop {
        if patterns.iter().any(|p| p.as_slice() == f) {
            return true;
        }
        if anchored {
            return false;
        }
        match memchr::memchr(b'/', f) {
            Some(i) => f = &f[i + 1..],
            None => return false,
        }
    }
}

fn exclude_fnmatch(pattern: &[u8], name: &[u8], anchored: bool, utf8: bool) -> bool {
    if fnmatch(pattern, name, utf8) {
        return true;
    }
    if !anchored {
        for i in 0..name.len() {
            if name[i] == b'/'
                && name.get(i + 1) != Some(&b'/')
                && fnmatch(pattern, &name[i + 1..], utf8)
            {
                return true;
            }
        }
    }
    false
}

fn units(s: &[u8], utf8: bool) -> Option<Vec<u32>> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let (u, len) = locale::decode(utf8, &s[i..]);
        if u >= locale::INVALID_BASE {
            return None;
        }
        out.push(u);
        i += len;
    }
    Some(out)
}

#[must_use]
pub fn fnmatch(pattern: &[u8], name: &[u8], utf8: bool) -> bool {
    let (Some(p), Some(n)) = (units(pattern, utf8), units(name, utf8)) else {
        return false;
    };
    let posixly_correct = std::env::var_os("POSIXLY_CORRECT").is_some();
    fnm(&p, &n, utf8, posixly_correct)
}

const BS: u32 = b'\\' as u32;
const STAR: u32 = b'*' as u32;
const QM: u32 = b'?' as u32;
const LB: u32 = b'[' as u32;
const RB: u32 = b']' as u32;

enum Bracket {
    Matched(usize),
    NotMatched,
    Fail,
    Literal,
}

fn bracket(p: &[u32], start: usize, fnc: u32, utf8: bool, posixly: bool) -> Bracket {
    let at = |i: usize| p.get(i).copied().unwrap_or(0);
    let mut i = start;
    let not = at(i) == u32::from(b'!') || (!posixly && at(i) == u32::from(b'^'));
    if not {
        i += 1;
    }
    let mut c = at(i);
    i += 1;
    let mut matched = false;
    loop {
        let mut normal = false;
        if c == BS {
            if at(i) == 0 && i >= p.len() {
                return Bracket::Fail;
            }
            c = at(i);
            i += 1;
            normal = true;
        } else if c == LB && at(i) == u32::from(b':') {
            let startp = i;
            let mut name = Vec::new();
            let mut as_normal = false;
            loop {
                if name.len() == 256 {
                    return Bracket::Fail;
                }
                i += 1;
                let cc = at(i);
                if cc == u32::from(b':') && at(i + 1) == RB {
                    i += 2;
                    break;
                }
                if !(u32::from(b'a')..u32::from(b'z')).contains(&cc) {
                    i = startp;
                    c = LB;
                    as_normal = true;
                    break;
                }
                name.push(cc as u8);
            }
            if as_normal {
                normal = true;
            } else {
                let Some(class) = CharClass::from_name(&name) else {
                    return Bracket::Fail;
                };
                if class.contains(utf8, fnc) {
                    matched = true;
                    break;
                }
                c = at(i);
                i += 1;
            }
        } else if c == LB && at(i) == u32::from(b'=') {
            i += 1;
            c = at(i);
            if c == 0 && i >= p.len() {
                return Bracket::Fail;
            }
            i += 1;
            c = at(i);
            if c != u32::from(b'=') || at(i + 1) != RB {
                return Bracket::Fail;
            }
            i += 2;
        } else if c == LB && at(i) == u32::from(b'.') {
            loop {
                i += 1;
                c = at(i);
                if c == 0 && i >= p.len() {
                    return Bracket::Fail;
                }
                if c == u32::from(b'.') && at(i + 1) == RB {
                    break;
                }
            }
            i += 2;
        } else if c == 0 && i > p.len() {
            return Bracket::Literal;
        } else {
            normal = true;
        }
        if normal {
            let is_range = at(i) == u32::from(b'-') && i + 1 < p.len() && at(i + 1) != RB;
            if !is_range && c == fnc {
                matched = true;
                break;
            }
            let cold = c;
            c = at(i);
            i += 1;
            if c == u32::from(b'-') && at(i) != RB {
                let mut cend = at(i);
                i += 1;
                if cend == BS {
                    cend = at(i);
                    i += 1;
                }
                if cend == 0 && i > p.len() {
                    return Bracket::Fail;
                }
                if cold <= fnc && fnc <= cend {
                    matched = true;
                    break;
                }
                c = at(i);
                i += 1;
            }
        }
        if c == RB {
            break;
        }
        if i > p.len() {
            return Bracket::Literal;
        }
    }
    if !matched {
        return if not {
            Bracket::Matched(i)
        } else {
            Bracket::NotMatched
        };
    }
    loop {
        let c = at(i);
        i += 1;
        if c == RB {
            break;
        }
        if i > p.len() {
            return Bracket::Fail;
        }
        if c == BS {
            if i >= p.len() {
                return Bracket::Fail;
            }
            i += 1;
        } else if c == LB && at(i) == u32::from(b':') {
            let startp = i;
            let mut c1 = 0;
            loop {
                i += 1;
                c1 += 1;
                if c1 == 256 {
                    return Bracket::Fail;
                }
                if at(i) == u32::from(b':') && at(i + 1) == RB {
                    break;
                }
                let cc = at(i);
                if !(u32::from(b'a')..u32::from(b'z')).contains(&cc) {
                    i = startp - 2;
                    break;
                }
            }
            i += 2;
        } else if c == LB && at(i) == u32::from(b'=') {
            i += 1;
            if at(i) == 0 && i >= p.len() {
                return Bracket::Fail;
            }
            i += 1;
            if at(i) != u32::from(b'=') || at(i + 1) != RB {
                return Bracket::Fail;
            }
            i += 2;
        } else if c == LB && at(i) == u32::from(b'.') {
            loop {
                i += 1;
                if i >= p.len() {
                    return Bracket::Fail;
                }
                if at(i) == u32::from(b'.') && at(i + 1) == RB {
                    break;
                }
            }
            i += 2;
        }
    }
    if not {
        Bracket::NotMatched
    } else {
        Bracket::Matched(i)
    }
}

fn fnm(p: &[u32], n: &[u32], utf8: bool, posixly: bool) -> bool {
    let mut pi = 0;
    let mut ni = 0;
    while pi < p.len() {
        let c = p[pi];
        pi += 1;
        match c {
            QM => {
                if ni == n.len() {
                    return false;
                }
            }
            BS => {
                let Some(&e) = p.get(pi) else {
                    return false;
                };
                pi += 1;
                if ni == n.len() || n[ni] != e {
                    return false;
                }
            }
            STAR => {
                let mut cc = p.get(pi).copied();
                while let Some(x) = cc {
                    if x != QM && x != STAR {
                        break;
                    }
                    pi += 1;
                    if x == QM {
                        if ni == n.len() {
                            return false;
                        }
                        ni += 1;
                    }
                    cc = p.get(pi).copied();
                }
                if pi >= p.len() {
                    return true;
                }
                let rest = &p[pi..];
                let first = if rest[0] == BS {
                    rest.get(1).copied()
                } else {
                    Some(rest[0])
                };
                for start in ni..n.len() {
                    if rest[0] != LB && first.is_some_and(|f| n[start] != f) {
                        continue;
                    }
                    if fnm(rest, &n[start..], utf8, posixly) {
                        return true;
                    }
                }
                return false;
            }
            LB => {
                if ni == n.len() {
                    return false;
                }
                match bracket(p, pi, n[ni], utf8, posixly) {
                    Bracket::Matched(next) => pi = next,
                    Bracket::NotMatched | Bracket::Fail => return false,
                    Bracket::Literal => {
                        if n[ni] != LB {
                            return false;
                        }
                    }
                }
            }
            _ => {
                if ni == n.len() || n[ni] != c {
                    return false;
                }
            }
        }
        ni += 1;
    }
    ni == n.len()
}

#[derive(Clone, Debug, Default)]
pub struct Excludes {
    files: [ExcludeList; 2],
    dirs: [ExcludeList; 2],
    utf8: bool,
}

impl Excludes {
    #[must_use]
    pub fn new(utf8: bool) -> Self {
        Self {
            utf8,
            ..Self::default()
        }
    }

    pub fn add_include(&mut self, glob: &[u8]) {
        for l in &mut self.files {
            l.add(glob, true);
        }
    }

    pub fn add_exclude(&mut self, glob: &[u8]) {
        for l in &mut self.files {
            l.add(glob, false);
        }
    }

    pub fn add_exclude_from(&mut self, content: &[u8]) {
        for line in content.split(|&b| b == b'\n') {
            let end = line
                .iter()
                .rposition(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
                .map_or(0, |i| i + 1);
            if end > 0 {
                self.add_exclude(&line[..end]);
            }
        }
    }

    pub fn add_exclude_dir(&mut self, glob: &[u8]) {
        let stripped = strip_trailing_slashes(glob);
        for l in &mut self.dirs {
            l.add(stripped, false);
        }
    }

    #[must_use]
    pub fn has_file_patterns(&self) -> bool {
        !self.files[0].segments.is_empty()
    }

    #[must_use]
    pub fn has_dir_patterns(&self) -> bool {
        !self.dirs[0].segments.is_empty()
    }

    #[must_use]
    pub fn skip_file(&self, name: &[u8], command_line: bool) -> bool {
        self.files[usize::from(command_line)].excluded(name, !command_line, self.utf8)
    }

    #[must_use]
    pub fn skip_dir(&self, name: &[u8], command_line: bool) -> bool {
        self.dirs[usize::from(command_line)].excluded(name, !command_line, self.utf8)
    }
}

#[must_use]
pub fn strip_trailing_slashes(p: &[u8]) -> &[u8] {
    let mut end = p.len();
    while end > 1 && p[end - 1] == b'/' {
        end -= 1;
    }
    &p[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnmatch_basics() {
        assert!(fnmatch(b"*.c", b"a.c", true));
        assert!(fnmatch(b"*.c", b"dir/a.c", true));
        assert!(!fnmatch(b"*.c", b"a.h", true));
        assert!(fnmatch(b"a?c", b"abc", true));
        assert!(fnmatch(b"[a-c]x", b"bx", true));
        assert!(fnmatch(b"[!a-c]x", b"dx", true));
        assert!(!fnmatch(b"[!a-c]x", b"ax", true));
        assert!(fnmatch(b"[]a]", b"]", true));
        assert!(fnmatch(b"[[:digit:]]*", b"9z", true));
        assert!(!fnmatch(b"[[:nope:]]", b"a", true));
        assert!(fnmatch(b"[ab", b"[ab", true));
        assert!(fnmatch(b"\\*", b"*", true));
        assert!(!fnmatch(b"a\\", b"a\\", true));
        assert!(fnmatch(b"*\xc3\xa9", b"caf\xc3\xa9", true));
        assert!(!fnmatch(b"*", b"caf\xe9", true));
        assert!(fnmatch(b"*", b"caf\xe9", false));
    }

    #[test]
    fn grep_include_exclude_rules() {
        let mut e = Excludes::new(true);
        e.add_include(b"*.c");
        assert!(!e.skip_file(b"a.c", false));
        assert!(e.skip_file(b"a.h", false));
        e.add_exclude(b"b.c");
        assert!(e.skip_file(b"b.c", false));
        assert!(!e.skip_file(b"a.c", false));
        let mut e = Excludes::new(true);
        e.add_exclude(b"*.o");
        assert!(!e.skip_file(b"x.c", false));
        assert!(e.skip_file(b"dir/x.o", true));
        e.add_exclude_dir(b"build/");
        assert!(e.skip_dir(b"build", false));
        assert!(e.skip_dir(b"src/build", true));
        assert!(!e.skip_dir(b"src/build", false));
        let mut e = Excludes::new(false);
        e.add_exclude_from(b"foo  \n\n*.tmp\nlast");
        assert!(e.skip_file(b"foo", false));
        assert!(e.skip_file(b"x.tmp", false));
        assert!(e.skip_file(b"last", false));
        assert!(!e.skip_file(b"other", false));
    }
}
