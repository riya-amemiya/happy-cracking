#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct FlagSet {
    pub(super) on: u8,
    pub(super) off: u8,
}

pub(super) const FLAG_I: u8 = 1;
pub(super) const FLAG_M: u8 = 2;
pub(super) const FLAG_S: u8 = 4;
pub(super) const FLAG_SWAP: u8 = 8;
pub(super) const FLAG_U: u8 = 16;
pub(super) const FLAG_X: u8 = 32;
pub(super) const FLAG_R: u8 = 64;

impl FlagSet {
    pub(super) fn apply(self, current: u8) -> u8 {
        (current | self.on) & !self.off
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AssertKind {
    StartLine,
    EndLine,
    StartText,
    EndText,
    WordBoundary,
    NotWordBoundary,
    WordStart,
    WordEnd,
    WordStartHalf,
    WordEndHalf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PerlKind {
    Digit,
    Space,
    Word,
}

#[derive(Clone, Debug)]
pub(super) struct Lit {
    pub(super) span: (usize, usize),
    pub(super) c: char,
    pub(super) hex_byte: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SetOp {
    Intersection,
    Difference,
    SymmetricDifference,
}

#[derive(Clone, Debug)]
pub(super) enum ClassItem {
    Literal(Lit),
    Range(Lit, Lit),
    Ascii { name: &'static str, negated: bool },
    Unicode { text: String, negated: bool },
    Perl { kind: PerlKind, negated: bool },
    Bracketed { negated: bool, set: Box<ClassSet> },
    Union(Vec<ClassItem>),
}

#[derive(Clone, Debug)]
pub(super) enum ClassSet {
    Item(ClassItem),
    BinaryOp(SetOp, Box<ClassSet>, Box<ClassSet>),
}

#[derive(Clone, Debug)]
pub(super) enum ClassAst {
    Perl { kind: PerlKind, negated: bool },
    Unicode { text: String, negated: bool },
    Bracketed { negated: bool, set: ClassSet },
}

#[derive(Clone, Debug)]
pub(super) enum GroupKind {
    Capture,
    NonCapture(FlagSet),
}

#[derive(Clone, Debug)]
pub(super) enum Ast {
    Empty,
    Flags(FlagSet),
    Literal(Lit),
    Dot((usize, usize)),
    Assertion(AssertKind),
    Class {
        span: (usize, usize),
        class: ClassAst,
    },
    Repetition {
        min: u32,
        max: Option<u32>,
        ast: Box<Ast>,
    },
    Group {
        kind: GroupKind,
        ast: Box<Ast>,
    },
    Alternation(Vec<Ast>),
    Concat(Vec<Ast>),
}

pub(super) struct ParseError;

pub(super) struct Parser<'p> {
    pattern: &'p str,
    chars: Vec<(usize, char)>,
    pos: usize,
    octal: bool,
    ignore_ws: bool,
    depth: u32,
}

const ASCII_CLASSES: &[&str] = &[
    "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print", "punct",
    "space", "upper", "word", "xdigit",
];

fn is_meta(c: char) -> bool {
    matches!(
        c,
        '\\' | '.'
            | '+'
            | '*'
            | '?'
            | '('
            | ')'
            | '|'
            | '['
            | ']'
            | '{'
            | '}'
            | '^'
            | '$'
            | '#'
            | '&'
            | '-'
            | '~'
    )
}

pub(super) fn is_meta_character(c: char) -> bool {
    is_meta(c)
}

impl<'p> Parser<'p> {
    pub(super) fn new(pattern: &'p str, octal: bool, ignore_ws: bool) -> Parser<'p> {
        Parser {
            pattern,
            chars: pattern.char_indices().collect(),
            pos: 0,
            octal,
            ignore_ws,
            depth: 0,
        }
    }

    pub(super) fn parse(mut self) -> Result<Ast, ParseError> {
        let ast = self.parse_alternation()?;
        if self.pos != self.chars.len() {
            return Err(ParseError);
        }
        Ok(ast)
    }

    fn offset(&self) -> usize {
        self.chars
            .get(self.pos)
            .map_or(self.pattern.len(), |&(i, _)| i)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).map(|&(_, c)| c)
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).map(|&(_, c)| c)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        if self.bump() == Some(c) {
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn rest_starts_with(&self, s: &str) -> bool {
        self.pattern[self.offset()..].starts_with(s)
    }

    fn skip_space(&mut self) {
        if !self.ignore_ws {
            return;
        }
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.pos += 1;
            } else if c == '#' {
                while let Some(c) = self.bump() {
                    if c == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn parse_alternation(&mut self) -> Result<Ast, ParseError> {
        let mut alts = vec![self.parse_concat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            alts.push(self.parse_concat()?);
        }
        Ok(if alts.len() == 1 {
            alts.pop().unwrap()
        } else {
            Ast::Alternation(alts)
        })
    }

    fn parse_concat(&mut self) -> Result<Ast, ParseError> {
        let mut items: Vec<Ast> = vec![];
        loop {
            self.skip_space();
            let Some(c) = self.peek() else { break };
            match c {
                '|' | ')' => break,
                '(' => {
                    let ast = self.parse_group()?;
                    items.push(ast);
                }
                '[' => {
                    let start = self.offset();
                    self.pos += 1;
                    let (negated, set) = self.parse_bracketed_body()?;
                    items.push(Ast::Class {
                        span: (start, self.offset()),
                        class: ClassAst::Bracketed { negated, set },
                    });
                }
                '.' => {
                    let start = self.offset();
                    self.pos += 1;
                    items.push(Ast::Dot((start, self.offset())));
                }
                '^' => {
                    self.pos += 1;
                    items.push(Ast::Assertion(AssertKind::StartLine));
                }
                '$' => {
                    self.pos += 1;
                    items.push(Ast::Assertion(AssertKind::EndLine));
                }
                '\\' => {
                    let ast = self.parse_escape_primitive()?;
                    items.push(ast);
                }
                '*' | '+' | '?' => {
                    self.pos += 1;
                    let (min, max) = match c {
                        '*' => (0, None),
                        '+' => (1, None),
                        _ => (0, Some(1)),
                    };
                    self.bump_if('?');
                    let sub = items.pop().ok_or(ParseError)?;
                    items.push(Ast::Repetition {
                        min,
                        max,
                        ast: Box::new(sub),
                    });
                }
                '{' => {
                    let (min, max) = self.parse_counted()?;
                    self.bump_if('?');
                    let sub = items.pop().ok_or(ParseError)?;
                    items.push(Ast::Repetition {
                        min,
                        max,
                        ast: Box::new(sub),
                    });
                }
                _ => {
                    let start = self.offset();
                    self.pos += 1;
                    items.push(Ast::Literal(Lit {
                        span: (start, self.offset()),
                        c,
                        hex_byte: false,
                    }));
                }
            }
        }
        Ok(match items.len() {
            0 => Ast::Empty,
            1 => items.pop().unwrap(),
            _ => Ast::Concat(items),
        })
    }

    fn bump_if(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_decimal(&mut self) -> Option<u32> {
        self.skip_space();
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if start == self.pos {
            return None;
        }
        let text: String = self.chars[start..self.pos]
            .iter()
            .map(|&(_, c)| c)
            .collect();
        self.skip_space();
        text.parse().ok()
    }

    fn parse_counted(&mut self) -> Result<(u32, Option<u32>), ParseError> {
        self.expect('{')?;
        let min = self.parse_decimal();
        if self.bump_if(',') {
            let max = self.parse_decimal();
            self.expect('}')?;
            Ok((min.unwrap_or(0), max))
        } else {
            self.expect('}')?;
            let n = min.ok_or(ParseError)?;
            Ok((n, Some(n)))
        }
    }

    fn parse_flag_chars(&mut self) -> Result<FlagSet, ParseError> {
        let mut set = FlagSet::default();
        let mut negate = false;
        loop {
            let Some(c) = self.peek() else {
                return Err(ParseError);
            };
            let bit = match c {
                ':' | ')' => break,
                '-' => {
                    negate = true;
                    self.pos += 1;
                    continue;
                }
                'i' => FLAG_I,
                'm' => FLAG_M,
                's' => FLAG_S,
                'U' => FLAG_SWAP,
                'u' => FLAG_U,
                'x' => FLAG_X,
                'R' => FLAG_R,
                _ => return Err(ParseError),
            };
            self.pos += 1;
            if negate {
                set.off |= bit;
                set.on &= !bit;
            } else {
                set.on |= bit;
                set.off &= !bit;
            }
        }
        Ok(set)
    }

    fn parse_group(&mut self) -> Result<Ast, ParseError> {
        self.expect('(')?;
        let saved_ws = self.ignore_ws;
        let kind = if self.peek() == Some('?') {
            self.pos += 1;
            if self.rest_starts_with("P<") || self.rest_starts_with("<") {
                if self.peek() == Some('P') {
                    self.pos += 1;
                }
                self.pos += 1;
                while let Some(c) = self.bump() {
                    if c == '>' {
                        break;
                    }
                }
                GroupKind::Capture
            } else {
                let set = self.parse_flag_chars()?;
                let new_ws = set.apply(if self.ignore_ws { FLAG_X } else { 0 }) & FLAG_X != 0;
                if self.bump_if(')') {
                    self.ignore_ws = new_ws;
                    return Ok(Ast::Flags(set));
                }
                self.expect(':')?;
                self.ignore_ws = new_ws;
                GroupKind::NonCapture(set)
            }
        } else {
            GroupKind::Capture
        };
        self.depth += 1;
        if self.depth > 10_000 {
            return Err(ParseError);
        }
        let ast = self.parse_alternation()?;
        self.depth -= 1;
        self.expect(')')?;
        self.ignore_ws = saved_ws;
        Ok(Ast::Group {
            kind,
            ast: Box::new(ast),
        })
    }

    fn parse_hex(&mut self, width: usize) -> Result<char, ParseError> {
        let value = if self.bump_if('{') {
            let mut text = String::new();
            loop {
                match self.bump() {
                    Some('}') => break,
                    Some(c) => text.push(c),
                    None => return Err(ParseError),
                }
            }
            u32::from_str_radix(text.trim(), 16).map_err(|_| ParseError)?
        } else {
            let mut text = String::new();
            for _ in 0..width {
                text.push(self.bump().ok_or(ParseError)?);
            }
            u32::from_str_radix(&text, 16).map_err(|_| ParseError)?
        };
        char::from_u32(value).ok_or(ParseError)
    }

    fn parse_unicode_class_text(&mut self, start: usize) -> Result<(), ParseError> {
        if self.bump_if('{') {
            loop {
                match self.bump() {
                    Some('}') => break,
                    Some(_) => {}
                    None => return Err(ParseError),
                }
            }
        } else {
            self.bump().ok_or(ParseError)?;
        }
        let _ = start;
        Ok(())
    }

    fn parse_escape_primitive(&mut self) -> Result<Ast, ParseError> {
        let start = self.offset();
        self.expect('\\')?;
        let c = self.bump().ok_or(ParseError)?;
        let lit = |p: &Parser<'_>, c: char, hex_byte: bool| {
            Ast::Literal(Lit {
                span: (start, p.offset()),
                c,
                hex_byte,
            })
        };
        match c {
            'x' => {
                let fixed = self.peek() != Some('{');
                let ch = self.parse_hex(2)?;
                Ok(lit(self, ch, fixed))
            }
            'u' => {
                let ch = self.parse_hex(4)?;
                Ok(lit(self, ch, false))
            }
            'U' => {
                let ch = self.parse_hex(8)?;
                Ok(lit(self, ch, false))
            }
            'p' | 'P' => {
                self.parse_unicode_class_text(start)?;
                let text = self.pattern[start..self.offset()].to_string();
                Ok(Ast::Class {
                    span: (start, self.offset()),
                    class: ClassAst::Unicode {
                        text,
                        negated: false,
                    },
                })
            }
            'd' | 's' | 'w' | 'D' | 'S' | 'W' => Ok(Ast::Class {
                span: (start, self.offset()),
                class: perl_class(c),
            }),
            'b' => {
                if self.peek() == Some('{') {
                    let save = self.pos;
                    self.pos += 1;
                    let mut name = String::new();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphabetic() || ch == '-' {
                            name.push(ch);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    if !name.is_empty() && self.bump_if('}') {
                        let kind = match name.as_str() {
                            "start" => AssertKind::WordStart,
                            "end" => AssertKind::WordEnd,
                            "start-half" => AssertKind::WordStartHalf,
                            "end-half" => AssertKind::WordEndHalf,
                            _ => return Err(ParseError),
                        };
                        return Ok(Ast::Assertion(kind));
                    }
                    self.pos = save;
                }
                Ok(Ast::Assertion(AssertKind::WordBoundary))
            }
            'B' => Ok(Ast::Assertion(AssertKind::NotWordBoundary)),
            'A' => Ok(Ast::Assertion(AssertKind::StartText)),
            'z' => Ok(Ast::Assertion(AssertKind::EndText)),
            '<' => Ok(Ast::Assertion(AssertKind::WordStart)),
            '>' => Ok(Ast::Assertion(AssertKind::WordEnd)),
            _ => {
                let ch = self.escaped_literal_char(c)?;
                Ok(lit(self, ch, false))
            }
        }
    }

    fn escaped_literal_char(&mut self, c: char) -> Result<char, ParseError> {
        match c {
            'a' => Ok('\x07'),
            'f' => Ok('\x0C'),
            't' => Ok('\t'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            'v' => Ok('\x0B'),
            '0'..='7' if self.octal => {
                let mut value = c.to_digit(8).unwrap();
                let mut count = 1;
                while count < 3 {
                    match self.peek().and_then(|d| d.to_digit(8)) {
                        Some(d) => {
                            value = value * 8 + d;
                            self.pos += 1;
                            count += 1;
                        }
                        None => break,
                    }
                }
                char::from_u32(value).ok_or(ParseError)
            }
            _ if is_meta(c) => Ok(c),
            _ if c.is_ascii() && !c.is_ascii_alphanumeric() => Ok(c),
            _ if c.is_whitespace() => Ok(c),
            _ => Err(ParseError),
        }
    }

    fn parse_bracketed_body(&mut self) -> Result<(bool, ClassSet), ParseError> {
        self.depth += 1;
        if self.depth > 10_000 {
            return Err(ParseError);
        }
        self.skip_space();
        let negated = self.bump_if('^');
        self.skip_space();
        let mut union: Vec<ClassItem> = vec![];
        if self.peek() == Some(']') {
            let start = self.offset();
            self.pos += 1;
            union.push(ClassItem::Literal(Lit {
                span: (start, self.offset()),
                c: ']',
                hex_byte: false,
            }));
        }
        let mut lhs: Option<ClassSet> = None;
        let mut pending_op: Option<SetOp> = None;
        loop {
            self.skip_space();
            let Some(c) = self.peek() else {
                return Err(ParseError);
            };
            if c == ']' {
                self.pos += 1;
                break;
            }
            let op = if self.rest_starts_with("&&") {
                Some(SetOp::Intersection)
            } else if self.rest_starts_with("--") {
                Some(SetOp::Difference)
            } else if self.rest_starts_with("~~") {
                Some(SetOp::SymmetricDifference)
            } else {
                None
            };
            if let Some(op) = op {
                self.pos += 2;
                let current = ClassSet::Item(ClassItem::Union(std::mem::take(&mut union)));
                let combined = match (lhs.take(), pending_op.take()) {
                    (Some(l), Some(pop)) => ClassSet::BinaryOp(pop, Box::new(l), Box::new(current)),
                    _ => current,
                };
                lhs = Some(combined);
                pending_op = Some(op);
                continue;
            }
            let item = self.parse_class_item()?;
            self.skip_space();
            if self.peek() == Some('-')
                && self.peek_at(1) != Some(']')
                && !self.rest_starts_with("--")
            {
                if let ClassItem::Literal(start_lit) = item {
                    self.pos += 1;
                    self.skip_space();
                    let end_item = self.parse_class_item()?;
                    let ClassItem::Literal(end_lit) = end_item else {
                        return Err(ParseError);
                    };
                    union.push(ClassItem::Range(start_lit, end_lit));
                    continue;
                }
                union.push(item);
                continue;
            }
            union.push(item);
        }
        self.depth -= 1;
        let current = ClassSet::Item(ClassItem::Union(union));
        let set = match (lhs, pending_op) {
            (Some(l), Some(op)) => ClassSet::BinaryOp(op, Box::new(l), Box::new(current)),
            _ => current,
        };
        Ok((negated, set))
    }

    fn parse_class_item(&mut self) -> Result<ClassItem, ParseError> {
        let start = self.offset();
        let c = self.peek().ok_or(ParseError)?;
        match c {
            '[' => {
                if self.rest_starts_with("[:") {
                    let rest = &self.pattern[start + 2..];
                    let (negated, rest) = match rest.strip_prefix('^') {
                        Some(r) => (true, r),
                        None => (false, rest),
                    };
                    if let Some(end) = rest.find(":]") {
                        let name = &rest[..end];
                        if let Some(&known) = ASCII_CLASSES.iter().find(|&&n| n == name) {
                            let consumed = 2 + usize::from(negated) + end + 2;
                            let target = start + consumed;
                            while self.offset() < target {
                                self.pos += 1;
                            }
                            return Ok(ClassItem::Ascii {
                                name: known,
                                negated,
                            });
                        }
                    }
                }
                self.pos += 1;
                let (negated, set) = self.parse_bracketed_body()?;
                Ok(ClassItem::Bracketed {
                    negated,
                    set: Box::new(set),
                })
            }
            '\\' => {
                self.pos += 1;
                let e = self.bump().ok_or(ParseError)?;
                match e {
                    'x' => {
                        let fixed = self.peek() != Some('{');
                        let ch = self.parse_hex(2)?;
                        Ok(ClassItem::Literal(Lit {
                            span: (start, self.offset()),
                            c: ch,
                            hex_byte: fixed,
                        }))
                    }
                    'u' | 'U' => {
                        let ch = self.parse_hex(if e == 'u' { 4 } else { 8 })?;
                        Ok(ClassItem::Literal(Lit {
                            span: (start, self.offset()),
                            c: ch,
                            hex_byte: false,
                        }))
                    }
                    'p' | 'P' => {
                        self.parse_unicode_class_text(start)?;
                        Ok(ClassItem::Unicode {
                            text: self.pattern[start..self.offset()].to_string(),
                            negated: false,
                        })
                    }
                    'd' | 's' | 'w' | 'D' | 'S' | 'W' => {
                        let ClassAst::Perl { kind, negated } = perl_class(e) else {
                            return Err(ParseError);
                        };
                        Ok(ClassItem::Perl { kind, negated })
                    }
                    _ => {
                        let ch = self.escaped_literal_char(e)?;
                        Ok(ClassItem::Literal(Lit {
                            span: (start, self.offset()),
                            c: ch,
                            hex_byte: false,
                        }))
                    }
                }
            }
            _ => {
                self.pos += 1;
                Ok(ClassItem::Literal(Lit {
                    span: (start, self.offset()),
                    c,
                    hex_byte: false,
                }))
            }
        }
    }
}

fn perl_class(c: char) -> ClassAst {
    let kind = match c.to_ascii_lowercase() {
        'd' => PerlKind::Digit,
        's' => PerlKind::Space,
        _ => PerlKind::Word,
    };
    ClassAst::Perl {
        kind,
        negated: c.is_ascii_uppercase(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AstAnalysis {
    pub(super) any_uppercase: bool,
    pub(super) any_literal: bool,
}

impl AstAnalysis {
    pub(super) fn from_ast(ast: &Ast) -> AstAnalysis {
        let mut analysis = AstAnalysis::default();
        analysis.visit(ast);
        analysis
    }

    fn done(self) -> bool {
        self.any_uppercase && self.any_literal
    }

    fn visit(&mut self, ast: &Ast) {
        if self.done() {
            return;
        }
        match ast {
            Ast::Empty | Ast::Flags(_) | Ast::Dot(_) | Ast::Assertion(_) => {}
            Ast::Literal(lit) => self.literal(lit),
            Ast::Class { class, .. } => {
                if let ClassAst::Bracketed { set, .. } = class {
                    self.class_set(set);
                }
            }
            Ast::Repetition { ast, .. } | Ast::Group { ast, .. } => self.visit(ast),
            Ast::Alternation(asts) | Ast::Concat(asts) => {
                for x in asts {
                    self.visit(x);
                }
            }
        }
    }

    fn class_set(&mut self, set: &ClassSet) {
        if self.done() {
            return;
        }
        match set {
            ClassSet::Item(item) => self.class_item(item),
            ClassSet::BinaryOp(_, lhs, rhs) => {
                self.class_set(lhs);
                self.class_set(rhs);
            }
        }
    }

    fn class_item(&mut self, item: &ClassItem) {
        if self.done() {
            return;
        }
        match item {
            ClassItem::Ascii { .. } | ClassItem::Unicode { .. } | ClassItem::Perl { .. } => {}
            ClassItem::Literal(lit) => self.literal(lit),
            ClassItem::Range(a, b) => {
                self.literal(a);
                self.literal(b);
            }
            ClassItem::Bracketed { set, .. } => self.class_set(set),
            ClassItem::Union(items) => {
                for x in items {
                    self.class_item(x);
                }
            }
        }
    }

    fn literal(&mut self, lit: &Lit) {
        self.any_literal = true;
        self.any_uppercase = self.any_uppercase || lit.c.is_uppercase();
    }

    #[cfg(test)]
    pub(super) fn from_pattern(pattern: &str) -> Option<AstAnalysis> {
        Parser::new(pattern, false, false)
            .parse()
            .ok()
            .map(|ast| AstAnalysis::from_ast(&ast))
    }
}

#[cfg(test)]
mod tests {
    use super::AstAnalysis;

    fn analysis(pattern: &str) -> AstAnalysis {
        AstAnalysis::from_pattern(pattern).unwrap()
    }

    #[test]
    fn ast_analysis_various() {
        let cases: &[(&str, bool, bool)] = &[
            ("", false, false),
            ("foo", false, true),
            ("Foo", true, true),
            ("foO", true, true),
            (r"foo\\", false, true),
            (r"foo\w", false, true),
            (r"foo\S", false, true),
            (r"foo\p{Ll}", false, true),
            (r"foo[a-z]", false, true),
            (r"foo[A-Z]", true, true),
            (r"foo[\S\t]", false, true),
            (r"foo\\S", true, true),
            (r"\p{Ll}", false, false),
            (r"aBc\w", true, true),
            (r"aa", false, true),
        ];
        for &(pattern, upper, literal) in cases {
            let x = analysis(pattern);
            assert_eq!(x.any_uppercase, upper, "uppercase for {pattern:?}");
            assert_eq!(x.any_literal, literal, "literal for {pattern:?}");
        }
    }
}
