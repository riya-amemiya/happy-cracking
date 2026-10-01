use super::ast::{Ast, CondKind, Group, GroupKind, Node, Verb};
use super::charset::CharSet;
use super::compile::lit_equivalents;

const MAX_LITS: usize = 256;
const MAX_LIT_LEN: usize = 24;
const MAX_SET_CHARS: u64 = 24;

type Lits = Option<Vec<(Vec<u8>, bool)>>;

pub struct Analysis<'a> {
    ast: &'a Ast,
}

impl<'a> Analysis<'a> {
    pub fn new(ast: &'a Ast) -> Analysis<'a> {
        Analysis { ast }
    }

    fn encode(&self, c: u32, out: &mut Vec<u8>) {
        super::compile::encode(c, self.ast.utf, out);
    }

    pub fn first_bytes(&self) -> Option<Box<[bool; 256]>> {
        let mut set = Box::new([false; 256]);
        let (empty, _) = self.fb_branches(&self.ast.root.branches, &mut set);
        if empty || set.iter().all(|&b| b) {
            return None;
        }
        Some(set)
    }

    fn fb_branches(&self, branches: &[Vec<Node>], set: &mut [bool; 256]) -> (bool, bool) {
        let mut empty = false;
        let mut stop = false;
        for b in branches {
            let (e, s) = self.fb_seq(b, set);
            empty |= e;
            stop |= s;
        }
        (empty, stop)
    }

    fn fb_seq(&self, nodes: &[Node], set: &mut [bool; 256]) -> (bool, bool) {
        for n in nodes {
            let (empty, stop) = self.fb_node(n, set);
            if stop {
                return (true, true);
            }
            if !empty {
                return (false, false);
            }
        }
        (true, false)
    }

    fn fb_all(set: &mut [bool; 256]) {
        set.fill(true);
    }

    fn fb_charset(&self, cs: &CharSet, set: &mut [bool; 256]) {
        for &(lo, hi) in cs.ranges() {
            if !self.ast.utf {
                for c in lo..=hi.min(0xff) {
                    set[c as usize] = true;
                }
                continue;
            }
            let mut a = lo;
            while a <= hi {
                let (seg_end, lead_lo, lead_hi) = if a < 0x80 {
                    let e = hi.min(0x7f);
                    (e, a, e)
                } else if a < 0x800 {
                    let e = hi.min(0x7ff);
                    (e, 0xc0 | (a >> 6), 0xc0 | (e >> 6))
                } else if a < 0x10000 {
                    let e = hi.min(0xffff);
                    (e, 0xe0 | (a >> 12), 0xe0 | (e >> 12))
                } else {
                    let e = hi.min(0x10_ffff);
                    (e, 0xf0 | (a >> 18), 0xf0 | (e >> 18))
                };
                for b in lead_lo..=lead_hi {
                    set[b as usize] = true;
                }
                if seg_end >= hi {
                    break;
                }
                a = seg_end + 1;
            }
        }
    }

    fn fb_node(&self, n: &Node, set: &mut [bool; 256]) -> (bool, bool) {
        match n {
            Node::Lit(l) => {
                let mut buf = Vec::new();
                for c in lit_equivalents(self.ast, *l) {
                    buf.clear();
                    self.encode(c, &mut buf);
                    if let Some(&b) = buf.first() {
                        set[b as usize] = true;
                    }
                }
                (false, false)
            }
            Node::Set(cs) => {
                self.fb_charset(cs, set);
                (false, false)
            }
            Node::Dot | Node::AllAny | Node::AnyByte | Node::ExtUni => {
                Self::fb_all(set);
                (false, false)
            }
            Node::Newline => {
                for b in [0x0a, 0x0b, 0x0c, 0x0d] {
                    set[b] = true;
                }
                if self.ast.utf {
                    set[0xc2] = true;
                    set[0xe2] = true;
                } else {
                    set[0x85] = true;
                }
                (false, false)
            }
            Node::Verb(Verb::Accept) => (true, true),
            Node::Verb(Verb::Fail) => (false, false),
            Node::Assert(_) | Node::SetSom | Node::Verb(_) => (true, false),
            Node::Group(g) => match g.kind {
                GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_) => {
                    (true, false)
                }
                _ => self.fb_branches(&g.branches, set),
            },
            Node::Repeat(r) => {
                if r.max == Some(0) {
                    return (true, false);
                }
                let (empty, stop) = self.fb_node(&r.node, set);
                (empty || r.min == 0, stop)
            }
            Node::BackRef(_) | Node::Recurse(_) => {
                Self::fb_all(set);
                (true, false)
            }
            Node::Cond(c) => {
                if let CondKind::Define = c.kind {
                    return (true, false);
                }
                let (empty, stop) = self.fb_branches(&c.branches, set);
                (empty || c.branches.len() == 1, stop)
            }
        }
    }

    pub fn prefixes(&self) -> Option<Vec<Vec<u8>>> {
        let lits = self.lits_branches(&self.ast.root.branches)?;
        if lits.is_empty() || lits.iter().any(|(s, _)| s.is_empty()) {
            return None;
        }
        let mut out: Vec<Vec<u8>> = lits.into_iter().map(|(s, _)| s).collect();
        out.sort();
        out.dedup();
        Some(out)
    }

    fn lits_branches(&self, branches: &[Vec<Node>]) -> Lits {
        let mut out = Vec::new();
        for b in branches {
            out.extend(self.lits_seq(b)?);
        }
        shrink(out)
    }

    fn lits_seq(&self, nodes: &[Node]) -> Lits {
        let mut acc: Vec<(Vec<u8>, bool)> = vec![(Vec::new(), true)];
        for n in nodes {
            if acc.iter().all(|(_, exact)| !exact) {
                break;
            }
            acc = concat(acc, self.lits_node(n))?;
        }
        Some(acc)
    }

    fn lits_node(&self, n: &Node) -> Lits {
        match n {
            Node::Lit(l) => {
                let mut out = Vec::new();
                for c in lit_equivalents(self.ast, *l) {
                    let mut buf = Vec::new();
                    self.encode(c, &mut buf);
                    out.push((buf, true));
                }
                Some(out)
            }
            Node::Set(cs) => {
                if cs.len() > MAX_SET_CHARS || cs.is_empty() {
                    return None;
                }
                let max = if self.ast.utf { 0x10_ffff } else { 0xff };
                let mut out = Vec::new();
                for &(lo, hi) in cs.ranges() {
                    for c in lo..=hi.min(max) {
                        let mut buf = Vec::new();
                        self.encode(c, &mut buf);
                        out.push((buf, true));
                    }
                }
                Some(out)
            }
            Node::Verb(Verb::Accept) => Some(vec![(Vec::new(), false)]),
            Node::Verb(Verb::Fail) => Some(Vec::new()),
            Node::Assert(_) | Node::SetSom | Node::Verb(_) => Some(vec![(Vec::new(), true)]),
            Node::Group(g) => match g.kind {
                GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_) => {
                    Some(vec![(Vec::new(), true)])
                }
                _ => self.lits_branches(&g.branches),
            },
            Node::Repeat(r) => {
                if r.max == Some(0) {
                    return Some(vec![(Vec::new(), true)]);
                }
                let one = self.lits_node(&r.node);
                let exact_max = r.max == Some(r.min);
                if r.min == 0 {
                    let mut out = vec![(Vec::new(), true)];
                    let inner = one?;
                    let single = r.max == Some(1);
                    out.extend(inner.into_iter().map(|(s, e)| (s, e && single)));
                    return shrink(out);
                }
                let mut acc = vec![(Vec::new(), true)];
                for _ in 0..r.min.min(8) {
                    acc = concat(acc, one.clone())?;
                }
                if r.min > 8 || !exact_max {
                    for e in &mut acc {
                        e.1 = false;
                    }
                }
                shrink(acc)
            }
            Node::Cond(c) => {
                if let CondKind::Define = c.kind {
                    return Some(vec![(Vec::new(), true)]);
                }
                let mut out = self.lits_branches(&c.branches)?;
                if c.branches.len() == 1 {
                    out.push((Vec::new(), true));
                }
                shrink(out)
            }
            Node::Dot
            | Node::AllAny
            | Node::AnyByte
            | Node::ExtUni
            | Node::Newline
            | Node::BackRef(_)
            | Node::Recurse(_) => None,
        }
    }

    pub fn required(&self) -> Option<Vec<u8>> {
        if has_accept(&self.ast.root.branches) {
            return None;
        }
        let mut best: Vec<u8> = Vec::new();
        let mut run: Vec<u8> = Vec::new();
        if self.ast.root.branches.len() == 1 {
            self.req_seq(&self.ast.root.branches[0], &mut run, &mut best);
        }
        flush(&mut run, &mut best);
        (best.len() >= 2).then_some(best)
    }

    fn req_seq(&self, nodes: &[Node], run: &mut Vec<u8>, best: &mut Vec<u8>) {
        for n in nodes {
            self.req_node(n, run, best);
        }
    }

    fn req_group(&self, g: &Group, run: &mut Vec<u8>, best: &mut Vec<u8>) {
        match g.kind {
            GroupKind::NonCapture | GroupKind::Capture(_) | GroupKind::Atomic | GroupKind::Top
                if g.branches.len() == 1 =>
            {
                self.req_seq(&g.branches[0], run, best);
            }
            GroupKind::LookAhead { neg: false, .. } | GroupKind::LookBehind { neg: false, .. }
                if g.branches.len() == 1 =>
            {
                flush(run, best);
                let mut inner = Vec::new();
                self.req_seq(&g.branches[0], &mut inner, best);
                flush(&mut inner, best);
            }
            GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_) => {}
            _ => flush(run, best),
        }
    }

    fn req_node(&self, n: &Node, run: &mut Vec<u8>, best: &mut Vec<u8>) {
        match n {
            Node::Assert(_) | Node::SetSom | Node::Verb(Verb::Mark(_)) => {}
            Node::Lit(l) => {
                let eq = lit_equivalents(self.ast, *l);
                if eq.len() == 1 {
                    self.encode(eq[0], run);
                } else {
                    flush(run, best);
                }
            }
            Node::Group(g) => self.req_group(g, run, best),
            Node::Repeat(r) if r.min >= 1 => {
                flush(run, best);
                let mut inner = Vec::new();
                self.req_node(&r.node, &mut inner, best);
                flush(&mut inner, best);
            }
            _ => flush(run, best),
        }
    }
}

fn flush(run: &mut Vec<u8>, best: &mut Vec<u8>) {
    if run.len() > best.len() {
        best.clone_from(run);
    }
    run.clear();
}

fn has_accept(branches: &[Vec<Node>]) -> bool {
    branches.iter().any(|b| b.iter().any(node_has_accept))
}

fn node_has_accept(n: &Node) -> bool {
    match n {
        Node::Verb(Verb::Accept) => true,
        Node::Group(g) => has_accept(&g.branches),
        Node::Repeat(r) => node_has_accept(&r.node),
        Node::Cond(c) => {
            has_accept(&c.branches)
                || matches!(&c.kind, CondKind::Assert(g) if has_accept(&g.branches))
        }
        _ => false,
    }
}

fn concat(a: Vec<(Vec<u8>, bool)>, b: Lits) -> Lits {
    let Some(b) = b else {
        return Some(a.into_iter().map(|(s, _)| (s, false)).collect());
    };
    let mut out = Vec::new();
    for (s, exact) in a {
        if !exact {
            out.push((s, false));
            continue;
        }
        for (t, e2) in &b {
            let mut u = s.clone();
            u.extend_from_slice(t);
            out.push((u, *e2));
        }
        if out.len() > MAX_LITS * 8 {
            out = shrink(out)?;
        }
    }
    shrink(out)
}

fn shrink(mut v: Vec<(Vec<u8>, bool)>) -> Lits {
    for e in &mut v {
        if e.0.len() > MAX_LIT_LEN {
            e.0.truncate(MAX_LIT_LEN);
            e.1 = false;
        }
    }
    v.sort();
    v.dedup();
    while v.len() > MAX_LITS {
        let longest = v.iter().map(|e| e.0.len()).max().unwrap_or(0);
        if longest <= 1 {
            return None;
        }
        for e in &mut v {
            if e.0.len() == longest {
                e.0.pop();
                e.1 = false;
            }
        }
        v.sort();
        v.dedup();
    }
    Some(v)
}
