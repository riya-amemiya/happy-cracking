use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::locale;
use super::ast::{Look, Node, UnitSet};

const PREV_WORD: u16 = 0x0001;
const PREV_NOTWORD: u16 = 0x0002;
const NEXT_WORD: u16 = 0x0004;
const NEXT_NOTWORD: u16 = 0x0008;
const PREV_NEWLINE: u16 = 0x0010;
const NEXT_NEWLINE: u16 = 0x0020;
const PREV_BEGBUF: u16 = 0x0040;
const NEXT_ENDBUF: u16 = 0x0080;

const CTX_WORD: u8 = 1;
const CTX_NEWLINE: u8 = 2;
const CTX_BEGBUF: u8 = 4;
const CTX_ENDBUF: u8 = 8;

const MAX_TREE: usize = 400_000;
const MAX_CACHED_STATES: usize = 50_000;
const MAX_SET_REGS_STEPS: usize = 50_000_000;

const NONE: u32 = u32::MAX;
const UNKNOWN: u32 = u32::MAX - 1;

const HALT: u8 = 1;
const ACCEPT_MB: u8 = 2;
const HAS_BACKREF: u8 = 4;
const HAS_CONSTRAINT: u8 = 8;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default, Clone, Copy)]
struct Fx(u64);

impl Fx {
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i + 8 <= bytes.len() {
            let mut w = [0u8; 8];
            w.copy_from_slice(&bytes[i..i + 8]);
            self.add(u64::from_le_bytes(w));
            i += 8;
        }
        for &b in &bytes[i..] {
            self.add(u64::from(b));
        }
    }

    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<Fx>>;

fn not_prev(c: u16, ctx: u8) -> bool {
    (c & PREV_WORD != 0 && ctx & CTX_WORD == 0)
        || (c & PREV_NOTWORD != 0 && ctx & CTX_WORD != 0)
        || (c & PREV_NEWLINE != 0 && ctx & CTX_NEWLINE == 0)
        || (c & PREV_BEGBUF != 0 && ctx & CTX_BEGBUF == 0)
}

fn not_next(c: u16, ctx: u8) -> bool {
    (c & NEXT_WORD != 0 && ctx & CTX_WORD == 0)
        || (c & NEXT_NOTWORD != 0 && ctx & CTX_WORD != 0)
        || (c & NEXT_NEWLINE != 0 && ctx & CTX_NEWLINE == 0)
        || (c & NEXT_ENDBUF != 0 && ctx & CTX_ENDBUF == 0)
}

fn set_insert(v: &mut Vec<u32>, x: u32) {
    if let Err(i) = v.binary_search(&x) {
        v.insert(i, x);
    }
}

fn set_contains(v: &[u32], x: u32) -> bool {
    v.binary_search(&x).is_ok()
}

fn set_union(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => {
                out.push(a[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(b[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}

fn set_merge(dest: &mut Vec<u32>, src: &[u32]) {
    if src.is_empty() {
        return;
    }
    if dest.is_empty() {
        dest.extend_from_slice(src);
        return;
    }
    let merged = set_union(dest, src);
    *dest = merged;
}

fn set_add_intersect(dest: &mut Vec<u32>, a: &[u32], b: &[u32]) {
    let (mut i, mut j) = (0, 0);
    let mut add = Vec::new();
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                add.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    set_merge(dest, &add);
}

fn set_remove(v: &mut Vec<u32>, x: u32) {
    if let Ok(i) = v.binary_search(&x) {
        v.remove(i);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Leaf(u32),
    End,
    BackRef(usize),
    Open(usize),
    Close(usize),
    Alt,
    Star,
    Anchor,
}

impl Kind {
    fn eps(self) -> bool {
        matches!(
            self,
            Kind::Open(_) | Kind::Close(_) | Kind::Alt | Kind::Star | Kind::Anchor
        )
    }

    fn subexp(self) -> Option<usize> {
        match self {
            Kind::Open(i) | Kind::Close(i) | Kind::BackRef(i) => Some(i),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NNode {
    kind: Kind,
    constraint: u16,
    duplicated: bool,
    opt_subexp: bool,
    accept_mb: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TKind {
    Leaf(u32),
    End,
    BackRef(usize),
    Anchor(u16),
    Subexp(usize),
    Open(usize),
    Close(usize),
    Concat,
    Alt,
    Star,
}

#[derive(Clone)]
struct TNode {
    kind: TKind,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    first: usize,
    next: Option<usize>,
    node_idx: u32,
    duplicated: bool,
    opt_subexp: bool,
}

#[derive(Default)]
struct Tree {
    t: Vec<TNode>,
}

impl Tree {
    fn create(&mut self, left: Option<usize>, right: Option<usize>, kind: TKind) -> usize {
        let id = self.t.len();
        self.t.push(TNode {
            kind,
            left,
            right,
            parent: None,
            first: id,
            next: None,
            node_idx: NONE,
            duplicated: false,
            opt_subexp: false,
        });
        if let Some(l) = left {
            self.t[l].parent = Some(id);
        }
        if let Some(r) = right {
            self.t[r].parent = Some(id);
        }
        id
    }

    fn duplicate(&mut self, root: usize) -> usize {
        let mut stack: Vec<(usize, Option<(usize, bool)>)> = vec![(root, None)];
        let mut dup_root = root;
        while let Some((orig, at)) = stack.pop() {
            let (kind, l, r) = (self.t[orig].kind, self.t[orig].left, self.t[orig].right);
            let d = self.create(None, None, kind);
            self.t[d].duplicated = true;
            match at {
                None => dup_root = d,
                Some((p, true)) => {
                    self.t[p].left = Some(d);
                    self.t[d].parent = Some(p);
                }
                Some((p, false)) => {
                    self.t[p].right = Some(d);
                    self.t[d].parent = Some(p);
                }
            }
            if let Some(r) = r {
                stack.push((r, Some((d, false))));
            }
            if let Some(l) = l {
                stack.push((l, Some((d, true))));
            }
        }
        dup_root
    }

    fn mark_opt_subexp(&mut self, root: usize, idx: usize) {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if self.t[n].kind == TKind::Subexp(idx) {
                self.t[n].opt_subexp = true;
            }
            stack.extend(self.t[n].left);
            stack.extend(self.t[n].right);
        }
    }

    fn postorder(&mut self, root: usize, f: &mut dyn FnMut(&mut Tree, usize)) {
        let mut node = root;
        loop {
            while let Some(c) = self.t[node].left.or(self.t[node].right) {
                node = c;
            }
            loop {
                f(self, node);
                let Some(p) = self.t[node].parent else {
                    return;
                };
                let prev = node;
                node = p;
                if !(self.t[node].right == Some(prev) || self.t[node].right.is_none()) {
                    break;
                }
            }
            let Some(r) = self.t[node].right else {
                return;
            };
            node = r;
        }
    }

    fn preorder(&mut self, root: usize, f: &mut dyn FnMut(&mut Tree, usize)) {
        let mut node = root;
        loop {
            f(self, node);
            if let Some(l) = self.t[node].left {
                node = l;
                continue;
            }
            let mut prev: Option<usize> = None;
            while self.t[node].right == prev || self.t[node].right.is_none() {
                prev = Some(node);
                match self.t[node].parent {
                    Some(p) => node = p,
                    None => return,
                }
            }
            let Some(r) = self.t[node].right else {
                return;
            };
            node = r;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TextMode {
    utf8: bool,
    mb: bool,
    icase: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ecl {
    Todo,
    Busy,
    Done(Vec<u32>),
}

#[derive(Debug)]
pub struct Gre {
    id: u64,
    nodes: Vec<NNode>,
    nexts: Vec<u32>,
    edests: Vec<Vec<u32>>,
    eclosures: Vec<Vec<u32>>,
    inveclosures: Vec<Vec<u32>>,
    sets: Vec<UnitSet>,
    period: Vec<bool>,
    init_node: u32,
    init_nodes: Vec<u32>,
    nbackref: usize,
    used_bkref_map: u64,
    plural: bool,
    word_ops: bool,
    re_nsub: usize,
    text: TextMode,
    fastmap: Option<Box<[bool; 256]>>,
}

struct TooBig;

struct Builder {
    tree: Tree,
    sets: Vec<UnitSet>,
    nbackref: usize,
    used_bkref_map: u64,
    word_ops: bool,
    mb: bool,
}

impl Builder {
    fn leaf(&mut self, s: UnitSet) -> usize {
        let id = self.sets.len() as u32;
        self.sets.push(s);
        self.tree.create(None, None, TKind::Leaf(id))
    }

    fn anchor(&mut self, c: u16) -> usize {
        self.tree.create(None, None, TKind::Anchor(c))
    }

    fn build(&mut self, n: &Node) -> Result<Option<usize>, TooBig> {
        if self.tree.t.len() > MAX_TREE {
            return Err(TooBig);
        }
        Ok(match n {
            Node::Empty | Node::Unsupported => None,
            Node::Set(s) if self.mb && s.alt() => {
                let sb = UnitSet::from_ranges(
                    s.ranges()
                        .iter()
                        .filter(|&&(lo, _)| lo < 0x80)
                        .map(|&(lo, hi)| (lo, hi.min(0x7f)))
                        .collect(),
                );
                let mbs = UnitSet::from_ranges(
                    s.ranges()
                        .iter()
                        .filter(|&&(lo, hi)| hi >= 0x80 && lo < locale::INVALID_BASE)
                        .map(|&(lo, hi)| (lo.max(0x80), hi.min(locale::MAX_CHAR)))
                        .collect(),
                );
                let a = self.leaf(sb);
                let b = self.leaf(mbs);
                Some(self.tree.create(Some(a), Some(b), TKind::Alt))
            }
            Node::Set(s) => Some(self.leaf(s.clone())),
            Node::Backref(i) => {
                let idx = i - 1;
                if idx < 64 {
                    self.used_bkref_map |= 1 << idx;
                }
                self.nbackref += 1;
                Some(self.tree.create(None, None, TKind::BackRef(idx)))
            }
            Node::Look(l) => Some(match l {
                Look::LineStart => self.anchor(PREV_NEWLINE),
                Look::LineEnd => self.anchor(NEXT_NEWLINE),
                Look::BufStart => self.anchor(PREV_BEGBUF),
                Look::BufEnd => self.anchor(NEXT_ENDBUF),
                Look::WordStart => {
                    self.word_ops = true;
                    self.anchor(PREV_NOTWORD | NEXT_WORD)
                }
                Look::WordEnd => {
                    self.word_ops = true;
                    self.anchor(PREV_WORD | NEXT_NOTWORD)
                }
                Look::WordBoundary => {
                    self.word_ops = true;
                    let a = self.anchor(PREV_NOTWORD | NEXT_WORD);
                    let b = self.anchor(PREV_WORD | NEXT_NOTWORD);
                    self.tree.create(Some(a), Some(b), TKind::Alt)
                }
                Look::NotWordBoundary => {
                    self.word_ops = true;
                    let a = self.anchor(PREV_WORD | NEXT_WORD);
                    let b = self.anchor(PREV_NOTWORD | NEXT_NOTWORD);
                    self.tree.create(Some(a), Some(b), TKind::Alt)
                }
            }),
            Node::Group { node, index } => {
                let body = self.build(node)?;
                Some(self.tree.create(body, None, TKind::Subexp(index - 1)))
            }
            Node::Concat(v) => {
                let mut tree = None;
                for x in v {
                    let e = self.build(x)?;
                    tree = match (tree, e) {
                        (Some(t), Some(e)) => {
                            Some(self.tree.create(Some(t), Some(e), TKind::Concat))
                        }
                        (None, e) => e,
                        (t, None) => t,
                    };
                }
                tree
            }
            Node::Alt(v) => {
                let mut tree = match v.first() {
                    Some(f) => self.build(f)?,
                    None => None,
                };
                for x in v.iter().skip(1) {
                    let b = self.build(x)?;
                    tree = Some(self.tree.create(tree, b, TKind::Alt));
                }
                tree
            }
            Node::Repeat { node, min, max } => {
                let elem = self.build(node)?;
                self.dup(elem, *min, *max)?
            }
        })
    }

    fn dup(
        &mut self,
        elem: Option<usize>,
        start: u32,
        end: Option<u32>,
    ) -> Result<Option<usize>, TooBig> {
        let Some(mut elem) = elem else {
            return Ok(None);
        };
        if start == 0 && end == Some(0) {
            return Ok(None);
        }
        let mut old_tree = None;
        let mut prev_copy = None;
        if start > 0 {
            let mut tree = elem;
            for _ in 2..=start {
                if self.tree.t.len() > MAX_TREE {
                    return Err(TooBig);
                }
                elem = self.tree.duplicate(elem);
                tree = self.tree.create(Some(tree), Some(elem), TKind::Concat);
            }
            if Some(start) == end {
                return Ok(Some(tree));
            }
            if start >= 2 {
                prev_copy = Some(elem);
            }
            elem = self.tree.duplicate(elem);
            old_tree = Some(tree);
        }
        if let TKind::Subexp(idx) = self.tree.t[elem].kind {
            self.tree.mark_opt_subexp(elem, idx);
            if let Some(p) = prev_copy {
                self.tree.mark_opt_subexp(p, idx);
            }
        }
        let mut tree = self.tree.create(
            Some(elem),
            None,
            if end.is_none() {
                TKind::Star
            } else {
                TKind::Alt
            },
        );
        if let Some(end) = end {
            for _ in (start + 2)..=end {
                if self.tree.t.len() > MAX_TREE {
                    return Err(TooBig);
                }
                elem = self.tree.duplicate(elem);
                tree = self.tree.create(Some(tree), Some(elem), TKind::Concat);
                tree = self.tree.create(Some(tree), None, TKind::Alt);
            }
        }
        Ok(Some(match old_tree {
            Some(o) => self.tree.create(Some(o), Some(tree), TKind::Concat),
            None => tree,
        }))
    }
}

struct Nfa {
    nodes: Vec<NNode>,
    nexts: Vec<u32>,
    edests: Vec<Vec<u32>>,
    ecl: Vec<Ecl>,
    org: Vec<u32>,
}

impl Nfa {
    fn add(&mut self, mut n: NNode) -> u32 {
        n.constraint = 0;
        self.nodes.push(n);
        self.nexts.push(NONE);
        self.edests.push(Vec::new());
        self.ecl.push(Ecl::Todo);
        self.org.push(NONE);
        (self.nodes.len() - 1) as u32
    }

    fn duplicate_node(&mut self, org: u32, constraint: u16) -> u32 {
        let base = self.nodes[org as usize];
        let d = self.add(base);
        let dn = &mut self.nodes[d as usize];
        dn.constraint = constraint | base.constraint;
        dn.duplicated = true;
        self.org[d as usize] = org;
        d
    }

    fn search_duplicated(&self, org: u32, constraint: u16) -> Option<u32> {
        let mut idx = self.nodes.len() - 1;
        while self.nodes[idx].duplicated && idx > 0 {
            if self.org[idx] == org && self.nodes[idx].constraint == constraint {
                return Some(idx as u32);
            }
            idx -= 1;
        }
        None
    }

    fn duplicate_closure(&mut self, top_org: u32, top_clone: u32, root: u32, init: u16) {
        let mut org = top_org;
        let mut clone = top_clone;
        let mut constraint = init;
        loop {
            let (org_dest, clone_dest);
            if matches!(self.nodes[org as usize].kind, Kind::BackRef(_)) {
                org_dest = self.nexts[org as usize];
                self.edests[clone as usize].clear();
                clone_dest = self.duplicate_node(org_dest, constraint);
                self.nexts[clone as usize] = self.nexts[org as usize];
                set_insert(&mut self.edests[clone as usize], clone_dest);
            } else if self.edests[org as usize].is_empty() {
                self.nexts[clone as usize] = self.nexts[org as usize];
                break;
            } else if self.edests[org as usize].len() == 1 {
                org_dest = self.edests[org as usize][0];
                self.edests[clone as usize].clear();
                if org == root && clone != org {
                    set_insert(&mut self.edests[clone as usize], org_dest);
                    break;
                }
                constraint |= self.nodes[org as usize].constraint;
                clone_dest = self.duplicate_node(org_dest, constraint);
                set_insert(&mut self.edests[clone as usize], clone_dest);
            } else {
                let first = self.edests[org as usize][0];
                let second = self.edests[org as usize][1];
                self.edests[clone as usize].clear();
                match self.search_duplicated(first, constraint) {
                    None => {
                        let c = self.duplicate_node(first, constraint);
                        set_insert(&mut self.edests[clone as usize], c);
                        self.duplicate_closure(first, c, root, constraint);
                    }
                    Some(c) => set_insert(&mut self.edests[clone as usize], c),
                }
                org_dest = second;
                clone_dest = self.duplicate_node(second, constraint);
                set_insert(&mut self.edests[clone as usize], clone_dest);
            }
            org = org_dest;
            clone = clone_dest;
        }
    }

    fn eclosure_iter(&mut self, node: u32, root: bool) -> Vec<u32> {
        let mut eclosure = vec![node];
        let mut incomplete = false;
        self.ecl[node as usize] = Ecl::Busy;
        let n = self.nodes[node as usize];
        if n.constraint != 0
            && !self.edests[node as usize].is_empty()
            && !self.nodes[self.edests[node as usize][0] as usize].duplicated
        {
            self.duplicate_closure(node, node, node, n.constraint);
        }
        if n.kind.eps() {
            let mut i = 0;
            while i < self.edests[node as usize].len() {
                let edest = self.edests[node as usize][i];
                i += 1;
                if self.ecl[edest as usize] == Ecl::Busy {
                    incomplete = true;
                    continue;
                }
                let elem = match &self.ecl[edest as usize] {
                    Ecl::Done(v) => v.clone(),
                    _ => self.eclosure_iter(edest, false),
                };
                set_merge(&mut eclosure, &elem);
                if self.ecl[edest as usize] == Ecl::Todo {
                    incomplete = true;
                }
            }
        }
        if incomplete && !root {
            self.ecl[node as usize] = Ecl::Todo;
        } else {
            self.ecl[node as usize] = Ecl::Done(eclosure.clone());
        }
        eclosure
    }

    fn calc_eclosure(&mut self) {
        let mut incomplete = false;
        let mut idx = 0;
        loop {
            if idx == self.nodes.len() {
                if !incomplete {
                    break;
                }
                incomplete = false;
                idx = 0;
            }
            if matches!(self.ecl[idx], Ecl::Done(_)) {
                idx += 1;
                continue;
            }
            self.eclosure_iter(idx as u32, true);
            if self.ecl[idx] == Ecl::Todo {
                incomplete = true;
            }
            idx += 1;
        }
    }
}

impl Gre {
    #[must_use]
    pub fn new(
        node: &Node,
        groups: usize,
        utf8: bool,
        byte_mode: bool,
        icase: bool,
    ) -> Option<Self> {
        let mut b = Builder {
            tree: Tree::default(),
            sets: Vec::new(),
            nbackref: 0,
            used_bkref_map: 0,
            word_ops: false,
            mb: utf8 && !byte_mode,
        };
        let body = b.build(node).ok()?;
        let eor = b.tree.create(None, None, TKind::End);
        let root = match body {
            Some(t) => b.tree.create(Some(t), Some(eor), TKind::Concat),
            None => eor,
        };
        let mut subexp_map: Vec<usize> = (0..groups).collect();
        let mut used = b.used_bkref_map;
        b.tree.preorder(root, &mut |t, n| match t.t[n].kind {
            TKind::BackRef(idx) => {
                let mapped = subexp_map.get(idx).copied().unwrap_or(idx);
                t.t[n].kind = TKind::BackRef(mapped);
                if mapped < 64 {
                    used |= 1 << mapped;
                }
            }
            TKind::Subexp(sidx) => {
                if let Some(l) = t.t[n].left
                    && let TKind::Subexp(other) = t.t[l].kind
                {
                    let inner = t.t[l].left;
                    t.t[n].left = inner;
                    if let Some(i) = inner {
                        t.t[i].parent = Some(n);
                    }
                    if other < subexp_map.len() && sidx < subexp_map.len() {
                        subexp_map[other] = subexp_map[sidx];
                    }
                    if other < 64 {
                        used &= !(1 << other);
                    }
                }
            }
            _ => {}
        });
        b.tree.postorder(root, &mut |t, n| {
            for side in [false, true] {
                let child = if side { t.t[n].right } else { t.t[n].left };
                let Some(c) = child else {
                    continue;
                };
                let TKind::Subexp(idx) = t.t[c].kind else {
                    continue;
                };
                let body = t.t[c].left;
                let opt = t.t[c].opt_subexp;
                let op = t.create(None, None, TKind::Open(idx));
                let cls = t.create(None, None, TKind::Close(idx));
                t.t[op].opt_subexp = opt;
                t.t[cls].opt_subexp = opt;
                let tree1 = match body {
                    Some(bd) => t.create(Some(bd), Some(cls), TKind::Concat),
                    None => cls,
                };
                let lowered = t.create(Some(op), Some(tree1), TKind::Concat);
                t.t[lowered].parent = Some(n);
                if side {
                    t.t[n].right = Some(lowered);
                } else {
                    t.t[n].left = Some(lowered);
                }
            }
        });
        let text = TextMode {
            utf8,
            mb: utf8 && !byte_mode,
            icase,
        };
        let sets = b.sets;
        let mut nfa = Nfa {
            nodes: Vec::new(),
            nexts: Vec::new(),
            edests: Vec::new(),
            ecl: Vec::new(),
            org: Vec::new(),
        };
        b.tree.postorder(root, &mut |t, n| {
            if t.t[n].kind == TKind::Concat {
                let l = t.t[n].left.unwrap_or(n);
                t.t[n].first = t.t[l].first;
                t.t[n].node_idx = t.t[l].node_idx;
                return;
            }
            t.t[n].first = n;
            let (kind, constraint) = match t.t[n].kind {
                TKind::Leaf(s) => (Kind::Leaf(s), 0),
                TKind::End => (Kind::End, 0),
                TKind::BackRef(i) => (Kind::BackRef(i), 0),
                TKind::Anchor(c) => (Kind::Anchor, c),
                TKind::Open(i) => (Kind::Open(i), 0),
                TKind::Close(i) => (Kind::Close(i), 0),
                TKind::Alt => (Kind::Alt, 0),
                TKind::Star | TKind::Subexp(_) | TKind::Concat => (Kind::Star, 0),
            };
            let accept_mb = match kind {
                Kind::Leaf(s) => {
                    utf8 && sets[s as usize]
                        .ranges()
                        .iter()
                        .any(|&(lo, hi)| hi >= 0x80 && lo < locale::INVALID_BASE)
                }
                _ => false,
            };
            let idx = nfa.add(NNode {
                kind,
                constraint: 0,
                duplicated: t.t[n].duplicated,
                opt_subexp: t.t[n].opt_subexp,
                accept_mb,
            });
            nfa.nodes[idx as usize].constraint = constraint;
            t.t[n].node_idx = idx;
        });
        b.tree.preorder(root, &mut |t, n| match t.t[n].kind {
            TKind::Star => {
                if let Some(l) = t.t[n].left {
                    t.t[l].next = Some(n);
                }
            }
            TKind::Concat => {
                if let (Some(l), Some(r)) = (t.t[n].left, t.t[n].right) {
                    t.t[l].next = Some(t.t[r].first);
                    t.t[r].next = t.t[n].next;
                }
            }
            _ => {
                let next = t.t[n].next;
                if let Some(l) = t.t[n].left {
                    t.t[l].next = next;
                }
                if let Some(r) = t.t[n].right {
                    t.t[r].next = next;
                }
            }
        });
        let mut plural = false;
        b.tree.preorder(root, &mut |t, n| {
            let idx = t.t[n].node_idx as usize;
            let next_idx = t.t[n].next.map_or(NONE, |x| t.t[x].node_idx);
            match t.t[n].kind {
                TKind::Concat | TKind::End | TKind::Subexp(_) => {}
                TKind::Star | TKind::Alt => {
                    plural = true;
                    let left = t.t[n].left.map_or(next_idx, |l| t.t[t.t[l].first].node_idx);
                    let right = t.t[n]
                        .right
                        .map_or(next_idx, |r| t.t[t.t[r].first].node_idx);
                    nfa.edests[idx] = if left == right {
                        vec![left]
                    } else {
                        vec![left.min(right), left.max(right)]
                    };
                }
                TKind::Anchor(_) | TKind::Open(_) | TKind::Close(_) => {
                    nfa.edests[idx] = vec![next_idx];
                }
                TKind::BackRef(_) => {
                    nfa.nexts[idx] = next_idx;
                    nfa.edests[idx] = vec![next_idx];
                }
                TKind::Leaf(_) => nfa.nexts[idx] = next_idx,
            }
        });
        nfa.calc_eclosure();
        let eclosures: Vec<Vec<u32>> = nfa
            .ecl
            .iter()
            .map(|e| match e {
                Ecl::Done(v) => v.clone(),
                _ => Vec::new(),
            })
            .collect();
        let mut inveclosures = vec![Vec::new(); nfa.nodes.len()];
        for (src, ecl) in eclosures.iter().enumerate() {
            for &e in ecl {
                inveclosures[e as usize].push(src as u32);
            }
        }
        let first = b.tree.t[b.tree.t[root].first].node_idx;
        let mut init_nodes = eclosures[first as usize].clone();
        if b.nbackref > 0 {
            let mut i = 0;
            while i < init_nodes.len() {
                let node = init_nodes[i];
                if let Kind::BackRef(bidx) = nfa.nodes[node as usize].kind
                    && init_nodes
                        .iter()
                        .any(|&c| nfa.nodes[c as usize].kind == Kind::Close(bidx))
                {
                    let dest = nfa.edests[node as usize][0];
                    if !set_contains(&init_nodes, dest) {
                        set_merge(&mut init_nodes, &eclosures[dest as usize]);
                        i = 0;
                    }
                }
                i += 1;
            }
        }
        let period: Vec<bool> = sets.iter().map(|s| *s == UnitSet::universe(utf8)).collect();
        let fastmap = first_bytes(&nfa.nodes, &init_nodes, &sets, utf8);
        Some(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            nodes: nfa.nodes,
            nexts: nfa.nexts,
            edests: nfa.edests,
            eclosures,
            inveclosures,
            sets,
            period,
            init_node: first,
            init_nodes,
            nbackref: b.nbackref,
            used_bkref_map: used,
            plural,
            word_ops: b.word_ops,
            re_nsub: groups,
            text,
            fastmap,
        })
    }

    #[must_use]
    pub fn search(
        &self,
        hay: &[u8],
        from: usize,
        limit: usize,
        not_eol: bool,
        newline_anchor: bool,
    ) -> Option<(usize, usize)> {
        if from > limit {
            return None;
        }
        self.run(&hay[..limit], from, limit, not_eol, newline_anchor)
    }

    #[must_use]
    pub fn match_at(
        &self,
        hay: &[u8],
        start: usize,
        length: usize,
        not_eol: bool,
        newline_anchor: bool,
    ) -> Option<usize> {
        if start > length || length > hay.len() {
            return None;
        }
        self.run(&hay[..length], start, start, not_eol, newline_anchor)
            .map(|(_, e)| e)
    }

    fn run(
        &self,
        hay: &[u8],
        start: usize,
        last_start: usize,
        not_eol: bool,
        newline_anchor: bool,
    ) -> Option<(usize, usize)> {
        thread_local! {
            static CACHE: RefCell<FxMap<u64, States>> = RefCell::new(FxMap::default());
        }
        let ubuf = if self.text.icase {
            Some(upper_buffer(hay, self.text.utf8))
        } else {
            None
        };
        let inp = Input {
            hay,
            ubuf,
            text: self.text,
            word_ops: self.word_ops,
            anchors: Anchors {
                newline_anchor,
                not_eol,
            },
        };
        CACHE.with(|c| {
            let mut map = c.borrow_mut();
            if map.len() > 64 {
                map.clear();
            }
            let st = map.entry(self.id).or_default();
            if st.v.len() > MAX_CACHED_STATES {
                *st = States::default();
            }
            self.search_internal(st, &inp, start, last_start)
        })
    }

    fn search_internal(
        &self,
        st: &mut States,
        inp: &Input<'_>,
        start: usize,
        last_start: usize,
    ) -> Option<(usize, usize)> {
        let init = self.init_states(st);
        let (mut start, mut last_start) = (start, last_start);
        if st.v[init[0] as usize].nodes.is_empty()
            && st.v[init[1] as usize].nodes.is_empty()
            && (st.v[init[2] as usize].nodes.is_empty() || !inp.anchors.newline_anchor)
        {
            if start != 0 && last_start != 0 {
                return None;
            }
            start = 0;
            last_start = 0;
        }
        let nmatch = self.re_nsub + 1;
        let scratch = std::mem::take(&mut st.scratch);
        let ents = std::mem::take(&mut st.ents);
        let tops = std::mem::take(&mut st.tops);
        let mut m = Mctx {
            re: self,
            st,
            inp,
            base: start,
            len: inp.hay.len() - start,
            cur: 0,
            log: scratch,
            sifted: Vec::new(),
            top: 0,
            ents,
            tops,
            max_mb: 0,
            match_last: 0,
            last_node: 0,
        };
        let r = self.search_loop(&mut m, init, start, last_start, nmatch);
        let upto = (m.top + 1).min(m.log.len());
        m.log[..upto].fill(NONE);
        m.st.scratch = std::mem::take(&mut m.log);
        m.ents.clear();
        m.tops.clear();
        m.st.ents = std::mem::take(&mut m.ents);
        m.st.tops = std::mem::take(&mut m.tops);
        r
    }

    fn search_loop(
        &self,
        m: &mut Mctx<'_>,
        init: [u32; 4],
        start: usize,
        last_start: usize,
        nmatch: usize,
    ) -> Option<(usize, usize)> {
        let inp = m.inp;
        let mut match_first = start;
        loop {
            if match_first > last_start {
                return None;
            }
            if let Some(fm) = &self.fastmap {
                while match_first < last_start && !fm[usize::from(inp.hay[match_first])] {
                    match_first += 1;
                }
                if match_first >= inp.hay.len() || !fm[usize::from(inp.hay[match_first])] {
                    return None;
                }
            }
            if self.text.mb && !inp.is_char_start(match_first) {
                match_first += 1;
                continue;
            }
            m.reset(match_first);
            if let Some(ml) = m.check_matching(init) {
                m.match_last = ml;
                if nmatch > 1 || self.nbackref > 0 {
                    let ps = m.log[ml];
                    m.last_node = m.check_halt_state_context(ps, ml);
                }
                if (nmatch > 1 && self.plural) || self.nbackref > 0 {
                    if m.prune_impossible_nodes() {
                        break;
                    }
                } else {
                    m.sifted.clear();
                    m.sifted.extend_from_slice(&m.log[..=ml]);
                    break;
                }
            }
            match_first += 1;
        }
        if nmatch > 1 && !m.set_regs(nmatch, self.plural && self.nbackref > 0) {
            return None;
        }
        Some((match_first, match_first + m.match_last))
    }

    fn init_states(&self, st: &mut States) -> [u32; 4] {
        if let Some(i) = st.init {
            return i;
        }
        let init = self.acquire_cd(st, &self.init_nodes, 0);
        let s = if st.v[init as usize].flags & HAS_CONSTRAINT != 0 {
            [
                init,
                self.acquire_cd(st, &self.init_nodes, CTX_WORD),
                self.acquire_cd(st, &self.init_nodes, CTX_NEWLINE),
                self.acquire_cd(st, &self.init_nodes, CTX_NEWLINE | CTX_BEGBUF),
            ]
        } else {
            [init; 4]
        };
        st.init = Some(s);
        s
    }

    fn acquire_ci(&self, st: &mut States, nodes: &[u32]) -> u32 {
        if nodes.is_empty() {
            return NONE;
        }
        if let Some(&s) = st.ci.get(nodes) {
            return s;
        }
        let mut flags = 0;
        for &n in nodes {
            let node = &self.nodes[n as usize];
            if node.accept_mb {
                flags |= ACCEPT_MB;
            }
            match node.kind {
                Kind::End => flags |= HALT,
                Kind::BackRef(_) => flags |= HAS_BACKREF,
                Kind::Anchor => flags |= HAS_CONSTRAINT,
                _ if node.constraint != 0 => flags |= HAS_CONSTRAINT,
                _ => {}
            }
        }
        let id = st.push(self, nodes.to_vec(), None, flags);
        st.ci.insert(nodes.to_vec(), id);
        id
    }

    fn acquire_cd(&self, st: &mut States, nodes: &[u32], ctx: u8) -> u32 {
        if nodes.is_empty() {
            return NONE;
        }
        if let Some(slots) = st.cd.get(nodes) {
            let s = slots[usize::from(ctx & 15)];
            if s != NONE {
                return s;
            }
        }
        let mut flags = 0;
        let mut kept = Vec::with_capacity(nodes.len());
        let mut any_constraint = false;
        for &n in nodes {
            let node = &self.nodes[n as usize];
            if node.accept_mb {
                flags |= ACCEPT_MB;
            }
            match node.kind {
                Kind::End => flags |= HALT,
                Kind::BackRef(_) => flags |= HAS_BACKREF,
                _ => {}
            }
            if node.constraint != 0 {
                any_constraint = true;
                flags |= HAS_CONSTRAINT;
                if not_prev(node.constraint, ctx) {
                    continue;
                }
            }
            kept.push(n);
        }
        let entrance = any_constraint.then(|| nodes.to_vec());
        let id = st.push(self, kept, entrance, flags);
        st.cd.entry(nodes.to_vec()).or_insert([NONE; 16])[usize::from(ctx & 15)] = id;
        id
    }

    fn word_bitset(&self, b: u8) -> bool {
        if !self.word_ops {
            return false;
        }
        if self.text.utf8 {
            b < 0x80 && locale::is_word_regex(true, u32::from(b))
        } else {
            locale::is_word_regex(false, u32::from(b))
        }
    }
}

fn utf8_lead(c: u32) -> u8 {
    let mut buf = Vec::with_capacity(4);
    locale::encode(true, c, &mut buf);
    buf.first().copied().unwrap_or(0xff)
}

fn first_bytes(
    nodes: &[NNode],
    init: &[u32],
    sets: &[UnitSet],
    utf8: bool,
) -> Option<Box<[bool; 256]>> {
    let mut map = Box::new([false; 256]);
    for &n in init {
        match nodes[n as usize].kind {
            Kind::End | Kind::BackRef(_) => return None,
            Kind::Leaf(s) => {
                for &(lo, hi) in sets[s as usize].ranges() {
                    if !utf8 {
                        for b in lo..=hi.min(0xff) {
                            map[b as usize] = true;
                        }
                        continue;
                    }
                    if lo < 0x80 {
                        for b in lo..=hi.min(0x7f) {
                            map[b as usize] = true;
                        }
                    }
                    let (mlo, mhi) = (lo.max(0x80), hi.min(locale::MAX_CHAR));
                    if mlo <= mhi {
                        for b in utf8_lead(mlo)..=utf8_lead(mhi) {
                            map[usize::from(b)] = true;
                        }
                    }
                    let (ilo, ihi) = (
                        lo.max(locale::INVALID_BASE),
                        hi.min(locale::INVALID_BASE + 0xff),
                    );
                    if ilo <= ihi {
                        for u in ilo..=ihi {
                            map[(u - locale::INVALID_BASE) as usize] = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Some(map)
}

#[derive(Default)]
struct States {
    v: Vec<State>,
    ci: FxMap<Vec<u32>, u32>,
    cd: FxMap<Vec<u32>, [u32; 16]>,
    init: Option<[u32; 4]>,
    scratch: Vec<u32>,
    ents: Vec<Ent>,
    tops: Vec<SubTop>,
}

struct State {
    nodes: Vec<u32>,
    non_eps: Vec<u32>,
    special: Rc<[u32]>,
    entrance: Option<Vec<u32>>,
    flags: u8,
    inv: Option<Vec<u32>>,
    sb: Option<Box<[u32; 256]>>,
}

impl States {
    fn push(&mut self, re: &Gre, nodes: Vec<u32>, entrance: Option<Vec<u32>>, flags: u8) -> u32 {
        let non_eps = nodes
            .iter()
            .copied()
            .filter(|&n| !re.nodes[n as usize].kind.eps())
            .collect();
        let special = nodes
            .iter()
            .copied()
            .filter(|&n| match re.nodes[n as usize].kind {
                Kind::BackRef(_) => true,
                Kind::Open(i) => i < 64 && re.used_bkref_map & (1 << i) != 0,
                _ => false,
            })
            .collect::<Vec<u32>>()
            .into();
        self.v.push(State {
            nodes,
            non_eps,
            special,
            entrance,
            flags,
            inv: None,
            sb: None,
        });
        (self.v.len() - 1) as u32
    }

    fn nodes(&self, s: u32) -> &[u32] {
        if s == NONE {
            &[]
        } else {
            &self.v[s as usize].nodes
        }
    }

    fn entrance(&self, s: u32) -> &[u32] {
        let st = &self.v[s as usize];
        st.entrance.as_deref().unwrap_or(&st.nodes)
    }

    fn flag(&self, s: u32, f: u8) -> bool {
        s != NONE && self.v[s as usize].flags & f != 0
    }
}

fn upper_buffer(hay: &[u8], utf8: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        let (u, len) = locale::decode(utf8, &hay[i..]);
        let up = if u < locale::INVALID_BASE {
            locale::to_upper(utf8, u)
        } else {
            u
        };
        if up == u {
            out.extend_from_slice(&hay[i..i + len]);
        } else {
            let mut enc = Vec::new();
            locale::encode(utf8, up, &mut enc);
            if enc.len() == len {
                out.extend_from_slice(&enc);
            } else {
                out.extend_from_slice(&hay[i..i + len]);
            }
        }
        i += len;
    }
    out
}

#[derive(Clone, Copy)]
struct Anchors {
    newline_anchor: bool,
    not_eol: bool,
}

struct Input<'h> {
    hay: &'h [u8],
    ubuf: Option<Vec<u8>>,
    text: TextMode,
    word_ops: bool,
    anchors: Anchors,
}

impl Input<'_> {
    fn buf(&self) -> &[u8] {
        self.ubuf.as_deref().unwrap_or(self.hay)
    }

    fn unit_at(&self, abs: usize) -> (u32, usize) {
        locale::decode(self.text.utf8, &self.hay[abs..])
    }

    fn char_start(&self, abs: usize) -> usize {
        if !self.text.mb || abs >= self.hay.len() || self.hay[abs] & 0xc0 != 0x80 {
            return abs;
        }
        for back in 1..=3.min(abs) {
            let s = abs - back;
            if self.hay[s] & 0xc0 != 0x80 {
                let (u, len) = locale::decode(true, &self.hay[s..]);
                if u < locale::INVALID_BASE && len > back {
                    return s;
                }
                return abs;
            }
        }
        abs
    }

    fn is_char_start(&self, abs: usize) -> bool {
        self.char_start(abs) == abs
    }

    fn ctx_char(&self, abs: usize, word_bitset: &dyn Fn(u8) -> bool) -> u8 {
        if self.text.mb {
            let s = self.char_start(abs);
            let (u, _) = self.unit_at(s);
            let u = if self.text.icase && u < locale::INVALID_BASE {
                locale::to_upper(true, u)
            } else {
                u
            };
            if self.word_ops && locale::is_word_regex(true, u) {
                return CTX_WORD;
            }
            if u == u32::from(b'\n') && self.anchors.newline_anchor {
                CTX_NEWLINE
            } else {
                0
            }
        } else {
            let b = self.hay[abs];
            let b = if self.text.icase {
                locale::to_upper(false, u32::from(b)) as u8
            } else {
                b
            };
            if word_bitset(b) {
                return CTX_WORD;
            }
            if b == b'\n' && self.anchors.newline_anchor {
                CTX_NEWLINE
            } else {
                0
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Ent {
    node: u32,
    str_idx: usize,
    from: usize,
    to: usize,
    eps_reachable: u64,
    more: bool,
}

#[derive(Default, Clone)]
struct Path {
    array: Vec<u32>,
    next_idx: usize,
}

struct SubLast {
    node: u32,
    str_idx: usize,
    path: Path,
}

struct SubTop {
    str_idx: usize,
    node: u32,
    path: Option<Path>,
    lasts: Vec<SubLast>,
}

struct Sctx {
    last_node: u32,
    last_str_idx: usize,
    limits: Vec<u32>,
}

struct Mctx<'a> {
    re: &'a Gre,
    st: &'a mut States,
    inp: &'a Input<'a>,
    base: usize,
    len: usize,
    cur: usize,
    log: Vec<u32>,
    sifted: Vec<u32>,
    top: usize,
    ents: Vec<Ent>,
    tops: Vec<SubTop>,
    max_mb: usize,
    match_last: usize,
    last_node: u32,
}

fn get(v: &[u32], i: usize) -> u32 {
    v.get(i).copied().unwrap_or(NONE)
}

fn put(v: &mut Vec<u32>, i: usize, s: u32) {
    if i >= v.len() {
        v.resize(i + 1, NONE);
    }
    v[i] = s;
}

impl Mctx<'_> {
    fn reset(&mut self, match_first: usize) {
        let upto = (self.top + 1).min(self.log.len());
        self.log[..upto].fill(NONE);
        if self.log.len() < self.inp.hay.len() + 2 {
            self.log.resize(self.inp.hay.len() + 2, NONE);
        }
        self.top = 0;
        self.base = match_first;
        self.len = self.inp.hay.len() - match_first;
        self.cur = 0;
        self.ents.clear();
        self.tops.clear();
        self.max_mb = 0;
    }

    fn context_at(&self, rel: isize) -> u8 {
        if rel < 0 {
            if self.base == 0 {
                return CTX_NEWLINE | CTX_BEGBUF;
            }
            return self
                .inp
                .ctx_char(self.base - 1, &|b| self.re.word_bitset(b));
        }
        let rel = rel as usize;
        if rel == self.len {
            return if self.inp.anchors.not_eol {
                CTX_ENDBUF
            } else {
                CTX_NEWLINE | CTX_ENDBUF
            };
        }
        self.inp
            .ctx_char(self.base + rel, &|b| self.re.word_bitset(b))
    }

    fn unit(&self, rel: usize) -> (u32, usize) {
        if rel >= self.len {
            return (u32::MAX, 0);
        }
        self.inp.unit_at(self.base + rel)
    }

    fn byte(&self, rel: usize) -> u8 {
        self.inp.buf()[self.base + rel]
    }

    fn node(&self, n: u32) -> &NNode {
        &self.re.nodes[n as usize]
    }

    fn leaf_contains(&self, n: u32, u: u32) -> bool {
        match self.node(n).kind {
            Kind::Leaf(s) => self.re.sets[s as usize].contains(u),
            _ => false,
        }
    }

    fn accept_bytes(&self, n: u32, rel: usize) -> usize {
        if !self.node(n).accept_mb {
            return 0;
        }
        let (u, len) = self.unit(rel);
        if len > 1 && self.leaf_contains(n, u) {
            len
        } else {
            0
        }
    }

    fn check_node_accept(&self, n: u32, rel: usize) -> bool {
        if rel >= self.len {
            return false;
        }
        let node = *self.node(n);
        let Kind::Leaf(s) = node.kind else {
            return false;
        };
        let ok = if self.re.period[s as usize] {
            !self.re.text.utf8 || self.re.text.mb || self.byte(rel) < 0x80
        } else {
            let (u, len) = self.unit(rel);
            len == 1 && self.re.sets[s as usize].contains(u)
        };
        if !ok {
            return false;
        }
        if node.constraint != 0 {
            let ctx = self.context_at(rel as isize);
            if not_next(node.constraint, ctx) {
                return false;
            }
        }
        true
    }

    fn accept_forward_sb(&self, n: u32, rel: usize, u: u32) -> bool {
        let node = *self.node(n);
        if !self.leaf_contains(n, u) {
            return false;
        }
        let c = node.constraint;
        if c == 0 {
            return true;
        }
        let b = self.byte(rel);
        if c & NEXT_NEWLINE != 0 && b != b'\n' {
            return false;
        }
        if c & NEXT_ENDBUF != 0 {
            return false;
        }
        let non_sb = self.re.text.mb && b >= 0x80;
        if c & NEXT_WORD != 0 && !non_sb && !self.re.word_bitset(b) {
            return false;
        }
        if c & NEXT_NOTWORD != 0 && !non_sb && self.re.word_bitset(b) {
            return false;
        }
        true
    }

    fn clean_state_log_if_needed(&mut self, next: usize) {
        if self.top < next {
            if self.log.len() <= next {
                self.log.resize(next + 1, NONE);
            }
            self.top = next;
        }
    }

    fn acquire_init(&mut self, init: [u32; 4]) -> u32 {
        if !self.st.flag(init[0], HAS_CONSTRAINT) {
            return init[0];
        }
        let ctx = self.context_at(-1);
        if ctx & CTX_WORD != 0 {
            init[1]
        } else if ctx == 0 {
            init[0]
        } else if ctx & CTX_BEGBUF != 0 && ctx & CTX_NEWLINE != 0 {
            init[3]
        } else if ctx & CTX_NEWLINE != 0 {
            init[2]
        } else if ctx & CTX_BEGBUF != 0 {
            let ent = self.st.entrance(init[0]).to_vec();
            self.re.acquire_cd(self.st, &ent, ctx)
        } else {
            init[0]
        }
    }

    fn check_matching(&mut self, init: [u32; 4]) -> Option<usize> {
        let mut match_last = None;
        let mut cur_state = self.acquire_init(init);
        self.log[0] = cur_state;
        if self.re.nbackref > 0 {
            let nodes = Rc::clone(&self.st.v[cur_state as usize].special);
            self.check_subexp_matching_top(&nodes, 0);
            if self.st.flag(cur_state, HAS_BACKREF) {
                self.transit_state_bkref(&nodes);
            }
        }
        if self.st.flag(cur_state, HALT)
            && (!self.st.flag(cur_state, HAS_CONSTRAINT)
                || self.check_halt_state_context(cur_state, 0) != 0)
        {
            match_last = Some(0);
        }
        while self.cur < self.len {
            let next = self.transit_state(cur_state);
            let mut next = self.merge_state_with_log(next);
            if next == NONE {
                next = self.find_recover_state();
                if next == NONE {
                    break;
                }
            }
            cur_state = next;
            if self.st.flag(cur_state, HALT)
                && (!self.st.flag(cur_state, HAS_CONSTRAINT)
                    || self.check_halt_state_context(cur_state, self.cur) != 0)
            {
                match_last = Some(self.cur);
            }
        }
        match_last
    }

    fn check_halt_state_context(&self, s: u32, idx: usize) -> u32 {
        if s == NONE {
            return 0;
        }
        let ctx = self.context_at(idx as isize);
        for &n in self.st.nodes(s) {
            let node = self.node(n);
            if node.kind == Kind::End && (node.constraint == 0 || !not_next(node.constraint, ctx)) {
                return n;
            }
        }
        0
    }

    fn transit_state(&mut self, s: u32) -> u32 {
        if self.st.flag(s, ACCEPT_MB) {
            self.transit_state_mb(s);
        }
        let rel = self.cur;
        self.cur += 1;
        let (u, len) = self.unit(rel);
        if len != 1 {
            return NONE;
        }
        let b = self.byte(rel);
        if let Some(t) = &self.st.v[s as usize].sb {
            let c = t[usize::from(b)];
            if c != UNKNOWN && (!self.re.text.mb || b < 0x80) {
                return c;
            }
        }
        let mut follows = Vec::new();
        for &n in &self.st.v[s as usize].nodes {
            if matches!(self.node(n).kind, Kind::Leaf(_)) && self.accept_forward_sb(n, rel, u) {
                let nx = self.re.nexts[n as usize];
                if nx != NONE {
                    set_merge(&mut follows, &self.re.eclosures[nx as usize]);
                }
            }
        }
        let ctx = if b == b'\n' {
            CTX_NEWLINE
        } else {
            self.context_at(rel as isize) & CTX_WORD
        };
        let r = self.re.acquire_cd(self.st, &follows, ctx);
        if !self.re.text.mb || b < 0x80 {
            let st = &mut self.st.v[s as usize];
            let t = st.sb.get_or_insert_with(|| Box::new([UNKNOWN; 256]));
            t[usize::from(b)] = r;
        }
        r
    }

    fn transit_state_mb(&mut self, s: u32) {
        let rel = self.cur;
        let nodes = self.st.nodes(s).to_vec();
        for n in nodes {
            let node = *self.node(n);
            if !node.accept_mb {
                continue;
            }
            if node.constraint != 0 {
                let ctx = self.context_at(rel as isize);
                if not_next(node.constraint, ctx) {
                    continue;
                }
            }
            let naccepted = self.accept_bytes(n, rel);
            if naccepted == 0 {
                continue;
            }
            let dest = rel + naccepted;
            self.max_mb = self.max_mb.max(naccepted);
            self.clean_state_log_if_needed(dest);
            let nx = self.re.nexts[n as usize];
            let new_nodes = &self.re.eclosures[nx as usize];
            let ds = self.log[dest];
            let dest_nodes = if ds == NONE {
                new_nodes.clone()
            } else {
                set_union(self.st.entrance(ds), new_nodes)
            };
            let ctx = self.context_at(dest as isize - 1);
            self.log[dest] = self.re.acquire_cd(self.st, &dest_nodes, ctx);
        }
    }

    fn merge_state_with_log(&mut self, next: u32) -> u32 {
        let cur = self.cur;
        let mut next = next;
        if cur > self.top {
            put(&mut self.log, cur, next);
            self.top = cur;
        } else if self.log[cur] == NONE {
            self.log[cur] = next;
        } else {
            let pstate = self.log[cur];
            let log_nodes = self.st.entrance(pstate).to_vec();
            let next_nodes = if next == NONE {
                log_nodes
            } else {
                set_union(self.st.entrance(next), &log_nodes)
            };
            let ctx = self.context_at(cur as isize - 1);
            next = self.re.acquire_cd(self.st, &next_nodes, ctx);
            self.log[cur] = next;
        }
        if self.re.nbackref > 0 && next != NONE {
            let nodes = Rc::clone(&self.st.v[next as usize].special);
            self.check_subexp_matching_top(&nodes, cur);
            if self.st.flag(next, HAS_BACKREF) {
                self.transit_state_bkref(&nodes);
                next = self.log[cur];
            }
        }
        next
    }

    fn find_recover_state(&mut self) -> u32 {
        loop {
            let max = self.top;
            let mut cur = self.cur;
            loop {
                cur += 1;
                if cur > max {
                    return NONE;
                }
                self.cur += 1;
                if self.log[cur] != NONE {
                    break;
                }
            }
            let s = self.merge_state_with_log(NONE);
            if s != NONE {
                return s;
            }
        }
    }

    fn check_subexp_matching_top(&mut self, nodes: &[u32], str_idx: usize) {
        for &n in nodes {
            if let Kind::Open(idx) = self.node(n).kind
                && idx < 64
                && self.re.used_bkref_map & (1 << idx) != 0
            {
                self.tops.push(SubTop {
                    str_idx,
                    node: n,
                    path: None,
                    lasts: Vec::new(),
                });
            }
        }
    }

    fn transit_state_bkref(&mut self, nodes: &[u32]) {
        let cur = self.cur;
        for &n in nodes {
            let node = *self.node(n);
            if !matches!(node.kind, Kind::BackRef(_)) {
                continue;
            }
            if node.constraint != 0 {
                let ctx = self.context_at(cur as isize);
                if not_next(node.constraint, ctx) {
                    continue;
                }
            }
            let mut bkc = self.ents.len();
            self.get_subexp(n, cur);
            while bkc < self.ents.len() {
                let ent = self.ents[bkc];
                bkc += 1;
                if ent.node != n || ent.str_idx != cur {
                    continue;
                }
                let len = ent.to - ent.from;
                let target = if len == 0 {
                    self.re.edests[n as usize][0]
                } else {
                    self.re.nexts[n as usize]
                };
                let new_dest = self.re.eclosures[target as usize].clone();
                let dest = cur + len;
                let ctx = self.context_at(dest as isize - 1);
                let ds = get(&self.log, dest);
                let prev = {
                    let c = self.log[cur];
                    if c == NONE { 0 } else { self.st.nodes(c).len() }
                };
                let merged = if ds == NONE {
                    new_dest.clone()
                } else {
                    set_union(self.st.entrance(ds), &new_dest)
                };
                let s = self.re.acquire_cd(self.st, &merged, ctx);
                put(&mut self.log, dest, s);
                if len == 0 && self.st.nodes(self.log[cur]).len() > prev {
                    self.check_subexp_matching_top(&new_dest, cur);
                    self.transit_state_bkref(&new_dest);
                }
            }
        }
    }

    fn search_cur_bkref_entry(&self, str_idx: usize) -> Option<usize> {
        let i = self.ents.partition_point(|e| e.str_idx < str_idx);
        (i < self.ents.len() && self.ents[i].str_idx == str_idx).then_some(i)
    }

    fn add_entry(&mut self, node: u32, str_idx: usize, from: usize, to: usize) {
        if let Some(last) = self.ents.last_mut()
            && last.str_idx == str_idx
        {
            last.more = true;
        }
        self.ents.push(Ent {
            node,
            str_idx,
            from,
            to,
            eps_reachable: if from == to { u64::MAX } else { 0 },
            more: false,
        });
        self.max_mb = self.max_mb.max(to - from);
    }

    fn get_subexp(&mut self, bkref_node: u32, bkref_str_idx: usize) {
        if let Some(mut i) = self.search_cur_bkref_entry(bkref_str_idx) {
            loop {
                if self.ents[i].node == bkref_node {
                    return;
                }
                if !self.ents[i].more {
                    break;
                }
                i += 1;
            }
        }
        let Some(subexp_num) = self.node(bkref_node).kind.subexp() else {
            return;
        };
        let mut t = 0;
        while t < self.tops.len() {
            let ti = t;
            t += 1;
            if self.node(self.tops[ti].node).kind.subexp() != Some(subexp_num) {
                continue;
            }
            let top_str = self.tops[ti].str_idx;
            let mut sl_str = top_str;
            let mut bkref_str_off = bkref_str_idx;
            let mut li = 0;
            let mut broke = false;
            while li < self.tops[ti].lasts.len() {
                let last_str = self.tops[ti].lasts[li].str_idx;
                let diff = last_str - sl_str;
                if diff > 0 {
                    if bkref_str_off + diff > self.len {
                        broke = true;
                        break;
                    }
                    let a = self.base + bkref_str_off;
                    let b = self.base + sl_str;
                    if self.inp.buf()[a..a + diff] != self.inp.buf()[b..b + diff] {
                        broke = true;
                        break;
                    }
                }
                bkref_str_off += diff;
                sl_str += diff;
                self.get_subexp_sub(ti, li, bkref_node, bkref_str_idx);
                li += 1;
            }
            if broke {
                continue;
            }
            if li > 0 {
                sl_str += 1;
            }
            while sl_str <= bkref_str_idx {
                let off = sl_str - top_str;
                if off > 0 {
                    if bkref_str_off >= self.len {
                        break;
                    }
                    let a = self.byte(bkref_str_off);
                    bkref_str_off += 1;
                    if a != self.byte(sl_str - 1) {
                        break;
                    }
                }
                let ls = self.log[sl_str];
                if ls == NONE {
                    sl_str += 1;
                    continue;
                }
                let cls = self.find_subexp_node(self.st.nodes(ls), subexp_num, true);
                let Some(cls_node) = cls else {
                    sl_str += 1;
                    continue;
                };
                let top_node = self.tops[ti].node;
                let mut path = self.tops[ti].path.take().unwrap_or_default();
                let ok = self.check_arrival(&mut path, top_node, top_str, cls_node, sl_str, true);
                self.tops[ti].path = Some(path);
                if !ok {
                    sl_str += 1;
                    continue;
                }
                self.tops[ti].lasts.push(SubLast {
                    node: cls_node,
                    str_idx: sl_str,
                    path: Path::default(),
                });
                let li = self.tops[ti].lasts.len() - 1;
                self.get_subexp_sub(ti, li, bkref_node, bkref_str_idx);
                sl_str += 1;
            }
        }
    }

    fn get_subexp_sub(&mut self, ti: usize, li: usize, bkref_node: u32, bkref_str: usize) -> bool {
        let (last_node, last_str) = {
            let l = &self.tops[ti].lasts[li];
            (l.node, l.str_idx)
        };
        let mut path = std::mem::take(&mut self.tops[ti].lasts[li].path);
        let ok = self.check_arrival(&mut path, last_node, last_str, bkref_node, bkref_str, false);
        self.tops[ti].lasts[li].path = path;
        if !ok {
            return false;
        }
        let top_str = self.tops[ti].str_idx;
        self.add_entry(bkref_node, bkref_str, top_str, last_str);
        let to = bkref_str + last_str - top_str;
        self.clean_state_log_if_needed(to);
        true
    }

    fn find_subexp_node(&self, nodes: &[u32], subexp: usize, close: bool) -> Option<u32> {
        nodes.iter().copied().find(|&n| {
            let k = self.node(n).kind;
            if close {
                k == Kind::Close(subexp)
            } else {
                k == Kind::Open(subexp)
            }
        })
    }

    fn check_arrival(
        &mut self,
        path: &mut Path,
        top_node: u32,
        top_str: usize,
        last_node: u32,
        last_str: usize,
        close: bool,
    ) -> bool {
        let Some(subexp_num) = self.node(top_node).kind.subexp() else {
            return false;
        };
        let need = last_str + self.max_mb + 2;
        if path.array.len() < need {
            path.array.resize(need, NONE);
        }
        let mut str_idx = if path.next_idx != 0 {
            path.next_idx
        } else {
            top_str
        };
        let mut context = self.context_at(str_idx as isize - 1);
        let mut cur_state = NONE;
        let mut next_nodes;
        if str_idx == top_str {
            next_nodes = vec![top_node];
            self.expand_ecl(&mut next_nodes, subexp_num, close);
        } else {
            cur_state = get(&path.array, str_idx);
            next_nodes = if self.st.flag(cur_state, HAS_BACKREF) {
                self.st.nodes(cur_state).to_vec()
            } else {
                Vec::new()
            };
        }
        if str_idx == top_str || self.st.flag(cur_state, HAS_BACKREF) {
            if !next_nodes.is_empty() {
                self.expand_bkref_cache(
                    &mut path.array,
                    &mut next_nodes,
                    str_idx,
                    subexp_num,
                    close,
                );
            }
            cur_state = self.re.acquire_cd(self.st, &next_nodes, context);
            put(&mut path.array, str_idx, cur_state);
        }
        let mut null_cnt = 0;
        while str_idx < last_str && null_cnt <= self.max_mb {
            next_nodes.clear();
            let s1 = get(&path.array, str_idx + 1);
            if s1 != NONE {
                set_merge(&mut next_nodes, self.st.nodes(s1));
            }
            if cur_state != NONE {
                let non_eps = self.st.v[cur_state as usize].non_eps.clone();
                self.check_arrival_add_next_nodes(
                    &mut path.array,
                    str_idx,
                    &non_eps,
                    &mut next_nodes,
                );
            }
            str_idx += 1;
            if !next_nodes.is_empty() {
                self.expand_ecl(&mut next_nodes, subexp_num, close);
                self.expand_bkref_cache(
                    &mut path.array,
                    &mut next_nodes,
                    str_idx,
                    subexp_num,
                    close,
                );
            }
            context = self.context_at(str_idx as isize - 1);
            cur_state = self.re.acquire_cd(self.st, &next_nodes, context);
            put(&mut path.array, str_idx, cur_state);
            null_cnt = if cur_state == NONE { null_cnt + 1 } else { 0 };
        }
        let cur_nodes = get(&path.array, last_str);
        path.next_idx = str_idx;
        cur_nodes != NONE && set_contains(self.st.nodes(cur_nodes), last_node)
    }

    fn check_arrival_add_next_nodes(
        &mut self,
        log: &mut Vec<u32>,
        str_idx: usize,
        cur_nodes: &[u32],
        next_nodes: &mut Vec<u32>,
    ) {
        for &n in cur_nodes {
            let mut naccepted = 0;
            if self.node(n).accept_mb {
                naccepted = self.accept_bytes(n, str_idx);
                if naccepted > 1 {
                    let next_node = self.re.nexts[n as usize];
                    let next_idx = str_idx + naccepted;
                    let ds = get(log, next_idx);
                    let mut union = if ds == NONE {
                        Vec::new()
                    } else {
                        self.st.nodes(ds).to_vec()
                    };
                    set_insert(&mut union, next_node);
                    let s = self.re.acquire_ci(self.st, &union);
                    put(log, next_idx, s);
                }
            }
            if naccepted > 0 || self.check_node_accept(n, str_idx) {
                set_insert(next_nodes, self.re.nexts[n as usize]);
            }
        }
    }

    fn expand_ecl(&self, cur_nodes: &mut Vec<u32>, ex_subexp: usize, close: bool) {
        let mut new_nodes = Vec::new();
        for &n in cur_nodes.iter() {
            let ecl = &self.re.eclosures[n as usize];
            if self.find_subexp_node(ecl, ex_subexp, close).is_none() {
                set_merge(&mut new_nodes, ecl);
            } else {
                self.expand_ecl_sub(&mut new_nodes, n, ex_subexp, close);
            }
        }
        *cur_nodes = new_nodes;
    }

    fn expand_ecl_sub(&self, dst: &mut Vec<u32>, target: u32, ex_subexp: usize, close: bool) {
        let mut cur = target;
        while !set_contains(dst, cur) {
            let k = self.node(cur).kind;
            let hit = if close {
                k == Kind::Close(ex_subexp)
            } else {
                k == Kind::Open(ex_subexp)
            };
            if hit {
                if close {
                    set_insert(dst, cur);
                }
                break;
            }
            set_insert(dst, cur);
            let ed = &self.re.edests[cur as usize];
            if ed.is_empty() {
                break;
            }
            if ed.len() == 2 {
                self.expand_ecl_sub(dst, ed[1], ex_subexp, close);
            }
            cur = ed[0];
        }
    }

    fn expand_bkref_cache(
        &mut self,
        log: &mut Vec<u32>,
        cur_nodes: &mut Vec<u32>,
        cur_str: usize,
        subexp_num: usize,
        close: bool,
    ) {
        let Some(start) = self.search_cur_bkref_entry(cur_str) else {
            return;
        };
        'restart: loop {
            let mut i = start;
            loop {
                let ent = self.ents[i];
                if set_contains(cur_nodes, ent.node) {
                    let to_idx = cur_str + ent.to - ent.from;
                    if to_idx == cur_str {
                        let next_node = self.re.edests[ent.node as usize][0];
                        if !set_contains(cur_nodes, next_node) {
                            let mut nd = vec![next_node];
                            self.expand_ecl(&mut nd, subexp_num, close);
                            set_merge(cur_nodes, &nd);
                            continue 'restart;
                        }
                    } else {
                        let next_node = self.re.nexts[ent.node as usize];
                        let ds = get(log, to_idx);
                        let skip = ds != NONE && set_contains(self.st.nodes(ds), next_node);
                        if !skip {
                            let mut union = if ds == NONE {
                                Vec::new()
                            } else {
                                self.st.nodes(ds).to_vec()
                            };
                            set_insert(&mut union, next_node);
                            let s = self.re.acquire_ci(self.st, &union);
                            put(log, to_idx, s);
                        }
                    }
                }
                if !ent.more {
                    break 'restart;
                }
                i += 1;
            }
        }
    }

    fn prune_impossible_nodes(&mut self) -> bool {
        let mut match_last = self.match_last;
        let mut halt_node = self.last_node;
        let mut sifted = vec![NONE; match_last + 1];
        if self.re.nbackref > 0 {
            let mut lim = vec![NONE; match_last + 1];
            loop {
                lim.fill(NONE);
                let mut sctx = Sctx {
                    last_node: halt_node,
                    last_str_idx: match_last,
                    limits: Vec::new(),
                };
                self.sift_states_backward(&mut sctx, &mut sifted, Some(&mut lim));
                if sifted[0] != NONE || lim[0] != NONE {
                    break;
                }
                loop {
                    if match_last == 0 {
                        return false;
                    }
                    match_last -= 1;
                    let s = self.log[match_last];
                    if s != NONE && self.st.flag(s, HALT) {
                        break;
                    }
                }
                halt_node = self.check_halt_state_context(self.log[match_last], match_last);
            }
            self.merge_state_array(&mut sifted, &lim, match_last + 1);
        } else {
            let mut sctx = Sctx {
                last_node: halt_node,
                last_str_idx: match_last,
                limits: Vec::new(),
            };
            self.sift_states_backward(&mut sctx, &mut sifted, None);
            if sifted[0] == NONE {
                return false;
            }
        }
        self.sifted = sifted;
        self.last_node = halt_node;
        self.match_last = match_last;
        true
    }

    fn merge_state_array(&mut self, dst: &mut [u32], src: &[u32], num: usize) {
        for i in 0..num {
            if dst[i] == NONE {
                dst[i] = src[i];
            } else if src[i] != NONE {
                let merged = set_union(self.st.nodes(dst[i]), self.st.nodes(src[i]));
                dst[i] = self.re.acquire_ci(self.st, &merged);
            }
        }
    }

    fn sift_states_backward(
        &mut self,
        sctx: &mut Sctx,
        sifted: &mut [u32],
        mut lim: Option<&mut Vec<u32>>,
    ) {
        let mut null_cnt = 0;
        let mut str_idx = sctx.last_str_idx;
        let mut cur_dest = vec![sctx.last_node];
        self.update_cur_sifted_state(sctx, sifted, lim.as_deref_mut(), str_idx, &mut cur_dest);
        while str_idx > 0 {
            null_cnt = if sifted[str_idx] == NONE {
                null_cnt + 1
            } else {
                0
            };
            if null_cnt > self.max_mb {
                sifted[..str_idx].fill(NONE);
                return;
            }
            cur_dest.clear();
            str_idx -= 1;
            if self.log[str_idx] != NONE {
                self.build_sifted_states(sctx, sifted, str_idx, &mut cur_dest);
            }
            self.update_cur_sifted_state(sctx, sifted, lim.as_deref_mut(), str_idx, &mut cur_dest);
        }
    }

    fn build_sifted_states(
        &mut self,
        sctx: &Sctx,
        sifted: &[u32],
        str_idx: usize,
        cur_dest: &mut Vec<u32>,
    ) {
        let src = self.st.v[self.log[str_idx] as usize].non_eps.clone();
        for prev in src {
            let mut naccepted = 0;
            if self.node(prev).accept_mb {
                naccepted = self.accept_bytes(prev, str_idx);
                if naccepted > 0
                    && str_idx + naccepted <= sctx.last_str_idx
                    && !set_contains(
                        self.st.nodes(get(sifted, str_idx + naccepted)),
                        self.re.nexts[prev as usize],
                    )
                {
                    naccepted = 0;
                }
            }
            if naccepted == 0
                && self.check_node_accept(prev, str_idx)
                && set_contains(
                    self.st.nodes(get(sifted, str_idx + 1)),
                    self.re.nexts[prev as usize],
                )
            {
                naccepted = 1;
            }
            if naccepted == 0 {
                continue;
            }
            if !sctx.limits.is_empty() {
                let to_idx = str_idx + naccepted;
                if self.check_dst_limits(
                    &sctx.limits,
                    self.re.nexts[prev as usize],
                    to_idx,
                    prev,
                    str_idx,
                ) {
                    continue;
                }
            }
            set_insert(cur_dest, prev);
        }
    }

    fn update_cur_sifted_state(
        &mut self,
        sctx: &mut Sctx,
        sifted: &mut [u32],
        lim: Option<&mut Vec<u32>>,
        str_idx: usize,
        dest_nodes: &mut Vec<u32>,
    ) {
        let cand_state = self.log[str_idx];
        let candidates = if cand_state == NONE {
            None
        } else {
            Some(self.st.nodes(cand_state).to_vec())
        };
        if dest_nodes.is_empty() {
            sifted[str_idx] = NONE;
        } else {
            if let Some(cand) = &candidates {
                self.add_epsilon_src_nodes(dest_nodes, cand);
                if !sctx.limits.is_empty() {
                    let limits = sctx.limits.clone();
                    self.check_subexp_limits(dest_nodes, cand, &limits, str_idx);
                }
            }
            sifted[str_idx] = self.re.acquire_ci(self.st, dest_nodes);
        }
        if let Some(cand) = candidates
            && self.st.flag(cand_state, HAS_BACKREF)
        {
            self.sift_states_bkref(sctx, sifted, lim, str_idx, &cand);
        }
    }

    fn add_epsilon_src_nodes(&mut self, dest_nodes: &mut Vec<u32>, candidates: &[u32]) {
        let s = self.re.acquire_ci(self.st, dest_nodes);
        if self.st.v[s as usize].inv.is_none() {
            let mut inv = Vec::new();
            for &n in dest_nodes.iter() {
                set_merge(&mut inv, &self.re.inveclosures[n as usize]);
            }
            self.st.v[s as usize].inv = Some(inv);
        }
        if let Some(inv) = self.st.v[s as usize].inv.as_deref() {
            set_add_intersect(dest_nodes, candidates, inv);
        }
    }

    fn sub_epsilon_src_nodes(&self, node: u32, dest_nodes: &mut Vec<u32>, candidates: &[u32]) {
        let inv = &self.re.inveclosures[node as usize];
        let mut except = Vec::new();
        for &cur in inv {
            if cur == node {
                continue;
            }
            if self.node(cur).kind.eps() {
                let ed = &self.re.edests[cur as usize];
                let e1 = ed[0];
                let e2 = ed.get(1).copied();
                let hit1 = !set_contains(inv, e1) && set_contains(dest_nodes, e1);
                let hit2 = e2.is_some_and(|e2| {
                    e2 > 0 && !set_contains(inv, e2) && set_contains(dest_nodes, e2)
                });
                if hit1 || hit2 {
                    set_add_intersect(&mut except, candidates, &self.re.inveclosures[cur as usize]);
                }
            }
        }
        for &cur in inv {
            if !set_contains(&except, cur) {
                set_remove(dest_nodes, cur);
            }
        }
    }

    fn check_dst_limits(
        &mut self,
        limits: &[u32],
        dst_node: u32,
        dst_idx: usize,
        src_node: u32,
        src_idx: usize,
    ) -> bool {
        let dst_bkref = self.search_cur_bkref_entry(dst_idx);
        let src_bkref = self.search_cur_bkref_entry(src_idx);
        for &lim in limits {
            let ent = self.ents[lim as usize];
            let Some(subexp_idx) = self.node(ent.node).kind.subexp() else {
                continue;
            };
            let dst_pos = self.calc_pos(lim as usize, subexp_idx, dst_node, dst_idx, dst_bkref);
            let src_pos = self.calc_pos(lim as usize, subexp_idx, src_node, src_idx, src_bkref);
            if src_pos != dst_pos {
                return true;
            }
        }
        false
    }

    fn calc_pos(
        &mut self,
        limit: usize,
        subexp_idx: usize,
        from_node: u32,
        str_idx: usize,
        bkref: Option<usize>,
    ) -> i32 {
        let lim = self.ents[limit];
        if str_idx < lim.from {
            return -1;
        }
        if lim.to < str_idx {
            return 1;
        }
        let boundaries = i32::from(str_idx == lim.from) | (i32::from(str_idx == lim.to) << 1);
        if boundaries == 0 {
            return 0;
        }
        self.calc_pos_1(boundaries, subexp_idx, from_node, bkref, 0)
    }

    fn calc_pos_1(
        &mut self,
        boundaries: i32,
        subexp_idx: usize,
        from_node: u32,
        bkref: Option<usize>,
        depth: usize,
    ) -> i32 {
        if depth > 10_000 {
            return i32::from(boundaries & 2 != 0);
        }
        let ecl = self.re.eclosures[from_node as usize].clone();
        for node in ecl {
            match self.node(node).kind {
                Kind::BackRef(_) => {
                    if let Some(mut e) = bkref {
                        loop {
                            let ent = self.ents[e];
                            if ent.node == node
                                && !(subexp_idx < 64 && ent.eps_reachable & (1 << subexp_idx) == 0)
                            {
                                let dst = self.re.edests[node as usize][0];
                                if dst == from_node {
                                    return if boundaries & 1 != 0 { -1 } else { 0 };
                                }
                                let cpos =
                                    self.calc_pos_1(boundaries, subexp_idx, dst, bkref, depth + 1);
                                if cpos == -1 {
                                    return -1;
                                }
                                if cpos == 0 && boundaries & 2 != 0 {
                                    return 0;
                                }
                                if subexp_idx < 64 {
                                    self.ents[e].eps_reachable &= !(1 << subexp_idx);
                                }
                            }
                            if !ent.more {
                                break;
                            }
                            e += 1;
                        }
                    }
                }
                Kind::Open(i) if boundaries & 1 != 0 && i == subexp_idx => return -1,
                Kind::Close(i) if boundaries & 2 != 0 && i == subexp_idx => return 0,
                _ => {}
            }
        }
        i32::from(boundaries & 2 != 0)
    }

    fn check_subexp_limits(
        &self,
        dest_nodes: &mut Vec<u32>,
        candidates: &[u32],
        limits: &[u32],
        str_idx: usize,
    ) {
        for &lim in limits {
            let ent = self.ents[lim as usize];
            if str_idx <= ent.from || ent.str_idx < str_idx {
                continue;
            }
            let Some(subexp_idx) = self.node(ent.node).kind.subexp() else {
                continue;
            };
            if ent.to == str_idx {
                let mut ops = None;
                let mut cls = None;
                for &n in dest_nodes.iter() {
                    match self.node(n).kind {
                        Kind::Open(i) if i == subexp_idx => ops = Some(n),
                        Kind::Close(i) if i == subexp_idx => cls = Some(n),
                        _ => {}
                    }
                }
                if let Some(o) = ops {
                    self.sub_epsilon_src_nodes(o, dest_nodes, candidates);
                }
                if let Some(c) = cls {
                    let mut i: isize = 0;
                    let mut guard = 0usize;
                    while (i as usize) < dest_nodes.len() {
                        let n = dest_nodes[i as usize];
                        if !set_contains(&self.re.inveclosures[n as usize], c)
                            && !set_contains(&self.re.eclosures[n as usize], c)
                        {
                            let before = dest_nodes.len();
                            self.sub_epsilon_src_nodes(n, dest_nodes, candidates);
                            guard += 1;
                            if dest_nodes.len() != before && guard < 1_000_000 {
                                i -= 1;
                            }
                        }
                        i += 1;
                        if i < 0 {
                            i = 0;
                        }
                    }
                }
            } else {
                let mut i = 0;
                while i < dest_nodes.len() {
                    let n = dest_nodes[i];
                    match self.node(n).kind {
                        Kind::Open(s) | Kind::Close(s) if s == subexp_idx => {
                            self.sub_epsilon_src_nodes(n, dest_nodes, candidates);
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
        }
    }

    fn sift_states_bkref(
        &mut self,
        sctx: &mut Sctx,
        sifted: &mut [u32],
        mut lim: Option<&mut Vec<u32>>,
        str_idx: usize,
        candidates: &[u32],
    ) {
        let Some(first_idx) = self.search_cur_bkref_entry(str_idx) else {
            return;
        };
        let mut local: Option<Sctx> = None;
        for &node in candidates {
            if node == sctx.last_node && str_idx == sctx.last_str_idx {
                continue;
            }
            if !matches!(self.node(node).kind, Kind::BackRef(_)) {
                continue;
            }
            let mut enabled = first_idx;
            loop {
                let entry = self.ents[enabled];
                if entry.node == node {
                    let len = entry.to - entry.from;
                    let to_idx = str_idx + len;
                    let dst_node = if len > 0 {
                        self.re.nexts[node as usize]
                    } else {
                        self.re.edests[node as usize][0]
                    };
                    let blocked = to_idx > sctx.last_str_idx
                        || sifted[to_idx] == NONE
                        || !set_contains(self.st.nodes(sifted[to_idx]), dst_node)
                        || self.check_dst_limits(&sctx.limits, node, str_idx, dst_node, to_idx);
                    if !blocked {
                        let mut l = local.take().unwrap_or_else(|| Sctx {
                            last_node: sctx.last_node,
                            last_str_idx: sctx.last_str_idx,
                            limits: sctx.limits.clone(),
                        });
                        l.last_node = node;
                        l.last_str_idx = str_idx;
                        set_insert(&mut l.limits, enabled as u32);
                        let saved = sifted[str_idx];
                        self.sift_states_backward(&mut l, sifted, lim.as_deref_mut());
                        if let Some(lm) = lim.as_deref_mut() {
                            self.merge_state_array(lm, sifted, str_idx + 1);
                        }
                        sifted[str_idx] = saved;
                        set_remove(&mut l.limits, enabled as u32);
                        local = Some(l);
                    }
                }
                if !self.ents[enabled].more {
                    break;
                }
                enabled += 1;
            }
        }
    }

    fn set_regs(&mut self, nmatch: usize, backtrack: bool) -> bool {
        let mut fs: Option<Vec<Regs>> = backtrack.then(Vec::new);
        let mut cur_node = i64::from(self.re.init_node);
        let mut pmatch = vec![(-1isize, -1isize); nmatch];
        pmatch[0] = (0, self.match_last as isize);
        let mut r = Regs {
            prev: pmatch.clone(),
            pmatch,
            eps_via: Vec::new(),
            idx: 0,
            node: 0,
        };
        let eo = self.match_last;
        let mut steps = 0usize;
        while r.idx <= eo {
            steps += 1;
            if steps > MAX_SET_REGS_STEPS {
                return true;
            }
            self.update_regs(&mut r, cur_node as u32, nmatch);
            if (r.idx == eo && cur_node == i64::from(self.last_node))
                || (fs.is_some() && set_contains(&r.eps_via, cur_node as u32))
            {
                cur_node = -1;
                if let Some(stack) = fs.as_mut()
                    && r.pmatch.iter().any(|&(so, e)| so > -1 && e == -1)
                {
                    cur_node = pop_fail(stack, &mut r);
                }
                if cur_node < 0 {
                    return true;
                }
            }
            cur_node = self.proceed_next_node(nmatch, &mut r, cur_node as u32, fs.as_mut());
            if cur_node < 0 {
                cur_node = match fs.as_mut() {
                    Some(stack) => pop_fail(stack, &mut r),
                    None => -1,
                };
                if cur_node < 0 {
                    return false;
                }
            }
        }
        true
    }

    fn update_regs(&self, r: &mut Regs, cur_node: u32, nmatch: usize) {
        let node = self.node(cur_node);
        let cur_idx = r.idx as isize;
        match node.kind {
            Kind::Open(i) => {
                let reg = i + 1;
                if reg < nmatch {
                    r.pmatch[reg] = (cur_idx, -1);
                }
            }
            Kind::Close(i) => {
                let reg = i + 1;
                if reg < nmatch {
                    if r.pmatch[reg].0 < cur_idx {
                        r.pmatch[reg].1 = cur_idx;
                        r.prev.clone_from(&r.pmatch);
                    } else if node.opt_subexp && r.prev[reg].0 != -1 {
                        r.pmatch.clone_from(&r.prev);
                    } else {
                        r.pmatch[reg].1 = cur_idx;
                    }
                }
            }
            _ => {}
        }
    }

    fn proceed_next_node(
        &self,
        nregs: usize,
        r: &mut Regs,
        node: u32,
        fs: Option<&mut Vec<Regs>>,
    ) -> i64 {
        let re = self.re;
        let nd = re.nodes[node as usize];
        if nd.kind.eps() {
            let cur_nodes = self.st.nodes(get(&self.sifted, r.idx));
            set_insert(&mut r.eps_via, node);
            let mut dest: i64 = -1;
            for &cand in &re.edests[node as usize] {
                if !set_contains(cur_nodes, cand) {
                    continue;
                }
                if dest == -1 {
                    dest = i64::from(cand);
                    continue;
                }
                if set_contains(&r.eps_via, dest as u32) {
                    return i64::from(cand);
                }
                if let Some(stack) = fs {
                    stack.push(Regs {
                        pmatch: r.pmatch.clone(),
                        prev: r.prev.clone(),
                        eps_via: r.eps_via.clone(),
                        idx: r.idx,
                        node: cand,
                    });
                }
                break;
            }
            return dest;
        }
        let mut naccepted: isize = 0;
        if nd.accept_mb {
            naccepted = self.accept_bytes(node, r.idx) as isize;
        } else if let Kind::BackRef(i) = nd.kind {
            let sub = i + 1;
            if sub < nregs {
                naccepted = r.pmatch[sub].1 - r.pmatch[sub].0;
            }
            if fs.is_some() {
                if sub >= nregs || r.pmatch[sub].0 == -1 || r.pmatch[sub].1 == -1 {
                    return -1;
                }
                if naccepted != 0 {
                    let n = naccepted as usize;
                    if self.len < r.idx + n {
                        return -1;
                    }
                    let so = self.base + r.pmatch[sub].0 as usize;
                    let at = self.base + r.idx;
                    let buf = self.inp.buf();
                    if buf[so..so + n] != buf[at..at + n] {
                        return -1;
                    }
                }
            }
            if naccepted == 0 {
                set_insert(&mut r.eps_via, node);
                let dest = re.edests[node as usize][0];
                if set_contains(self.st.nodes(get(&self.sifted, r.idx)), dest) {
                    return i64::from(dest);
                }
            }
        }
        if naccepted != 0 || self.check_node_accept(node, r.idx) {
            let dest = re.nexts[node as usize];
            let np = if naccepted == 0 {
                r.idx as isize + 1
            } else {
                r.idx as isize + naccepted
            };
            if np < 0 {
                return -1;
            }
            r.idx = np as usize;
            if fs.is_some()
                && (r.idx > self.match_last
                    || get(&self.sifted, r.idx) == NONE
                    || !set_contains(self.st.nodes(get(&self.sifted, r.idx)), dest))
            {
                return -1;
            }
            r.eps_via.clear();
            return i64::from(dest);
        }
        -1
    }
}

struct Regs {
    pmatch: Vec<(isize, isize)>,
    prev: Vec<(isize, isize)>,
    eps_via: Vec<u32>,
    idx: usize,
    node: u32,
}

fn pop_fail(stack: &mut Vec<Regs>, r: &mut Regs) -> i64 {
    let Some(e) = stack.pop() else {
        return -1;
    };
    let node = e.node;
    *r = e;
    i64::from(node)
}
