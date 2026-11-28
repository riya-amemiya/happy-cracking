use std::collections::HashMap;

use super::super::locale::MAX_CHAR;
use super::ast::{Look, Node, UnitSet};

const MATCH: u32 = u32::MAX;
const FLAG: u32 = 0x8000_0000;
const MAX_STATES: usize = 3000;
const MAX_INSTS: usize = 60_000;
const MAX_WORK: usize = 6_000_000;

#[derive(Clone, Copy, Debug)]
enum BI {
    Bytes(u8, u8, u32),
    Split(u32, u32),
    Look(Look, u32),
    Match,
}

struct Builder {
    insts: Vec<BI>,
    utf8: bool,
}

#[derive(Debug)]
struct Fail;

fn utf8_encode(c: u32) -> Vec<u8> {
    char::from_u32(c).map_or_else(Vec::new, |ch| {
        let mut b = [0u8; 4];
        ch.encode_utf8(&mut b).as_bytes().to_vec()
    })
}

fn utf8_split(lo: u32, hi: u32, out: &mut Vec<Vec<(u8, u8)>>) {
    if lo > hi {
        return;
    }
    if lo <= 0xd7ff && hi >= 0xd800 {
        utf8_split(lo, 0xd7ff, out);
        utf8_split(0xe000.max(lo), hi, out);
        return;
    }
    if (0xd800..=0xdfff).contains(&lo) {
        utf8_split(0xe000, hi, out);
        return;
    }
    for max in [0x7f, 0x7ff, 0xffff] {
        if lo <= max && max < hi {
            utf8_split(lo, max, out);
            utf8_split(max + 1, hi, out);
            return;
        }
    }
    if hi < 0x80 {
        out.push(vec![(lo as u8, hi as u8)]);
        return;
    }
    for i in 1..4 {
        let m: u32 = (1 << (6 * i)) - 1;
        if lo & !m != hi & !m {
            if lo & m != 0 {
                utf8_split(lo, lo | m, out);
                utf8_split((lo | m) + 1, hi, out);
                return;
            }
            if hi & m != m {
                utf8_split(lo, (hi & !m) - 1, out);
                utf8_split(hi & !m, hi, out);
                return;
            }
        }
    }
    let a = utf8_encode(lo);
    let b = utf8_encode(hi);
    out.push(a.into_iter().zip(b).collect());
}

#[derive(Default)]
struct Trie {
    children: Vec<((u8, u8), Trie)>,
    leaf: bool,
}

impl Trie {
    fn insert(&mut self, seq: &[(u8, u8)]) {
        match seq.split_first() {
            None => self.leaf = true,
            Some((r, rest)) => {
                if let Some(idx) = self.children.iter().position(|(k, _)| k == r) {
                    self.children[idx].1.insert(rest);
                } else {
                    let mut t = Trie::default();
                    t.insert(rest);
                    self.children.push((*r, t));
                }
            }
        }
    }
}

impl Builder {
    fn push(&mut self, i: BI) -> Result<u32, Fail> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Fail);
        }
        self.insts.push(i);
        Ok((self.insts.len() - 1) as u32)
    }

    fn alts(&mut self, entries: &[u32], next: u32) -> Result<u32, Fail> {
        let Some((&last, rest)) = entries.split_last() else {
            return Ok(next);
        };
        let mut cur = last;
        for &e in rest.iter().rev() {
            cur = self.push(BI::Split(e, cur))?;
        }
        Ok(cur)
    }

    fn trie(&mut self, t: &Trie, next: u32) -> Result<u32, Fail> {
        let mut entries = Vec::new();
        for ((lo, hi), child) in &t.children {
            let after = if child.children.is_empty() {
                next
            } else {
                self.trie(child, next)?
            };
            entries.push(self.push(BI::Bytes(*lo, *hi, after))?);
        }
        self.alts(&entries, next)
    }

    fn set(&mut self, s: &UnitSet, next: u32) -> Result<u32, Fail> {
        if self.utf8 {
            if s.has_invalid_units() {
                return Err(Fail);
            }
            let mut seqs = Vec::new();
            for &(lo, hi) in s.ranges() {
                if lo > MAX_CHAR {
                    continue;
                }
                utf8_split(lo, hi.min(MAX_CHAR), &mut seqs);
            }
            let mut t = Trie::default();
            for sq in &seqs {
                t.insert(sq);
            }
            if t.children.is_empty() {
                return self.push(BI::Bytes(1, 0, next));
            }
            self.trie(&t, next)
        } else {
            let mut entries = Vec::new();
            for &(lo, hi) in s.ranges() {
                if lo > 0xff {
                    continue;
                }
                entries.push(self.push(BI::Bytes(lo as u8, hi.min(0xff) as u8, next))?);
            }
            if entries.is_empty() {
                return self.push(BI::Bytes(1, 0, next));
            }
            self.alts(&entries, next)
        }
    }

    fn compile(&mut self, node: &Node, next: u32) -> Result<u32, Fail> {
        match node {
            Node::Empty => Ok(next),
            Node::Unsupported | Node::Backref(_) => Err(Fail),
            Node::Set(s) => self.set(s, next),
            Node::Look(l) => self.push(BI::Look(*l, next)),
            Node::Group { node, .. } => self.compile(node, next),
            Node::Concat(v) => {
                let mut cur = next;
                for n in v.iter().rev() {
                    cur = self.compile(n, cur)?;
                }
                Ok(cur)
            }
            Node::Alt(v) => {
                let entries = v
                    .iter()
                    .map(|n| self.compile(n, next))
                    .collect::<Result<Vec<_>, _>>()?;
                self.alts(&entries, next)
            }
            Node::Repeat { node, min, max } => {
                let mut cur = next;
                match max {
                    None => {
                        let l = self.push(BI::Split(0, next))?;
                        let body = self.compile(node, l)?;
                        self.insts[l as usize] = BI::Split(body, next);
                        cur = l;
                    }
                    Some(m) => {
                        for _ in *min..*m {
                            let e = self.compile(node, cur)?;
                            cur = self.push(BI::Split(e, cur))?;
                        }
                    }
                }
                for _ in 0..*min {
                    cur = self.compile(node, cur)?;
                }
                Ok(cur)
            }
        }
    }
}

const CTX_OTHER: u8 = 0;
const CTX_WORD: u8 = 1;
const CTX_NEWLINE: u8 = 2;

#[derive(Clone, Debug)]
enum Skip {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Table(Box<[bool; 256]>),
}

#[derive(Clone, Debug)]
pub struct LineDfa {
    classes: [u8; 256],
    nc: usize,
    ptable: Vec<u32>,
    accept_end: Vec<bool>,
    skip: Vec<Skip>,
    eol: u8,
}

struct Ctx {
    insts: Vec<BI>,
    start: u32,
    words: bool,
    contexts: bool,
}

impl Ctx {
    fn byte_ctx(&self, b: u8, eol: u8) -> u8 {
        if !self.contexts {
            CTX_OTHER
        } else if b == eol {
            CTX_NEWLINE
        } else if self.words && (b == b'_' || b.is_ascii_alphanumeric()) {
            CTX_WORD
        } else {
            CTX_OTHER
        }
    }

    fn look(l: Look, prev: u8, next: u8) -> bool {
        let pw = prev == CTX_WORD;
        let nw = next == CTX_WORD;
        match l {
            Look::LineStart | Look::BufStart => prev == CTX_NEWLINE,
            Look::LineEnd | Look::BufEnd => next == CTX_NEWLINE,
            Look::WordStart => !pw && nw,
            Look::WordEnd => pw && !nw,
            Look::WordBoundary => pw != nw,
            Look::NotWordBoundary => pw == nw,
        }
    }

    fn closure(
        &self,
        pcs: &[u32],
        prev: u8,
        next: u8,
        seen: &mut [u32],
        out: &mut Vec<u32>,
    ) -> bool {
        let generation = seen[0].wrapping_add(1).max(1);
        seen[0] = generation;
        out.clear();
        let mut matched = false;
        let mut stack: Vec<u32> = pcs.iter().rev().copied().collect();
        stack.push(self.start);
        while let Some(pc) = stack.pop() {
            let slot = pc as usize + 1;
            if seen[slot] == generation {
                continue;
            }
            seen[slot] = generation;
            match self.insts[pc as usize] {
                BI::Bytes(..) => out.push(pc),
                BI::Match => matched = true,
                BI::Split(a, b) => {
                    stack.push(b);
                    stack.push(a);
                }
                BI::Look(l, n) => {
                    if Self::look(l, prev, next) {
                        stack.push(n);
                    }
                }
            }
        }
        matched
    }
}

impl LineDfa {
    #[must_use]
    pub fn build(node: &Node, utf8: bool, eol: u8) -> Option<Self> {
        if utf8 && node.has_word_look() {
            return None;
        }
        let mut b = Builder {
            insts: Vec::new(),
            utf8,
        };
        let m = b.push(BI::Match).ok()?;
        let start = b.compile(node, m).ok()?;
        let words = node.has_word_look();
        let contexts = node.any(&|n| matches!(n, Node::Look(_)));
        let ctx = Ctx {
            insts: b.insts,
            start,
            words,
            contexts,
        };
        let mut bounds = [false; 257];
        bounds[0] = true;
        let mut mark = |lo: usize, hi: usize| {
            bounds[lo] = true;
            bounds[hi + 1] = true;
        };
        for i in &ctx.insts {
            if let BI::Bytes(lo, hi, _) = *i
                && lo <= hi
            {
                mark(usize::from(lo), usize::from(hi));
            }
        }
        mark(usize::from(eol), usize::from(eol));
        if words {
            for c in 0..=255u8 {
                if c == b'_' || c.is_ascii_alphanumeric() {
                    mark(usize::from(c), usize::from(c));
                }
            }
        }
        let mut classes = [0u8; 256];
        let mut reps: Vec<u8> = Vec::new();
        let mut cls: i32 = -1;
        for i in 0..256 {
            if bounds[i] {
                cls += 1;
                reps.push(i as u8);
            }
            classes[i] = cls as u8;
        }
        let nc = reps.len();
        let mut states: Vec<(Vec<u32>, u8)> = Vec::new();
        let mut index: HashMap<(Vec<u32>, u8), u32> = HashMap::new();
        let start_key = (Vec::new(), if contexts { CTX_NEWLINE } else { CTX_OTHER });
        index.insert(start_key.clone(), 0);
        states.push(start_key);
        let mut table: Vec<u32> = Vec::new();
        let mut accept_end = Vec::new();
        let mut seen = vec![0u32; ctx.insts.len() + 1];
        let mut clo = Vec::new();
        let contexts_used = ctx.contexts;
        let mut si = 0;
        let mut work = 0usize;
        let ctx_list: &[u8] = if contexts_used {
            &[CTX_OTHER, CTX_WORD, CTX_NEWLINE]
        } else {
            &[CTX_OTHER]
        };
        let mut per_ctx: Vec<(bool, Vec<Vec<u32>>)> = Vec::new();
        while si < states.len() {
            let (pcs, pctx) = states[si].clone();
            accept_end.push(ctx.closure(&pcs, pctx, CTX_NEWLINE, &mut seen, &mut clo));
            per_ctx.clear();
            for &nctx in ctx_list {
                let matched = ctx.closure(&pcs, pctx, nctx, &mut seen, &mut clo);
                let mut targets: Vec<Vec<u32>> = vec![Vec::new(); nc];
                for &pc in &clo {
                    if let BI::Bytes(lo, hi, n) = ctx.insts[pc as usize]
                        && lo <= hi
                    {
                        let first = usize::from(classes[usize::from(lo)]);
                        let last = usize::from(classes[usize::from(hi)]);
                        for t in &mut targets[first..=last] {
                            t.push(n);
                        }
                        work += last - first + 1;
                    }
                }
                work += clo.len() + 1;
                if work > MAX_WORK {
                    return None;
                }
                per_ctx.push((matched, targets));
            }
            for (class, &rep) in reps.iter().enumerate() {
                let nctx = if rep == eol {
                    CTX_NEWLINE
                } else {
                    ctx.byte_ctx(rep, eol)
                };
                let slot = ctx_list.iter().position(|&c| c == nctx).unwrap_or(0);
                let (matched, targets) = &per_ctx[slot];
                if *matched {
                    table.push(MATCH);
                    continue;
                }
                if rep == eol {
                    table.push(0);
                    continue;
                }
                let mut next = targets[class].clone();
                next.sort_unstable();
                next.dedup();
                let key = (next, ctx.byte_ctx(rep, eol));
                let id = if let Some(&id) = index.get(&key) {
                    id
                } else {
                    if states.len() >= MAX_STATES {
                        return None;
                    }
                    let id = states.len() as u32;
                    index.insert(key.clone(), id);
                    states.push(key);
                    id
                };
                table.push(id);
            }
            si += 1;
        }
        let skip: Vec<Skip> = (0..states.len())
            .map(|s| {
                let mut esc = Vec::new();
                let mut tbl = [false; 256];
                for byte in 0..=255u8 {
                    if table[s * nc + usize::from(classes[usize::from(byte)])] != s as u32 {
                        esc.push(byte);
                        tbl[usize::from(byte)] = true;
                    }
                }
                match esc.as_slice() {
                    [a] => Skip::One(*a),
                    [a, b] => Skip::Two(*a, *b),
                    [a, b, c] => Skip::Three(*a, *b, *c),
                    _ if esc.len() <= 200 => Skip::Table(Box::new(tbl)),
                    _ => Skip::None,
                }
            })
            .collect();
        let has_skip: Vec<bool> = skip
            .iter()
            .map(|s: &Skip| !matches!(s, Skip::None))
            .collect();
        let ptable = table
            .iter()
            .map(|&t| {
                if t == MATCH {
                    MATCH
                } else {
                    let base = t * nc as u32;
                    if has_skip[t as usize] {
                        base | FLAG
                    } else {
                        base
                    }
                }
            })
            .collect();
        Some(Self {
            classes,
            nc,
            ptable,
            accept_end,
            skip,
            eol,
        })
    }

    #[must_use]
    pub fn find(&self, buf: &[u8], at: usize) -> Option<usize> {
        let n = buf.len();
        let nc = self.nc as u32;
        let mut off: u32 = 0;
        let mut i = at;
        'outer: while i < n {
            match &self.skip[(off / nc) as usize] {
                Skip::None => {}
                Skip::One(a) => {
                    let Some(k) = memchr::memchr(*a, &buf[i..]) else {
                        break 'outer;
                    };
                    i += k;
                }
                Skip::Two(a, b) => {
                    let Some(k) = memchr::memchr2(*a, *b, &buf[i..]) else {
                        break 'outer;
                    };
                    i += k;
                }
                Skip::Three(a, b, c) => {
                    let Some(k) = memchr::memchr3(*a, *b, *c, &buf[i..]) else {
                        break 'outer;
                    };
                    i += k;
                }
                Skip::Table(t) => {
                    while i + 8 <= n {
                        let c = &buf[i..i + 8];
                        let hit = t[usize::from(c[0])]
                            | t[usize::from(c[1])]
                            | t[usize::from(c[2])]
                            | t[usize::from(c[3])]
                            | t[usize::from(c[4])]
                            | t[usize::from(c[5])]
                            | t[usize::from(c[6])]
                            | t[usize::from(c[7])];
                        if hit {
                            break;
                        }
                        i += 8;
                    }
                    while i < n && !t[usize::from(buf[i])] {
                        i += 1;
                    }
                    if i == n {
                        break 'outer;
                    }
                }
            }
            loop {
                let t = self.ptable[off as usize + usize::from(self.classes[usize::from(buf[i])])];
                i += 1;
                if t & FLAG != 0 {
                    if t == MATCH {
                        return Some(i - 1);
                    }
                    off = t & !FLAG;
                    if i == n {
                        break 'outer;
                    }
                    continue 'outer;
                }
                off = t;
                if i == n {
                    break 'outer;
                }
            }
        }
        if n > at && buf[n - 1] != self.eol && self.accept_end[(off / nc) as usize] {
            return Some(n);
        }
        None
    }

    #[must_use]
    pub fn is_match_line(&self, line: &[u8]) -> bool {
        let mut off: u32 = 0;
        for &b in line {
            let t = self.ptable[off as usize + usize::from(self.classes[usize::from(b)])];
            if t == MATCH {
                return true;
            }
            off = t & !FLAG;
        }
        self.accept_end[(off / self.nc as u32) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::super::dfa;
    use super::*;

    fn build(p: &str, utf8: bool) -> LineDfa {
        let r = dfa::parse(
            p.as_bytes(),
            dfa::DfaSyntax {
                extended: true,
                icase: false,
                utf8,
            },
        );
        LineDfa::build(&r.result.unwrap(), utf8, b'\n').unwrap()
    }

    #[test]
    fn finds_lines() {
        let d = build("[A-Z][a-z]+_[a-z]+", false);
        assert_eq!(d.find(b"abc\nxx Foo_bar\n", 0), Some(12));
        assert_eq!(d.find(b"abc\nxx foo_bar\n", 0), None);
        let d = build("^$", false);
        assert_eq!(d.find(b"abc\n\nx", 0), Some(4));
        assert_eq!(d.find(b"abc\n", 0), None);
        let d = build("b$", false);
        assert_eq!(d.find(b"ab", 0), Some(2));
        let d = build("\\<in", false);
        assert!(d.is_match_line(b"x in") && !d.is_match_line(b"xin"));
        let d = build("\u{e9}.", true);
        assert!(d.is_match_line("caf\u{e9}\u{65e5}".as_bytes()));
        assert!(!d.is_match_line(b"caf\xc3\xa9\xff"));
    }
}
