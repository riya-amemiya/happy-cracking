use super::super::locale::{self, CharClass, INVALID_BASE};
use super::ast::{Look, Node, UnitSet};

const RE_DUP_MAX: i64 = 0x7fff;
const BRACKET_NAME_BUF_SIZE: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegError {
    BadPat,
    ECollate,
    ECtype,
    EEscape,
    ESubreg,
    EBrack,
    EParen,
    EBrace,
    BadBr,
    ERange,
    ESize,
    ERParen,
}

impl RegError {
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::BadPat => "Invalid regular expression",
            Self::ECollate => "Invalid collation character",
            Self::ECtype => "Invalid character class name",
            Self::EEscape => "Trailing backslash",
            Self::ESubreg => "Invalid back reference",
            Self::EBrack => "Unmatched [, [^, [:, [., or [=",
            Self::EParen => "Unmatched ( or \\(",
            Self::EBrace => "Unmatched \\{",
            Self::BadBr => "Invalid content of \\{\\}",
            Self::ERange => "Invalid range end",
            Self::ESize => "Regular expression too big",
            Self::ERParen => "Unmatched ) or \\)",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Syntax {
    pub extended: bool,
    pub icase: bool,
    pub utf8: bool,
}

#[derive(Clone, Copy)]
struct PChar {
    up: u32,
    raw: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Character,
    BackSlash,
    Alt,
    BackRef(usize),
    Anchor(Look),
    Word,
    NotWord,
    Space,
    NotSpace,
    OpenSub,
    CloseSub,
    DupPlus,
    DupQuestion,
    DupAsterisk,
    OpenDupNum,
    CloseDupNum,
    OpenBracket,
    Period,
    End,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    c: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BKind {
    Character,
    CloseBracket,
    NonMatchList,
    CharsetRange,
    OpenCollElem,
    OpenEquivClass,
    OpenCharClass,
    End,
}

#[derive(Clone, Copy, Debug)]
struct BToken {
    kind: BKind,
    c: u32,
}

enum Elem {
    SbChar(u32),
    MbChar(u32),
    CollSym(Vec<u8>),
    EquivClass(Vec<u8>),
    CharClass(Vec<u8>),
}

pub struct Parsed {
    pub node: Node,
    pub groups: usize,
    pub byte_mode: bool,
}

struct Parser {
    s: Vec<PChar>,
    pos: usize,
    syn: Syntax,
    nsub: usize,
    completed: u32,
    complex: bool,
}

fn byte(c: u32) -> Option<u8> {
    u8::try_from(c).ok()
}

fn is_ascii(c: u32, b: u8) -> bool {
    c == u32::from(b)
}

pub fn parse(pattern: &[u8], syn: Syntax) -> Result<Parsed, RegError> {
    let mut s = Vec::with_capacity(pattern.len());
    let mut i = 0;
    while i < pattern.len() {
        let (raw, len) = locale::decode(syn.utf8, &pattern[i..]);
        let up = if syn.icase && raw < INVALID_BASE {
            locale::to_upper(syn.utf8, raw)
        } else {
            raw
        };
        s.push(PChar { up, raw });
        i += len;
    }
    let mut p = Parser {
        s,
        pos: 0,
        syn,
        nsub: 0,
        completed: 0,
        complex: false,
    };
    let mut tok = p.fetch_token(true);
    let tree = p.parse_reg_exp(&mut tok, 0)?;
    Ok(Parsed {
        node: tree.unwrap_or(Node::Empty),
        groups: p.nsub,
        byte_mode: !syn.icase && !p.complex,
    })
}

impl Parser {
    fn ere(&self) -> bool {
        self.syn.extended
    }

    fn up(&self, i: usize) -> Option<u32> {
        self.s.get(i).map(|c| c.up)
    }

    fn peek_token(&self, caret_here: bool) -> (Token, usize) {
        self.peek_token_at(self.pos, caret_here)
    }

    fn peek_token_at(&self, pos: usize, caret_here: bool) -> (Token, usize) {
        let Some(c) = self.up(pos) else {
            return (
                Token {
                    kind: Kind::End,
                    c: 0,
                },
                0,
            );
        };
        let ere = self.ere();
        if is_ascii(c, b'\\') {
            if pos + 1 >= self.s.len() {
                return (
                    Token {
                        kind: Kind::BackSlash,
                        c,
                    },
                    1,
                );
            }
            let next = self.s[pos + 1];
            let c2 = if next.raw < 0x80 || next.raw >= INVALID_BASE {
                next.raw
            } else {
                next.up
            };
            let kind = match byte(c2) {
                Some(b'|') if !ere => Kind::Alt,
                Some(d @ b'1'..=b'9') => Kind::BackRef(usize::from(d - b'1')),
                Some(b'<') => Kind::Anchor(Look::WordStart),
                Some(b'>') => Kind::Anchor(Look::WordEnd),
                Some(b'b') => Kind::Anchor(Look::WordBoundary),
                Some(b'B') => Kind::Anchor(Look::NotWordBoundary),
                Some(b'w') => Kind::Word,
                Some(b'W') => Kind::NotWord,
                Some(b's') => Kind::Space,
                Some(b'S') => Kind::NotSpace,
                Some(b'`') => Kind::Anchor(Look::BufStart),
                Some(b'\'') => Kind::Anchor(Look::BufEnd),
                Some(b'(') if !ere => Kind::OpenSub,
                Some(b')') if !ere => Kind::CloseSub,
                Some(b'+') if !ere => Kind::DupPlus,
                Some(b'?') if !ere => Kind::DupQuestion,
                Some(b'{') if !ere => Kind::OpenDupNum,
                Some(b'}') if !ere => Kind::CloseDupNum,
                _ => Kind::Character,
            };
            return (Token { kind, c: c2 }, 2);
        }
        let kind = match byte(c) {
            Some(b'\n') => Kind::Alt,
            Some(b'|') if ere => Kind::Alt,
            Some(b'*') => Kind::DupAsterisk,
            Some(b'+') if ere => Kind::DupPlus,
            Some(b'?') if ere => Kind::DupQuestion,
            Some(b'{') if ere => Kind::OpenDupNum,
            Some(b'}') if ere => Kind::CloseDupNum,
            Some(b'(') if ere => Kind::OpenSub,
            Some(b')') if ere => Kind::CloseSub,
            Some(b'[') => Kind::OpenBracket,
            Some(b'.') => Kind::Period,
            Some(b'^') => {
                if ere
                    || caret_here
                    || pos == 0
                    || self.up(pos - 1).is_some_and(|p| is_ascii(p, b'\n'))
                {
                    Kind::Anchor(Look::LineStart)
                } else {
                    Kind::Character
                }
            }
            Some(b'$') => {
                if ere || pos + 1 == self.s.len() {
                    Kind::Anchor(Look::LineEnd)
                } else {
                    let next = self.peek_token_at(pos + 1, false).0.kind;
                    if matches!(next, Kind::Alt | Kind::CloseSub) {
                        Kind::Anchor(Look::LineEnd)
                    } else {
                        Kind::Character
                    }
                }
            }
            _ => Kind::Character,
        };
        (Token { kind, c }, 1)
    }

    fn fetch_token(&mut self, caret_here: bool) -> Token {
        let (t, len) = self.peek_token(caret_here);
        self.pos += len;
        t
    }

    fn parse_reg_exp(&mut self, tok: &mut Token, nest: usize) -> Result<Option<Node>, RegError> {
        let initial = self.completed;
        let mut branches = vec![self.parse_branch(tok, nest)?];
        while tok.kind == Kind::Alt {
            *tok = self.fetch_token(true);
            let branch = if tok.kind != Kind::Alt
                && tok.kind != Kind::End
                && (nest == 0 || tok.kind != Kind::CloseSub)
            {
                let accumulated = self.completed;
                self.completed = initial;
                let b = self.parse_branch(tok, nest)?;
                self.completed |= accumulated;
                b
            } else {
                None
            };
            branches.push(branch);
        }
        if branches.len() == 1 {
            return Ok(branches.pop().unwrap_or(None));
        }
        Ok(Some(Node::alt(
            branches
                .into_iter()
                .map(|b| b.unwrap_or(Node::Empty))
                .collect(),
        )))
    }

    fn parse_branch(&mut self, tok: &mut Token, nest: usize) -> Result<Option<Node>, RegError> {
        let mut items = Vec::new();
        if let Some(n) = self.parse_expression(tok, nest)? {
            items.push(n);
        }
        while tok.kind != Kind::Alt
            && tok.kind != Kind::End
            && (nest == 0 || tok.kind != Kind::CloseSub)
        {
            if let Some(n) = self.parse_expression(tok, nest)? {
                items.push(n);
            }
        }
        Ok(match items.len() {
            0 => None,
            1 => items.pop(),
            _ => Some(Node::Concat(items)),
        })
    }

    fn char_node(&self, c: u32) -> Node {
        let set = UnitSet::single(c);
        if self.syn.icase {
            Node::Set(set.upper_preimage(self.syn.utf8))
        } else {
            Node::Set(set)
        }
    }

    fn alt_set(&self, set: UnitSet, alt: bool) -> Node {
        match self.icase_set(set) {
            Node::Set(s) => Node::Set(s.with_alt(alt)),
            other => other,
        }
    }

    fn icase_set(&self, set: UnitSet) -> Node {
        if self.syn.icase {
            Node::Set(set.upper_preimage(self.syn.utf8))
        } else {
            Node::Set(set)
        }
    }

    fn class_op(&mut self, class: CharClass, extra: &[u32], negate: bool) -> Node {
        self.complex = true;
        let utf8 = self.syn.utf8;
        let mut set = UnitSet::from_ranges(class.ranges(utf8));
        if !extra.is_empty() {
            set = set.union(&UnitSet::from_units(extra));
        }
        if negate {
            set = set.negate(utf8);
        }
        self.alt_set(set, utf8)
    }

    fn parse_expression(&mut self, tok: &mut Token, nest: usize) -> Result<Option<Node>, RegError> {
        let ere = self.ere();
        let tree: Option<Node> = match tok.kind {
            Kind::Character | Kind::CloseDupNum => Some(self.char_node(tok.c)),
            Kind::OpenSub => self.parse_sub_exp(tok, nest + 1)?,
            Kind::OpenBracket => Some(self.parse_bracket_exp()?),
            Kind::BackRef(idx) => {
                if self.completed & (1 << idx) == 0 {
                    return Err(RegError::ESubreg);
                }
                Some(Node::Backref(idx + 1))
            }
            Kind::OpenDupNum | Kind::DupAsterisk | Kind::DupPlus | Kind::DupQuestion => {
                if ere {
                    *tok = self.fetch_token(false);
                    return self.parse_expression(tok, nest);
                }
                Some(self.char_node(tok.c))
            }
            Kind::CloseSub => {
                if !ere {
                    return Err(RegError::ERParen);
                }
                Some(self.char_node(tok.c))
            }
            Kind::Anchor(look) => {
                if !matches!(
                    look,
                    Look::LineStart | Look::LineEnd | Look::BufStart | Look::BufEnd
                ) {
                    self.complex = true;
                }
                *tok = self.fetch_token(false);
                return Ok(Some(Node::Look(look)));
            }
            Kind::Period => Some(Node::Set(UnitSet::universe(self.syn.utf8))),
            Kind::Word => Some(self.class_op(CharClass::Alnum, &[u32::from(b'_')], false)),
            Kind::NotWord => Some(self.class_op(CharClass::Alnum, &[u32::from(b'_')], true)),
            Kind::Space => Some(self.class_op(CharClass::Space, &[], false)),
            Kind::NotSpace => Some(self.class_op(CharClass::Space, &[], true)),
            Kind::Alt | Kind::End => return Ok(None),
            Kind::BackSlash => return Err(RegError::EEscape),
        };
        *tok = self.fetch_token(false);
        let mut tree = tree;
        while matches!(
            tok.kind,
            Kind::DupAsterisk | Kind::DupPlus | Kind::DupQuestion | Kind::OpenDupNum
        ) {
            match self.parse_dup_op(tree, tok)? {
                DupResult::Tree(t) => tree = t,
                DupResult::Rollback(t) => {
                    tree = t;
                    break;
                }
            }
        }
        Ok(tree)
    }

    fn parse_sub_exp(&mut self, tok: &mut Token, nest: usize) -> Result<Option<Node>, RegError> {
        let cur = self.nsub;
        self.nsub += 1;
        *tok = self.fetch_token(true);
        let inner = if tok.kind == Kind::CloseSub {
            None
        } else {
            let t = self.parse_reg_exp(tok, nest)?;
            if tok.kind != Kind::CloseSub {
                return Err(RegError::EParen);
            }
            t
        };
        if cur <= 8 {
            self.completed |= 1 << cur;
        }
        Ok(Some(Node::Group {
            node: Box::new(inner.unwrap_or(Node::Empty)),
            index: cur + 1,
        }))
    }

    fn fetch_number(&mut self, tok: &mut Token) -> i64 {
        let mut num: i64 = -1;
        loop {
            *tok = self.fetch_token(false);
            let c = tok.c;
            if tok.kind == Kind::End {
                return -2;
            }
            if tok.kind == Kind::CloseDupNum || is_ascii(c, b',') {
                break;
            }
            num = if tok.kind != Kind::Character
                || !(u32::from(b'0')..=u32::from(b'9')).contains(&c)
                || num == -2
            {
                -2
            } else if num == -1 {
                i64::from(c) - i64::from(b'0')
            } else {
                (RE_DUP_MAX + 1).min(num * 10 + i64::from(c) - i64::from(b'0'))
            };
        }
        num
    }

    fn parse_dup_op(&mut self, elem: Option<Node>, tok: &mut Token) -> Result<DupResult, RegError> {
        let start_pos = self.pos;
        let start_tok = *tok;
        let (start, end) = if tok.kind == Kind::OpenDupNum {
            let mut start = self.fetch_number(tok);
            let mut end: i64 = 0;
            if start == -1 {
                if tok.kind == Kind::Character && is_ascii(tok.c, b',') {
                    start = 0;
                } else {
                    return Err(RegError::BadBr);
                }
            }
            if start != -2 {
                end = if tok.kind == Kind::CloseDupNum {
                    start
                } else if tok.kind == Kind::Character && is_ascii(tok.c, b',') {
                    self.fetch_number(tok)
                } else {
                    -2
                };
            }
            if start == -2 || end == -2 {
                if !self.syn.extended {
                    return Err(if tok.kind == Kind::End {
                        RegError::EBrace
                    } else {
                        RegError::BadBr
                    });
                }
                self.pos = start_pos;
                *tok = Token {
                    kind: Kind::Character,
                    c: start_tok.c,
                };
                return Ok(DupResult::Rollback(elem));
            }
            if (end != -1 && start > end) || tok.kind != Kind::CloseDupNum {
                return Err(RegError::BadBr);
            }
            if RE_DUP_MAX < if end == -1 { start } else { end } {
                return Err(RegError::ESize);
            }
            (start, end)
        } else {
            let start = i64::from(tok.kind == Kind::DupPlus);
            let end = if tok.kind == Kind::DupQuestion { 1 } else { -1 };
            (start, end)
        };
        *tok = self.fetch_token(false);
        let Some(elem) = elem else {
            return Ok(DupResult::Tree(None));
        };
        if start == 0 && end == 0 {
            return Ok(DupResult::Tree(None));
        }
        Ok(DupResult::Tree(Some(Node::Repeat {
            node: Box::new(elem),
            min: start as u32,
            max: (end >= 0).then_some(end as u32),
        })))
    }

    fn peek_token_bracket(&mut self) -> (BToken, usize) {
        let Some(c) = self.up(self.pos) else {
            return (
                BToken {
                    kind: BKind::End,
                    c: 0,
                },
                0,
            );
        };
        if is_ascii(c, b'[') {
            let c2 = self.up(self.pos + 1).unwrap_or(0);
            let kind = match byte(c2) {
                Some(b'.') => BKind::OpenCollElem,
                Some(b'=') => BKind::OpenEquivClass,
                Some(b':') => BKind::OpenCharClass,
                _ => {
                    return (
                        BToken {
                            kind: BKind::Character,
                            c,
                        },
                        1,
                    );
                }
            };
            return (BToken { kind, c: c2 }, 2);
        }
        let kind = match byte(c) {
            Some(b']') => BKind::CloseBracket,
            Some(b'^') => BKind::NonMatchList,
            Some(b'-') => {
                let triple = self.pos + 2 < self.s.len()
                    && self.up(self.pos + 1).is_some_and(|d| is_ascii(d, b'-'))
                    && self.up(self.pos + 2).is_some_and(|d| is_ascii(d, b'-'));
                if triple {
                    self.pos += 2;
                    BKind::Character
                } else {
                    BKind::CharsetRange
                }
            }
            _ => BKind::Character,
        };
        (BToken { kind, c }, 1)
    }

    fn is_mb(&self, c: u32) -> bool {
        self.syn.utf8 && (0x80..INVALID_BASE).contains(&c)
    }

    fn parse_bracket_element(
        &mut self,
        token: BToken,
        token_len: usize,
        accept_hyphen: bool,
    ) -> Result<Elem, RegError> {
        if let Some(c) = self.up(self.pos)
            && self.is_mb(c)
        {
            self.pos += 1;
            return Ok(Elem::MbChar(c));
        }
        self.pos += token_len;
        match token.kind {
            BKind::OpenCollElem | BKind::OpenEquivClass | BKind::OpenCharClass => {
                return self.parse_bracket_symbol(token);
            }
            BKind::CharsetRange if !accept_hyphen => {
                let save = self.pos;
                let (t2, _) = self.peek_token_bracket();
                self.pos = save;
                if t2.kind != BKind::CloseBracket {
                    return Err(RegError::ERange);
                }
            }
            _ => {}
        }
        Ok(Elem::SbChar(token.c))
    }

    fn parse_bracket_symbol(&mut self, token: BToken) -> Result<Elem, RegError> {
        let delim = token.c;
        if self.pos >= self.s.len() {
            return Err(RegError::EBrack);
        }
        let mut name: Vec<u8> = Vec::new();
        loop {
            if name.len() >= BRACKET_NAME_BUF_SIZE {
                return Err(RegError::EBrack);
            }
            let pc = self.s[self.pos];
            let ch = if token.kind == BKind::OpenCharClass {
                pc.raw
            } else {
                pc.up
            };
            self.pos += 1;
            if self.pos >= self.s.len() {
                return Err(RegError::EBrack);
            }
            if ch == delim && self.up(self.pos).is_some_and(|d| is_ascii(d, b']')) {
                break;
            }
            locale::encode(self.syn.utf8, ch, &mut name);
            if name.len() > BRACKET_NAME_BUF_SIZE {
                return Err(RegError::EBrack);
            }
        }
        self.pos += 1;
        Ok(match token.kind {
            BKind::OpenCollElem => Elem::CollSym(name),
            BKind::OpenEquivClass => Elem::EquivClass(name),
            _ => Elem::CharClass(name),
        })
    }

    fn parse_byte(&self, b: u32) -> Option<u32> {
        if self.syn.utf8 && b >= 0x80 {
            None
        } else {
            Some(b)
        }
    }

    fn range_endpoint(&self, e: &Elem) -> Result<u32, RegError> {
        match e {
            Elem::EquivClass(_) | Elem::CharClass(_) => Err(RegError::ERange),
            Elem::CollSym(name) if name.len() > 1 => Err(RegError::ECollate),
            Elem::CollSym(name) => self
                .parse_byte(name.first().map_or(0, |&b| u32::from(b)))
                .ok_or(RegError::ECollate),
            Elem::SbChar(c) => {
                let b = if *c >= INVALID_BASE {
                    *c - INVALID_BASE
                } else {
                    *c
                };
                self.parse_byte(b).ok_or(RegError::ECollate)
            }
            Elem::MbChar(c) => Ok(*c),
        }
    }

    fn add_single_byte(&self, ranges: &mut Vec<(u32, u32)>, b: u32) {
        if self.syn.utf8 {
            if b < 0x80 {
                ranges.push((b, b));
            }
        } else {
            let v = if b >= INVALID_BASE {
                b - INVALID_BASE
            } else {
                b
            };
            ranges.push((v, v));
        }
    }

    fn parse_bracket_exp(&mut self) -> Result<Node, RegError> {
        let utf8 = self.syn.utf8;
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        let mut complex_part = false;
        let (mut token, mut token_len) = self.peek_token_bracket();
        if token.kind == BKind::End {
            return Err(RegError::BadPat);
        }
        let mut non_match = false;
        if token.kind == BKind::NonMatchList {
            non_match = true;
            self.pos += token_len;
            (token, token_len) = self.peek_token_bracket();
            if token.kind == BKind::End {
                return Err(RegError::BadPat);
            }
        }
        if token.kind == BKind::CloseBracket {
            token.kind = BKind::Character;
        }
        let mut first_round = true;
        loop {
            let start_elem = self.parse_bracket_element(token, token_len, first_round)?;
            first_round = false;
            (token, token_len) = self.peek_token_bracket();
            let mut range_end: Option<Elem> = None;
            if !matches!(start_elem, Elem::CharClass(_) | Elem::EquivClass(_)) {
                if token.kind == BKind::End {
                    return Err(RegError::EBrack);
                }
                if token.kind == BKind::CharsetRange {
                    self.pos += token_len;
                    let (token2, token_len2) = self.peek_token_bracket();
                    if token2.kind == BKind::End {
                        return Err(RegError::EBrack);
                    }
                    if token2.kind == BKind::CloseBracket {
                        self.pos -= token_len;
                        token.kind = BKind::Character;
                    } else {
                        range_end = Some(self.parse_bracket_element(token2, token_len2, true)?);
                        (token, token_len) = self.peek_token_bracket();
                    }
                }
            }
            if let Some(end_elem) = range_end {
                let lo = self.range_endpoint(&start_elem)?;
                let hi = self.range_endpoint(&end_elem)?;
                if lo > hi {
                    return Err(RegError::ERange);
                }
                ranges.push((lo, hi));
                complex_part = true;
                self.complex = true;
            } else {
                match start_elem {
                    Elem::SbChar(c) => self.add_single_byte(&mut ranges, c),
                    Elem::MbChar(c) => {
                        complex_part = true;
                        self.complex = true;
                        ranges.push((c, c));
                    }
                    Elem::EquivClass(name) | Elem::CollSym(name) => {
                        if name.len() != 1 {
                            return Err(RegError::ECollate);
                        }
                        self.add_single_byte(&mut ranges, u32::from(name[0]));
                    }
                    Elem::CharClass(name) => {
                        let name: &[u8] = if self.syn.icase
                            && (name.as_slice() == b"upper" || name.as_slice() == b"lower")
                        {
                            b"alpha"
                        } else {
                            &name
                        };
                        let class = CharClass::from_name(name).ok_or(RegError::ECtype)?;
                        complex_part = true;
                        self.complex = true;
                        ranges.extend(class.ranges(utf8));
                    }
                }
            }
            if token.kind == BKind::End {
                return Err(RegError::EBrack);
            }
            if token.kind == BKind::CloseBracket {
                break;
            }
        }
        self.pos += token_len;
        let mut set = UnitSet::from_ranges(ranges);
        if non_match {
            complex_part = true;
            self.complex = true;
            set = set.negate(utf8);
        }
        let sb_part = set.ranges().first().is_some_and(|&(lo, _)| lo < 0x80);
        Ok(self.alt_set(set, utf8 && complex_part && sb_part))
    }
}

enum DupResult {
    Tree(Option<Node>),
    Rollback(Option<Node>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(p: &str, extended: bool) -> Option<&'static str> {
        parse(
            p.as_bytes(),
            Syntax {
                extended,
                icase: false,
                utf8: true,
            },
        )
        .err()
        .map(RegError::message)
    }

    #[test]
    fn gnu_error_messages() {
        assert_eq!(err("a\\(", false), Some("Unmatched ( or \\("));
        assert_eq!(err("a(", true), Some("Unmatched ( or \\("));
        assert_eq!(err(")", true), None);
        assert_eq!(err("a\\{1", false), Some("Unmatched \\{"));
        assert_eq!(err("a{1", true), None);
        assert_eq!(err("a{2,1}", true), Some("Invalid content of \\{\\}"));
        assert_eq!(err("a\\)", false), Some("Unmatched ) or \\)"));
        assert_eq!(err("[", false), Some("Invalid regular expression"));
        assert_eq!(err("[a", false), Some("Unmatched [, [^, [:, [., or [="));
        assert_eq!(
            err("[[:foo:]]", false),
            Some("Invalid character class name")
        );
        assert_eq!(err("[z-a]", false), Some("Invalid range end"));
        assert_eq!(err("a\\", false), Some("Trailing backslash"));
        assert_eq!(err("\\(a\\)\\2", false), Some("Invalid back reference"));
        assert_eq!(err("a{32768}", true), Some("Regular expression too big"));
        assert_eq!(err("[[=ab=]]", false), Some("Invalid collation character"));
        assert_eq!(
            err("[[=\u{e9}=]]", false),
            Some("Invalid collation character")
        );
        assert_eq!(err("(*)", true), Some("Unmatched ( or \\("));
        assert_eq!(err("a{}", true), Some("Invalid content of \\{\\}"));
        assert_eq!(err("a{1,2,3}", true), Some("Invalid content of \\{\\}"));
        assert_eq!(err("\\(a\\)\\|\\1", false), Some("Invalid back reference"));
    }
}
