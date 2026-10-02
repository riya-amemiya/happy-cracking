use super::ast::{Assert, Ast, CondKind, GroupKind, Newline, Node, Verb};
use super::lits::Analysis;
use super::prog::{Inst, Item, NONE, Prog};
use super::start;
use memchr::memmem;

const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug)]
enum Searcher {
    Byte(u8),
    Byte2(u8, u8),
    Byte3(u8, u8, u8),
    Table(Box<[bool; 256]>, bool),
    Memmem(Box<memmem::Finder<'static>>),
    Multi(regex::bytes::Regex),
}

impl Searcher {
    fn from_bytes(set: &[bool; 256]) -> Searcher {
        let bytes: Vec<u8> = (0..=255u8).filter(|&b| set[b as usize]).collect();
        match bytes.as_slice() {
            [a] => Searcher::Byte(*a),
            [a, b] => Searcher::Byte2(*a, *b),
            [a, b, c] => Searcher::Byte3(*a, *b, *c),
            _ => Searcher::Table(Box::new(*set), false),
        }
    }

    fn from_lits(lits: &[Vec<u8>]) -> Option<Searcher> {
        if lits.iter().all(|l| l.len() == 1) {
            let mut set = [false; 256];
            for l in lits {
                set[l[0] as usize] = true;
            }
            return Some(Searcher::from_bytes(&set));
        }
        if lits.len() == 1 {
            return Some(Searcher::Memmem(Box::new(
                memmem::Finder::new(&lits[0]).into_owned(),
            )));
        }
        let mut pat = String::from("(?s-u:");
        for (i, l) in lits.iter().enumerate() {
            if i > 0 {
                pat.push('|');
            }
            for &b in l {
                pat.push_str("\\x");
                pat.push(char::from(HEX[usize::from(b >> 4)]));
                pat.push(char::from(HEX[usize::from(b & 15)]));
            }
        }
        pat.push(')');
        regex::bytes::RegexBuilder::new(&pat)
            .unicode(false)
            .build()
            .ok()
            .map(Searcher::Multi)
    }

    fn find(&self, hay: &[u8], p: usize) -> Option<usize> {
        let rest = &hay[p..];
        let i = match self {
            Searcher::Byte(a) => memchr::memchr(*a, rest),
            Searcher::Byte2(a, b) => memchr::memchr2(*a, *b, rest),
            Searcher::Byte3(a, b, c) => memchr::memchr3(*a, *b, *c, rest),
            Searcher::Table(t, false) => rest.iter().position(|&b| t[b as usize]),
            Searcher::Table(t, true) => {
                let mut i = p;
                loop {
                    let off = hay[i..].iter().position(|&b| t[b as usize])?;
                    i += off;
                    let cur = hay[i];
                    let prev = if i == 0 {
                        Some(false)
                    } else {
                        (hay[i - 1] < 0x80).then(|| is_word_byte(hay[i - 1]))
                    };
                    let cur_w = (cur < 0x80).then(|| is_word_byte(cur));
                    match (prev, cur_w) {
                        (Some(a), Some(b)) if a == b => i += 1,
                        _ => return Some(i),
                    }
                }
            }
            Searcher::Memmem(f) => f.find(rest),
            Searcher::Multi(re) => re.find(rest).map(|m| m.start()),
        };
        i.map(|i| p + i)
    }
}

#[derive(Debug)]
pub struct Prefilter {
    anchored: bool,
    startline: bool,
    first: Option<(u8, Option<u8>)>,
    req: Option<(u8, Option<u8>)>,
    minlen: usize,
    interp: bool,
    searcher: Option<Searcher>,
    lead_rep: Option<Item>,
    required: Option<memmem::Finder<'static>>,
    lookback: usize,
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn starts_with_wordb(nodes: &[Node]) -> bool {
    match nodes.first() {
        Some(Node::Assert(Assert::WordB { neg: false, .. })) => true,
        Some(Node::Group(g)) => {
            matches!(
                g.kind,
                GroupKind::NonCapture | GroupKind::Capture(_) | GroupKind::Atomic
            ) && g.branches.iter().all(|b| starts_with_wordb(b))
        }
        _ => false,
    }
}

fn has_exact_verbs(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Verb(Verb::Commit(_) | Verb::Prune(_) | Verb::Skip | Verb::SkipArg(_)) | Node::Recurse(_) => true,
        Node::Group(g) => g.branches.iter().any(|b| has_exact_verbs(b)),
        Node::Repeat(r) => has_exact_verbs(std::slice::from_ref(&r.node)),
        Node::Cond(c) => {
            c.branches.iter().any(|b| has_exact_verbs(b))
                || matches!(&c.kind, CondKind::Assert(g) if g.branches.iter().any(|b| has_exact_verbs(b)))
        }
        _ => false,
    })
}

impl Prefilter {
    pub fn new(ast: &Ast, prog: &Prog) -> Prefilter {
        let root = &ast.root.branches;
        let anchoring = start::anchoring(ast);
        let anchored = anchoring.anchored;
        let units = if ast.no_start_opt {
            start::CodeUnits::default()
        } else {
            start::analyze(ast)
        };
        let startline = !ast.no_start_opt && units.first.is_none() && anchoring.startline;
        let req = if anchored && !units.req_vary {
            None
        } else {
            units.req
        };
        let minlen = if ast.no_start_opt || units.has_accept {
            0
        } else {
            let m = root
                .iter()
                .map(|b| super::compile::min_len_nodes(b))
                .min()
                .unwrap_or(0);
            usize::try_from(m.min(65535)).unwrap_or(65535)
        };
        let exact = root.iter().any(|b| has_exact_verbs(b));
        let analysis = Analysis::new(ast);
        let searcher = if exact || anchored || startline {
            None
        } else {
            match analysis.prefixes().and_then(|l| Searcher::from_lits(&l)) {
                Some(s) => Some(s),
                None if units.first.is_some() => None,
                None => analysis.first_bytes().map(|set| {
                    if root.iter().all(|b| starts_with_wordb(b)) {
                        Searcher::Table(set, true)
                    } else {
                        Searcher::from_bytes(&set)
                    }
                }),
            }
        };
        let required = if exact {
            None
        } else {
            analysis
                .required()
                .map(|r| memmem::Finder::new(&r).into_owned())
        };
        Prefilter {
            anchored,
            startline,
            first: units.first,
            req,
            minlen,
            interp: !prog.jit,
            searcher,
            lead_rep: match prog.insts.first() {
                Some(Inst::Rep { item, min, max, .. }) if !exact && *min >= 1 && *max == NONE => {
                    Some(*item)
                }
                _ => None,
            },
            required,
            lookback: (prog.max_lookbehind as usize).saturating_mul(4),
        }
    }

    pub fn lead_rep(&self) -> Option<Item> {
        self.lead_rep
    }

    pub fn rejects(&self, hay: &[u8], start: usize) -> bool {
        match &self.required {
            Some(f) => f
                .find(&hay[start.saturating_sub(self.lookback)..])
                .is_none(),
            None => false,
        }
    }

    pub fn viable(&self, hay: &[u8], p: usize, limit: usize, req_at: &mut usize) -> bool {
        let rest = limit - p;
        if rest < self.minlen {
            return false;
        }
        let Some((r, other)) = self.req else {
            return true;
        };
        let in_window = if self.interp {
            rest < 5000 || (!self.anchored && rest < 5_000_000)
        } else {
            rest <= 500_000
        };
        if !in_window {
            return true;
        }
        let from = p + usize::from(self.first.is_some());
        let searched = *req_at == usize::MAX
            || if self.interp {
                from > *req_at
            } else {
                p >= *req_at
            };
        if !searched {
            return true;
        }
        if from >= limit {
            return false;
        }
        let hay = &hay[from..limit];
        let found = match other {
            Some(o) => memchr::memchr2(r, o, hay),
            None => memchr::memchr(r, hay),
        };
        match found {
            Some(i) => {
                *req_at = from + i;
                true
            }
            None => false,
        }
    }

    pub fn anchored(&self) -> bool {
        self.anchored
    }

    pub fn next(&self, prog: &Prog, hay: &[u8], p: usize, start: usize) -> Option<usize> {
        if p > hay.len() {
            return None;
        }
        if self.anchored {
            return (p == start).then_some(p);
        }
        if let Some(s) = &self.searcher {
            return s.find(hay, p);
        }
        if let Some((b, other)) = self.first {
            let rest = &hay[p..];
            let found = match other {
                Some(o) => memchr::memchr2(b, o, rest),
                None => memchr::memchr(b, rest),
            };
            return found.map(|i| p + i);
        }
        if self.startline && p > start {
            return next_line_start(prog, hay, p);
        }
        Some(p)
    }
}

fn next_line_start(prog: &Prog, hay: &[u8], p: usize) -> Option<usize> {
    let from = p - 1;
    let q = match prog.newline {
        Newline::Lf | Newline::CrLf => memchr::memchr(b'\n', &hay[from..]).map(|i| from + i + 1),
        Newline::Cr => memchr::memchr(b'\r', &hay[from..]).map(|i| from + i + 1),
        Newline::Nul => memchr::memchr(0, &hay[from..]).map(|i| from + i + 1),
        Newline::AnyCrLf => memchr::memchr2(b'\n', b'\r', &hay[from..]).map(|i| {
            let q = from + i + 1;
            if hay[q - 1] == b'\r' && q < hay.len() && hay[q] == b'\n' {
                q + 1
            } else {
                q
            }
        }),
        Newline::Any => {
            let mut i = from;
            let mut found = None;
            while i < hay.len() {
                let b = hay[i];
                let len = match b {
                    0x0a..=0x0d => Some(1),
                    0x85 if !prog.utf => Some(1),
                    0xc2 if prog.utf && i + 1 < hay.len() && hay[i + 1] == 0x85 => Some(2),
                    0xe2 if prog.utf
                        && i + 2 < hay.len()
                        && hay[i + 1] == 0x80
                        && (hay[i + 2] | 1) == 0xa9 =>
                    {
                        Some(3)
                    }
                    _ => None,
                };
                if let Some(l) = len {
                    let mut q = i + l;
                    if b == b'\r' && q < hay.len() && hay[q] == b'\n' {
                        q += 1;
                    }
                    found = Some(q);
                    break;
                }
                i += 1;
            }
            found
        }
    };
    let q = q?;
    if q < p {
        return Some(p);
    }
    if q >= hay.len() && q > hay.len() {
        return None;
    }
    Some(q)
}
