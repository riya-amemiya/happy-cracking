use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::super::locale::{self, INVALID_BASE};
use super::ast::{Look, Node, UnitSet};

const MAX_INSTS: usize = 4_000_000;

#[derive(Clone, Copy, Debug)]
enum Inst {
    Set(u32, u32),
    Split(u32, u32),
    Look(Look, u32),
    Match,
}

#[derive(Debug)]
pub struct TooBig;

#[derive(Clone, Debug)]
struct Prog {
    insts: Vec<Inst>,
    sets: Vec<UnitSet>,
    start: u32,
}

struct Compiler {
    insts: Vec<Inst>,
    sets: Vec<UnitSet>,
}

impl Compiler {
    fn push(&mut self, inst: Inst) -> Result<u32, TooBig> {
        if self.insts.len() >= MAX_INSTS {
            return Err(TooBig);
        }
        self.insts.push(inst);
        Ok((self.insts.len() - 1) as u32)
    }

    fn set(&mut self, s: UnitSet, next: u32) -> Result<u32, TooBig> {
        let idx = self.sets.len() as u32;
        self.sets.push(s);
        self.push(Inst::Set(idx, next))
    }

    fn compile(&mut self, node: &Node, next: u32) -> Result<u32, TooBig> {
        match node {
            Node::Empty | Node::Backref(_) => Ok(next),
            Node::Unsupported => self.set(UnitSet::default(), next),
            Node::Set(s) => self.set(s.clone(), next),
            Node::Look(l) => self.push(Inst::Look(*l, next)),
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
                let mut cur = *entries.last().unwrap_or(&next);
                for &e in entries.iter().rev().skip(1) {
                    cur = self.push(Inst::Split(e, cur))?;
                }
                Ok(cur)
            }
            Node::Repeat { node, min, max } => {
                let mut cur = next;
                match max {
                    None => {
                        let l = self.push(Inst::Split(0, next))?;
                        let body = self.compile(node, l)?;
                        self.insts[l as usize] = Inst::Split(body, next);
                        cur = l;
                    }
                    Some(m) => {
                        for _ in *min..*m {
                            let e = self.compile(node, cur)?;
                            cur = self.push(Inst::Split(e, cur))?;
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

#[derive(Default)]
struct SparseSet {
    dense: Vec<u32>,
    sparse: Vec<u32>,
}

impl SparseSet {
    fn reset(&mut self, n: usize) {
        self.dense.clear();
        if self.sparse.len() < n {
            self.sparse.resize(n, 0);
        }
    }

    fn contains(&self, v: u32) -> bool {
        let i = self.sparse[v as usize] as usize;
        i < self.dense.len() && self.dense[i] == v
    }

    fn insert(&mut self, v: u32) -> bool {
        let i = self.sparse[v as usize] as usize;
        if i < self.dense.len() && self.dense[i] == v {
            return false;
        }
        self.sparse[v as usize] = self.dense.len() as u32;
        self.dense.push(v);
        true
    }
}

#[derive(Default)]
struct Threads {
    set: SparseSet,
    list: Vec<(u32, usize)>,
}

impl Threads {
    fn reset(&mut self, n: usize) {
        self.set.reset(n);
        self.list.clear();
    }
}

#[derive(Default)]
struct Scratch {
    clist: Threads,
    nlist: Threads,
    stack: Vec<u32>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

type Cap = Option<(usize, usize)>;
type Outcomes = Rc<Vec<(usize, Vec<Cap>)>>;

const MAX_EVAL_WORK: usize = 2_000_000;
const QUICK_BUDGET: u64 = 20_000;

struct Exhausted;

type Cont<'a> = dyn FnMut(usize, &mut [Cap]) -> bool + 'a;

#[derive(Clone, Copy)]
struct Quick<'h, 'b> {
    ctx: Ctx<'h>,
    budget: &'b Cell<u64>,
}

#[derive(Default)]
struct Memo {
    map: HashMap<(usize, usize, Vec<Cap>), Outcomes>,
    pure: HashMap<usize, bool>,
    work: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct TextMode {
    pub utf8: bool,
    pub icase: bool,
    pub nl_quirk: bool,
}

#[derive(Clone, Copy)]
struct Ctx<'h> {
    hay: &'h [u8],
    limit: usize,
    not_eol: bool,
    text: TextMode,
    start: usize,
}

#[derive(Clone, Copy)]
struct At {
    pos: usize,
    prev: Option<u32>,
    next: Option<u32>,
}

impl Ctx<'_> {
    fn word(&self, unit: Option<u32>) -> bool {
        let utf8 = self.text.utf8;
        unit.is_some_and(|u| {
            let u = if self.text.icase {
                locale::to_upper(utf8, u)
            } else {
                u
            };
            locale::is_word_regex(utf8, u)
        })
    }

    fn next_unit(&self, pos: usize) -> Option<u32> {
        (pos < self.limit).then(|| locale::decode(self.text.utf8, &self.hay[pos..self.limit]).0)
    }

    fn prev_unit(&self, pos: usize) -> Option<u32> {
        locale::decode_before(self.text.utf8, self.hay, pos).map(|(u, _)| u)
    }

    fn look(&self, look: Look, at: At, start: usize) -> bool {
        match look {
            Look::LineStart => {
                at.pos == 0
                    || (self.text.nl_quirk && at.pos > start && at.prev == Some(u32::from(b'\n')))
            }
            Look::BufStart => at.pos == 0,
            Look::LineEnd => at.pos == self.limit && !self.not_eol,
            Look::BufEnd => at.pos == self.limit,
            Look::WordStart => !self.word(at.prev) && self.word(at.next),
            Look::WordEnd => self.word(at.prev) && !self.word(at.next),
            Look::WordBoundary => self.word(at.prev) != self.word(at.next),
            Look::NotWordBoundary => self.word(at.prev) == self.word(at.next),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Starts {
    Chars,
    Bytes { nullable: bool },
}

#[derive(Clone, Debug)]
pub struct Prefilter {
    pub re: regex::bytes::Regex,
    pub exact: bool,
}

#[derive(Clone, Debug)]
pub struct Engine {
    prog: Option<Prog>,
    node: Node,
    text: TextMode,
    groups: usize,
    prefilter: Option<Prefilter>,
    starts: Starts,
}

fn nullable_mid(node: &Node, root: &Node) -> bool {
    match node {
        Node::Empty => true,
        Node::Set(_) | Node::Unsupported | Node::Look(_) => false,
        Node::Concat(v) => v.iter().all(|n| nullable_mid(n, root)),
        Node::Alt(v) => v.iter().any(|n| nullable_mid(n, root)),
        Node::Repeat { node, min, .. } => *min == 0 || nullable_mid(node, root),
        Node::Group { node, .. } => nullable_mid(node, root),
        Node::Backref(i) => root.find_group(*i).is_some_and(|g| nullable_mid(g, root)),
    }
}

impl Engine {
    pub fn new(node: Node, text: TextMode, byte_starts: bool) -> Result<Self, TooBig> {
        let groups = node.max_group();
        let starts = if byte_starts {
            Starts::Bytes {
                nullable: nullable_mid(&node, &node),
            }
        } else {
            Starts::Chars
        };
        let prog = if node.has_backref() {
            None
        } else {
            let mut c = Compiler {
                insts: Vec::new(),
                sets: Vec::new(),
            };
            let m = c.push(Inst::Match)?;
            let start = c.compile(&node, m)?;
            Some(Prog {
                insts: c.insts,
                sets: c.sets,
                start,
            })
        };
        Ok(Self {
            prog,
            node,
            text,
            groups,
            prefilter: None,
            starts,
        })
    }

    #[must_use]
    pub fn align(&self, hay: &[u8], at: usize) -> usize {
        if !self.text.utf8 || at == 0 || at >= hay.len() || hay[at] & 0xc0 != 0x80 {
            return at;
        }
        for back in 1..=3.min(at) {
            let s = at - back;
            let (u, len) = locale::decode(true, &hay[s..]);
            if u < INVALID_BASE && s + len > at {
                return s + len;
            }
            if hay[s] & 0xc0 != 0x80 {
                break;
            }
        }
        at
    }

    #[must_use]
    pub fn with_prefilter(mut self, prefilter: Option<Prefilter>) -> Self {
        self.prefilter = prefilter;
        self
    }

    #[must_use]
    pub fn longest_at(
        &self,
        hay: &[u8],
        start: usize,
        limit: usize,
        not_eol: bool,
    ) -> Option<usize> {
        let ctx = Ctx {
            hay,
            limit,
            not_eol,
            text: self.text,
            start: 0,
        };
        match &self.prog {
            Some(p) => pike(p, ctx, start, true).map(|(_, e)| e),
            None => self.bt_longest(ctx, start, &mut Memo::default()),
        }
    }

    #[must_use]
    pub fn search(
        &self,
        hay: &[u8],
        from: usize,
        limit: usize,
        not_eol: bool,
    ) -> Option<(usize, usize)> {
        let aligned = self.align(hay, from);
        if aligned != from && self.starts == (Starts::Bytes { nullable: true }) {
            return Some((from, from));
        }
        let from = aligned;
        if let Some(pf) = &self.prefilter
            && limit == hay.len()
            && !not_eol
            && from <= hay.len()
        {
            let m = pf.re.find_at(hay, from)?;
            if pf.exact {
                let s = m.start();
                return self.longest_at(hay, s, limit, not_eol).map(|e| (s, e));
            }
            return self.search_slow(hay, m.start(), limit, not_eol, Some(&pf.re));
        }
        self.search_slow(hay, from, limit, not_eol, None)
    }

    fn search_slow(
        &self,
        hay: &[u8],
        from: usize,
        limit: usize,
        not_eol: bool,
        pf: Option<&regex::bytes::Regex>,
    ) -> Option<(usize, usize)> {
        let ctx = Ctx {
            hay,
            limit,
            not_eol,
            text: self.text,
            start: 0,
        };
        if let Some(p) = &self.prog {
            return pike(p, ctx, from, false);
        }
        let mut memo = Memo::default();
        let mut pos = from;
        loop {
            pos = self.align(hay, pos);
            if let Some(e) = self.bt_longest(ctx, pos, &mut memo) {
                return Some((pos, e));
            }
            if pos >= limit {
                return None;
            }
            pos += locale::decode(self.text.utf8, &hay[pos..limit]).1;
            if let Some(re) = pf {
                pos = re.find_at(hay, pos)?.start();
            }
        }
    }

    fn quick_longest(&self, ctx: Ctx<'_>, start: usize) -> Result<Option<usize>, Exhausted> {
        let budget = Cell::new(QUICK_BUDGET);
        let mut caps = vec![None; self.groups + 1];
        let mut best: Option<usize> = None;
        let q = Quick {
            ctx,
            budget: &budget,
        };
        self.bt(&self.node, q, start, &mut caps, &mut |end, _| {
            if best.is_none_or(|b| end > b) {
                best = Some(end);
            }
            false
        });
        if budget.get() > 0 {
            Ok(best)
        } else {
            Err(Exhausted)
        }
    }

    fn bt(
        &self,
        node: &Node,
        q: Quick<'_, '_>,
        pos: usize,
        caps: &mut [Cap],
        k: &mut Cont<'_>,
    ) -> bool {
        if q.budget.get() == 0 {
            return true;
        }
        q.budget.set(q.budget.get() - 1);
        let ctx = q.ctx;
        match node {
            Node::Empty => k(pos, caps),
            Node::Unsupported => false,
            Node::Set(s) => {
                if pos >= ctx.limit {
                    return false;
                }
                let (u, len) = locale::decode(ctx.text.utf8, &ctx.hay[pos..ctx.limit]);
                s.contains(u) && k(pos + len, caps)
            }
            Node::Look(l) => {
                let at = At {
                    pos,
                    prev: ctx.prev_unit(pos),
                    next: ctx.next_unit(pos),
                };
                ctx.look(*l, at, ctx.start) && k(pos, caps)
            }
            Node::Group { node, index } => {
                let index = *index;
                self.bt(node, q, pos, caps, &mut |end, caps| {
                    let old = caps[index];
                    caps[index] = Some((pos, end));
                    let stop = k(end, caps);
                    caps[index] = old;
                    stop
                })
            }
            Node::Backref(i) => {
                let Some((s, e)) = caps.get(*i).copied().flatten() else {
                    return false;
                };
                self.backref_len(ctx, s, e, pos)
                    .is_some_and(|len| k(pos + len, caps))
            }
            Node::Concat(v) => self.bt_seq(v, q, pos, caps, k),
            Node::Alt(v) => v.iter().any(|n| self.bt(n, q, pos, caps, k)),
            Node::Repeat { node, min, max } => self.bt_rep(node, (*min, *max, 0), q, pos, caps, k),
        }
    }

    fn bt_seq(
        &self,
        v: &[Node],
        q: Quick<'_, '_>,
        pos: usize,
        caps: &mut [Cap],
        k: &mut Cont<'_>,
    ) -> bool {
        match v.split_first() {
            None => k(pos, caps),
            Some((first, rest)) => self.bt(first, q, pos, caps, &mut |p, caps| {
                self.bt_seq(rest, q, p, caps, k)
            }),
        }
    }

    fn bt_rep(
        &self,
        node: &Node,
        state: (u32, Option<u32>, u32),
        q: Quick<'_, '_>,
        pos: usize,
        caps: &mut [Cap],
        k: &mut Cont<'_>,
    ) -> bool {
        let (min, max, count) = state;
        if max.is_none_or(|m| count < m) {
            let stop = self.bt(node, q, pos, caps, &mut |p, caps| {
                if p == pos && count >= min {
                    return false;
                }
                self.bt_rep(node, (min, max, count + 1), q, p, caps, k)
            });
            if stop {
                return true;
            }
        }
        count >= min && k(pos, caps)
    }

    fn bt_longest(&self, ctx: Ctx<'_>, start: usize, memo: &mut Memo) -> Option<usize> {
        let ctx = Ctx { start, ..ctx };
        if self.text.nl_quirk {
            *memo = Memo::default();
        }
        if let Ok(r) = self.quick_longest(ctx, start) {
            return r;
        }
        let init = vec![None; self.groups + 1];
        let out = self.eval(&self.node, ctx, start, &init, memo);
        out.iter().map(|(e, _)| *e).max()
    }

    fn eval(
        &self,
        node: &Node,
        ctx: Ctx<'_>,
        pos: usize,
        caps: &[Cap],
        memo: &mut Memo,
    ) -> Outcomes {
        let addr = std::ptr::from_ref(node) as usize;
        let pure = *memo.pure.entry(addr).or_insert_with(|| !node.has_backref());
        if pure && caps.iter().any(Option::is_some) {
            let blank = vec![None; caps.len()];
            let base = self.eval(node, ctx, pos, &blank, memo);
            let merged: Vec<(usize, Vec<Cap>)> = base
                .iter()
                .map(|(e, c)| {
                    let joined = c.iter().zip(caps).map(|(new, old)| new.or(*old)).collect();
                    (*e, joined)
                })
                .collect();
            return Rc::new(merged);
        }
        let key = (addr, pos, caps.to_vec());
        if let Some(r) = memo.map.get(&key) {
            return Rc::clone(r);
        }
        memo.work += 1;
        let result = if memo.work > MAX_EVAL_WORK {
            Vec::new()
        } else {
            self.eval_uncached(node, ctx, pos, caps, memo)
        };
        let rc = Rc::new(result);
        memo.map.insert(key, Rc::clone(&rc));
        rc
    }

    fn eval_uncached(
        &self,
        node: &Node,
        ctx: Ctx<'_>,
        pos: usize,
        caps: &[Cap],
        memo: &mut Memo,
    ) -> Vec<(usize, Vec<Cap>)> {
        match node {
            Node::Empty => vec![(pos, caps.to_vec())],
            Node::Unsupported => Vec::new(),
            Node::Set(s) => {
                if pos >= ctx.limit {
                    return Vec::new();
                }
                let (u, len) = locale::decode(ctx.text.utf8, &ctx.hay[pos..ctx.limit]);
                if s.contains(u) {
                    vec![(pos + len, caps.to_vec())]
                } else {
                    Vec::new()
                }
            }
            Node::Look(l) => {
                let at = At {
                    pos,
                    prev: ctx.prev_unit(pos),
                    next: ctx.next_unit(pos),
                };
                if ctx.look(*l, at, ctx.start) {
                    vec![(pos, caps.to_vec())]
                } else {
                    Vec::new()
                }
            }
            Node::Group { node, index } => self
                .eval(node, ctx, pos, caps, memo)
                .iter()
                .map(|(e, c)| {
                    let mut c = c.clone();
                    c[*index] = Some((pos, *e));
                    (*e, c)
                })
                .collect(),
            Node::Backref(i) => {
                let Some((s, e)) = caps.get(*i).copied().flatten() else {
                    return Vec::new();
                };
                self.backref_len(ctx, s, e, pos)
                    .map_or_else(Vec::new, |len| vec![(pos + len, caps.to_vec())])
            }
            Node::Concat(v) => {
                let mut frontier: Vec<(usize, Vec<Cap>)> = vec![(pos, caps.to_vec())];
                for child in v {
                    let mut seen = HashSet::new();
                    let mut next = Vec::new();
                    for (p, c) in &frontier {
                        for (e, c2) in self.eval(child, ctx, *p, c, memo).iter() {
                            if seen.insert((*e, c2.clone())) {
                                next.push((*e, c2.clone()));
                            }
                        }
                    }
                    if next.is_empty() {
                        return Vec::new();
                    }
                    frontier = next;
                }
                frontier
            }
            Node::Alt(v) => {
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for child in v {
                    for (e, c) in self.eval(child, ctx, pos, caps, memo).iter() {
                        if seen.insert((*e, c.clone())) {
                            out.push((*e, c.clone()));
                        }
                    }
                }
                out
            }
            Node::Repeat { node, min, max } => {
                self.eval_repeat(node, (*min, *max), ctx, pos, caps, memo)
            }
        }
    }

    fn eval_repeat(
        &self,
        node: &Node,
        bounds: (u32, Option<u32>),
        ctx: Ctx<'_>,
        pos: usize,
        caps: &[Cap],
        memo: &mut Memo,
    ) -> Vec<(usize, Vec<Cap>)> {
        let (min, max) = bounds;
        let mut results = Vec::new();
        let mut done: HashSet<(usize, Vec<Cap>)> = HashSet::new();
        let mut frontier: Vec<(usize, Vec<Cap>)> = vec![(pos, caps.to_vec())];
        if min == 0 {
            done.insert((pos, caps.to_vec()));
            results.push((pos, caps.to_vec()));
        }
        let mut count = 0u32;
        while !frontier.is_empty() && max.is_none_or(|m| count < m) {
            let mut seen = HashSet::new();
            let mut next = Vec::new();
            for (p, c) in &frontier {
                for (e, c2) in self.eval(node, ctx, *p, c, memo).iter() {
                    if *e == *p && count >= min {
                        continue;
                    }
                    let state = (*e, c2.clone());
                    if count + 1 > min && max.is_none() && done.contains(&state) {
                        continue;
                    }
                    if seen.insert(state.clone()) {
                        next.push(state);
                    }
                }
            }
            count += 1;
            if count >= min {
                for s in &next {
                    if done.insert(s.clone()) || max.is_some() {
                        results.push(s.clone());
                    }
                }
            }
            frontier = next;
        }
        results.sort_unstable();
        results.dedup();
        results
    }

    fn backref_len(&self, ctx: Ctx<'_>, s: usize, e: usize, pos: usize) -> Option<usize> {
        let want = &ctx.hay[s..e];
        if !self.text.icase {
            return (ctx.hay.len().min(ctx.limit) >= pos + want.len()
                && &ctx.hay[pos..pos + want.len()] == want)
                .then_some(want.len());
        }
        let mut i = 0;
        let mut j = pos;
        while i < want.len() {
            if j >= ctx.limit {
                return None;
            }
            let (a, la) = locale::decode(self.text.utf8, &want[i..]);
            let (b, lb) = locale::decode(self.text.utf8, &ctx.hay[j..ctx.limit]);
            if locale::to_upper(self.text.utf8, a) != locale::to_upper(self.text.utf8, b) {
                return None;
            }
            i += la;
            j += lb;
        }
        Some(j - pos)
    }
}

fn add_thread(
    prog: &Prog,
    t: &mut Threads,
    stack: &mut Vec<u32>,
    pc0: u32,
    start: usize,
    ctx: &Ctx<'_>,
    at: At,
) {
    stack.push(pc0 << 1);
    while let Some(entry) = stack.pop() {
        let pc = entry >> 1;
        let flag = entry & 1;
        if (flag == 1 && t.set.contains(pc << 1)) || !t.set.insert(entry) {
            continue;
        }
        match prog.insts[pc as usize] {
            Inst::Set(..) => t.list.push((pc, start)),
            Inst::Match => {
                if flag == 0 {
                    t.list.push((pc, start));
                }
            }
            Inst::Split(a, b) => {
                stack.push((b << 1) | flag);
                stack.push((a << 1) | flag);
            }
            Inst::Look(l, n) => {
                if ctx.look(l, at, start) {
                    stack.push((n << 1) | flag);
                } else if l == Look::LineEnd
                    && ctx.text.nl_quirk
                    && at.pos < ctx.limit
                    && at.next == Some(u32::from(b'\n'))
                {
                    stack.push((n << 1) | 1);
                }
            }
        }
    }
}

fn pike(prog: &Prog, ctx: Ctx<'_>, from: usize, anchored: bool) -> Option<(usize, usize)> {
    SCRATCH.with(|cell| match cell.try_borrow_mut() {
        Ok(mut s) => pike_with(prog, ctx, from, anchored, &mut s),
        Err(_) => pike_with(prog, ctx, from, anchored, &mut Scratch::default()),
    })
}

fn pike_with(
    prog: &Prog,
    ctx: Ctx<'_>,
    from: usize,
    anchored: bool,
    scratch: &mut Scratch,
) -> Option<(usize, usize)> {
    let n = prog.insts.len();
    let Scratch {
        clist,
        nlist,
        stack,
    } = scratch;
    clist.reset(2 * n);
    nlist.reset(2 * n);
    stack.clear();
    let mut best: Option<(usize, usize)> = None;
    let mut pos = from;
    let mut prev = ctx.prev_unit(pos);
    loop {
        let next_dec =
            (pos < ctx.limit).then(|| locale::decode(ctx.text.utf8, &ctx.hay[pos..ctx.limit]));
        let next = next_dec.map(|(u, _)| u);
        if best.is_none() && (!anchored || pos == from) {
            let at = At { pos, prev, next };
            add_thread(prog, clist, stack, prog.start, pos, &ctx, at);
        }
        for &(pc, st) in &clist.list {
            if let Inst::Match = prog.insts[pc as usize]
                && best.is_none_or(|(bs, be)| st < bs || (st == bs && pos > be))
            {
                best = Some((st, pos));
            }
        }
        if let Some((bs, _)) = best {
            clist.list.retain(|&(_, st)| st <= bs);
        }
        let Some((u, len)) = next_dec else {
            break;
        };
        if clist.list.is_empty() && (best.is_some() || anchored) {
            break;
        }
        nlist.reset(2 * n);
        let npos = pos + len;
        let following =
            (npos < ctx.limit).then(|| locale::decode(ctx.text.utf8, &ctx.hay[npos..ctx.limit]).0);
        let at = At {
            pos: npos,
            prev: Some(u),
            next: following,
        };
        for &(pc, st) in &clist.list {
            if let Inst::Set(idx, nx) = prog.insts[pc as usize]
                && prog.sets[idx as usize].contains(u)
            {
                add_thread(prog, nlist, stack, nx, st, &ctx, at);
            }
        }
        std::mem::swap(clist, nlist);
        prev = Some(u);
        pos = npos;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::super::syntax::{self, Syntax};
    use super::*;

    fn engine(p: &str, extended: bool) -> Engine {
        let parsed = syntax::parse(
            p.as_bytes(),
            Syntax {
                extended,
                icase: false,
                utf8: true,
            },
        )
        .unwrap();
        Engine::new(
            parsed.node,
            TextMode {
                utf8: true,
                icase: false,
                nl_quirk: false,
            },
            true,
        )
        .unwrap()
    }

    #[test]
    fn leftmost_longest() {
        let e = engine("a|ab", true);
        assert_eq!(e.search(b"abab", 0, 4, false), Some((0, 2)));
        assert_eq!(e.search(b"abab", 2, 4, false), Some((2, 4)));
        let e = engine("a*", false);
        assert_eq!(e.search(b"baaa", 0, 4, false), Some((0, 0)));
        assert_eq!(e.search(b"baaa", 1, 4, false), Some((1, 4)));
        let e = engine("\\(abc\\)\\1", false);
        assert_eq!(e.search(b"xabcabc", 0, 7, false), Some((1, 7)));
        let e = engine("(a|ab)(c|bab)", true);
        assert_eq!(e.search(b"abab", 0, 4, false), Some((0, 4)));
        let e = engine("\\b.", false);
        assert_eq!(e.search(b"ab cd", 1, 5, false), Some((2, 3)));
    }
}
