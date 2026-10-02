use super::ast::{
    Assert, Ast, BackRef, Cond, CondKind, Group, GroupKind, Lit, Node, RefTarget, Verb,
};
use super::charset::is_turkish_i;
use super::ucd;
use std::cell::Cell;

const UNSET: u32 = u32::MAX;
const NONE: u32 = u32::MAX - 1;
const CASELESS: u32 = 1;
const VARY: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unit {
    cu: u32,
    flags: u32,
}

const UNIT_UNSET: Unit = Unit {
    cu: 0,
    flags: UNSET,
};

#[derive(Clone, Copy, Debug)]
struct Branch {
    first: Unit,
    req: Unit,
    zfirst: Unit,
    zreq: Unit,
    groupset: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupClass {
    Plain,
    PosAssert,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodeUnits {
    pub first: Option<(u8, Option<u8>)>,
    pub req: Option<(u8, Option<u8>)>,
    pub req_vary: bool,
    pub has_accept: bool,
}

struct Analyzer<'a> {
    ast: &'a Ast,
    vary: Cell<u32>,
    accept: Cell<bool>,
}

pub fn analyze(ast: &Ast) -> CodeUnits {
    let a = Analyzer {
        ast,
        vary: Cell::new(0),
        accept: Cell::new(false),
    };
    let (mut first, mut req) = a.regex(&ast.root.branches, None);
    if a.accept.get() {
        req = Unit { cu: 0, flags: NONE };
    }
    if first.flags >= NONE {
        let asserted = a.asserted(&ast.root.branches, 0);
        if asserted.flags < NONE && asserted.cu != req.cu {
            first = asserted;
        }
    }
    CodeUnits {
        first: a.finish(first),
        req: a.finish(req),
        req_vary: req.flags < NONE && req.flags & VARY != 0,
        has_accept: a.accept.get(),
    }
}

impl Analyzer<'_> {
    fn finish(&self, u: Unit) -> Option<(u8, Option<u8>)> {
        if u.flags >= NONE {
            return None;
        }
        let cu = u8::try_from(u.cu).ok()?;
        if u.flags & CASELESS == 0 {
            return Some((cu, None));
        }
        let utf = self.ast.utf;
        let ucp = self.ast.ucp;
        let other = if cu < 128 || (!utf && !ucp && cu < 255) {
            if cu.is_ascii_alphabetic() {
                Some(cu ^ 0x20)
            } else {
                None
            }
        } else if ucp && !utf {
            let o = other_case(u32::from(cu));
            if o == u32::from(cu) {
                None
            } else {
                u8::try_from(o).ok()
            }
        } else {
            None
        };
        Some((cu, other))
    }

    fn regex(&self, branches: &[Vec<Node>], lead: Option<&Group>) -> (Unit, Unit) {
        let mut first = UNIT_UNSET;
        let mut req = UNIT_UNSET;
        for (i, br) in branches.iter().enumerate() {
            let (bf, mut breq) = self.branch(br, if i == 0 { lead } else { None });
            if i == 0 {
                first = bf;
                req = breq;
                continue;
            }
            if first != bf {
                if first.flags < NONE && req.flags >= NONE {
                    req = first;
                }
                first.flags = NONE;
            }
            if first.flags >= NONE && bf.flags < NONE && breq.flags >= NONE {
                breq = bf;
            }
            if req.flags & !VARY != breq.flags & !VARY || req.cu != breq.cu {
                req.flags = NONE;
            } else {
                req = Unit {
                    cu: breq.cu,
                    flags: req.flags | breq.flags,
                };
            }
        }
        (first, req)
    }

    fn branch(&self, nodes: &[Node], lead: Option<&Group>) -> (Unit, Unit) {
        let mut b = Branch {
            first: UNIT_UNSET,
            req: UNIT_UNSET,
            zfirst: UNIT_UNSET,
            zreq: UNIT_UNSET,
            groupset: false,
        };
        if let Some(g) = lead {
            self.group(&mut b, g);
        }
        for n in nodes {
            self.node(&mut b, n);
        }
        (b.first, b.req)
    }

    fn node(&self, b: &mut Branch, n: &Node) {
        match n {
            Node::Lit(l) => self.lit(b, *l),
            Node::Set(_)
            | Node::Dot
            | Node::AllAny
            | Node::AnyByte
            | Node::ExtUni
            | Node::Newline => {
                if b.first.flags == UNSET {
                    b.first.flags = NONE;
                }
                b.zfirst = b.first;
                b.zreq = b.req;
            }
            Node::Assert(a) => match a {
                Assert::CircM => {
                    if b.first.flags == UNSET {
                        b.first.flags = NONE;
                        b.zfirst.flags = NONE;
                    }
                }
                Assert::Circ | Assert::Doll | Assert::DollM => {}
                _ => {
                    b.zfirst = b.first;
                    b.zreq = b.req;
                }
            },
            Node::SetSom => {
                b.zfirst = b.first;
                b.zreq = b.req;
            }
            Node::BackRef(br) => {
                if b.first.flags == UNSET {
                    b.first.flags = NONE;
                    if !self.dup_ref(br) {
                        b.zfirst.flags = NONE;
                    }
                }
            }
            Node::Recurse(_) => {
                b.groupset = false;
                if b.first.flags == UNSET {
                    b.first.flags = NONE;
                }
                b.zfirst = b.first;
            }
            Node::Verb(Verb::Accept) => {
                self.accept.set(true);
                if b.first.flags == UNSET {
                    b.first.flags = NONE;
                }
            }
            Node::Verb(_) => {}
            Node::Group(g) => self.group(b, g),
            Node::Cond(c) => self.cond(b, c),
            Node::Repeat(r) => {
                self.node(b, &r.node);
                if r.min == 0 {
                    b.first = b.zfirst;
                    b.req = b.zreq;
                }
                let single = r.min == 1 && r.max == Some(1);
                match &r.node {
                    Node::Lit(l) if !single && r.min > 1 && !self.lit_is_prop(*l) => {
                        let bytes = self.encode(l.c);
                        if bytes.len() == 1 {
                            b.req = Unit {
                                cu: bytes[0],
                                flags: self.vary.get() | if l.fold { CASELESS } else { 0 },
                            };
                        }
                    }
                    Node::Group(_) | Node::Cond(_)
                        if r.min > 1 && b.groupset && b.req.flags >= NONE =>
                    {
                        b.req = b.first;
                    }
                    _ => {}
                }
                if r.max != Some(r.min) {
                    self.vary.set(VARY);
                }
            }
        }
    }

    fn dup_ref(&self, br: &BackRef) -> bool {
        match &br.target {
            RefTarget::Number(_) => false,
            RefTarget::Name(name) => self.ast.names.iter().any(|n| &n.name == name && n.isdup),
        }
    }

    fn lit_is_prop(&self, l: Lit) -> bool {
        if !(self.ast.utf || self.ast.ucp) || !l.fold {
            return false;
        }
        if self.ast.turkish && !l.restrict && is_turkish_i(l.c) {
            return true;
        }
        match ucd::case_set(l.c) {
            Some(s) if s.len() > 2 => !(l.restrict && s.iter().copied().min().unwrap_or(0) < 128),
            _ => false,
        }
    }

    fn encode(&self, c: u32) -> Vec<u32> {
        if !self.ast.utf {
            return vec![c];
        }
        let mut buf = [0u8; 4];
        match char::from_u32(c) {
            Some(ch) => ch.encode_utf8(&mut buf).bytes().map(u32::from).collect(),
            None => vec![c],
        }
    }

    fn lit(&self, b: &mut Branch, l: Lit) {
        if self.lit_is_prop(l) {
            if b.first.flags == UNSET {
                b.first.flags = NONE;
                b.zfirst.flags = NONE;
            }
            return;
        }
        let bytes = self.encode(l.c);
        let single = bytes.len() == 1;
        let opt = if l.fold { CASELESS } else { 0 };
        let last = bytes[bytes.len() - 1];
        if b.first.flags == UNSET {
            b.zfirst.flags = NONE;
            b.zreq = b.req;
            if single || !l.fold {
                b.first = Unit {
                    cu: bytes[0],
                    flags: opt,
                };
                if !single {
                    b.req = Unit {
                        cu: last,
                        flags: self.vary.get(),
                    };
                }
            } else {
                b.first.flags = NONE;
                b.req.flags = NONE;
            }
        } else {
            b.zfirst = b.first;
            b.zreq = b.req;
            if single || !l.fold {
                b.req = Unit {
                    cu: last,
                    flags: opt | self.vary.get(),
                };
            }
        }
    }

    fn group(&self, b: &mut Branch, g: &Group) {
        let class = match g.kind {
            GroupKind::LookAhead { neg: false, .. } => GroupClass::PosAssert,
            GroupKind::LookAhead { neg: true, .. }
            | GroupKind::LookBehind { .. }
            | GroupKind::Scs(_) => GroupClass::Other,
            _ => GroupClass::Plain,
        };
        let tempvary = self.vary.get();
        let (sf, sr) = self.regex(&g.branches, None);
        Self::apply_group(b, sf, sr, class, tempvary);
    }

    fn cond(&self, b: &mut Branch, c: &Cond) {
        let lead = match &c.kind {
            CondKind::Define => {
                self.regex(&c.branches, None);
                return;
            }
            CondKind::Assert(g) => Some(&**g),
            _ => None,
        };
        let tempvary = self.vary.get();
        let (mut sf, mut sr) = self.regex(&c.branches, lead);
        if c.branches.len() == 1 {
            sf.flags = NONE;
            sr.flags = NONE;
        }
        Self::apply_group(b, sf, sr, GroupClass::Plain, tempvary);
    }

    fn apply_group(b: &mut Branch, sf: Unit, mut sr: Unit, class: GroupClass, tempvary: u32) {
        b.zreq = b.req;
        b.zfirst = b.first;
        b.groupset = false;
        match class {
            GroupClass::Plain => {
                if b.first.flags == UNSET && sf.flags != UNSET {
                    if sf.flags < NONE {
                        b.first = sf;
                        b.groupset = true;
                    } else {
                        b.first.flags = NONE;
                    }
                    b.zfirst.flags = NONE;
                } else if sf.flags < NONE && sr.flags >= NONE {
                    sr = Unit {
                        cu: sf.cu,
                        flags: sf.flags | tempvary,
                    };
                }
                if sr.flags < NONE {
                    b.req = sr;
                }
            }
            GroupClass::PosAssert => {
                if sr.flags < NONE && sf.flags < NONE {
                    b.req = sr;
                }
            }
            GroupClass::Other => {}
        }
    }

    fn significant(nodes: &[Node], skipassert: bool) -> Option<&Node> {
        for n in nodes {
            match n {
                Node::Group(g)
                    if skipassert
                        && matches!(
                            g.kind,
                            GroupKind::LookAhead { neg: true, .. } | GroupKind::LookBehind { .. }
                        ) => {}
                Node::Assert(Assert::WordB { .. }) if skipassert => {}
                Node::Verb(
                    Verb::Mark(_)
                    | Verb::SkipArg(_)
                    | Verb::Commit(true)
                    | Verb::Prune(true)
                    | Verb::Then(true),
                ) => {}
                Node::Cond(c)
                    if c.branches.len() == 1
                        && matches!(c.kind, CondKind::Define | CondKind::Bool(false)) => {}
                Node::Repeat(r) if r.max == Some(0) => {}
                _ => return Some(n),
            }
        }
        None
    }

    fn asserted(&self, branches: &[Vec<Node>], inassert: u32) -> Unit {
        let mut c = Unit { cu: 0, flags: NONE };
        for br in branches {
            let d = match Self::significant(br, true) {
                Some(Node::Group(g)) => self.asserted_group(g, inassert),
                Some(Node::Repeat(r)) if r.min >= 1 => match &r.node {
                    Node::Group(g) => self.asserted_group(g, inassert),
                    Node::Lit(l) => self.asserted_lit(*l, inassert),
                    _ => None,
                },
                Some(Node::Lit(l)) => self.asserted_lit(*l, inassert),
                _ => None,
            };
            let Some(d) = d else {
                return Unit { cu: 0, flags: NONE };
            };
            if c.flags >= NONE {
                c = d;
            } else if c != d {
                return Unit { cu: 0, flags: NONE };
            }
        }
        c
    }

    fn asserted_group(&self, g: &Group, inassert: u32) -> Option<Unit> {
        let inner = match g.kind {
            GroupKind::LookAhead { neg: false, .. } => inassert + 1,
            GroupKind::NonCapture
            | GroupKind::Capture(_)
            | GroupKind::Atomic
            | GroupKind::ScriptRun => inassert,
            _ => return None,
        };
        let d = self.asserted(&g.branches, inner);
        (d.flags < NONE).then_some(d)
    }

    fn asserted_lit(&self, l: Lit, inassert: u32) -> Option<Unit> {
        if inassert == 0 || self.lit_is_prop(l) {
            return None;
        }
        let cu = self.encode(l.c)[0];
        if l.fold {
            if cu >= 0x80 {
                return None;
            }
            return Some(Unit {
                cu,
                flags: CASELESS,
            });
        }
        Some(Unit { cu, flags: 0 })
    }
}

fn other_case(c: u32) -> u32 {
    match ucd::case_set(c) {
        Some(s) => s.iter().copied().find(|&o| o != c).unwrap_or(c),
        None => c,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchoring {
    pub anchored: bool,
    pub startline: bool,
}

struct AnchorCtx {
    backref_map: u32,
    pruneorskip: bool,
    dotstar: bool,
}

fn group_bit(n: u32) -> u32 {
    if n < 32 { 1u32 << n } else { 1 }
}

fn collect(nodes: &[Node], map: &mut u32, pruneorskip: &mut bool) {
    for n in nodes {
        match n {
            Node::BackRef(br) => {
                for &g in &br.groups {
                    *map |= group_bit(g);
                }
            }
            Node::Verb(Verb::Prune(_) | Verb::Skip | Verb::SkipArg(_)) => *pruneorskip = true,
            Node::Group(g) => {
                for b in &g.branches {
                    collect(b, map, pruneorskip);
                }
            }
            Node::Repeat(r) => collect(std::slice::from_ref(&r.node), map, pruneorskip),
            Node::Cond(c) => {
                if let CondKind::Assert(g) = &c.kind {
                    for b in &g.branches {
                        collect(b, map, pruneorskip);
                    }
                }
                for b in &c.branches {
                    collect(b, map, pruneorskip);
                }
            }
            _ => {}
        }
    }
}

pub fn anchoring(ast: &Ast) -> Anchoring {
    let mut backref_map = 0u32;
    let mut pruneorskip = false;
    for b in &ast.root.branches {
        collect(b, &mut backref_map, &mut pruneorskip);
    }
    let a = Analyzer {
        ast,
        vary: Cell::new(0),
        accept: Cell::new(false),
    };
    let ctx = AnchorCtx {
        backref_map,
        pruneorskip,
        dotstar: !ast.no_dotstar_anchor,
    };
    let anchored = a.anchored_branches(&ast.root.branches, None, 0, 0, false, &ctx);
    let startline = !anchored && a.startline_branches(&ast.root.branches, 0, 0, false, &ctx);
    Anchoring {
        anchored,
        startline,
    }
}

impl Analyzer<'_> {
    fn dotstar_ok(map: u32, atom: u32, inassert: bool, ctx: &AnchorCtx) -> bool {
        map & ctx.backref_map == 0 && atom == 0 && !ctx.pruneorskip && !inassert && ctx.dotstar
    }

    fn anchored_branches(
        &self,
        branches: &[Vec<Node>],
        lead: Option<&Group>,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        if branches.is_empty() {
            return false;
        }
        for (i, br) in branches.iter().enumerate() {
            let ok = match (i, lead) {
                (0, Some(g)) => self.anchored_group(g, map, atom, inassert, ctx),
                _ => match Self::significant(br, false) {
                    Some(n) => self.anchored_node(n, map, atom, inassert, ctx),
                    None => false,
                },
            };
            if !ok {
                return false;
            }
        }
        true
    }

    fn anchored_group(
        &self,
        g: &Group,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        match g.kind {
            GroupKind::NonCapture | GroupKind::Top => {
                self.anchored_branches(&g.branches, None, map, atom, inassert, ctx)
            }
            GroupKind::Capture(n) => {
                self.anchored_branches(&g.branches, None, map | group_bit(n), atom, inassert, ctx)
            }
            GroupKind::LookAhead { neg: false, .. } => {
                self.anchored_branches(&g.branches, None, map, atom, true, ctx)
            }
            GroupKind::Atomic => {
                self.anchored_branches(&g.branches, None, map, atom + 1, inassert, ctx)
            }
            _ => false,
        }
    }

    fn anchored_cond(
        &self,
        c: &Cond,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        if c.branches.len() < 2 {
            return false;
        }
        let lead = match &c.kind {
            CondKind::Assert(g) => Some(&**g),
            _ => None,
        };
        self.anchored_branches(&c.branches, lead, map, atom, inassert, ctx)
    }

    fn anchored_node(
        &self,
        n: &Node,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        match n {
            Node::Group(g) => self.anchored_group(g, map, atom, inassert, ctx),
            Node::Cond(c) => self.anchored_cond(c, map, atom, inassert, ctx),
            Node::Repeat(r) => match &r.node {
                Node::Group(g) if r.min >= 1 => self.anchored_group(g, map, atom, inassert, ctx),
                Node::Cond(c) if r.min >= 1 => self.anchored_cond(c, map, atom, inassert, ctx),
                Node::AllAny if r.min == 0 && r.max.is_none() => {
                    Self::dotstar_ok(map, atom, inassert, ctx)
                }
                _ => false,
            },
            Node::Assert(Assert::Circ | Assert::Sod | Assert::Som) => true,
            _ => false,
        }
    }

    fn startline_branches(
        &self,
        branches: &[Vec<Node>],
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        if branches.is_empty() {
            return false;
        }
        branches
            .iter()
            .all(|br| match Self::significant(br, false) {
                Some(n) => self.startline_node(n, map, atom, inassert, ctx),
                None => false,
            })
    }

    fn startline_group(
        &self,
        g: &Group,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        match g.kind {
            GroupKind::NonCapture | GroupKind::Top => {
                self.startline_branches(&g.branches, map, atom, inassert, ctx)
            }
            GroupKind::Capture(n) => {
                self.startline_branches(&g.branches, map | group_bit(n), atom, inassert, ctx)
            }
            GroupKind::LookAhead { neg: false, .. } => {
                self.startline_branches(&g.branches, map, atom, true, ctx)
            }
            GroupKind::Atomic => self.startline_branches(&g.branches, map, atom + 1, inassert, ctx),
            _ => false,
        }
    }

    fn startline_cond(
        &self,
        c: &Cond,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        let CondKind::Assert(g) = &c.kind else {
            return false;
        };
        if !self.startline_group(g, map, atom, true, ctx) {
            return false;
        }
        match c.branches.first().and_then(|b| Self::significant(b, false)) {
            Some(Node::Cond(_)) | None => false,
            Some(n) => self.startline_node(n, map, atom, inassert, ctx),
        }
    }

    fn startline_node(
        &self,
        n: &Node,
        map: u32,
        atom: u32,
        inassert: bool,
        ctx: &AnchorCtx,
    ) -> bool {
        match n {
            Node::Group(g) => self.startline_group(g, map, atom, inassert, ctx),
            Node::Cond(c) => self.startline_cond(c, map, atom, inassert, ctx),
            Node::Repeat(r) => match &r.node {
                Node::Group(g) if r.min >= 1 => self.startline_group(g, map, atom, inassert, ctx),
                Node::Cond(c) if r.min >= 1 => self.startline_cond(c, map, atom, inassert, ctx),
                Node::Dot if r.min == 0 && r.max.is_none() => {
                    Self::dotstar_ok(map, atom, inassert, ctx)
                }
                _ => false,
            },
            Node::Assert(Assert::Circ | Assert::CircM) => true,
            _ => false,
        }
    }
}
