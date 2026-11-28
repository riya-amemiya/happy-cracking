use std::sync::OnceLock;

const FLAG_LETTERS: &[u8] = b"imsxUuR";

#[derive(Clone, Copy)]
struct Flags(u8);

impl Flags {
    fn bit(letter: u8) -> u8 {
        FLAG_LETTERS
            .iter()
            .position(|&l| l == letter)
            .map_or(0, |i| 1 << i)
    }

    fn on(self, letter: u8) -> bool {
        self.0 & Self::bit(letter) != 0
    }

    fn with(self, letter: u8, on: bool) -> Flags {
        if on {
            Flags(self.0 | Self::bit(letter))
        } else {
            Flags(self.0 & !Self::bit(letter))
        }
    }

    fn class(self, text: &str) -> Node {
        let on: String = b"imsxUR"
            .iter()
            .filter(|&&l| self.on(l))
            .map(|&l| char::from(l))
            .collect();
        let unicode = self.on(b'u');
        let off = if unicode { "" } else { "-u" };
        let prefix = if on.is_empty() && off.is_empty() {
            String::new()
        } else {
            format!("(?{on}{off})")
        };
        Node::Class(format!("{prefix}\\A(?:{text})\\z"), unicode)
    }
}

enum Node {
    Empty,
    Flags,
    Lit(Vec<u8>),
    Fixed(bool),
    Class(String, bool),
    Look(bool),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Capture(Box<Node>),
    Rep(Box<Node>),
}

fn concat(items: Vec<Node>) -> Node {
    let mut out: Vec<Node> = Vec::new();
    let push = |node: Node, out: &mut Vec<Node>| match node {
        Node::Empty | Node::Flags => {}
        Node::Lit(bytes) => match out.last_mut() {
            Some(Node::Lit(prev)) => prev.extend_from_slice(&bytes),
            _ => out.push(Node::Lit(bytes)),
        },
        other => out.push(other),
    };
    for item in items {
        match item {
            Node::Concat(inner) => {
                for sub in inner {
                    push(sub, &mut out);
                }
            }
            other => push(other, &mut out),
        }
    }
    match out.len() {
        0 => Node::Empty,
        1 => out.pop().unwrap(),
        _ => Node::Concat(out),
    }
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

fn decode(s: &[u8], pos: usize) -> Option<(char, usize)> {
    let rest = &s[pos..];
    let len = match rest.first()? {
        b if *b < 0x80 => 1,
        b if *b >= 0xF0 => 4,
        b if *b >= 0xE0 => 3,
        _ => 2,
    };
    let c = std::str::from_utf8(rest.get(..len)?).ok()?.chars().next()?;
    Some((c, len))
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_verbose(&mut self, flags: Flags) {
        if !flags.on(b'x') {
            return;
        }
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r' | b'\x0B' | b'\x0C') => self.pos += 1,
                Some(b'#') => {
                    while let Some(b) = self.peek() {
                        self.pos += 1;
                        if b == b'\n' {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn alternation(&mut self, flags: &mut Flags) -> Option<Node> {
        let mut alts = Vec::new();
        let mut items = Vec::new();
        loop {
            self.skip_verbose(*flags);
            match self.peek() {
                None | Some(b')') => break,
                Some(b'|') => {
                    self.pos += 1;
                    alts.push(concat(std::mem::take(&mut items)));
                }
                Some(_) => {
                    let atom = self.atom(flags)?;
                    let atom = self.repetitions(atom, *flags)?;
                    items.push(atom);
                }
            }
        }
        if alts.is_empty() {
            return Some(concat(items));
        }
        alts.push(concat(items));
        Some(Node::Alt(alts))
    }

    fn repetitions(&mut self, mut atom: Node, flags: Flags) -> Option<Node> {
        loop {
            self.skip_verbose(flags);
            let (min, max) = match self.peek() {
                Some(b'*') => {
                    self.pos += 1;
                    (0, None)
                }
                Some(b'+') => {
                    self.pos += 1;
                    (1, None)
                }
                Some(b'?') => {
                    self.pos += 1;
                    (0, Some(1))
                }
                Some(b'{') => self.counted()?,
                _ => return Some(atom),
            };
            if matches!(atom, Node::Flags) {
                return None;
            }
            self.eat(b'?');
            atom = match (min, max) {
                (_, Some(0)) => Node::Empty,
                (1, Some(1)) => atom,
                _ => Node::Rep(Box::new(atom)),
            };
        }
    }

    fn number(&mut self) -> Option<u32> {
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.s[start..self.pos])
            .ok()?
            .parse()
            .ok()
    }

    fn counted(&mut self) -> Option<(u32, Option<u32>)> {
        self.pos += 1;
        let ws = |p: &mut Self| {
            while p.peek().is_some_and(|b| b.is_ascii_whitespace()) {
                p.pos += 1;
            }
        };
        ws(self);
        let min = self.number()?;
        ws(self);
        let max = if self.eat(b',') {
            ws(self);
            if self.peek() == Some(b'}') {
                None
            } else {
                Some(self.number()?)
            }
        } else {
            Some(min)
        };
        ws(self);
        if !self.eat(b'}') || max.is_some_and(|m| m < min) {
            return None;
        }
        Some((min, max))
    }

    fn atom(&mut self, flags: &mut Flags) -> Option<Node> {
        let b = self.peek()?;
        match b {
            b'(' => self.group(flags),
            b'[' => {
                let start = self.pos;
                self.pos += 1;
                self.bracket_body()?;
                let text = std::str::from_utf8(&self.s[start..self.pos]).ok()?;
                Some(single_dot_class(text).unwrap_or_else(|| flags.class(text)))
            }
            b'.' => {
                self.pos += 1;
                Some(Node::Fixed(false))
            }
            b'^' => {
                self.pos += 1;
                Some(Node::Look(!flags.on(b'm')))
            }
            b'$' => {
                self.pos += 1;
                Some(Node::Look(false))
            }
            b'\\' => self.escape(*flags),
            b'*' | b'+' | b'?' | b'{' | b')' => None,
            _ => {
                let (c, len) = decode(self.s, self.pos)?;
                self.pos += len;
                Some(literal(c, *flags))
            }
        }
    }

    fn group(&mut self, flags: &mut Flags) -> Option<Node> {
        self.pos += 1;
        let mut inner = *flags;
        let capture = if self.eat(b'?') {
            if self.s[self.pos..].starts_with(b"P<") || self.s[self.pos..].starts_with(b"<") {
                let close = self.s[self.pos..].iter().position(|&b| b == b'>')?;
                let name = &self.s[self.pos..self.pos + close];
                let name = name
                    .strip_prefix(b"P<")
                    .or_else(|| name.strip_prefix(b"<"))?;
                if name.is_empty()
                    || !name.iter().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'[' | b']')
                    })
                    || name[0].is_ascii_digit()
                {
                    return None;
                }
                self.pos += close + 1;
                true
            } else {
                let mut on = true;
                let mut any = false;
                loop {
                    let c = self.peek()?;
                    self.pos += 1;
                    match c {
                        b'i' | b'm' | b's' | b'x' | b'u' | b'U' | b'R' => {
                            inner = inner.with(c, on);
                        }
                        b'-' if on => {
                            on = false;
                            any = false;
                            continue;
                        }
                        b')' => {
                            if !on && !any {
                                return None;
                            }
                            *flags = inner;
                            return Some(Node::Flags);
                        }
                        b':' => {
                            if !on && !any {
                                return None;
                            }
                            break;
                        }
                        _ => return None,
                    }
                    any = true;
                }
                false
            }
        } else {
            true
        };
        let body = self.alternation(&mut inner)?;
        if !self.eat(b')') {
            return None;
        }
        Some(if capture {
            Node::Capture(Box::new(body))
        } else {
            body
        })
    }

    fn braced_or(&mut self, fixed: usize) -> Option<u32> {
        if self.eat(b'{') {
            let close = self.s[self.pos..].iter().position(|&b| b == b'}')?;
            let digits = std::str::from_utf8(&self.s[self.pos..self.pos + close]).ok()?;
            self.pos += close + 1;
            u32::from_str_radix(digits.trim(), 16).ok()
        } else {
            let digits = std::str::from_utf8(self.s.get(self.pos..self.pos + fixed)?).ok()?;
            self.pos += fixed;
            if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(digits, 16).ok()
        }
    }

    fn escape(&mut self, flags: Flags) -> Option<Node> {
        let start = self.pos;
        self.pos += 1;
        let (c, len) = decode(self.s, self.pos)?;
        self.pos += len;
        let code = match c {
            'A' => return Some(Node::Look(true)),
            'z' | 'b' | 'B' | '<' | '>' => {
                if c == 'b' && self.peek() == Some(b'{') {
                    let close = self.s[self.pos..].iter().position(|&b| b == b'}')?;
                    self.pos += close + 1;
                }
                return Some(Node::Look(false));
            }
            'd' | 'D' | 's' | 'S' | 'w' | 'W' => {
                return Some(flags.class(std::str::from_utf8(&self.s[start..self.pos]).ok()?));
            }
            'p' | 'P' => {
                if self.eat(b'{') {
                    let close = self.s[self.pos..].iter().position(|&b| b == b'}')?;
                    self.pos += close + 1;
                } else {
                    let (_, l) = decode(self.s, self.pos)?;
                    self.pos += l;
                }
                return Some(flags.class(std::str::from_utf8(&self.s[start..self.pos]).ok()?));
            }
            'x' => self.braced_or(2)?,
            'u' => self.braced_or(4)?,
            'U' => self.braced_or(8)?,
            'n' => 10,
            't' => 9,
            'r' => 13,
            'a' => 7,
            'f' => 12,
            'v' => 11,
            c if c.is_ascii() && !c.is_ascii_alphanumeric() => u32::from(c),
            _ => return None,
        };
        if !flags.on(b'u') && c == 'x' {
            let byte = u8::try_from(code).ok()?;
            if byte >= 0x80 {
                return Some(Node::Lit(vec![byte]));
            }
        }
        Some(literal(char::from_u32(code)?, flags))
    }

    fn bracket_body(&mut self) -> Option<()> {
        self.eat(b'^');
        if self.peek() == Some(b']') {
            self.pos += 1;
        }
        loop {
            match self.peek()? {
                b']' => {
                    self.pos += 1;
                    return Some(());
                }
                b'\\' => {
                    self.pos += 1;
                    let c = self.peek()?;
                    self.pos += 1;
                    if matches!(c, b'x' | b'u' | b'U' | b'p' | b'P') && self.peek() == Some(b'{') {
                        let close = self.s[self.pos..].iter().position(|&b| b == b'}')?;
                        self.pos += close + 1;
                    } else if c >= 0x80 {
                        let (_, l) = decode(self.s, self.pos - 1)?;
                        self.pos += l - 1;
                    }
                }
                b'[' => {
                    self.pos += 1;
                    if self.peek() == Some(b':') {
                        let close = self.s[self.pos..].windows(2).position(|w| w == b":]")?;
                        self.pos += close + 2;
                    } else {
                        self.bracket_body()?;
                    }
                }
                _ => self.pos += 1,
            }
        }
    }
}

fn single_dot_class(text: &str) -> Option<Node> {
    matches!(text, "[.]" | "[\\.]").then(|| Node::Lit(vec![b'.']))
}

fn literal(c: char, flags: Flags) -> Node {
    if !flags.on(b'i') {
        let mut buf = [0u8; 4];
        return Node::Lit(c.encode_utf8(&mut buf).as_bytes().to_vec());
    }
    if c.is_ascii_alphabetic() {
        return Node::Fixed(true);
    }
    if c.is_ascii() || !flags.on(b'u') {
        let mut buf = [0u8; 4];
        return Node::Lit(c.encode_utf8(&mut buf).as_bytes().to_vec());
    }
    let mut candidates: Vec<char> = Vec::new();
    let mut add = |it: &mut dyn Iterator<Item = char>| {
        let all: Vec<char> = it.collect();
        if let [single] = all[..] {
            candidates.push(single);
        }
    };
    add(&mut c.to_uppercase());
    add(&mut c.to_lowercase());
    let lowers: Vec<char> = c.to_lowercase().collect();
    if let [l] = lowers[..] {
        add(&mut l.to_uppercase());
    }
    candidates.retain(|&v| v != c);
    let Ok(re) = regex::Regex::new(&format!("(?i)\\A{}\\z", regex::escape(&c.to_string()))) else {
        return Node::Fixed(false);
    };
    let variants: Vec<char> = candidates
        .into_iter()
        .filter(|v| re.is_match(&v.to_string()))
        .collect();
    if variants.is_empty() {
        let mut buf = [0u8; 4];
        Node::Lit(c.encode_utf8(&mut buf).as_bytes().to_vec())
    } else {
        Node::Fixed(c.is_uppercase() || variants.iter().any(|v| v.is_uppercase()))
    }
}

fn uppercase_chars() -> &'static [char] {
    static CHARS: OnceLock<Vec<char>> = OnceLock::new();
    CHARS.get_or_init(|| {
        (0..=0x1F18Au32)
            .filter_map(char::from_u32)
            .filter(|c| c.is_uppercase())
            .collect()
    })
}

fn class_has_uppercase(anchored: &str, unicode: bool) -> bool {
    if unicode {
        let Ok(re) = regex::Regex::new(anchored) else {
            return false;
        };
        let member = |c: Option<char>| c.is_some_and(|c| re.is_match(c.encode_utf8(&mut [0; 4])));
        uppercase_chars().iter().any(|&u| {
            let code = u32::from(u);
            member(Some(u))
                && (!member(code.checked_sub(1).and_then(char::from_u32))
                    || !member(char::from_u32(code + 1)))
        })
    } else {
        let Ok(re) = regex::bytes::Regex::new(anchored) else {
            return false;
        };
        let member = |b: Option<u8>| b.is_some_and(|b| re.is_match(&[b]));
        (0..=255u8).any(|b| {
            char::from(b).is_uppercase()
                && member(Some(b))
                && (!member(b.checked_sub(1)) || !member(b.checked_add(1)))
        })
    }
}

fn has_uppercase(node: &Node) -> bool {
    match node {
        Node::Empty | Node::Flags | Node::Look(_) => false,
        Node::Fixed(v) => *v,
        Node::Lit(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => s.chars().any(char::is_uppercase),
            Err(_) => bytes.iter().any(|&b| char::from(b).is_uppercase()),
        },
        Node::Class(text, unicode) => class_has_uppercase(text, *unicode),
        Node::Concat(items) | Node::Alt(items) => items.iter().any(has_uppercase),
        Node::Capture(inner) | Node::Rep(inner) => has_uppercase(inner),
    }
}

fn parse(pattern: &str) -> Option<Node> {
    let mut parser = Parser {
        s: pattern.as_bytes(),
        pos: 0,
    };
    let mut flags = Flags(0).with(b'u', true);
    let node = parser.alternation(&mut flags)?;
    (parser.pos == pattern.len()).then_some(node)
}

pub(super) fn pattern_has_uppercase_char(pattern: &str) -> bool {
    parse(pattern).is_some_and(|n| has_uppercase(&n))
}

pub(super) fn pattern_matches_strings_with_leading_dot(pattern: &str) -> bool {
    match parse(pattern) {
        Some(Node::Concat(items)) => {
            matches!(items.first(), Some(Node::Look(true)))
                && matches!(items.get(1), Some(Node::Lit(b)) if b.starts_with(b"."))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercase_detection_matches_fd() {
        assert!(pattern_has_uppercase_char("A"));
        assert!(pattern_has_uppercase_char("foo.EXE"));
        assert!(!pattern_has_uppercase_char("a"));
        assert!(!pattern_has_uppercase_char("foo.exe123"));
        assert!(pattern_has_uppercase_char("foo.[a-zA-Z]"));
        assert!(!pattern_has_uppercase_char(r"\Acargo"));
        assert!(!pattern_has_uppercase_char(r"carg\x6F"));
        assert!(pattern_has_uppercase_char(r"\x41"));
        assert!(pattern_has_uppercase_char(r"\w+"));
        assert!(!pattern_has_uppercase_char(r"\d+\s*"));
        assert!(!pattern_has_uppercase_char(r".*\.rs$"));
        assert!(pattern_has_uppercase_char("(?i)abc"));
        assert!(!pattern_has_uppercase_char("(?i)123"));
        assert!(pattern_has_uppercase_char(r"\p{Lu}"));
        assert!(!pattern_has_uppercase_char("[^a-z]"));
        assert!(pattern_has_uppercase_char("(?i)[^a-z]"));
        assert!(!pattern_has_uppercase_char("(?-u)^[^/]*\\.rs$"));
        assert!(!pattern_has_uppercase_char("(?-u)^[^/]*\\xC3\\xA9[^/]*$"));
        assert!(pattern_has_uppercase_char("(?-u)^[^/]*\\xC3\\x89[^/]*$"));
        assert!(pattern_has_uppercase_char("(?-u)a\\xC3"));
        assert!(pattern_has_uppercase_char("(?:a|B)"));
        assert!(!pattern_has_uppercase_char("A{0}"));
        assert!(!pattern_has_uppercase_char("(unclosed"));
        assert!(!pattern_has_uppercase_char("[A-Z"));
        assert!(!pattern_has_uppercase_char("\u{e9}"));
        assert!(pattern_has_uppercase_char("\u{c9}"));
        assert!(pattern_has_uppercase_char("(?i)\u{e9}"));
        assert!(pattern_has_uppercase_char("[[:upper:]]"));
        assert!(!pattern_has_uppercase_char("(?x) a # B\n"));
        assert!(pattern_has_uppercase_char("(?P<n>X)"));
    }

    #[test]
    fn leading_dot_detection_matches_fd() {
        assert!(pattern_matches_strings_with_leading_dot("^\\.gitignore"));
        assert!(!pattern_matches_strings_with_leading_dot("^.gitignore"));
        assert!(!pattern_matches_strings_with_leading_dot("\\.gitignore"));
        assert!(!pattern_matches_strings_with_leading_dot("^gitignore"));
        assert!(pattern_matches_strings_with_leading_dot("\\A\\.x"));
        assert!(pattern_matches_strings_with_leading_dot("^(?i)\\.git"));
        assert!(pattern_matches_strings_with_leading_dot("^[.]foo"));
        assert!(pattern_matches_strings_with_leading_dot("(?:^\\.foo)"));
        assert!(!pattern_matches_strings_with_leading_dot("(^\\.foo)"));
        assert!(!pattern_matches_strings_with_leading_dot("^\\.foo|bar"));
        assert!(!pattern_matches_strings_with_leading_dot("(?m)^\\.foo"));
        assert!(!pattern_matches_strings_with_leading_dot("^\\.+"));
        assert!(pattern_matches_strings_with_leading_dot("^\\."));
    }
}
