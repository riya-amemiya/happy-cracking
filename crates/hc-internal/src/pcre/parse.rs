use super::ast::{
    Assert, Ast, BackRef, Cond, CondKind, Group, GroupKind, Lit, NamedGroup, Newline, Node,
    RefTarget, RepKind, Repeat, Verb,
};
use super::charset::{CaseMode, CharSet, is_turkish_i};
use super::error::Error;
use super::ucd::{self, PropKind};

const MAX_GROUP_NUMBER: u32 = 65535;
const MAX_REPEAT_COUNT: u32 = 65535;
const MAX_NAME_SIZE: usize = 128;
const MAX_NAME_COUNT: usize = 10000;
const MAX_MARK: usize = 255;
const PARENS_NEST_LIMIT: u32 = 250;
const ECLASS_NEST_LIMIT: i32 = 15;
const MAX_UTF: u32 = 0x10_ffff;

type RepeatCounts = Result<Option<(u32, Option<u32>, usize)>, (u32, usize)>;

#[allow(clippy::struct_excessive_bools)]
pub struct ParseConfig {
    pub caseless: bool,
    pub multi_line: bool,
    pub dotall: bool,
    pub extended: bool,
    pub utf: bool,
    pub ucp: bool,
    pub crlf: bool,
    pub dollar_endonly: bool,
    pub ascii_bsd: bool,
    pub match_line: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
struct Opts {
    caseless: bool,
    multiline: bool,
    dotall: bool,
    extended: bool,
    extended_more: bool,
    no_auto_capture: bool,
    dupnames: bool,
    ungreedy: bool,
    restrict: bool,
    ascii_bsd: bool,
    ascii_bss: bool,
    ascii_bsw: bool,
    ascii_digit: bool,
    ascii_posix: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Esc {
    A,
    G,
    K,
    UpperB,
    LowerB,
    UpperD,
    LowerD,
    UpperS,
    LowerS,
    UpperW,
    LowerW,
    UpperN,
    UpperC,
    UpperP,
    LowerP,
    UpperR,
    UpperH,
    LowerH,
    UpperV,
    LowerV,
    UpperX,
    UpperZ,
    LowerZ,
    UpperE,
    UpperQ,
    LowerG,
    LowerK,
}

enum Escape {
    Char(u32),
    Backref(u32),
    Special(Esc),
}

#[derive(Clone, Debug)]
enum FrameKind {
    Top,
    Group(GroupKind),
    AtomicScriptRun,
    Cond {
        kind: Option<CondKind>,
        name_offset: usize,
    },
}

struct Frame {
    kind: FrameKind,
    branches: Vec<Vec<Node>>,
    cur: Vec<Node>,
    offset: usize,
    opts: Opts,
    depth: u32,
    reset: Option<(u32, u32)>,
    condassert: bool,
    cond_assert_target: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RangeState {
    No,
    Started,
    ForbidNo,
    ForbidStarted,
    OkEscaped,
    OkLiteral,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpState {
    Empty,
    Operand,
    Operator,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ClassMode {
    Normal,
    PerlExt,
    PerlExtLeaf,
}

enum CItem {
    Char(u32),
    RangeStart,
    Set(CharSet),
    Open(bool, bool),
    Close,
    Op(u8),
    Not,
}

#[allow(clippy::struct_excessive_bools)]
pub struct Parser<'a> {
    pat: &'a [u8],
    ptr: usize,
    utf: bool,
    ucp: bool,
    opts: Opts,
    newline: Newline,
    bracount: u32,
    names: Vec<NamedGroup>,
    nest_depth: u32,
    frames: Vec<Frame>,
    okquantifier: bool,
    last_quant: bool,
    expect_cond_assert: i32,
    has_lookbehind: bool,
    has_crorlf: bool,
    has_then: bool,
    has_skip_arg: bool,
    dupcap_used: bool,
    turkish: bool,
    accept_start: Option<usize>,
    small_ref_offset: [Option<usize>; 10],
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

fn xdigit(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some(u32::from(c - b'0')),
        b'a'..=b'f' => Some(u32::from(c - b'a' + 10)),
        b'A'..=b'F' => Some(u32::from(c - b'A' + 10)),
        _ => None,
    }
}

fn is_ctype_space(c: u32) -> bool {
    matches!(c, 0x09..=0x0d | 0x20)
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

#[allow(clippy::struct_excessive_bools)]
pub struct StartInfo {
    pub utf: bool,
    pub ucp: bool,
    pub newline: Newline,
    pub bsr_anycrlf: bool,
    pub turkish: bool,
    pub restrict: bool,
    pub match_limit: u32,
    pub depth_limit: u32,
    pub heap_limit: u32,
    pub notempty: bool,
    pub notempty_atstart: bool,
    pub no_auto_possess: bool,
    pub no_start_opt: bool,
    pub no_jit: bool,
    pub no_dotstar_anchor: bool,
    pub skip: usize,
}

fn parse_start(pat: &[u8], cfg: &ParseConfig) -> Result<StartInfo, Error> {
    let mut info = StartInfo {
        utf: cfg.utf || cfg.ucp,
        ucp: cfg.ucp,
        newline: if cfg.crlf {
            Newline::AnyCrLf
        } else {
            Newline::Lf
        },
        bsr_anycrlf: false,
        turkish: false,
        restrict: false,
        match_limit: u32::MAX,
        depth_limit: u32::MAX,
        heap_limit: u32::MAX,
        notempty: false,
        notempty_atstart: false,
        no_auto_possess: false,
        no_start_opt: false,
        no_jit: false,
        no_dotstar_anchor: false,
        skip: 0,
    };
    let items: &[(&[u8], u8)] = &[
        (b"UTF8)", 0),
        (b"UTF)", 0),
        (b"UCP)", 1),
        (b"NOTEMPTY)", 2),
        (b"NOTEMPTY_ATSTART)", 3),
        (b"NO_AUTO_POSSESS)", 4),
        (b"NO_DOTSTAR_ANCHOR)", 5),
        (b"NO_JIT)", 6),
        (b"NO_START_OPT)", 7),
        (b"CASELESS_RESTRICT)", 8),
        (b"TURKISH_CASING)", 9),
        (b"LIMIT_HEAP=", 10),
        (b"LIMIT_MATCH=", 11),
        (b"LIMIT_DEPTH=", 12),
        (b"LIMIT_RECURSION=", 12),
        (b"CR)", 20),
        (b"LF)", 21),
        (b"CRLF)", 22),
        (b"ANY)", 23),
        (b"NUL)", 24),
        (b"ANYCRLF)", 25),
        (b"BSR_ANYCRLF)", 30),
        (b"BSR_UNICODE)", 31),
    ];
    let len = pat.len();
    let mut skip = 0usize;
    while len - skip >= 2 && pat[skip] == b'(' && pat[skip + 1] == b'*' {
        let mut found = false;
        for &(name, kind) in items {
            if len - skip - 2 >= name.len() && &pat[skip + 2..skip + 2 + name.len()] == name {
                skip += name.len() + 2;
                found = true;
                match kind {
                    0 => info.utf = true,
                    1 => info.ucp = true,
                    2 => info.notempty = true,
                    3 => info.notempty_atstart = true,
                    4 => info.no_auto_possess = true,
                    5 => info.no_dotstar_anchor = true,
                    6 => info.no_jit = true,
                    7 => info.no_start_opt = true,
                    8 => info.restrict = true,
                    9 => info.turkish = true,
                    10..=12 => {
                        let mut c: u32 = 0;
                        let mut pp = skip;
                        while pp < len && pat[pp].is_ascii_digit() {
                            if c > u32::MAX / 10 - 1 {
                                break;
                            }
                            c = c * 10 + u32::from(pat[pp] - b'0');
                            pp += 1;
                        }
                        if pp >= len || pp == skip || pat[pp] != b')' {
                            return Err(Error::compile(60, pp));
                        }
                        match kind {
                            10 => info.heap_limit = info.heap_limit.min(c),
                            11 => info.match_limit = info.match_limit.min(c),
                            _ => info.depth_limit = info.depth_limit.min(c),
                        }
                        skip = pp + 1;
                    }
                    20 => info.newline = Newline::Cr,
                    21 => info.newline = Newline::Lf,
                    22 => info.newline = Newline::CrLf,
                    23 => info.newline = Newline::Any,
                    24 => info.newline = Newline::Nul,
                    25 => info.newline = Newline::AnyCrLf,
                    30 => info.bsr_anycrlf = true,
                    _ => info.bsr_anycrlf = false,
                }
                break;
            }
        }
        if !found {
            break;
        }
    }
    info.skip = skip;
    if info.turkish {
        if !info.utf && !info.ucp {
            return Err(Error::compile(104, skip));
        }
        if !info.utf {
            return Err(Error::compile(105, skip));
        }
        if info.restrict {
            return Err(Error::compile(106, skip));
        }
    }
    Ok(info)
}

pub fn parse(pattern: &str, cfg: &ParseConfig) -> Result<Ast, Error> {
    let pat = pattern.as_bytes();
    let info = parse_start(pat, cfg)?;
    let opts = Opts {
        caseless: cfg.caseless,
        multiline: cfg.multi_line,
        dotall: cfg.dotall,
        extended: cfg.extended,
        restrict: info.restrict,
        ascii_bsd: cfg.ascii_bsd,
        ..Opts::default()
    };
    let mut p = Parser {
        pat,
        ptr: info.skip,
        utf: info.utf,
        ucp: info.ucp,
        opts,
        newline: info.newline,
        bracount: 0,
        names: Vec::new(),
        nest_depth: 0,
        frames: Vec::new(),
        okquantifier: false,
        last_quant: false,
        expect_cond_assert: 0,
        has_lookbehind: false,
        has_crorlf: false,
        has_then: false,
        has_skip_arg: false,
        dupcap_used: false,
        turkish: info.turkish,
        accept_start: None,
        small_ref_offset: [None; 10],
    };
    let root = p.run()?;
    let root = if cfg.match_line {
        let (circ, doll) = if cfg.multi_line {
            (Assert::CircM, Assert::DollM)
        } else {
            (Assert::Circ, Assert::Doll)
        };
        Group {
            kind: GroupKind::Top,
            branches: vec![vec![
                Node::Assert(circ),
                Node::Group(Box::new(Group {
                    kind: GroupKind::NonCapture,
                    ..root
                })),
                Node::Assert(doll),
            ]],
            offset: 0,
        }
    } else {
        root
    };
    let mut ast = Ast {
        root,
        dollar_endonly: cfg.dollar_endonly,
        capture_count: p.bracount,
        names: p.names,
        utf: info.utf,
        ucp: info.ucp,
        newline: info.newline,
        bsr_anycrlf: info.bsr_anycrlf,
        turkish: info.turkish,
        match_limit: info.match_limit,
        depth_limit: info.depth_limit,
        heap_limit: info.heap_limit,
        notempty: info.notempty,
        notempty_atstart: info.notempty_atstart,
        no_auto_possess: info.no_auto_possess,
        no_start_opt: info.no_start_opt,
        no_jit: info.no_jit,
        no_dotstar_anchor: info.no_dotstar_anchor,
        has_crorlf: p.has_crorlf,
        has_then: p.has_then,
        has_skip_arg: p.has_skip_arg,
        dupcap_used: p.dupcap_used,
        max_lookbehind: 0,
    };
    super::check::check(&mut ast, pat.len(), p.has_lookbehind, &p.small_ref_offset)?;
    Ok(ast)
}

impl Parser<'_> {
    fn end(&self) -> usize {
        self.pat.len()
    }

    fn at(&self, i: usize) -> u8 {
        self.pat[i]
    }

    fn get_char_at(&self, i: usize) -> (u32, usize) {
        let b = self.pat[i];
        if !self.utf || b < 0x80 {
            return (u32::from(b), 1);
        }
        let n = if b >= 0xf0 {
            4
        } else if b >= 0xe0 {
            3
        } else {
            2
        };
        let mut c = u32::from(b) & (0x7f >> n);
        for k in 1..n {
            c = (c << 6) | (u32::from(self.pat[i + k]) & 0x3f);
        }
        (c, n)
    }

    fn next_char(&mut self) -> u32 {
        let (c, n) = self.get_char_at(self.ptr);
        self.ptr += n;
        c
    }

    fn max_char(&self) -> u32 {
        if self.utf { MAX_UTF } else { 0xff }
    }

    fn case_mode(&self, opts: Opts) -> CaseMode {
        CaseMode {
            unicode: self.utf || self.ucp,
            restrict: opts.restrict,
            turkish: self.turkish,
            max: self.max_char(),
        }
    }

    fn fail<T>(&self, code: u32) -> Result<T, Error> {
        Err(Error::compile(code, self.ptr))
    }

    fn fail_at<T>(code: u32, at: usize) -> Result<T, Error> {
        Err(Error::compile(code, at))
    }

    fn fail_back<T>(&mut self, code: u32) -> Result<T, Error> {
        self.ptr -= 1;
        if self.utf {
            while self.ptr > 0 && self.at(self.ptr) & 0xc0 == 0x80 {
                self.ptr -= 1;
            }
        }
        self.fail(code)
    }

    fn fail_forward<T>(&mut self, code: u32) -> Result<T, Error> {
        self.ptr += 1;
        if self.utf {
            while self.ptr < self.end() && self.at(self.ptr) & 0xc0 == 0x80 {
                self.ptr += 1;
            }
        }
        self.fail(code)
    }

    fn cur(&mut self) -> &mut Vec<Node> {
        &mut self.frames.last_mut().expect("frame").cur
    }

    fn push_node(&mut self, n: Node, okq: bool) {
        self.cur().push(n);
        self.okquantifier = okq;
    }

    fn push_lit(&mut self, c: u32) {
        if c == 0x0d || c == 0x0a {
            self.has_crorlf = true;
        }
        let lit = Lit {
            c,
            fold: self.opts.caseless,
            restrict: self.opts.restrict,
        };
        self.push_node(Node::Lit(lit), true);
    }

    fn read_number(
        &mut self,
        allow_sign: i64,
        max_value: u32,
        max_error: u32,
    ) -> Result<Option<u32>, Error> {
        let mut p = self.ptr;
        let end = self.end();
        let mut sign = 0i32;
        let mut max_value = i64::from(max_value);
        if allow_sign >= 0 && p < end {
            if self.at(p) == b'+' {
                sign = 1;
                max_value -= allow_sign;
                p += 1;
            } else if self.at(p) == b'-' {
                sign = -1;
                p += 1;
            }
        }
        if p >= end || !is_digit(self.at(p)) {
            return Ok(None);
        }
        let mut n: i64 = 0;
        while p < end && is_digit(self.at(p)) {
            n = n * 10 + i64::from(self.at(p) - b'0');
            p += 1;
            if n > max_value {
                while p < end && is_digit(self.at(p)) {
                    p += 1;
                }
                self.ptr = p;
                return self.fail(max_error);
            }
        }
        if allow_sign >= 0 && sign != 0 {
            if n == 0 {
                self.ptr = p;
                return self.fail(26);
            }
            if sign > 0 {
                n += allow_sign;
            } else if n > allow_sign {
                self.ptr = p;
                return self.fail(15);
            } else {
                n = allow_sign + 1 - n;
            }
        }
        self.ptr = p;
        Ok(Some(n as u32))
    }

    fn read_number_noerr(&self, p: &mut usize, allow_sign: i64, max_value: u32) -> NumRes {
        let save = *p;
        let end = self.end();
        let mut q = *p;
        let mut sign = 0i32;
        let mut max_value = i64::from(max_value);
        if allow_sign >= 0 && q < end {
            if self.at(q) == b'+' {
                sign = 1;
                max_value -= allow_sign;
                q += 1;
            } else if self.at(q) == b'-' {
                sign = -1;
                q += 1;
            }
        }
        if q >= end || !is_digit(self.at(q)) {
            *p = save;
            return NumRes::None;
        }
        let mut n: i64 = 0;
        while q < end && is_digit(self.at(q)) {
            n = n * 10 + i64::from(self.at(q) - b'0');
            q += 1;
            if n > max_value {
                while q < end && is_digit(self.at(q)) {
                    q += 1;
                }
                *p = q;
                return NumRes::TooBig;
            }
        }
        if allow_sign >= 0 && sign != 0 {
            if n == 0 {
                *p = q;
                return NumRes::Zero;
            }
            if sign > 0 {
                n += allow_sign;
            } else if n > allow_sign {
                *p = q;
                return NumRes::NonExistent;
            } else {
                n = allow_sign + 1 - n;
            }
        }
        *p = q;
        NumRes::Num(n as u32)
    }

    fn read_repeat_counts(&self, start: usize) -> RepeatCounts {
        let end = self.end();
        let sp = |c: u8| c == b' ' || c == b'\t';
        let mut p = start;
        while p < end && sp(self.at(p)) {
            p += 1;
        }
        let mut pp = p;
        let mut had_min = false;
        if pp < end && is_digit(self.at(pp)) {
            had_min = true;
            pp += 1;
            while pp < end && is_digit(self.at(pp)) {
                pp += 1;
            }
        }
        while pp < end && sp(self.at(pp)) {
            pp += 1;
        }
        if pp >= end {
            return Ok(None);
        }
        if self.at(pp) == b'}' {
            if !had_min {
                return Ok(None);
            }
        } else {
            if self.at(pp) != b',' {
                return Ok(None);
            }
            pp += 1;
            while pp < end && sp(self.at(pp)) {
                pp += 1;
            }
            if pp >= end {
                return Ok(None);
            }
            if is_digit(self.at(pp)) {
                pp += 1;
                while pp < end && is_digit(self.at(pp)) {
                    pp += 1;
                }
            } else if !had_min {
                return Ok(None);
            }
            while pp < end && sp(self.at(pp)) {
                pp += 1;
            }
            if pp >= end || self.at(pp) != b'}' {
                return Ok(None);
            }
        }
        let read = |p: &mut usize| -> Result<Option<u32>, (u32, usize)> {
            if *p >= end || !is_digit(self.at(*p)) {
                return Ok(None);
            }
            let mut n: u64 = 0;
            while *p < end && is_digit(self.at(*p)) {
                n = n * 10 + u64::from(self.at(*p) - b'0');
                *p += 1;
                if n > u64::from(MAX_REPEAT_COUNT) {
                    while *p < end && is_digit(self.at(*p)) {
                        *p += 1;
                    }
                    return Err((5, *p));
                }
            }
            Ok(Some(n as u32))
        };
        let min;
        let max: Option<u32>;
        match read(&mut p)? {
            None => {
                min = 0;
                p += 1;
                while p < end && sp(self.at(p)) {
                    p += 1;
                }
                max = read(&mut p)?;
            }
            Some(n) => {
                min = n;
                while p < end && sp(self.at(p)) {
                    p += 1;
                }
                if self.at(p) == b'}' {
                    max = Some(min);
                } else {
                    p += 1;
                    while p < end && sp(self.at(p)) {
                        p += 1;
                    }
                    max = read(&mut p)?;
                    if let Some(m) = max
                        && m < min
                    {
                        return Err((4, p));
                    }
                }
            }
        }
        while p < end && sp(self.at(p)) {
            p += 1;
        }
        p += 1;
        Ok(Some((min, max, p)))
    }

    fn check_escape(&mut self, isclass: bool) -> Result<Escape, Error> {
        let end = self.end();
        if self.ptr >= end {
            return self.fail(1);
        }
        let c = self.next_char();
        if !(u32::from(b'0')..=u32::from(b'z')).contains(&c) {
            return Ok(Escape::Char(c));
        }
        let cb = c as u8;
        let simple = match cb {
            b':' | b';' | b'<' | b'=' | b'>' | b'?' | b'@' | b'[' | b'\\' | b']' | b'^' | b'_'
            | b'`' => Some(Escape::Char(c)),
            b'a' => Some(Escape::Char(7)),
            b'e' => Some(Escape::Char(0x1b)),
            b'f' => Some(Escape::Char(0x0c)),
            b'n' => Some(Escape::Char(0x0a)),
            b'r' => Some(Escape::Char(0x0d)),
            b't' => Some(Escape::Char(9)),
            b'A' => Some(Escape::Special(Esc::A)),
            b'B' => Some(Escape::Special(Esc::UpperB)),
            b'C' => Some(Escape::Special(Esc::UpperC)),
            b'D' => Some(Escape::Special(Esc::UpperD)),
            b'E' => Some(Escape::Special(Esc::UpperE)),
            b'G' => Some(Escape::Special(Esc::G)),
            b'H' => Some(Escape::Special(Esc::UpperH)),
            b'K' => Some(Escape::Special(Esc::K)),
            b'N' => Some(Escape::Special(Esc::UpperN)),
            b'P' => Some(Escape::Special(Esc::UpperP)),
            b'Q' => Some(Escape::Special(Esc::UpperQ)),
            b'R' => Some(Escape::Special(Esc::UpperR)),
            b'S' => Some(Escape::Special(Esc::UpperS)),
            b'V' => Some(Escape::Special(Esc::UpperV)),
            b'W' => Some(Escape::Special(Esc::UpperW)),
            b'X' => Some(Escape::Special(Esc::UpperX)),
            b'Z' => Some(Escape::Special(Esc::UpperZ)),
            b'b' => Some(Escape::Special(Esc::LowerB)),
            b'd' => Some(Escape::Special(Esc::LowerD)),
            b'h' => Some(Escape::Special(Esc::LowerH)),
            b'k' => Some(Escape::Special(Esc::LowerK)),
            b'p' => Some(Escape::Special(Esc::LowerP)),
            b's' => Some(Escape::Special(Esc::LowerS)),
            b'v' => Some(Escape::Special(Esc::LowerV)),
            b'w' => Some(Escape::Special(Esc::LowerW)),
            b'z' => Some(Escape::Special(Esc::LowerZ)),
            _ => None,
        };
        if let Some(e) = simple {
            if let Escape::Special(Esc::UpperN) = e
                && self.ptr < end
                && self.at(self.ptr) == b'{'
            {
                let mut p = self.ptr + 1;
                while p < end && (self.at(p) == b' ' || self.at(p) == b'\t') {
                    p += 1;
                }
                if end - p > 1 && self.at(p) == b'U' && self.at(p + 1) == b'+' {
                    self.ptr = p + 2;
                    if self.utf {
                        return self.hex_braced_body();
                    }
                    while self.ptr < end && xdigit(self.at(self.ptr)).is_some() {
                        self.ptr += 1;
                    }
                    while self.ptr < end
                        && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t')
                    {
                        self.ptr += 1;
                    }
                    if self.ptr < end && self.at(self.ptr) == b'}' {
                        self.ptr += 1;
                    }
                    return self.fail(93);
                }
                if isclass {
                    self.ptr += 1;
                    return self.fail(37);
                }
                match self.read_repeat_counts(p) {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        self.ptr += 1;
                        return self.fail(37);
                    }
                    Err((code, _)) => return self.fail(code),
                }
            }
            return Ok(e);
        }
        match cb {
            b'F' | b'l' | b'L' | b'u' | b'U' => self.fail(37),
            b'g' => {
                if isclass {
                    return Ok(Escape::Char(c));
                }
                if self.ptr >= end {
                    return self.fail(57);
                }
                let nc = self.at(self.ptr);
                if nc == b'<' || nc == b'\'' {
                    return Ok(Escape::Special(Esc::LowerG));
                }
                if nc == b'{' {
                    let mut p = self.ptr + 1;
                    while p < end && (self.at(p) == b' ' || self.at(p) == b'\t') {
                        p += 1;
                    }
                    let s = match self.read_number_noerr(
                        &mut p,
                        i64::from(self.bracount),
                        MAX_GROUP_NUMBER,
                    ) {
                        NumRes::None => return Ok(Escape::Special(Esc::LowerK)),
                        NumRes::Num(s) => s,
                        NumRes::TooBig => return self.fail(61),
                        NumRes::Zero => return self.fail(26),
                        NumRes::NonExistent => return self.fail(15),
                    };
                    while p < end && (self.at(p) == b' ' || self.at(p) == b'\t') {
                        p += 1;
                    }
                    if p >= end || self.at(p) != b'}' {
                        return Self::fail_at(119, p);
                    }
                    self.ptr = p + 1;
                    if s == 0 {
                        return self.fail(15);
                    }
                    return Ok(Escape::Backref(s));
                }
                let r = self.read_number(i64::from(self.bracount), MAX_GROUP_NUMBER, 61)?;
                match r {
                    None => self.fail(57),
                    Some(0) => self.fail(15),
                    Some(s) => Ok(Escape::Backref(s)),
                }
            }
            b'1'..=b'9' => {
                if !isclass {
                    let oldptr = self.ptr;
                    self.ptr -= 1;
                    let mut p = self.ptr;
                    let s = match self.read_number_noerr(&mut p, -1, MAX_GROUP_NUMBER) {
                        NumRes::Num(n) => i64::from(n),
                        _ => i64::from(i32::MAX),
                    };
                    self.ptr = p;
                    if s < 10 || cb >= b'8' || s <= i64::from(self.bracount) {
                        if s > i64::from(MAX_GROUP_NUMBER) {
                            return self.fail(61);
                        }
                        return Ok(Escape::Backref(s as u32));
                    }
                    self.ptr = oldptr;
                }
                if cb >= b'8' {
                    return Ok(Escape::Char(c));
                }
                self.octal(c - u32::from(b'0'), 0)
            }
            b'0' => self.octal(0, 0),
            b'o' => {
                if self.ptr >= end || self.at(self.ptr) != b'{' {
                    return self.fail(55);
                }
                self.ptr += 1;
                while self.ptr < end && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t') {
                    self.ptr += 1;
                }
                if self.ptr >= end || self.at(self.ptr) == b'}' {
                    return self.fail(78);
                }
                let mut v: u32 = 0;
                let mut overflow = false;
                while self.ptr < end && (b'0'..=b'7').contains(&self.at(self.ptr)) {
                    let d = self.at(self.ptr);
                    self.ptr += 1;
                    if v == 0 && d == b'0' {
                        continue;
                    }
                    v = (v << 3) + u32::from(d - b'0');
                    if v > self.max_char() {
                        overflow = true;
                        break;
                    }
                }
                while self.ptr < end && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t') {
                    self.ptr += 1;
                }
                if overflow {
                    while self.ptr < end && (b'0'..=b'7').contains(&self.at(self.ptr)) {
                        self.ptr += 1;
                    }
                    return self.fail(34);
                }
                if self.utf && (0xd800..=0xdfff).contains(&v) {
                    return self.fail(73);
                }
                if self.ptr < end && self.at(self.ptr) == b'}' {
                    self.ptr += 1;
                    Ok(Escape::Char(v))
                } else if self.ptr < end {
                    self.fail_forward(64)
                } else {
                    self.fail(64)
                }
            }
            b'x' => {
                if self.ptr < end && self.at(self.ptr) == b'{' {
                    self.ptr += 1;
                    while self.ptr < end
                        && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t')
                    {
                        self.ptr += 1;
                    }
                    return self.hex_braced_body();
                }
                if self.ptr >= end {
                    return self.fail(78);
                }
                let Some(d1) = xdigit(self.at(self.ptr)) else {
                    return self.fail(78);
                };
                self.ptr += 1;
                let mut v = d1;
                if self.ptr < end
                    && let Some(d2) = xdigit(self.at(self.ptr))
                {
                    self.ptr += 1;
                    v = (v << 4) | d2;
                }
                Ok(Escape::Char(v))
            }
            b'c' => {
                if self.ptr >= end {
                    return self.fail(2);
                }
                let mut v = u32::from(self.at(self.ptr));
                if (u32::from(b'a')..=u32::from(b'z')).contains(&v) {
                    v -= 32;
                }
                if !(32..=126).contains(&v) {
                    return self.fail_forward(68);
                }
                self.ptr += 1;
                Ok(Escape::Char(v ^ 0x40))
            }
            _ => self.fail(3),
        }
    }

    fn hex_braced_body(&mut self) -> Result<Escape, Error> {
        let end = self.end();
        if self.ptr >= end || self.at(self.ptr) == b'}' {
            return self.fail(78);
        }
        let mut v: u32 = 0;
        let mut overflow = false;
        while self.ptr < end {
            let Some(d) = xdigit(self.at(self.ptr)) else {
                break;
            };
            self.ptr += 1;
            if v == 0 && d == 0 {
                continue;
            }
            v = (v << 4) | d;
            if v > self.max_char() {
                overflow = true;
                break;
            }
        }
        while self.ptr < end && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t') {
            self.ptr += 1;
        }
        if overflow {
            while self.ptr < end && xdigit(self.at(self.ptr)).is_some() {
                self.ptr += 1;
            }
            return self.fail(34);
        }
        if self.utf && (0xd800..=0xdfff).contains(&v) {
            return self.fail(73);
        }
        if self.ptr < end && self.at(self.ptr) == b'}' {
            self.ptr += 1;
            Ok(Escape::Char(v))
        } else if self.ptr < end {
            self.fail_forward(67)
        } else {
            self.fail(67)
        }
    }

    fn octal(&mut self, first: u32, _unused: u32) -> Result<Escape, Error> {
        let end = self.end();
        let mut v = first;
        let mut i = 0;
        while i < 2 && self.ptr < end && (b'0'..=b'7').contains(&self.at(self.ptr)) {
            v = v * 8 + u32::from(self.at(self.ptr) - b'0');
            self.ptr += 1;
            i += 1;
        }
        if v > 0xff && !self.utf {
            return self.fail(51);
        }
        Ok(Escape::Char(v))
    }

    fn get_ucp(&mut self) -> Result<(bool, PropKind, u16), Error> {
        let end = self.end();
        let mut name: Vec<u8> = Vec::new();
        let mut vpos: Option<usize> = None;
        let mut neg = false;
        if self.ptr >= end {
            return self.fail(46);
        }
        let c = self.next_char();
        if c == u32::from(b'{') {
            if self.ptr >= end {
                return self.fail(46);
            }
            let mut i = 0usize;
            let mut last;
            loop {
                if i >= 49 {
                    return self.fail(46);
                }
                if self.ptr >= end {
                    return self.fail(46);
                }
                last = self.next_char();
                while last == u32::from(b'_')
                    || last == u32::from(b'-')
                    || last == 0x20
                    || (0x09..=0x0d).contains(&last)
                {
                    if self.ptr >= end {
                        return self.fail(46);
                    }
                    last = self.next_char();
                }
                if i == 0 && !neg && last == u32::from(b'^') {
                    neg = true;
                    continue;
                }
                if last == u32::from(b'}') {
                    break;
                }
                if !(u32::from(b'&')..=u32::from(b'z')).contains(&last) {
                    return self.fail(46);
                }
                let mut b = last as u8;
                if b.is_ascii_uppercase() {
                    b |= 0x20;
                } else if (b == b':' || b == b'=') && vpos.is_none() {
                    vpos = Some(i);
                }
                name.push(b);
                i += 1;
            }
        } else if (u32::from(b'A')..=u32::from(b'Z')).contains(&c) {
            name.push((c as u8) | 0x20);
        } else if (u32::from(b'a')..=u32::from(b'z')).contains(&c) {
            name.push(c as u8);
        } else {
            return self.fail(46);
        }
        let mut ptscript: Option<PropKind> = None;
        let lookup_name: Vec<u8> = if let Some(v) = vpos {
            let prefix = &name[..v];
            let value = &name[v + 1..];
            if prefix == b"bidiclass" || prefix == b"bc" {
                let mut n = b"bidi".to_vec();
                n.extend_from_slice(value);
                n
            } else if prefix == b"script" || prefix == b"sc" {
                ptscript = Some(PropKind::Sc);
                value.to_vec()
            } else if prefix == b"scriptextensions" || prefix == b"scx" {
                ptscript = Some(PropKind::Scx);
                value.to_vec()
            } else {
                return self.fail(47);
            }
        } else {
            name.clone()
        };
        match ucd::lookup_property_name(&lookup_name) {
            Some((kind, value)) => {
                if vpos.is_none() || ptscript.is_none() {
                    let kind = if kind == PropKind::Sc {
                        PropKind::Scx
                    } else {
                        kind
                    };
                    return Ok((neg, kind, value));
                }
                if kind == PropKind::Sc {
                    return Ok((neg, ptscript.unwrap_or(PropKind::Scx), value));
                }
                self.fail(47)
            }
            None => self.fail(47),
        }
    }

    fn check_posix_syntax(&self, p: usize) -> Option<usize> {
        let end = self.end();
        let terminator = self.at(p);
        let mut q = p + 1;
        while end - q >= 2 {
            let c = self.at(q);
            let n = self.at(q + 1);
            if c == b'\\' && (n == b']' || n == b'\\') {
                q += 1;
            } else if (c == b'[' && n == terminator) || c == b']' {
                return None;
            } else if c == terminator && n == b']' {
                return Some(q);
            }
            q += 1;
        }
        None
    }

    fn read_name(&mut self, terminator: u8, is_group: bool) -> Result<(Vec<u8>, usize), Error> {
        let end = self.end();
        let is_braced = terminator == b'}';
        self.ptr += 1;
        if is_braced {
            while self.ptr < end && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t') {
                self.ptr += 1;
            }
        }
        if self.ptr >= end {
            return self.fail(if is_group { 62 } else { 60 });
        }
        let start = self.ptr;
        if self.utf && is_group {
            let (c, _) = self.get_char_at(self.ptr);
            let cat = ucd::general_category(c);
            if cat == ucd::ND {
                self.ptr += self.get_char_at(self.ptr).1;
                return self.fail(44);
            }
            let mut c = c;
            loop {
                let cat = ucd::general_category(c);
                if cat != ucd::ND && ucd::gentype(cat) != ucd::GT_L && c != u32::from(b'_') {
                    break;
                }
                let (_, n) = self.get_char_at(self.ptr);
                self.ptr += n;
                if self.ptr >= end {
                    break;
                }
                c = self.get_char_at(self.ptr).0;
            }
        } else {
            if is_group && is_digit(self.at(self.ptr)) {
                self.ptr += 1;
                return self.fail(44);
            }
            while self.ptr < end && is_word_byte(self.at(self.ptr)) {
                self.ptr += 1;
            }
        }
        if self.ptr > start + MAX_NAME_SIZE {
            return self.fail(48);
        }
        let name = self.pat[start..self.ptr].to_vec();
        if is_group {
            if self.ptr == start {
                return self.fail(62);
            }
            if is_braced {
                while self.ptr < end && (self.at(self.ptr) == b' ' || self.at(self.ptr) == b'\t') {
                    self.ptr += 1;
                }
            }
            if terminator != 0 {
                if self.ptr >= end || self.at(self.ptr) != terminator {
                    return self.fail(42);
                }
                self.ptr += 1;
            }
        }
        Ok((name, start))
    }

    fn escape_set(&self, e: Esc, opts: Opts) -> CharSet {
        let ucp = self.ucp;
        let max = self.max_char();
        let (base, neg) = match e {
            Esc::LowerD | Esc::UpperD => {
                let s = if ucp && !opts.ascii_bsd {
                    CharSet::from_ranges(ucd::property_ranges(PropKind::Pc, u16::from(ucd::ND)))
                } else {
                    CharSet::from_ranges(vec![(0x30, 0x39)])
                };
                (s, e == Esc::UpperD)
            }
            Esc::LowerS | Esc::UpperS => {
                let s = if ucp && !opts.ascii_bss {
                    CharSet::from_ranges(ucd::property_ranges(PropKind::Space, 0))
                } else {
                    CharSet::from_ranges(vec![(0x09, 0x0d), (0x20, 0x20)])
                };
                (s, e == Esc::UpperS)
            }
            Esc::LowerW | Esc::UpperW => {
                let s = if ucp && !opts.ascii_bsw {
                    CharSet::from_ranges(ucd::property_ranges(PropKind::Word, 0))
                } else {
                    CharSet::from_ranges(vec![
                        (0x30, 0x39),
                        (0x41, 0x5a),
                        (0x5f, 0x5f),
                        (0x61, 0x7a),
                    ])
                };
                (s, e == Esc::UpperW)
            }
            Esc::LowerH | Esc::UpperH => (CharSet::from_ranges(ucd::hspace()), e == Esc::UpperH),
            _ => (CharSet::from_ranges(ucd::vspace()), e == Esc::UpperV),
        };
        let base = base.clamp(max);
        if neg { base.negate(max) } else { base }
    }

    fn prop_set(&self, neg: bool, kind: PropKind, value: u16, caseless: bool) -> CharSet {
        let max = self.max_char();
        let (kind, value) = if caseless
            && kind == PropKind::Pc
            && (value == u16::from(ucd::LU)
                || value == u16::from(ucd::LL)
                || value == u16::from(ucd::LT))
        {
            (PropKind::Lamp, 0)
        } else {
            (kind, value)
        };
        let s = CharSet::from_ranges(ucd::property_ranges(kind, value)).clamp(max);
        if neg { s.negate(max) } else { s }
    }

    fn posix_set(&self, idx: usize, neg: bool, opts: Opts) -> CharSet {
        let max = self.max_char();
        let ucp_ok = self.ucp
            && !opts.ascii_posix
            && !(opts.ascii_digit && (idx == PC_DIGIT || idx == PC_XDIGIT));
        if ucp_ok {
            let sub: Option<(PropKind, u16)> = match idx {
                0 => Some((PropKind::Gc, u16::from(ucd::GT_L))),
                1 => Some((PropKind::Pc, u16::from(ucd::LL))),
                2 => Some((PropKind::Pc, u16::from(ucd::LU))),
                3 => Some((PropKind::Alnum, 0)),
                6 => Some((PropKind::Pc, u16::from(ucd::CC))),
                7 => Some((PropKind::Pc, u16::from(ucd::ND))),
                8 => Some((PropKind::PxGraph, 0)),
                9 => Some((PropKind::PxPrint, 0)),
                10 => Some((PropKind::PxPunct, 0)),
                11 => Some((PropKind::PxSpace, 0)),
                12 => Some((PropKind::Word, 0)),
                13 => Some((PropKind::PxXdigit, 0)),
                _ => None,
            };
            if let Some((k, v)) = sub {
                let s = CharSet::from_ranges(ucd::property_ranges(k, v)).clamp(max);
                return if neg { s.negate(max) } else { s };
            }
            if idx == 5 {
                let s = CharSet::from_ranges(ucd::hspace()).clamp(max);
                return if neg { s.negate(max) } else { s };
            }
        }
        let idx = if opts.caseless && idx <= 2 { 0 } else { idx };
        let ranges: Vec<(u32, u32)> = match idx {
            0 => vec![(0x41, 0x5a), (0x61, 0x7a)],
            1 => vec![(0x61, 0x7a)],
            2 => vec![(0x41, 0x5a)],
            3 => vec![(0x30, 0x39), (0x41, 0x5a), (0x61, 0x7a)],
            4 => vec![(0, 0x7f)],
            5 => vec![(0x09, 0x09), (0x20, 0x20)],
            6 => vec![(0, 0x1f), (0x7f, 0x7f)],
            7 => vec![(0x30, 0x39)],
            8 => vec![(0x21, 0x7e)],
            9 => vec![(0x20, 0x7e)],
            10 => vec![(0x21, 0x2f), (0x3a, 0x40), (0x5b, 0x60), (0x7b, 0x7e)],
            11 => vec![(0x09, 0x0d), (0x20, 0x20)],
            12 => vec![(0x30, 0x39), (0x41, 0x5a), (0x5f, 0x5f), (0x61, 0x7a)],
            _ => vec![(0x30, 0x39), (0x41, 0x46), (0x61, 0x66)],
        };
        let s = CharSet::from_ranges(ranges);
        if neg { s.negate(max) } else { s }
    }

    fn open_frame(&mut self, kind: FrameKind, offset: usize, condassert_target: bool) {
        self.nest_depth += 1;
        let opts = self.opts;
        self.frames.push(Frame {
            kind,
            branches: Vec::new(),
            cur: Vec::new(),
            offset,
            opts,
            depth: self.nest_depth,
            reset: None,
            condassert: false,
            cond_assert_target: condassert_target,
        });
        self.okquantifier = false;
    }

    fn run(&mut self) -> Result<Group, Error> {
        self.frames.push(Frame {
            kind: FrameKind::Top,
            branches: Vec::new(),
            cur: Vec::new(),
            offset: 0,
            opts: self.opts,
            depth: 0,
            reset: None,
            condassert: false,
            cond_assert_target: false,
        });
        if self.opts.extended_more {
            self.opts.extended = true;
        }
        let mut inescq = false;
        let end = self.end();
        while self.ptr < end {
            if self.nest_depth > PARENS_NEST_LIMIT {
                return self.fail(19);
            }
            let c = self.next_char();
            if inescq {
                if c == u32::from(b'\\') && self.ptr < end && self.at(self.ptr) == b'E' {
                    inescq = false;
                    self.ptr += 1;
                } else {
                    self.last_quant = false;
                    self.push_lit(c);
                }
                continue;
            }
            if c == u32::from(b'\\') && self.ptr < end {
                let n = self.at(self.ptr);
                if n == b'Q' || n == b'E' {
                    if self.expect_cond_assert > 0
                        && n == b'Q'
                        && !(end - self.ptr >= 3
                            && self.at(self.ptr + 1) == b'\\'
                            && self.at(self.ptr + 2) == b'E')
                    {
                        self.ptr -= 1;
                        return self.fail(28);
                    }
                    inescq = n == b'Q';
                    self.ptr += 1;
                    continue;
                }
            }
            if self.opts.extended {
                if c < 256 && is_ctype_space(c) {
                    continue;
                }
                if c == 0x85 || (c | 1) == 0x200f || (c | 1) == 0x2029 {
                    continue;
                }
                if c == u32::from(b'#') {
                    while self.ptr < end {
                        if let Some(len) = self.is_newline_in_pattern(self.ptr) {
                            self.ptr += len;
                            break;
                        }
                        let (_, n) = self.get_char_at(self.ptr);
                        self.ptr += n;
                    }
                    continue;
                }
            }
            if c == u32::from(b'(')
                && end - self.ptr >= 2
                && self.at(self.ptr) == b'?'
                && self.at(self.ptr + 1) == b'#'
            {
                self.ptr += 1;
                loop {
                    self.ptr += 1;
                    if self.ptr >= end || self.at(self.ptr) == b')' {
                        break;
                    }
                }
                if self.ptr >= end {
                    return self.fail(18);
                }
                self.ptr += 1;
                continue;
            }
            if self.expect_cond_assert > 0 {
                let p = self.ptr;
                let mut ok = c == u32::from(b'(')
                    && end - p >= 3
                    && (self.at(p) == b'?' || self.at(p) == b'*');
                if ok {
                    if self.at(p) == b'*' {
                        ok = self.at(p + 1).is_ascii_lowercase();
                    } else {
                        ok = match self.at(p + 1) {
                            b'C' => self.expect_cond_assert == 2,
                            b'=' | b'!' => true,
                            b'<' => self.at(p + 2) == b'=' || self.at(p + 2) == b'!',
                            _ => false,
                        };
                    }
                }
                if !ok {
                    if self.expect_cond_assert == 2 {
                        return self.fail(28);
                    }
                    return self.fail_back(28);
                }
            }
            let prev_expect_cond_assert = self.expect_cond_assert;
            self.expect_cond_assert = 0;
            let prev_okquantifier = self.okquantifier;
            let prev_quant = self.last_quant;
            self.okquantifier = false;
            self.last_quant = false;
            if prev_quant && (c == u32::from(b'?') || c == u32::from(b'+')) {
                self.modify_last_quantifier(c == u32::from(b'+'));
                continue;
            }
            if c >= 0x80 || !b"\\^$.*+?{[()|".contains(&(c as u8)) {
                self.push_lit(c);
                continue;
            }
            match c as u8 {
                b'\\' => {
                    self.parse_escape_item()?;
                }
                b'^' => {
                    let a = if self.opts.multiline {
                        Assert::CircM
                    } else {
                        Assert::Circ
                    };
                    self.push_node(Node::Assert(a), false);
                }
                b'$' => {
                    let a = if self.opts.multiline {
                        Assert::DollM
                    } else {
                        Assert::Doll
                    };
                    self.push_node(Node::Assert(a), false);
                }
                b'.' => {
                    let n = if self.opts.dotall {
                        Node::AllAny
                    } else {
                        Node::Dot
                    };
                    self.push_node(n, true);
                }
                b'*' => self.apply_quantifier(prev_okquantifier, 0, None, 0)?,
                b'+' => self.apply_quantifier(prev_okquantifier, 1, None, 0)?,
                b'?' => self.apply_quantifier(prev_okquantifier, 0, Some(1), 0)?,
                b'{' => match self.read_repeat_counts(self.ptr) {
                    Err((code, at)) => return Self::fail_at(code, at),
                    Ok(None) => self.push_lit(c),
                    Ok(Some((min, max, np))) => {
                        self.ptr = np;
                        self.apply_quantifier(prev_okquantifier, min, max, 1)?;
                    }
                },
                b'[' => self.parse_class(ClassMode::Normal)?,
                b'(' => self.parse_open_paren(prev_expect_cond_assert)?,
                b'|' => {
                    let f = self.frames.last_mut().expect("frame");
                    if let Some((reset_group, max_group)) = f.reset.as_mut() {
                        if self.bracount > *max_group {
                            *max_group = self.bracount;
                        }
                        self.bracount = *reset_group;
                    }
                    let cur = std::mem::take(&mut f.cur);
                    f.branches.push(cur);
                    self.okquantifier = false;
                }
                b')' => self.close_paren()?,
                _ => self.push_lit(c),
            }
        }
        if self.nest_depth != 0 {
            return self.fail(14);
        }
        let mut top = self.frames.pop().expect("top frame");
        let cur = std::mem::take(&mut top.cur);
        top.branches.push(cur);
        Ok(Group {
            kind: GroupKind::Top,
            branches: top.branches,
            offset: 0,
        })
    }

    fn is_newline_in_pattern(&self, p: usize) -> Option<usize> {
        let end = self.end();
        let (c, n) = self.get_char_at(p);
        match self.newline {
            Newline::Lf => (c == 0x0a).then_some(1),
            Newline::Cr => (c == 0x0d).then_some(1),
            Newline::Nul => (c == 0).then_some(1),
            Newline::CrLf => (c == 0x0d && p + 1 < end && self.at(p + 1) == 0x0a).then_some(2),
            Newline::AnyCrLf => match c {
                0x0a => Some(1),
                0x0d => Some(if p + 1 < end && self.at(p + 1) == 0x0a {
                    2
                } else {
                    1
                }),
                _ => None,
            },
            Newline::Any => match c {
                0x0a..=0x0c => Some(1),
                0x0d => Some(if p + 1 < end && self.at(p + 1) == 0x0a {
                    2
                } else {
                    1
                }),
                0x85 | 0x2028 | 0x2029 => Some(n),
                _ => None,
            },
        }
    }

    fn modify_last_quantifier(&mut self, possessive: bool) {
        let ungreedy = self.opts.ungreedy;
        if let Some(Node::Repeat(r)) = self.cur().last_mut() {
            r.kind = if possessive {
                RepKind::Possessive
            } else if ungreedy {
                RepKind::Greedy
            } else {
                RepKind::Lazy
            };
        }
    }

    fn apply_quantifier(
        &mut self,
        prev_ok: bool,
        min: u32,
        max: Option<u32>,
        _kind: u32,
    ) -> Result<(), Error> {
        if !prev_ok {
            return self.fail(9);
        }
        if let Some(start) = self.accept_start.take()
            && matches!(self.cur().last(), Some(Node::Verb(Verb::Accept)))
        {
            let items: Vec<Node> = self.cur().drain(start..).collect();
            let g = Group {
                kind: GroupKind::NonCapture,
                branches: vec![items],
                offset: 0,
            };
            self.cur().push(Node::Group(Box::new(g)));
        }
        let kind = if self.opts.ungreedy {
            RepKind::Lazy
        } else {
            RepKind::Greedy
        };
        let node = self.cur().pop().expect("quantified item");
        self.cur().push(Node::Repeat(Box::new(Repeat {
            node,
            min,
            max,
            kind,
        })));
        self.last_quant = true;
        self.okquantifier = false;
        Ok(())
    }

    fn parse_escape_item(&mut self) -> Result<(), Error> {
        let esc = self.check_escape(false)?;
        match esc {
            Escape::Char(c) => self.push_lit(c),
            Escape::Backref(n) => {
                let offset = self.ptr;
                if n < 10 {
                    let slot = &mut self.small_ref_offset[n as usize];
                    if slot.is_none() {
                        *slot = Some(offset);
                    }
                }
                let br = BackRef {
                    target: RefTarget::Number(n),
                    groups: Vec::new(),
                    fold: self.opts.caseless,
                    restrict: self.opts.restrict,
                    offset,
                };
                self.push_node(Node::BackRef(Box::new(br)), true);
            }
            Escape::Special(e) => match e {
                Esc::UpperC => self.push_node(Node::AnyByte, true),
                Esc::UpperX => self.push_node(Node::ExtUni, true),
                Esc::UpperH
                | Esc::LowerH
                | Esc::UpperV
                | Esc::LowerV
                | Esc::LowerD
                | Esc::UpperD
                | Esc::LowerS
                | Esc::UpperS
                | Esc::LowerW
                | Esc::UpperW => {
                    let s = self.escape_set(e, self.opts);
                    self.push_node(Node::Set(Box::new(s)), true);
                }
                Esc::UpperN => self.push_node(Node::Dot, true),
                Esc::UpperR => self.push_node(Node::Newline, true),
                Esc::LowerP | Esc::UpperP => {
                    let (neg, kind, value) = self.get_ucp()?;
                    let neg = neg != (e == Esc::UpperP);
                    let n = if kind == PropKind::Any {
                        if neg {
                            Node::Set(Box::new(CharSet::new()))
                        } else {
                            Node::AllAny
                        }
                    } else {
                        Node::Set(Box::new(self.prop_set(
                            neg,
                            kind,
                            value,
                            self.opts.caseless,
                        )))
                    };
                    self.push_node(n, true);
                }
                Esc::LowerG | Esc::LowerK => {
                    let end = self.end();
                    if self.ptr >= end || !matches!(self.at(self.ptr), b'{' | b'<' | b'\'') {
                        return self.fail(if e == Esc::LowerG { 57 } else { 69 });
                    }
                    let open = self.at(self.ptr);
                    let terminator = match open {
                        b'<' => b'>',
                        b'\'' => b'\'',
                        _ => b'}',
                    };
                    if e == Esc::LowerG && terminator != b'}' {
                        let mut p = self.ptr + 1;
                        match self.read_number_noerr(
                            &mut p,
                            i64::from(self.bracount),
                            MAX_GROUP_NUMBER,
                        ) {
                            NumRes::Num(i) => {
                                if p >= end || self.at(p) != terminator {
                                    return Self::fail_at(119, p);
                                }
                                self.ptr = p + 1;
                                let offset = self.ptr;
                                let r = super::ast::Recurse {
                                    target: RefTarget::Number(i),
                                    group: i,
                                    offset,
                                    returns: Vec::new(),
                                    ret_groups: Vec::new(),
                                };
                                self.push_node(Node::Recurse(Box::new(r)), true);
                                return Ok(());
                            }
                            NumRes::TooBig => return self.fail(61),
                            NumRes::Zero => return self.fail(26),
                            NumRes::NonExistent => return self.fail(15),
                            NumRes::None => {}
                        }
                    }
                    let (name, offset) = self.read_name(terminator, true)?;
                    if e == Esc::LowerK || terminator == b'}' {
                        let br = BackRef {
                            target: RefTarget::Name(name),
                            groups: Vec::new(),
                            fold: self.opts.caseless,
                            restrict: self.opts.restrict,
                            offset,
                        };
                        self.push_node(Node::BackRef(Box::new(br)), true);
                    } else {
                        let r = super::ast::Recurse {
                            target: RefTarget::Name(name),
                            group: 0,
                            offset,
                            returns: Vec::new(),
                            ret_groups: Vec::new(),
                        };
                        self.push_node(Node::Recurse(Box::new(r)), true);
                    }
                }
                Esc::A => self.push_node(Node::Assert(Assert::Sod), false),
                Esc::G => self.push_node(Node::Assert(Assert::Som), false),
                Esc::K => self.push_node(Node::SetSom, false),
                Esc::LowerZ => self.push_node(Node::Assert(Assert::Eod), false),
                Esc::UpperZ => self.push_node(Node::Assert(Assert::EodN), false),
                Esc::LowerB | Esc::UpperB => {
                    let ucp = self.ucp && !self.opts.ascii_bsw;
                    let a = Assert::WordB {
                        ucp,
                        neg: e == Esc::UpperB,
                    };
                    self.push_node(Node::Assert(a), false);
                }
                Esc::UpperE | Esc::UpperQ => {}
            },
        }
        Ok(())
    }

    fn parse_class(&mut self, initial_mode: ClassMode) -> Result<(), Error> {
        let end = self.end();
        if initial_mode == ClassMode::Normal {
            let p = self.ptr;
            if end - p >= 6
                && (&self.pat[p..p + 6] == b"[:<:]]" || &self.pat[p..p + 6] == b"[:>:]]")
            {
                let start = self.at(p + 2) == b'<';
                let ucp = self.ucp;
                let w = if ucp {
                    CharSet::from_ranges(ucd::property_ranges(PropKind::Word, 0))
                        .clamp(self.max_char())
                } else {
                    CharSet::from_ranges(vec![
                        (0x30, 0x39),
                        (0x41, 0x5a),
                        (0x5f, 0x5f),
                        (0x61, 0x7a),
                    ])
                };
                let wb = Node::Assert(Assert::WordB {
                    ucp: self.ucp && !self.opts.ascii_bsw,
                    neg: false,
                });
                self.cur().push(wb);
                let kind = if start {
                    GroupKind::LookAhead {
                        neg: false,
                        atomic: true,
                    }
                } else {
                    self.has_lookbehind = true;
                    GroupKind::LookBehind {
                        neg: false,
                        atomic: true,
                    }
                };
                let g = Group {
                    kind,
                    branches: vec![vec![Node::Set(Box::new(w))]],
                    offset: 0,
                };
                self.cur().push(Node::Group(Box::new(g)));
                self.ptr += 6;
                self.okquantifier = true;
                return Ok(());
            }
            if p < end
                && matches!(self.at(p), b':' | b'.' | b'=')
                && let Some(tempptr) = self.check_posix_syntax(p)
            {
                let code = if self.at(p) == b':' { 12 } else { 13 };
                self.ptr = tempptr + 2;
                return self.fail(code);
            }
        }
        let items = self.parse_class_items(initial_mode)?;
        if initial_mode == ClassMode::Normal
            && let Some(lit) = self.class_as_literal(&items)
        {
            if lit.c == 0x0d || lit.c == 0x0a {
                self.has_crorlf = true;
            }
            self.push_node(Node::Lit(lit), true);
            return Ok(());
        }
        let set = self.eval_class(&items);
        self.push_node(Node::Set(Box::new(set)), true);
        Ok(())
    }

    fn class_as_literal(&self, items: &[CItem]) -> Option<Lit> {
        let opts = self.opts;
        match items {
            [CItem::Open(false, false), CItem::Char(c), CItem::Close] if *c != u32::MAX => {
                Some(Lit {
                    c: *c,
                    fold: opts.caseless,
                    restrict: opts.restrict,
                })
            }
            [
                CItem::Open(false, false),
                CItem::Char(c),
                CItem::Char(d),
                CItem::Close,
            ] if *c != u32::MAX && *d != u32::MAX => {
                let c = *c;
                let set = ucd::case_set(c);
                let multi = set.is_some_and(|s| s.len() > 2);
                if (multi && !(opts.restrict && c < 128 && *d < 128))
                    || (self.turkish && !opts.restrict && is_turkish_i(c))
                {
                    return None;
                }
                let other = if (self.utf || self.ucp) && c > 127 {
                    set.and_then(|s| s.iter().copied().find(|&o| o != c))
                        .unwrap_or(c)
                } else if c < 128 && (c as u8).is_ascii_alphabetic() {
                    c ^ 0x20
                } else {
                    c
                };
                (other != c && *d == other).then_some(Lit {
                    c,
                    fold: true,
                    restrict: opts.restrict,
                })
            }
            _ => None,
        }
    }

    fn parse_class_items(&mut self, initial_mode: ClassMode) -> Result<Vec<CItem>, Error> {
        let end = self.end();
        let mut items: Vec<CItem> = Vec::new();
        let mut inescq = false;
        let mut class_mode = initial_mode;
        let mut class_depth: i32 = -1;
        let mut range_state = RangeState::No;
        let mut op_state = OpState::Empty;
        let mut range_forbid_ptr = 0usize;
        let mut c: u32 = u32::from(b'[');
        let opts = self.opts;
        let extended_more = opts.extended_more;
        loop {
            let mut char_is_literal = true;
            let mut goto_literal = false;
            let mut skip_rest = false;
            if inescq {
                if c == u32::from(b'\\') && self.ptr < end && self.at(self.ptr) == b'E' {
                    inescq = false;
                    self.ptr += 1;
                    skip_rest = true;
                } else {
                    if class_mode == ClassMode::PerlExt {
                        return self.fail(116);
                    }
                    goto_literal = true;
                }
            }
            if !skip_rest && !goto_literal {
                if (c == 0x20 || c == 0x09) && (extended_more || class_mode >= ClassMode::PerlExt) {
                } else if class_depth >= 0
                    && c == u32::from(b'[')
                    && end - self.ptr >= 3
                    && matches!(self.at(self.ptr), b':' | b'.' | b'=')
                    && self.check_posix_syntax(self.ptr).is_some()
                {
                    let tempptr = self.check_posix_syntax(self.ptr).unwrap_or(0);
                    if range_state == RangeState::Started {
                        self.ptr = tempptr + 2;
                        return self.fail(50);
                    }
                    if range_state == RangeState::ForbidStarted {
                        self.ptr = range_forbid_ptr;
                        return self.fail(50);
                    }
                    if op_state == OpState::Operand && class_mode == ClassMode::PerlExt {
                        self.ptr = tempptr + 2;
                        return self.fail(113);
                    }
                    if self.at(self.ptr) != b':' {
                        self.ptr = tempptr + 2;
                        return self.fail(13);
                    }
                    self.ptr += 1;
                    let mut neg = false;
                    if self.at(self.ptr) == b'^' {
                        neg = true;
                        self.ptr += 1;
                    }
                    let name = &self.pat[self.ptr..tempptr];
                    let idx = POSIX_NAMES.iter().position(|n| n.as_bytes() == name);
                    self.ptr = tempptr + 2;
                    let Some(idx) = idx else {
                        return self.fail(30);
                    };
                    range_state = RangeState::ForbidNo;
                    op_state = OpState::Operand;
                    items.push(CItem::Set(self.posix_set(idx, neg, opts)));
                } else if (c == u32::from(b'[')
                    && (class_depth < 0 || class_mode == ClassMode::PerlExt))
                    || (c == u32::from(b'(') && class_mode == ClassMode::PerlExt)
                {
                    let start_c = c;
                    let new_mode = if start_c == u32::from(b'[')
                        && class_mode == ClassMode::PerlExt
                        && class_depth >= 0
                    {
                        ClassMode::PerlExtLeaf
                    } else {
                        class_mode
                    };
                    if range_state == RangeState::Started
                        && let Some(CItem::RangeStart) = items.last()
                    {
                        items.pop();
                        items.push(CItem::Char(u32::from(b'-')));
                    }
                    if op_state == OpState::Operand && class_mode == ClassMode::PerlExt {
                        return self.fail(113);
                    }
                    if class_depth >= ECLASS_NEST_LIMIT - 1 {
                        self.ptr -= 1;
                        return self.fail(107);
                    }
                    let mut negate = false;
                    loop {
                        if self.ptr >= end {
                            return self.fail(if start_c == u32::from(b'(') { 14 } else { 6 });
                        }
                        c = self.next_char();
                        if new_mode == ClassMode::PerlExt {
                            break;
                        } else if c == u32::from(b'\\') {
                            if self.ptr < end && self.at(self.ptr) == b'E' {
                                self.ptr += 1;
                            } else if end - self.ptr >= 3
                                && &self.pat[self.ptr..self.ptr + 3] == b"Q\\E"
                            {
                                self.ptr += 3;
                            } else {
                                break;
                            }
                        } else if (c == 0x20 || c == 0x09)
                            && (extended_more || new_mode >= ClassMode::PerlExt)
                        {
                        } else if !negate && c == u32::from(b'^') {
                            negate = true;
                        } else {
                            break;
                        }
                    }
                    items.push(CItem::Open(negate, new_mode == ClassMode::PerlExt));
                    range_state = RangeState::No;
                    op_state = OpState::Empty;
                    class_mode = new_mode;
                    class_depth += 1;
                    if c == u32::from(b']') && new_mode != ClassMode::PerlExt {
                        range_state = RangeState::OkLiteral;
                        op_state = OpState::Operand;
                        items.push(CItem::Char(c));
                    } else {
                        continue;
                    }
                } else if c == u32::from(b']')
                    || (c == u32::from(b')') && class_mode == ClassMode::PerlExt)
                {
                    if class_mode == ClassMode::PerlExt {
                        if c == u32::from(b']') && class_depth != 0 {
                            self.ptr -= 1;
                            return self.fail(14);
                        }
                        if c == u32::from(b')') && class_depth < 1 {
                            return self.fail(22);
                        }
                    }
                    if op_state == OpState::Operator {
                        return self.fail(110);
                    }
                    if class_mode == ClassMode::PerlExt && op_state == OpState::Empty {
                        return self.fail(114);
                    }
                    if range_state == RangeState::Started
                        && let Some(CItem::RangeStart) = items.last()
                    {
                        items.pop();
                        items.push(CItem::Char(u32::from(b'-')));
                    }
                    items.push(CItem::Close);
                    class_depth -= 1;
                    if class_depth < 0 {
                        if class_mode == ClassMode::PerlExt {
                            if self.ptr >= end || self.at(self.ptr) != b')' {
                                return self.fail(115);
                            }
                            self.ptr += 1;
                        }
                        break;
                    }
                    range_state = RangeState::No;
                    op_state = OpState::Operand;
                    if class_mode == ClassMode::PerlExtLeaf {
                        class_mode = ClassMode::PerlExt;
                    }
                } else if class_mode == ClassMode::PerlExt
                    && matches!(c, 0x2b | 0x7c | 0x2d | 0x26 | 0x5e)
                {
                    if op_state != OpState::Operand {
                        return self.fail(109);
                    }
                    let op = match c as u8 {
                        b'+' | b'|' => b'|',
                        b'-' => b'-',
                        b'&' => b'&',
                        _ => b'^',
                    };
                    items.push(CItem::Op(op));
                    range_state = RangeState::No;
                    op_state = OpState::Operator;
                } else if class_mode == ClassMode::PerlExt && c == u32::from(b'!') {
                    if op_state == OpState::Operand {
                        return self.fail(113);
                    }
                    items.push(CItem::Not);
                    range_state = RangeState::No;
                    op_state = OpState::Operator;
                } else if c == u32::from(b'\\') {
                    let esc = self.check_escape(true)?;
                    let mut set: Option<CharSet> = None;
                    match esc {
                        Escape::Char(v) => {
                            c = v;
                            char_is_literal = false;
                            goto_literal = true;
                        }
                        Escape::Backref(_) => {
                            goto_literal = true;
                        }
                        Escape::Special(e) => match e {
                            Esc::LowerB => {
                                c = 8;
                                char_is_literal = false;
                                goto_literal = true;
                            }
                            Esc::LowerK => {
                                c = u32::from(b'k');
                                char_is_literal = false;
                                goto_literal = true;
                            }
                            Esc::UpperQ => {
                                inescq = true;
                            }
                            Esc::UpperE => {}
                            Esc::UpperN => return self.fail(71),
                            Esc::LowerH
                            | Esc::UpperH
                            | Esc::LowerV
                            | Esc::UpperV
                            | Esc::LowerD
                            | Esc::UpperD
                            | Esc::LowerS
                            | Esc::UpperS
                            | Esc::LowerW
                            | Esc::UpperW => {
                                set = Some(self.escape_set(e, opts));
                            }
                            Esc::LowerP | Esc::UpperP => {
                                let (neg, kind, value) = self.get_ucp()?;
                                let neg = neg != (e == Esc::UpperP);
                                let s = if kind == PropKind::Any {
                                    if neg {
                                        CharSet::new()
                                    } else {
                                        CharSet::from_ranges(vec![(0, self.max_char())])
                                    }
                                } else {
                                    self.prop_set(neg, kind, value, opts.caseless)
                                };
                                set = Some(s);
                            }
                            _ => {
                                return self.fail(7);
                            }
                        },
                    }
                    if let Some(s) = set {
                        if range_state == RangeState::Started {
                            return self.fail(50);
                        }
                        if range_state == RangeState::ForbidStarted {
                            self.ptr = range_forbid_ptr;
                            return self.fail(50);
                        }
                        if op_state == OpState::Operand && class_mode == ClassMode::PerlExt {
                            return self.fail(113);
                        }
                        items.push(CItem::Set(s));
                        range_state = RangeState::ForbidNo;
                        op_state = OpState::Operand;
                    }
                } else if class_mode == ClassMode::PerlExt {
                    return self.fail(116);
                } else if c == u32::from(b'-') && range_state >= RangeState::OkEscaped {
                    items.push(CItem::RangeStart);
                    range_state = RangeState::Started;
                } else if c == u32::from(b'-') && range_state == RangeState::ForbidNo {
                    items.push(CItem::Char(u32::from(b'-')));
                    range_state = RangeState::ForbidStarted;
                    range_forbid_ptr = self.ptr;
                } else {
                    goto_literal = true;
                }
            }
            if goto_literal {
                if op_state == OpState::Operand && class_mode == ClassMode::PerlExt {
                    return self.fail(113);
                }
                if c == 0x0d || c == 0x0a {
                    self.has_crorlf = true;
                }
                if range_state == RangeState::Started {
                    items.pop();
                    let start = match items.last() {
                        Some(CItem::Char(a)) => *a,
                        _ => c,
                    };
                    match c.cmp(&start) {
                        std::cmp::Ordering::Equal => {}
                        std::cmp::Ordering::Less => return self.fail(8),
                        std::cmp::Ordering::Greater => {
                            items.pop();
                            items.push(CItem::Set(CharSet::from_ranges(vec![(start, c)])));
                            items.push(CItem::Char(u32::MAX));
                        }
                    }
                    range_state = RangeState::No;
                    op_state = OpState::Operand;
                } else if range_state == RangeState::ForbidStarted {
                    self.ptr = range_forbid_ptr;
                    return self.fail(50);
                } else {
                    range_state = if char_is_literal {
                        RangeState::OkLiteral
                    } else {
                        RangeState::OkEscaped
                    };
                    op_state = OpState::Operand;
                    items.push(CItem::Char(c));
                }
            }
            if self.ptr >= end {
                if class_mode == ClassMode::PerlExt && class_depth > 0 {
                    return self.fail(14);
                }
                return self.fail(6);
            }
            c = self.next_char();
        }
        Ok(items)
    }

    fn eval_class(&self, items: &[CItem]) -> CharSet {
        let opts = self.opts;
        let max = self.max_char();
        let mode = self.case_mode(opts);
        let fold = opts.caseless;
        let mut stack: Vec<ClassAcc> = Vec::new();
        let mut result: Option<CharSet> = None;
        let mut i = 0usize;
        while i < items.len() {
            match &items[i] {
                CItem::Open(neg, ext) => stack.push(ClassAcc::new(*neg, *ext, max)),
                CItem::Char(c) => {
                    if *c == u32::MAX {
                        i += 1;
                        continue;
                    }
                    let acc = stack.last_mut().expect("class");
                    acc.add_lit(*c, *c);
                }
                CItem::RangeStart => {
                    let acc = stack.last_mut().expect("class");
                    acc.add_lit(u32::from(b'-'), u32::from(b'-'));
                }
                CItem::Set(s) => {
                    let is_range = matches!(items.get(i + 1), Some(CItem::Char(u32::MAX)));
                    let acc = stack.last_mut().expect("class");
                    if is_range {
                        for &(a, b) in s.ranges() {
                            acc.add_lit(a, b);
                        }
                    } else {
                        acc.add_set(s);
                    }
                }
                CItem::Op(op) => {
                    let acc = stack.last_mut().expect("class");
                    acc.push_op(*op);
                }
                CItem::Not => {
                    let acc = stack.last_mut().expect("class");
                    acc.push_not();
                }
                CItem::Close => {
                    let acc = stack.pop().expect("class");
                    let set = acc.finish(fold, mode);
                    match stack.last_mut() {
                        Some(parent) => parent.add_operand(set),
                        None => result = Some(set),
                    }
                }
            }
            i += 1;
        }
        result.unwrap_or_default()
    }

    fn parse_open_paren(&mut self, prev_expect_cond_assert: i32) -> Result<(), Error> {
        let end = self.end();
        let open_ptr = self.ptr - 1;
        if self.ptr >= end {
            return self.fail(14);
        }
        if self.at(self.ptr) != b'?' {
            if self.at(self.ptr) != b'*' {
                if self.opts.no_auto_capture {
                    self.open_frame(FrameKind::Group(GroupKind::NonCapture), open_ptr, false);
                } else {
                    if self.bracount >= MAX_GROUP_NUMBER {
                        return self.fail(97);
                    }
                    self.bracount += 1;
                    let n = self.bracount;
                    self.open_frame(FrameKind::Group(GroupKind::Capture(n)), open_ptr, false);
                }
                return Ok(());
            }
            if end - self.ptr <= 1 || self.at(self.ptr + 1) == b')' {
                return Ok(());
            }
            let c1 = self.at(self.ptr + 1);
            if c1.is_ascii_lowercase() {
                let (name, name_off) = self.read_name(0, false)?;
                if self.ptr >= end {
                    return self.fail(14);
                }
                if self.at(self.ptr) != b':' {
                    return self.fail_forward(95);
                }
                let which = ALPHA_ASSERTIONS
                    .iter()
                    .find(|(n, _)| n.as_bytes() == name.as_slice())
                    .map(|x| x.1);
                let Some(which) = which else {
                    return self.fail(95);
                };
                if prev_expect_cond_assert > 0 && !matches!(which, 1..=4) {
                    return self.fail(28);
                }
                match which {
                    0 => {
                        self.ptr += 1;
                        self.open_frame(FrameKind::Group(GroupKind::Atomic), open_ptr, false);
                    }
                    1 => {
                        self.ptr += 1;
                        self.open_assertion(
                            GroupKind::LookAhead {
                                neg: false,
                                atomic: true,
                            },
                            open_ptr,
                            prev_expect_cond_assert,
                        );
                    }
                    2 => {
                        self.ptr += 1;
                        self.open_assertion(
                            GroupKind::LookAhead {
                                neg: true,
                                atomic: true,
                            },
                            open_ptr,
                            prev_expect_cond_assert,
                        );
                    }
                    3 | 4 => {
                        self.has_lookbehind = true;
                        let off = self.ptr - 1 - 2;
                        self.ptr += 1;
                        let _ = name_off;
                        self.open_assertion(
                            GroupKind::LookBehind {
                                neg: which == 4,
                                atomic: true,
                            },
                            off,
                            prev_expect_cond_assert,
                        );
                    }
                    5 => {
                        self.ptr += 1;
                        self.open_assertion(
                            GroupKind::LookAhead {
                                neg: false,
                                atomic: false,
                            },
                            open_ptr,
                            prev_expect_cond_assert,
                        );
                    }
                    6 => {
                        self.has_lookbehind = true;
                        let off = self.ptr - 1 - 2;
                        self.ptr += 1;
                        self.open_assertion(
                            GroupKind::LookBehind {
                                neg: false,
                                atomic: false,
                            },
                            off,
                            prev_expect_cond_assert,
                        );
                    }
                    7 => {
                        self.ptr += 1;
                        self.parse_scs(open_ptr)?;
                    }
                    8 => {
                        self.ptr += 1;
                        self.open_frame(FrameKind::Group(GroupKind::ScriptRun), open_ptr, false);
                    }
                    _ => {
                        self.ptr += 1;
                        self.open_frame(FrameKind::AtomicScriptRun, open_ptr, false);
                    }
                }
                return Ok(());
            }
            let (name, _) = self.read_name(0, false)?;
            if self.ptr >= end || (self.at(self.ptr) != b':' && self.at(self.ptr) != b')') {
                return self.fail(60);
            }
            let verb = VERBS
                .iter()
                .find(|(n, _)| n.as_bytes() == name.as_slice())
                .map(|x| x.1);
            let Some(verb) = verb else {
                return self.fail(60);
            };
            if self.at(self.ptr) == b':' && self.ptr + 1 < end && self.at(self.ptr + 1) == b')' {
                self.ptr += 1;
            }
            if verb <= 1 && self.at(self.ptr) != b':' {
                return self.fail(66);
            }
            let start_idx = self.cur().len();
            let has_arg = self.at(self.ptr) == b':';
            self.ptr += 1;
            let mut arg: Vec<u8> = Vec::new();
            if has_arg {
                let namestart = self.ptr;
                loop {
                    if self.ptr >= end {
                        return self.fail(60);
                    }
                    let (_, n) = self.get_char_at(self.ptr);
                    if self.at(self.ptr) == b')' {
                        self.ptr += 1;
                        if self.ptr - namestart - 1 > MAX_MARK {
                            self.ptr -= 1;
                            return self.fail(76);
                        }
                        break;
                    }
                    arg.extend_from_slice(&self.pat[self.ptr..self.ptr + n]);
                    self.ptr += n;
                }
            }
            let mut okq = false;
            match verb {
                0 | 1 => self.cur().push(Node::Verb(Verb::Mark(arg))),
                2 => {
                    if has_arg {
                        self.cur().push(Node::Verb(Verb::Mark(arg)));
                    }
                    self.cur().push(Node::Verb(Verb::Accept));
                    okq = true;
                }
                3 | 4 => {
                    if has_arg {
                        self.cur().push(Node::Verb(Verb::Mark(arg)));
                    }
                    self.cur().push(Node::Verb(Verb::Fail));
                }
                5 => self.cur().push(Node::Verb(Verb::Commit(has_arg))),
                6 => self.cur().push(Node::Verb(Verb::Prune(has_arg))),
                7 => {
                    if has_arg {
                        self.has_skip_arg = true;
                        self.cur().push(Node::Verb(Verb::SkipArg(arg)));
                    } else {
                        self.cur().push(Node::Verb(Verb::Skip));
                    }
                }
                _ => {
                    self.has_then = true;
                    self.cur().push(Node::Verb(Verb::Then(has_arg)));
                }
            }
            self.accept_start = Some(start_idx);
            self.okquantifier = okq;
            return Ok(());
        }
        self.ptr += 1;
        if self.ptr >= end {
            return self.fail(14);
        }
        let c = self.at(self.ptr);
        match c {
            b'P' => {
                self.ptr += 1;
                if self.ptr >= end {
                    return self.fail(14);
                }
                match self.at(self.ptr) {
                    b'<' => self.define_named_group(b'>', open_ptr),
                    b'>' => self.recurse_by_name(),
                    b'=' => {
                        let (name, offset) = self.read_name(b')', true)?;
                        let br = BackRef {
                            target: RefTarget::Name(name),
                            groups: Vec::new(),
                            fold: self.opts.caseless,
                            restrict: self.opts.restrict,
                            offset,
                        };
                        self.push_node(Node::BackRef(Box::new(br)), true);
                        Ok(())
                    }
                    _ => self.fail_forward(41),
                }
            }
            b'R' => {
                self.ptr += 1;
                if self.ptr >= end || !matches!(self.at(self.ptr), b')' | b'(') {
                    return self.fail(58);
                }
                self.finish_recursion(RefTarget::Number(0), 0, self.ptr)
            }
            b'+' | b'0'..=b'9' => {
                if c == b'+' {
                    if self.ptr + 1 >= end {
                        self.ptr += 1;
                        return self.fail(14);
                    }
                    if !is_digit(self.at(self.ptr + 1)) {
                        self.ptr += 1;
                        return self.fail_forward(29);
                    }
                }
                self.recursion_by_number()
            }
            b'-' if end - self.ptr > 1 && is_digit(self.at(self.ptr + 1)) => {
                self.recursion_by_number()
            }
            b'&' => self.recurse_by_name(),
            b'C' => self.parse_callout(prev_expect_cond_assert),
            b'(' => self.parse_condition(open_ptr),
            b'>' => {
                self.ptr += 1;
                self.open_frame(FrameKind::Group(GroupKind::Atomic), open_ptr, false);
                Ok(())
            }
            b'=' => {
                self.ptr += 1;
                self.open_assertion(
                    GroupKind::LookAhead {
                        neg: false,
                        atomic: true,
                    },
                    open_ptr,
                    prev_expect_cond_assert,
                );
                Ok(())
            }
            b'*' => {
                self.ptr += 1;
                self.open_assertion(
                    GroupKind::LookAhead {
                        neg: false,
                        atomic: false,
                    },
                    open_ptr,
                    prev_expect_cond_assert,
                );
                Ok(())
            }
            b'!' => {
                self.ptr += 1;
                self.open_assertion(
                    GroupKind::LookAhead {
                        neg: true,
                        atomic: true,
                    },
                    open_ptr,
                    prev_expect_cond_assert,
                );
                Ok(())
            }
            b'<' => {
                if end - self.ptr <= 1 || !matches!(self.at(self.ptr + 1), b'=' | b'!' | b'*') {
                    return self.define_named_group(b'>', open_ptr);
                }
                let k = self.at(self.ptr + 1);
                let kind = match k {
                    b'=' => GroupKind::LookBehind {
                        neg: false,
                        atomic: true,
                    },
                    b'!' => GroupKind::LookBehind {
                        neg: true,
                        atomic: true,
                    },
                    _ => GroupKind::LookBehind {
                        neg: false,
                        atomic: false,
                    },
                };
                self.has_lookbehind = true;
                let off = self.ptr - 2;
                self.ptr += 2;
                self.open_assertion(kind, off, prev_expect_cond_assert);
                Ok(())
            }
            b'\'' => self.define_named_group(b'\'', open_ptr),
            b'[' => {
                self.ptr += 1;
                let items = self.parse_class_items(ClassMode::PerlExt)?;
                let set = self.eval_class(&items);
                self.push_node(Node::Set(Box::new(set)), true);
                Ok(())
            }
            _ => self.parse_options(open_ptr),
        }
    }

    fn open_assertion(&mut self, kind: GroupKind, offset: usize, prev_expect: i32) {
        let condassert = prev_expect > 0;
        self.open_frame(FrameKind::Group(kind), offset, false);
        if condassert {
            let f = self.frames.last_mut().expect("frame");
            f.condassert = true;
        }
    }

    fn parse_capture_list(&mut self) -> Result<Vec<(RefTarget, usize)>, Error> {
        let end = self.end();
        if self.ptr >= end || self.at(self.ptr) != b'(' {
            return self.fail(118);
        }
        let mut refs = Vec::new();
        loop {
            self.ptr += 1;
            let item = self.ptr;
            if self.ptr >= end {
                return self.fail(117);
            }
            match self.read_number(i64::from(self.bracount), MAX_GROUP_NUMBER, 61)? {
                Some(0) => return self.fail(15),
                Some(i) => refs.push((RefTarget::Number(i), item)),
                None => {
                    let terminator = match self.at(self.ptr) {
                        b'<' => b'>',
                        b'\'' => b'\'',
                        _ => return self.fail(117),
                    };
                    let (name, off) = self.read_name(terminator, true)?;
                    refs.push((RefTarget::Name(name), off));
                }
            }
            if self.ptr >= end {
                return self.fail(14);
            }
            if self.at(self.ptr) == b')' {
                break;
            }
            if self.at(self.ptr) != b',' {
                return self.fail(24);
            }
        }
        self.ptr += 1;
        Ok(refs)
    }

    fn parse_scs(&mut self, open_ptr: usize) -> Result<(), Error> {
        let refs = self.parse_capture_list()?;
        self.open_frame(FrameKind::Group(GroupKind::Scs(refs)), open_ptr, false);
        Ok(())
    }

    fn define_named_group(&mut self, terminator: u8, open_ptr: usize) -> Result<(), Error> {
        let (name, _) = self.read_name(terminator, true)?;
        if self.bracount >= MAX_GROUP_NUMBER {
            return self.fail(97);
        }
        self.bracount += 1;
        let number = self.bracount;
        self.open_frame(
            FrameKind::Group(GroupKind::Capture(number)),
            open_ptr,
            false,
        );
        if self.names.len() >= MAX_NAME_COUNT {
            return self.fail(49);
        }
        let mut isdup = false;
        let mut skip = false;
        let dupnames = self.opts.dupnames;
        for i in 0..self.names.len() {
            if self.names[i].name == name {
                if self.names[i].number == number {
                    skip = true;
                    break;
                }
                if !dupnames {
                    return Err(Error::compile(43, self.ptr));
                }
                isdup = true;
                self.names[i].isdup = true;
                skip = self.names[i..]
                    .iter()
                    .any(|g| g.name == name && g.number == number);
                break;
            } else if self.names[i].number == number {
                return Err(Error::compile(65, self.ptr));
            }
        }
        if !skip {
            self.names.push(NamedGroup {
                name,
                number,
                isdup,
            });
        }
        Ok(())
    }

    fn recurse_by_name(&mut self) -> Result<(), Error> {
        let (name, offset) = self.read_name(0, true)?;
        self.finish_recursion(RefTarget::Name(name), 0, offset)
    }

    fn recursion_by_number(&mut self) -> Result<(), Error> {
        let allow = if is_digit(self.at(self.ptr)) {
            -1
        } else {
            i64::from(self.bracount)
        };
        let r = self.read_number(allow, MAX_GROUP_NUMBER, 61)?;
        let Some(i) = r else {
            return self.fail(14);
        };
        self.finish_recursion(RefTarget::Number(i), i, self.ptr)
    }

    fn finish_recursion(
        &mut self,
        target: RefTarget,
        group: u32,
        offset: usize,
    ) -> Result<(), Error> {
        let end = self.end();
        let returns = if self.ptr < end && self.at(self.ptr) == b'(' {
            self.parse_capture_list()?
        } else {
            Vec::new()
        };
        if self.ptr >= end || self.at(self.ptr) != b')' {
            return self.fail(14);
        }
        self.ptr += 1;
        let r = super::ast::Recurse {
            target,
            group,
            offset,
            returns,
            ret_groups: Vec::new(),
        };
        self.push_node(Node::Recurse(Box::new(r)), true);
        Ok(())
    }

    fn parse_callout(&mut self, prev_expect: i32) -> Result<(), Error> {
        let end = self.end();
        self.ptr += 1;
        if self.ptr >= end {
            return self.fail(14);
        }
        self.expect_cond_assert = prev_expect - 1;
        let c = self.at(self.ptr);
        if c != b')' && !is_digit(c) {
            let startptr = self.ptr;
            let delim = match c {
                b'`' | b'\'' | b'"' | b'^' | b'%' | b'#' | b'$' => c,
                b'{' => b'}',
                _ => return self.fail_forward(82),
            };
            loop {
                self.ptr += 1;
                if self.ptr >= end {
                    self.ptr = startptr;
                    return self.fail(81);
                }
                if self.at(self.ptr) == delim {
                    self.ptr += 1;
                    if self.ptr >= end || self.at(self.ptr) != delim {
                        break;
                    }
                }
            }
        } else {
            let mut n = 0u32;
            while self.ptr < end && is_digit(self.at(self.ptr)) {
                n = n * 10 + u32::from(self.at(self.ptr) - b'0');
                self.ptr += 1;
                if n > 255 {
                    return self.fail(38);
                }
            }
        }
        if self.ptr >= end || self.at(self.ptr) != b')' {
            return self.fail(39);
        }
        self.ptr += 1;
        Ok(())
    }

    fn parse_condition(&mut self, open_ptr: usize) -> Result<(), Error> {
        let end = self.end();
        self.ptr += 1;
        if self.ptr >= end {
            return self.fail(14);
        }
        if self.at(self.ptr) == b'?' || self.at(self.ptr) == b'*' {
            self.open_frame(
                FrameKind::Cond {
                    kind: None,
                    name_offset: open_ptr,
                },
                0,
                false,
            );
            self.ptr -= 1;
            self.expect_cond_assert = 2;
            return Ok(());
        }
        let kind: CondKind;
        let mut name_offset = open_ptr;
        let mut p = self.ptr;
        match self.read_number_noerr(&mut p, i64::from(self.bracount), MAX_GROUP_NUMBER) {
            NumRes::Num(i) => {
                self.ptr = p;
                if i == 0 {
                    return self.fail(15);
                }
                name_offset = self.ptr - 2;
                kind = CondKind::Group(RefTarget::Number(i), Vec::new());
            }
            NumRes::TooBig => {
                self.ptr = p;
                return self.fail(61);
            }
            NumRes::Zero => {
                self.ptr = p;
                return self.fail(26);
            }
            NumRes::NonExistent => {
                self.ptr = p;
                return self.fail(15);
            }
            NumRes::None => {
                if end - self.ptr >= 10
                    && &self.pat[self.ptr..self.ptr + 7] == b"VERSION"
                    && self.at(self.ptr + 7) != b')'
                {
                    self.ptr += 7;
                    let mut ge = false;
                    if self.at(self.ptr) == b'>' {
                        ge = true;
                        self.ptr += 1;
                    }
                    let eq = self.at(self.ptr) == b'=';
                    if eq {
                        self.ptr += 1;
                    }
                    if !eq || self.ptr >= end || !is_digit(self.at(self.ptr)) {
                        if !ge && self.ptr < end {
                            return self.fail_forward(79);
                        }
                        return self.fail(79);
                    }
                    let major = self.read_number(-1, 1000, 79)?.unwrap_or(0);
                    let mut minor = 0u32;
                    if self.ptr < end && self.at(self.ptr) == b'.' {
                        self.ptr += 1;
                        if self.ptr >= end || !is_digit(self.at(self.ptr)) {
                            if self.ptr < end {
                                return self.fail_forward(79);
                            }
                            return self.fail(79);
                        }
                        minor = self.read_number(-1, 1000, 79)?.unwrap_or(0);
                    }
                    if self.ptr >= end || self.at(self.ptr) != b')' {
                        if self.ptr < end {
                            return self.fail_forward(79);
                        }
                        return self.fail(79);
                    }
                    let val = if ge {
                        10 > major || (major == 10 && 48 >= minor)
                    } else {
                        major == 10 && minor == 48
                    };
                    kind = CondKind::Bool(val);
                } else {
                    let mut was_r_amp = false;
                    let terminator;
                    if self.at(self.ptr) == b'R'
                        && end - self.ptr > 1
                        && self.at(self.ptr + 1) == b'&'
                    {
                        terminator = b')';
                        was_r_amp = true;
                        self.ptr += 1;
                    } else if self.at(self.ptr) == b'<' {
                        terminator = b'>';
                    } else if self.at(self.ptr) == b'\'' {
                        terminator = b'\'';
                    } else {
                        terminator = b')';
                        self.ptr -= 1;
                    }
                    let (name, noff) = self.read_name(terminator, true)?;
                    name_offset = noff;
                    if was_r_amp {
                        kind = CondKind::RName(name, Vec::new());
                        self.ptr -= 1;
                    } else if terminator == b')' {
                        if name == b"DEFINE" {
                            kind = CondKind::Define;
                        } else if name[0] == b'R' && name[1..].iter().all(u8::is_ascii_digit) {
                            kind = CondKind::RNumber(name, None, Vec::new());
                        } else {
                            kind = CondKind::Group(RefTarget::Name(name), Vec::new());
                        }
                        self.ptr -= 1;
                    } else {
                        kind = CondKind::Group(RefTarget::Name(name), Vec::new());
                    }
                }
            }
        }
        if self.ptr >= end || self.at(self.ptr) != b')' {
            return self.fail(24);
        }
        self.ptr += 1;
        let err_offset = match &kind {
            CondKind::Group(RefTarget::Number(_), _) => name_offset.saturating_sub(2),
            CondKind::Bool(_) => 0,
            _ => name_offset,
        };
        self.open_frame(
            FrameKind::Cond {
                kind: Some(kind),
                name_offset,
            },
            err_offset,
            false,
        );
        Ok(())
    }

    fn parse_options(&mut self, open_ptr: usize) -> Result<(), Error> {
        let end = self.end();
        let c = self.at(self.ptr);
        if c == b'|' {
            self.open_frame(FrameKind::Group(GroupKind::NonCapture), open_ptr, false);
            let bc = self.bracount;
            let f = self.frames.last_mut().expect("frame");
            f.reset = Some((bc, bc));
            self.dupcap_used = true;
            self.ptr += 1;
            return Ok(());
        }
        let old = self.opts;
        let mut hyphenok = true;
        let mut set = Opts::default();
        let mut unset = Opts::default();
        let mut setting = true;
        let mut opts = self.opts;
        if self.ptr < end && self.at(self.ptr) == b'^' {
            opts.caseless = false;
            opts.multiline = false;
            opts.no_auto_capture = false;
            opts.dotall = false;
            opts.extended = false;
            opts.extended_more = false;
            opts.restrict = false;
            hyphenok = false;
            self.ptr += 1;
        }
        while self.ptr < end && self.at(self.ptr) != b')' && self.at(self.ptr) != b':' {
            let ch = self.at(self.ptr);
            self.ptr += 1;
            let target = if setting { &mut set } else { &mut unset };
            match ch {
                b'-' => {
                    if !hyphenok {
                        return self.fail(94);
                    }
                    setting = false;
                    hyphenok = false;
                }
                b'a' => {
                    let next = if self.ptr < end { self.at(self.ptr) } else { 0 };
                    match next {
                        b'D' => {
                            target.ascii_bsd = true;
                            self.ptr += 1;
                        }
                        b'P' => {
                            target.ascii_posix = true;
                            target.ascii_digit = true;
                            self.ptr += 1;
                        }
                        b'S' => {
                            target.ascii_bss = true;
                            self.ptr += 1;
                        }
                        b'T' => {
                            target.ascii_digit = true;
                            self.ptr += 1;
                        }
                        b'W' => {
                            target.ascii_bsw = true;
                            self.ptr += 1;
                        }
                        _ => {
                            target.ascii_bsd = true;
                            target.ascii_bss = true;
                            target.ascii_bsw = true;
                            target.ascii_digit = true;
                            target.ascii_posix = true;
                        }
                    }
                }
                b'J' => target.dupnames = true,
                b'i' => target.caseless = true,
                b'm' => target.multiline = true,
                b'n' => target.no_auto_capture = true,
                b'r' => target.restrict = true,
                b's' => target.dotall = true,
                b'U' => target.ungreedy = true,
                b'x' => {
                    target.extended = true;
                    if self.ptr < end && self.at(self.ptr) == b'x' {
                        target.extended_more = true;
                        self.ptr += 1;
                    }
                }
                _ => {
                    return self.fail(11);
                }
            }
        }
        if (set.extended && !set.extended_more) || unset.extended {
            unset.extended_more = true;
        }
        apply_opts(&mut opts, &set, true);
        apply_opts(&mut opts, &unset, false);
        if self.ptr >= end {
            return self.fail(14);
        }
        let closing = self.at(self.ptr) == b')';
        self.ptr += 1;
        if closing {
            self.opts = opts;
            self.okquantifier = false;
        } else {
            self.opts = old;
            self.open_frame(FrameKind::Group(GroupKind::NonCapture), open_ptr, false);
            self.opts = opts;
        }
        Ok(())
    }

    fn close_paren(&mut self) -> Result<(), Error> {
        if self.frames.len() <= 1 {
            return self.fail(22);
        }
        let mut f = self.frames.pop().expect("frame");
        self.opts = f.opts;
        if let Some((_, max_group)) = f.reset
            && max_group > self.bracount
        {
            self.bracount = max_group;
        }
        let _ = f.depth;
        self.nest_depth -= 1;
        let cur = std::mem::take(&mut f.cur);
        f.branches.push(cur);
        let okq = !f.condassert;
        let node = match f.kind {
            FrameKind::Top => unreachable!("top frame"),
            FrameKind::Group(kind) => Node::Group(Box::new(Group {
                kind,
                branches: f.branches,
                offset: f.offset,
            })),
            FrameKind::AtomicScriptRun => {
                let inner = Group {
                    kind: GroupKind::Atomic,
                    branches: f.branches,
                    offset: f.offset,
                };
                Node::Group(Box::new(Group {
                    kind: GroupKind::ScriptRun,
                    branches: vec![vec![Node::Group(Box::new(inner))]],
                    offset: f.offset,
                }))
            }
            FrameKind::Cond { kind, name_offset } => {
                let mut branches = f.branches;
                let kind = if let Some(k) = kind {
                    k
                } else {
                    let first = branches.first_mut().expect("branch");
                    let pos = first
                        .iter()
                        .position(|n| matches!(n, Node::Group(g) if matches!(g.kind, GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. })));
                    match pos {
                        Some(i) => {
                            let n = first.remove(i);
                            let Node::Group(g) = n else { unreachable!() };
                            CondKind::Assert(g)
                        }
                        None => CondKind::Bool(false),
                    }
                };
                Node::Cond(Box::new(Cond {
                    kind,
                    branches,
                    offset: f.offset,
                    name_offset,
                }))
            }
        };
        let _ = f.cond_assert_target;
        self.cur().push(node);
        self.okquantifier = okq;
        Ok(())
    }
}

fn apply_opts(opts: &mut Opts, delta: &Opts, set: bool) {
    let apply = |flag: bool, target: &mut bool| {
        if flag {
            *target = set;
        }
    };
    apply(delta.caseless, &mut opts.caseless);
    apply(delta.multiline, &mut opts.multiline);
    apply(delta.dotall, &mut opts.dotall);
    apply(delta.extended, &mut opts.extended);
    apply(delta.extended_more, &mut opts.extended_more);
    apply(delta.no_auto_capture, &mut opts.no_auto_capture);
    apply(delta.dupnames, &mut opts.dupnames);
    apply(delta.ungreedy, &mut opts.ungreedy);
    apply(delta.restrict, &mut opts.restrict);
    apply(delta.ascii_bsd, &mut opts.ascii_bsd);
    apply(delta.ascii_bss, &mut opts.ascii_bss);
    apply(delta.ascii_bsw, &mut opts.ascii_bsw);
    apply(delta.ascii_digit, &mut opts.ascii_digit);
    apply(delta.ascii_posix, &mut opts.ascii_posix);
}

enum NumRes {
    None,
    Num(u32),
    TooBig,
    Zero,
    NonExistent,
}

const PC_DIGIT: usize = 7;
const PC_XDIGIT: usize = 13;

static POSIX_NAMES: [&str; 14] = [
    "alpha", "lower", "upper", "alnum", "ascii", "blank", "cntrl", "digit", "graph", "print",
    "punct", "space", "word", "xdigit",
];

static ALPHA_ASSERTIONS: [(&str, u8); 19] = [
    ("pla", 1),
    ("plb", 3),
    ("napla", 5),
    ("naplb", 6),
    ("nla", 2),
    ("nlb", 4),
    ("positive_lookahead", 1),
    ("positive_lookbehind", 3),
    ("non_atomic_positive_lookahead", 5),
    ("non_atomic_positive_lookbehind", 6),
    ("negative_lookahead", 2),
    ("negative_lookbehind", 4),
    ("scs", 7),
    ("scan_substring", 7),
    ("atomic", 0),
    ("sr", 8),
    ("asr", 9),
    ("script_run", 8),
    ("atomic_script_run", 9),
];

static VERBS: [(&str, u8); 9] = [
    ("", 0),
    ("MARK", 1),
    ("ACCEPT", 2),
    ("F", 3),
    ("FAIL", 4),
    ("COMMIT", 5),
    ("PRUNE", 6),
    ("SKIP", 7),
    ("THEN", 8),
];

struct ClassAcc {
    neg: bool,
    ext: bool,
    max: u32,
    lits: Vec<(u32, u32)>,
    sets: Vec<CharSet>,
    operands: Vec<CharSet>,
    ops: Vec<u8>,
}

impl ClassAcc {
    fn new(neg: bool, ext: bool, max: u32) -> ClassAcc {
        ClassAcc {
            neg,
            ext,
            max,
            lits: Vec::new(),
            sets: Vec::new(),
            operands: Vec::new(),
            ops: Vec::new(),
        }
    }

    fn add_lit(&mut self, a: u32, b: u32) {
        self.lits.push((a, b));
    }

    fn add_set(&mut self, s: &CharSet) {
        if self.ext {
            self.add_operand(s.clone());
        } else {
            self.sets.push(s.clone());
        }
    }

    fn add_operand(&mut self, s: CharSet) {
        if !self.ext {
            self.sets.push(s);
            return;
        }
        let mut s = s;
        while self.ops.last() == Some(&b'!') {
            self.ops.pop();
            s = s.negate(self.max);
        }
        self.operands.push(s);
    }

    fn push_not(&mut self) {
        self.ops.push(b'!');
    }

    fn reduce(&mut self, min_prec: u8) {
        while let Some(&op) = self.ops.last() {
            if op == b'!' {
                break;
            }
            let prec = if op == b'&' { 2 } else { 1 };
            if prec < min_prec {
                break;
            }
            self.ops.pop();
            let b = self.operands.pop().unwrap_or_default();
            let a = self.operands.pop().unwrap_or_default();
            let r = match op {
                b'&' => a.intersect(&b),
                b'|' => a.union(&b),
                b'-' => a.subtract(&b, self.max),
                _ => a.xor(&b, self.max),
            };
            self.operands.push(r);
        }
    }

    fn push_op(&mut self, op: u8) {
        let prec = if op == b'&' { 2 } else { 1 };
        self.reduce(prec);
        self.ops.push(op);
    }

    fn finish(mut self, fold: bool, mode: CaseMode) -> CharSet {
        let max = self.max;
        let mut result = if self.ext {
            self.reduce(0);
            self.operands.pop().unwrap_or_default()
        } else {
            let mut leaf = CharSet::from_ranges(std::mem::take(&mut self.lits));
            if fold {
                leaf = leaf.case_closure(mode);
            }
            for s in &self.sets {
                leaf = leaf.union(s);
            }
            leaf
        };
        result = result.clamp(max);
        if self.neg {
            result = result.negate(max);
        }
        result
    }
}
