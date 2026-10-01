use super::super::locale::{self, CharClass};
use super::ast::{Look, Node, UnitSet, dfa_fold_char};

const RE_DUP_MAX: i32 = 0x7fff;
const NOTCHAR: i32 = 256;
const EOF: i32 = -1;

#[derive(Clone, Copy, Debug)]
pub struct DfaSyntax {
    pub extended: bool,
    pub icase: bool,
    pub utf8: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    End,
    Backref,
    BegLine,
    EndLine,
    BegWord,
    EndWord,
    LimWord,
    NotLimWord,
    QMark,
    Star,
    Plus,
    Repmn,
    Or,
    LParen,
    RParen,
    AnyChar,
    WChar,
    Set(UnitSet),
}

pub struct DfaParse {
    pub warnings: Vec<String>,
    pub result: Result<Node, String>,
}

struct Lexer<'a> {
    s: &'a [u8],
    pos: usize,
    syn: DfaSyntax,
    lasttok: Tok,
    parens: usize,
    minrep: i32,
    maxrep: i32,
    wctok: Option<u32>,
    laststart: bool,
    warnings: Vec<String>,
    tok: Tok,
}

pub fn parse(pattern: &[u8], syn: DfaSyntax) -> DfaParse {
    let mut lx = Lexer {
        s: pattern,
        pos: 0,
        syn,
        lasttok: Tok::End,
        parens: 0,
        minrep: -1,
        maxrep: -1,
        wctok: None,
        laststart: true,
        warnings: Vec::new(),
        tok: Tok::End,
    };
    let result = lx.run();
    DfaParse {
        warnings: lx.warnings,
        result,
    }
}

impl Lexer<'_> {
    fn left(&self) -> usize {
        self.s.len() - self.pos
    }

    fn ere(&self) -> bool {
        self.syn.extended
    }

    fn fetch_wc(&mut self) -> i32 {
        let (unit, len) = locale::decode(self.syn.utf8, &self.s[self.pos..]);
        self.pos += len;
        if unit >= locale::INVALID_BASE {
            self.wctok = None;
            return (unit - locale::INVALID_BASE) as i32;
        }
        self.wctok = Some(unit);
        if len == 1 { unit as i32 } else { EOF }
    }

    fn bracket_fetch_wc(&mut self) -> Result<i32, String> {
        if self.left() == 0 {
            return Err("unbalanced [".into());
        }
        Ok(self.fetch_wc())
    }

    fn warn(&mut self, msg: &str) {
        self.warnings.push(format!("warning: {msg}"));
    }

    fn set_tok(&mut self, t: Tok) -> Tok {
        self.lasttok = t.clone();
        t
    }

    fn run(&mut self) -> Result<Node, String> {
        self.tok = self.lex()?;
        let node = self.regexp()?;
        if self.tok != Tok::End {
            return Err("unbalanced )".into());
        }
        Ok(node)
    }

    fn fold_byte(&self, c: u32) -> UnitSet {
        if self.syn.icase && CharClass::Alpha.contains(false, c) {
            UnitSet::from_units(&dfa_fold_char(false, c))
        } else {
            UnitSet::single(c)
        }
    }

    fn parse_bracket_exp(&mut self) -> Result<Tok, String> {
        let utf8 = self.syn.utf8;
        let mut known = true;
        let mut ccl: Vec<(u32, u32)> = Vec::new();
        let mut chars: Vec<u32> = Vec::new();
        let mut c = self.bracket_fetch_wc()?;
        let invert = c == i32::from(b'^');
        if invert {
            c = self.bracket_fetch_wc()?;
            known = !utf8;
        }
        let mut wc = self.wctok;
        let mut c1: i32;
        let mut wc1: Option<u32> = None;
        let mut colon_warning_state = i32::from(c == i32::from(b':'));
        loop {
            c1 = NOTCHAR;
            colon_warning_state &= !2;
            let mut skip_to_next = false;
            if c == i32::from(b'[') {
                c1 = self.bracket_fetch_wc()?;
                wc1 = self.wctok;
                if c1 == i32::from(b':') || c1 == i32::from(b'.') || c1 == i32::from(b'=') {
                    let mut name: Vec<u8> = Vec::new();
                    let mut invalid = false;
                    loop {
                        c = self.bracket_fetch_wc()?;
                        if self.left() == 0 || (c == c1 && self.s.get(self.pos) == Some(&b']')) {
                            break;
                        }
                        if name.len() < 32 {
                            name.push(c as u8);
                        } else {
                            invalid = true;
                        }
                    }
                    c = self.bracket_fetch_wc()?;
                    wc = self.wctok;
                    if c1 == i32::from(b':') {
                        let lookup: &[u8] = if self.syn.icase
                            && (name.as_slice() == b"upper" || name.as_slice() == b"lower")
                        {
                            b"alpha"
                        } else {
                            &name
                        };
                        let class = if invalid {
                            None
                        } else {
                            CharClass::from_name(lookup)
                        };
                        let Some(class) = class else {
                            return Err("invalid character class".into());
                        };
                        if utf8 && class != CharClass::Digit {
                            known = false;
                        } else {
                            ccl.extend(class.ranges(false));
                        }
                    } else {
                        known = false;
                    }
                    colon_warning_state |= 8;
                    c1 = self.bracket_fetch_wc()?;
                    wc1 = self.wctok;
                    skip_to_next = true;
                }
            }
            if !skip_to_next {
                if c1 == NOTCHAR {
                    c1 = self.bracket_fetch_wc()?;
                    wc1 = self.wctok;
                }
                let mut handled = false;
                if c1 == i32::from(b'-') {
                    let mut c2 = self.bracket_fetch_wc()?;
                    let wc2 = self.wctok;
                    if c2 == i32::from(b'[') && self.s.get(self.pos) == Some(&b'.') {
                        known = false;
                        c2 = i32::from(b']');
                    }
                    if c2 == i32::from(b']') {
                        self.pos -= 1;
                    } else {
                        colon_warning_state |= 8;
                        c1 = self.bracket_fetch_wc()?;
                        wc1 = self.wctok;
                        if wc != wc2 || wc.is_none() {
                            let digit = |x: i32| (i32::from(b'0')..=i32::from(b'9')).contains(&x);
                            if !utf8 || (digit(c) && digit(c2)) {
                                if c >= 0 && c2 >= c {
                                    for ci in c..=c2 {
                                        let ci = ci as u32;
                                        if self.syn.icase && CharClass::Alpha.contains(false, ci) {
                                            ccl.extend(
                                                dfa_fold_char(false, ci)
                                                    .into_iter()
                                                    .map(|f| (f, f)),
                                            );
                                        } else {
                                            ccl.push((ci, ci));
                                        }
                                    }
                                }
                            } else {
                                known = false;
                            }
                            handled = true;
                        }
                    }
                }
                if !handled {
                    colon_warning_state |= if c == i32::from(b':') { 2 } else { 4 };
                    if utf8 {
                        match wc {
                            None => known = false,
                            Some(w) => {
                                if self.syn.icase {
                                    chars.extend(dfa_fold_char(true, w));
                                } else {
                                    chars.push(w);
                                }
                            }
                        }
                    } else if c >= 0 {
                        let set = self.fold_byte(c as u32);
                        ccl.extend_from_slice(set.ranges());
                    }
                }
            }
            wc = wc1;
            c = c1;
            if c == i32::from(b']') {
                break;
            }
        }
        if colon_warning_state == 7 {
            return Err("character class syntax is [[:space:]], not [:space:]".into());
        }
        if !known {
            return Ok(Tok::Backref);
        }
        ccl.extend(chars.into_iter().map(|w| (w, w)));
        let mut set = UnitSet::from_ranges(ccl);
        if invert {
            set = set.negate(utf8);
        }
        Ok(Tok::Set(set))
    }

    fn stray(&mut self) {
        let msg = match self.wctok {
            Some(w) if locale::is_print(self.syn.utf8, w) => {
                if locale::is_space(self.syn.utf8, w) {
                    "stray \\ before white space".to_string()
                } else {
                    let mut buf = Vec::new();
                    locale::encode(self.syn.utf8, w, &mut buf);
                    format!("stray \\ before {}", String::from_utf8_lossy(&buf))
                }
            }
            _ => "stray \\ before unprintable character".to_string(),
        };
        self.warn(&msg);
    }

    fn normal_char(&mut self, c: i32) -> Tok {
        self.laststart = false;
        if self.syn.utf8 {
            return self.set_tok(Tok::WChar);
        }
        let set = self.fold_byte(c as u32);
        self.set_tok(Tok::Set(set))
    }

    fn lex(&mut self) -> Result<Tok, String> {
        let mut backslash = false;
        loop {
            if self.left() == 0 {
                return Ok(self.set_tok(Tok::End));
            }
            let c = self.fetch_wc();
            let b = u8::try_from(c).ok();
            let ere = self.ere();
            match b {
                Some(b'\\') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    if self.left() == 0 {
                        return Err("unfinished \\ escape".into());
                    }
                    backslash = true;
                }
                Some(b'^') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    if ere || matches!(self.lasttok, Tok::End | Tok::LParen | Tok::Or) {
                        return Ok(self.set_tok(Tok::BegLine));
                    }
                    return Ok(self.normal_char(c));
                }
                Some(b'$') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    let left = self.left();
                    let rest = &self.s[self.pos..];
                    let bk = usize::from(!ere);
                    let idx = bk & usize::from(rest.first() == Some(&b'\\'));
                    if ere
                        || left == 0
                        || (left > bk && rest.get(idx) == Some(&b')'))
                        || (left > bk && rest.get(idx) == Some(&b'|'))
                        || rest.first() == Some(&b'\n')
                    {
                        return Ok(self.set_tok(Tok::EndLine));
                    }
                    return Ok(self.normal_char(c));
                }
                Some(b'1'..=b'9') => {
                    if !backslash {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = false;
                    return Ok(self.set_tok(Tok::Backref));
                }
                Some(d @ (b'`' | b'\'' | b'<' | b'>' | b'b' | b'B')) => {
                    if !backslash {
                        return Ok(self.normal_char(c));
                    }
                    let t = match d {
                        b'`' => Tok::BegLine,
                        b'\'' => Tok::EndLine,
                        b'<' => Tok::BegWord,
                        b'>' => Tok::EndWord,
                        b'b' => Tok::LimWord,
                        _ => Tok::NotLimWord,
                    };
                    return Ok(self.set_tok(t));
                }
                Some(d @ (b'?' | b'+')) => {
                    if backslash == ere {
                        return Ok(self.normal_char(c));
                    }
                    if self.laststart {
                        if !ere {
                            return Ok(self.default_case(c, backslash));
                        }
                        self.warn(if d == b'?' {
                            "? at start of expression"
                        } else {
                            "+ at start of expression"
                        });
                    }
                    let t = if d == b'?' { Tok::QMark } else { Tok::Plus };
                    return Ok(self.set_tok(t));
                }
                Some(b'*') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    if self.laststart {
                        if !ere {
                            return Ok(self.default_case(c, backslash));
                        }
                        self.warn("* at start of expression");
                    }
                    return Ok(self.set_tok(Tok::Star));
                }
                Some(b'{') => {
                    if backslash == ere {
                        return Ok(self.normal_char(c));
                    }
                    return self.interval(c, backslash);
                }
                Some(b'|') => {
                    if backslash == ere {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = true;
                    return Ok(self.set_tok(Tok::Or));
                }
                Some(b'\n') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = true;
                    return Ok(self.set_tok(Tok::Or));
                }
                Some(b'(') => {
                    if backslash == ere {
                        return Ok(self.normal_char(c));
                    }
                    self.parens += 1;
                    self.laststart = true;
                    return Ok(self.set_tok(Tok::LParen));
                }
                Some(b')') => {
                    if backslash == ere {
                        return Ok(self.normal_char(c));
                    }
                    if self.parens == 0 && ere {
                        return Ok(self.normal_char(c));
                    }
                    self.parens = self.parens.saturating_sub(1);
                    self.laststart = false;
                    return Ok(self.set_tok(Tok::RParen));
                }
                Some(b'.') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = false;
                    if self.syn.utf8 {
                        return Ok(self.set_tok(Tok::AnyChar));
                    }
                    return Ok(self.set_tok(Tok::Set(UnitSet::universe(false))));
                }
                Some(d @ (b's' | b'S' | b'w' | b'W')) => {
                    if !backslash {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = false;
                    if self.syn.utf8 {
                        return Ok(self.set_tok(Tok::Backref));
                    }
                    let mut set = if d == b's' || d == b'S' {
                        UnitSet::from_ranges(CharClass::Space.ranges(false))
                    } else {
                        UnitSet::from_ranges(CharClass::Alnum.ranges(false))
                            .union(&UnitSet::single(u32::from(b'_')))
                    };
                    if d == b'S' || d == b'W' {
                        set = set.negate(false);
                    }
                    return Ok(self.set_tok(Tok::Set(set)));
                }
                Some(b'[') => {
                    if backslash {
                        return Ok(self.normal_char(c));
                    }
                    self.laststart = false;
                    let t = self.parse_bracket_exp()?;
                    return Ok(self.set_tok(t));
                }
                Some(b']' | b'}') => return Ok(self.normal_char(c)),
                _ => return Ok(self.default_case(c, backslash)),
            }
        }
    }

    fn default_case(&mut self, c: i32, backslash: bool) -> Tok {
        if backslash {
            self.stray();
        }
        self.normal_char(c)
    }

    fn interval(&mut self, c: i32, backslash: bool) -> Result<Tok, String> {
        let s = self.s;
        let lim = s.len();
        let mut p = self.pos;
        self.minrep = -1;
        self.maxrep = -1;
        let digit = |b: u8| b.is_ascii_digit();
        while p != lim && digit(s[p]) {
            let d = i32::from(s[p] - b'0');
            self.minrep = if self.minrep < 0 {
                d
            } else {
                (RE_DUP_MAX + 1).min(self.minrep * 10 + d)
            };
            p += 1;
        }
        if p != lim {
            if s[p] == b',' {
                if self.minrep < 0 {
                    self.minrep = 0;
                }
                loop {
                    p += 1;
                    if p == lim || !digit(s[p]) {
                        break;
                    }
                    let d = i32::from(s[p] - b'0');
                    self.maxrep = if self.maxrep < 0 {
                        d
                    } else {
                        (RE_DUP_MAX + 1).min(self.maxrep * 10 + d)
                    };
                }
            } else {
                self.maxrep = self.minrep;
            }
        }
        let mut ok = true;
        if backslash {
            if p != lim && s[p] == b'\\' {
                p += 1;
            } else {
                ok = false;
                if p != lim {
                    p += 1;
                }
            }
        }
        if ok {
            if p != lim && s[p] == b'}' {
                p += 1;
            } else {
                ok = false;
                if p != lim {
                    p += 1;
                }
            }
        }
        let invalid_content =
            !(ok && self.minrep >= 0 && (self.maxrep < 0 || self.minrep <= self.maxrep));
        if invalid_content && self.ere() {
            return Ok(self.normal_char(c));
        }
        if self.laststart {
            if !self.ere() {
                return Ok(self.default_case(c, backslash));
            }
            self.warn("{...} at start of expression");
        }
        if invalid_content {
            return Err("invalid content of \\{\\}".into());
        }
        if RE_DUP_MAX < self.maxrep {
            return Err("regular expression too big".into());
        }
        self.pos = p;
        self.laststart = false;
        Ok(self.set_tok(Tok::Repmn))
    }

    fn atom(&mut self) -> Result<Node, String> {
        let tok = self.tok.clone();
        let node = match tok {
            Tok::Set(set) => Node::Set(set),
            Tok::Backref => Node::Unsupported,
            Tok::BegLine => Node::Look(Look::LineStart),
            Tok::EndLine => Node::Look(Look::LineEnd),
            Tok::BegWord => Node::Look(Look::WordStart),
            Tok::EndWord => Node::Look(Look::WordEnd),
            Tok::LimWord => Node::Look(Look::WordBoundary),
            Tok::NotLimWord => Node::Look(Look::NotWordBoundary),
            Tok::AnyChar => Node::Set(UnitSet::universe(true)),
            Tok::WChar => match self.wctok {
                None => Node::Unsupported,
                Some(w) => {
                    if self.syn.icase {
                        Node::Set(UnitSet::from_units(&dfa_fold_char(true, w)))
                    } else {
                        Node::Set(UnitSet::single(w))
                    }
                }
            },
            Tok::LParen => {
                self.tok = self.lex()?;
                let inner = self.regexp()?;
                if self.tok != Tok::RParen {
                    return Err("unbalanced (".into());
                }
                self.tok = self.lex()?;
                return Ok(Node::Group {
                    node: Box::new(inner),
                    index: 0,
                });
            }
            _ => return Ok(Node::Empty),
        };
        self.tok = self.lex()?;
        Ok(node)
    }

    fn closure(&mut self) -> Result<Node, String> {
        let mut node = self.atom()?;
        loop {
            match self.tok {
                Tok::Repmn if self.minrep != 0 || self.maxrep != 0 => {
                    node = Node::Repeat {
                        node: Box::new(node),
                        min: self.minrep.max(0) as u32,
                        max: (self.maxrep >= 0).then_some(self.maxrep as u32),
                    };
                    self.tok = self.lex()?;
                }
                Tok::Repmn => {
                    self.tok = self.lex()?;
                    node = self.closure()?;
                }
                Tok::QMark | Tok::Star | Tok::Plus => {
                    let (min, max) = match self.tok {
                        Tok::QMark => (0, Some(1)),
                        Tok::Star => (0, None),
                        _ => (1, None),
                    };
                    node = Node::Repeat {
                        node: Box::new(node),
                        min,
                        max,
                    };
                    self.tok = self.lex()?;
                }
                _ => return Ok(node),
            }
        }
    }

    fn branch(&mut self) -> Result<Node, String> {
        let mut items = vec![self.closure()?];
        while !matches!(self.tok, Tok::RParen | Tok::Or | Tok::End) {
            items.push(self.closure()?);
        }
        Ok(Node::concat(items))
    }

    fn regexp(&mut self) -> Result<Node, String> {
        let mut alts = vec![self.branch()?];
        while self.tok == Tok::Or {
            self.tok = self.lex()?;
            alts.push(self.branch()?);
        }
        Ok(Node::alt(alts))
    }
}

#[must_use]
pub fn supported(node: &Node, utf8: bool) -> bool {
    !(node.any(&|n| matches!(n, Node::Unsupported | Node::Backref(_)))
        || utf8 && node.has_word_look())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warnings(p: &str, extended: bool) -> (Vec<String>, Option<String>) {
        let r = parse(
            p.as_bytes(),
            DfaSyntax {
                extended,
                icase: false,
                utf8: true,
            },
        );
        (r.warnings, r.result.err())
    }

    #[test]
    fn gnu_warnings() {
        assert_eq!(
            warnings("*a", true).0,
            vec!["warning: * at start of expression"]
        );
        assert_eq!(warnings("*a", false).0, Vec::<String>::new());
        assert_eq!(warnings("\\d", false).0, vec!["warning: stray \\ before d"]);
        assert_eq!(
            warnings("a\\ b", false).0,
            vec!["warning: stray \\ before white space"]
        );
        assert_eq!(warnings("\\{", false).0, vec!["warning: stray \\ before {"]);
        assert_eq!(warnings("\\{", true).0, Vec::<String>::new());
        assert_eq!(
            warnings("{1}a", true).0,
            vec!["warning: {...} at start of expression"]
        );
        assert_eq!(
            warnings("[:space:]", false).1.as_deref(),
            Some("character class syntax is [[:space:]], not [:space:]")
        );
        assert_eq!(
            warnings("\\\u{e9}", false).0,
            vec!["warning: stray \\ before \u{e9}"]
        );
        let (w, e) = warnings("{99999}a", true);
        assert_eq!(w, vec!["warning: {...} at start of expression"]);
        assert_eq!(e.as_deref(), Some("regular expression too big"));
    }
}
