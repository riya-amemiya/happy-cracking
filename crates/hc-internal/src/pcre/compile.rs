use super::ast::{Assert, Ast, CondKind, Group, GroupKind, Lit, Node, RepKind, Verb};
use super::charset::{CaseMode, CharSet, caseless_equivalents};
use super::check::Ctx;
use super::error::Error;
use super::prog::{
    AssertOp, ClassMatcher, CondTest, Inst, Item, LookKind, NONE, Prog, RepMode, VerbOp,
};

const MAX_INSTS: usize = 4_000_000;

#[derive(Debug)]
pub struct LookInfo {
    pub kind: LookKind,
    pub reg: u32,
    pub success_pc: u32,
}

struct Compiler<'a> {
    ast: &'a Ast,
    insts: Vec<Inst>,
    classes: Vec<ClassMatcher>,
    class_ids: std::collections::HashMap<CharSet, u32>,
    pool: Vec<u8>,
    lists: Vec<Vec<u32>>,
    names: Vec<Vec<u8>>,
    nregs: u32,
    group_entry: Vec<u32>,
    calls: Vec<(usize, u32)>,
    lb: Ctx<'a>,
    looks: Vec<LookInfo>,
    look_stack: Vec<usize>,
    open_caps: Vec<(u32, usize)>,
    possess: std::collections::HashSet<usize>,
}

pub struct Compiled {
    pub prog: Prog,
    pub looks: Vec<LookInfo>,
}

pub fn compile(ast: &Ast, invalid_ok: bool) -> Result<Compiled, Error> {
    let ncap = ast.capture_count as usize + 1;
    let mut c = Compiler {
        ast,
        insts: Vec::new(),
        classes: Vec::new(),
        class_ids: std::collections::HashMap::new(),
        pool: Vec::new(),
        lists: Vec::new(),
        names: Vec::new(),
        nregs: (ncap * 3) as u32,
        group_entry: vec![NONE; ncap],
        calls: Vec::new(),
        lb: Ctx::new(ast, [None; 10]),
        looks: Vec::new(),
        look_stack: Vec::new(),
        open_caps: Vec::new(),
        possess: super::possess::analyze(ast),
    };
    c.group_entry[0] = 0;
    c.compile_branches(&ast.root, false, None)?;
    c.emit(Inst::Match)?;
    for &(at, group) in &c.calls {
        let target = c.group_entry[group as usize];
        if let Inst::Call { target: t, .. } = &mut c.insts[at] {
            *t = target;
        }
    }
    let has_any_byte = c.insts.iter().any(|i| {
        matches!(
            i,
            Inst::Item(Item::AnyByte)
                | Inst::Rep {
                    item: Item::AnyByte,
                    ..
                }
        )
    });
    let jit = !(ast.no_jit
        || (ast.utf && invalid_ok && has_any_byte)
        || ast
            .root
            .branches
            .iter()
            .flatten()
            .any(|n| accept_in_non_atomic_assert(n, false)));
    let interp = ast.utf && invalid_ok && !jit;
    let prog = Prog {
        insts: c.insts,
        classes: c.classes,
        pool: c.pool,
        lists: c.lists,
        names: c.names,
        nregs: c.nregs as usize,
        ncap,
        utf: ast.utf,
        ucp: ast.ucp,
        invalid_ok,
        jit,
        interp,
        newline: ast.newline,
        bsr_anycrlf: ast.bsr_anycrlf,
        has_crorlf: ast.has_crorlf,
        notempty: ast.notempty,
        notempty_atstart: ast.notempty_atstart,
        match_limit: ast.match_limit,
        depth_limit: ast.depth_limit,
        heap_limit: ast.heap_limit,
        max_lookbehind: ast.max_lookbehind,
        turkish: ast.turkish,
        dollar_endonly: ast.dollar_endonly,
    };
    Ok(Compiled {
        prog,
        looks: c.looks,
    })
}

pub fn case_mode(ast: &Ast, restrict: bool) -> CaseMode {
    CaseMode {
        unicode: ast.utf || ast.ucp,
        restrict,
        turkish: ast.turkish,
        max: if ast.utf { 0x10_ffff } else { 0xff },
    }
}

pub fn lit_equivalents(ast: &Ast, l: Lit) -> Vec<u32> {
    if !l.fold {
        return vec![l.c];
    }
    caseless_equivalents(l.c, case_mode(ast, l.restrict))
}

pub fn encode(c: u32, utf: bool, out: &mut Vec<u8>) {
    if !utf {
        out.push(c as u8);
        return;
    }
    let ch = char::from_u32(c).unwrap_or('\u{fffd}');
    let mut buf = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
}

pub fn min_len_nodes(nodes: &[Node]) -> u64 {
    let mut total: u64 = 0;
    for n in nodes {
        if matches!(n, Node::Verb(Verb::Accept)) {
            return total;
        }
        total = total.saturating_add(min_len(n));
    }
    total
}

pub fn min_len(n: &Node) -> u64 {
    match n {
        Node::Lit(_)
        | Node::Set(_)
        | Node::Dot
        | Node::AllAny
        | Node::AnyByte
        | Node::Newline
        | Node::ExtUni => 1,
        Node::Group(g) => match g.kind {
            GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_) => 0,
            _ => g
                .branches
                .iter()
                .map(|b| min_len_nodes(b))
                .min()
                .unwrap_or(0),
        },
        Node::Repeat(r) => min_len(&r.node).saturating_mul(u64::from(r.min)),
        Node::Cond(c) => {
            if matches!(c.kind, CondKind::Define) {
                return 0;
            }
            if c.branches.len() < 2 {
                return 0;
            }
            c.branches
                .iter()
                .map(|b| min_len_nodes(b))
                .min()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

fn accept_in_non_atomic_assert(n: &Node, inside: bool) -> bool {
    let in_group = |g: &Group| {
        let na = matches!(
            g.kind,
            GroupKind::LookAhead { atomic: false, .. }
                | GroupKind::LookBehind { atomic: false, .. }
                | GroupKind::Scs(_)
        );
        g.branches
            .iter()
            .flatten()
            .any(|c| accept_in_non_atomic_assert(c, inside || na))
    };
    match n {
        Node::Verb(Verb::Accept) => inside,
        Node::Group(g) => in_group(g),
        Node::Repeat(r) => accept_in_non_atomic_assert(&r.node, inside),
        Node::Cond(c) => {
            matches!(&c.kind, CondKind::Assert(g) if in_group(g))
                || c.branches
                    .iter()
                    .flatten()
                    .any(|c| accept_in_non_atomic_assert(c, inside))
        }
        _ => false,
    }
}

impl Compiler<'_> {
    fn pc(&self) -> u32 {
        self.insts.len() as u32
    }

    fn emit(&mut self, i: Inst) -> Result<u32, Error> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Error::compile(20, 0));
        }
        self.insts.push(i);
        Ok(self.insts.len() as u32 - 1)
    }

    fn reg(&mut self, n: u32) -> u32 {
        let r = self.nregs;
        self.nregs += n;
        r
    }

    fn class_id(&mut self, set: &CharSet) -> u32 {
        if let Some(&id) = self.class_ids.get(set) {
            return id;
        }
        let id = self.classes.len() as u32;
        self.classes.push(ClassMatcher::new(set, self.ast.utf));
        self.class_ids.insert(set.clone(), id);
        id
    }

    fn list(&mut self, v: Vec<u32>) -> u32 {
        if let Some(i) = self.lists.iter().position(|l| *l == v) {
            return i as u32;
        }
        self.lists.push(v);
        self.lists.len() as u32 - 1
    }

    fn name(&mut self, v: &[u8]) -> u32 {
        if let Some(i) = self.names.iter().position(|l| l.as_slice() == v) {
            return i as u32;
        }
        self.names.push(v.to_vec());
        self.names.len() as u32 - 1
    }

    fn patch(&mut self, at: u32, target: u32) {
        match &mut self.insts[at as usize] {
            Inst::Jmp(t) => *t = target,
            Inst::Fork { other, .. } => *other = target,
            Inst::Alt { next, .. } => *next = target,
            Inst::ThenBarrier { bound } => *bound = target,
            Inst::LookStart { cont, .. }
            | Inst::LookEnd { cont, .. }
            | Inst::ScsStart { cont, .. } => {
                *cont = target;
            }
            Inst::Cond { else_pc, .. } => *else_pc = target,
            _ => {}
        }
    }

    fn lit_item(&mut self, l: Lit) -> Item {
        let eq = lit_equivalents(self.ast, l);
        let utf = self.ast.utf;
        if eq.len() == 1 {
            let c = eq[0];
            if c < 128 || !utf {
                return Item::Byte(c as u8);
            }
            let start = self.pool.len() as u32;
            encode(c, utf, &mut self.pool);
            return Item::Bytes(start, self.pool.len() as u32 - start);
        }
        if eq.len() == 2 && (eq[1] < 128 || !utf) {
            return Item::Byte2(eq[0] as u8, eq[1] as u8);
        }
        let set = CharSet::from_ranges(eq.iter().map(|&c| (c, c)).collect());
        Item::Class(self.class_id(&set))
    }

    fn single_item(&mut self, n: &Node) -> Option<Item> {
        match n {
            Node::Lit(l) => Some(self.lit_item(*l)),
            Node::Set(s) => {
                if let Some(c) = s.single_char() {
                    return Some(self.lit_item(Lit {
                        c,
                        fold: false,
                        restrict: false,
                    }));
                }
                Some(Item::Class(self.class_id(s)))
            }
            Node::Dot => Some(Item::Any),
            Node::AllAny => Some(Item::AllAny),
            Node::AnyByte => Some(Item::AnyByte),
            _ => None,
        }
    }

    fn compile_branches(
        &mut self,
        g: &Group,
        is_look: bool,
        lb: Option<&[(u32, u32)]>,
    ) -> Result<(), Error> {
        let nb = g.branches.len();
        let mut jumps = Vec::new();
        let mut prev_alt: Option<u32> = None;
        let mut barrier: Option<u32> = None;
        for (i, b) in g.branches.iter().enumerate() {
            if let Some(a) = prev_alt.take() {
                let here = self.pc();
                self.patch(a, here);
            }
            if i + 1 < nb {
                prev_alt = Some(self.emit(Inst::Alt {
                    next: NONE,
                    then_all: is_look,
                })?);
            } else if nb > 1 && self.ast.has_then && !is_look {
                barrier = Some(self.emit(Inst::ThenBarrier { bound: NONE })?);
            }
            let mut check = false;
            if let Some(lens) = lb {
                let (mn, mx) = lens.get(i).copied().unwrap_or((0, 0));
                if mn == mx {
                    if mx > 0 {
                        self.emit(Inst::Reverse(mx))?;
                    }
                } else {
                    self.emit(Inst::VReverse { min: mn, max: mx })?;
                    check = true;
                }
            }
            self.compile_seq(b)?;
            if check {
                let look = *self.look_stack.last().expect("look");
                let reg = self.looks[look].reg;
                self.emit(Inst::CheckPos(reg))?;
            }
            if i + 1 < nb {
                jumps.push(self.emit(Inst::Jmp(NONE))?);
            }
        }
        let end = self.pc();
        for j in jumps {
            self.patch(j, end);
        }
        if let Some(b) = barrier {
            self.patch(b, end);
        }
        Ok(())
    }

    fn compile_seq(&mut self, nodes: &[Node]) -> Result<(), Error> {
        let mut i = 0usize;
        while i < nodes.len() {
            if let Node::Lit(l) = &nodes[i] {
                let mut bytes = Vec::new();
                let mut fold_bytes = Vec::new();
                let mut j = i;
                let utf = self.ast.utf;
                let first_eq = lit_equivalents(self.ast, *l);
                let fold_mode = first_eq.len() == 2 && first_eq[1] < 128 && first_eq[0] < 128;
                if first_eq.len() == 1 || fold_mode {
                    while j < nodes.len() {
                        let Node::Lit(l2) = &nodes[j] else { break };
                        if j + 1 < nodes.len() && matches!(nodes[j + 1], Node::Repeat(_)) {
                            break;
                        }
                        let eq = lit_equivalents(self.ast, *l2);
                        if fold_mode {
                            if eq.len() == 1 && eq[0] < 128 && !(eq[0] as u8).is_ascii_alphabetic()
                            {
                                fold_bytes.push(eq[0] as u8);
                            } else if eq.len() == 2
                                && eq[1] < 128
                                && eq[0] < 128
                                && (eq[0] as u8).eq_ignore_ascii_case(&(eq[1] as u8))
                            {
                                fold_bytes.push((eq[1] as u8).to_ascii_lowercase());
                            } else {
                                break;
                            }
                        } else if eq.len() == 1 {
                            encode(eq[0], utf, &mut bytes);
                        } else {
                            break;
                        }
                        j += 1;
                    }
                }
                if j - i >= 2 {
                    if fold_mode {
                        let start = self.pool.len() as u32;
                        self.pool.extend_from_slice(&fold_bytes);
                        self.emit(Inst::Item(Item::Bytes(
                            start | 0x8000_0000,
                            fold_bytes.len() as u32,
                        )))?;
                    } else {
                        let start = self.pool.len() as u32;
                        self.pool.extend_from_slice(&bytes);
                        self.emit(Inst::Item(Item::Bytes(start, bytes.len() as u32)))?;
                    }
                    i = j;
                    continue;
                }
            }
            let next = nodes.get(i + 1);
            self.compile_node(&nodes[i], next)?;
            i += 1;
        }
        Ok(())
    }

    fn compile_node(&mut self, n: &Node, next: Option<&Node>) -> Result<(), Error> {
        match n {
            Node::Lit(_) | Node::Set(_) | Node::Dot | Node::AllAny | Node::AnyByte => {
                let item = self.single_item(n).expect("item");
                self.emit(Inst::Item(item))?;
            }
            Node::Newline => {
                self.emit(Inst::Newline)?;
            }
            Node::ExtUni => {
                self.emit(Inst::ExtUni)?;
            }
            Node::Assert(a) => {
                let op = match a {
                    Assert::Circ => AssertOp::Circ,
                    Assert::CircM => AssertOp::CircM,
                    Assert::Doll => AssertOp::Doll,
                    Assert::DollM => AssertOp::DollM,
                    Assert::Sod => AssertOp::Sod,
                    Assert::Eod => AssertOp::Eod,
                    Assert::EodN => AssertOp::EodN,
                    Assert::Som => AssertOp::Som,
                    Assert::WordB {
                        ucp: false,
                        neg: false,
                    } => AssertOp::WordB,
                    Assert::WordB {
                        ucp: false,
                        neg: true,
                    } => AssertOp::NotWordB,
                    Assert::WordB {
                        ucp: true,
                        neg: false,
                    } => AssertOp::UcpWordB,
                    Assert::WordB {
                        ucp: true,
                        neg: true,
                    } => AssertOp::UcpNotWordB,
                };
                self.emit(Inst::Assert(op))?;
            }
            Node::SetSom => {
                self.emit(Inst::SetSom)?;
            }
            Node::Group(g) => self.compile_group(g, true)?,
            Node::Repeat(r) => self.compile_repeat(r, next)?,
            Node::BackRef(br) => {
                let list = self.list(br.groups.clone());
                self.emit(Inst::BackRef {
                    list,
                    fold: br.fold,
                    restrict: br.restrict,
                })?;
            }
            Node::Recurse(rc) => {
                let rets = if rc.ret_groups.is_empty() {
                    NONE
                } else {
                    self.list(rc.ret_groups.clone())
                };
                let at = self.emit(Inst::Call {
                    group: rc.group,
                    target: NONE,
                    rets,
                })?;
                self.calls.push((at as usize, rc.group));
            }
            Node::Cond(c) => self.compile_cond(c)?,
            Node::Verb(v) => self.compile_verb(v)?,
        }
        Ok(())
    }

    fn compile_verb(&mut self, v: &Verb) -> Result<(), Error> {
        match v {
            Verb::Accept => {
                let closes: Vec<u32> = match self.look_stack.last() {
                    Some(&li) => self
                        .open_caps
                        .iter()
                        .rev()
                        .filter(|(_, depth)| *depth > li)
                        .map(|(g, _)| *g)
                        .collect(),
                    None => self.open_caps.iter().rev().map(|(g, _)| *g).collect(),
                };
                let closes = self.list(closes);
                let look = self.look_stack.last().map_or(NONE, |&l| l as u32);
                self.emit(Inst::Accept { closes, look })?;
            }
            Verb::Fail => {
                self.emit(Inst::Fail)?;
            }
            Verb::Commit(_) => {
                self.emit(Inst::Verb(VerbOp::Commit))?;
            }
            Verb::Prune(_) => {
                self.emit(Inst::Verb(VerbOp::Prune))?;
            }
            Verb::Skip => {
                self.emit(Inst::Verb(VerbOp::Skip))?;
            }
            Verb::SkipArg(name) => {
                let id = self.name(name);
                self.emit(Inst::Verb(VerbOp::SkipArg(id)))?;
            }
            Verb::Then(_) => {
                self.emit(Inst::Verb(VerbOp::Then))?;
            }
            Verb::Mark(name) => {
                if self.ast.has_skip_arg {
                    let id = self.name(name);
                    self.emit(Inst::Verb(VerbOp::Mark(id)))?;
                }
            }
        }
        Ok(())
    }

    fn look_depth_marker(&self) -> usize {
        self.look_stack.last().map_or(0, |&l| l + 1)
    }

    fn compile_group(&mut self, g: &Group, register_entry: bool) -> Result<(), Error> {
        match &g.kind {
            GroupKind::Top | GroupKind::NonCapture => self.compile_branches(g, false, None),
            GroupKind::Capture(n) => {
                let n = *n;
                self.emit(Inst::CapOpen(n))?;
                if register_entry && self.group_entry[n as usize] == NONE {
                    self.group_entry[n as usize] = self.pc();
                }
                let depth = self.look_depth_marker();
                self.open_caps.push((n, depth));
                self.compile_branches(g, false, None)?;
                self.open_caps.pop();
                self.emit(Inst::CapClose(n))?;
                Ok(())
            }
            GroupKind::Atomic => {
                let r = self.reg(1);
                self.emit(Inst::AtomStart(r))?;
                self.compile_branches(g, false, None)?;
                self.emit(Inst::AtomEnd(r))?;
                Ok(())
            }
            GroupKind::ScriptRun => {
                let r = self.reg(1);
                self.emit(Inst::ScriptRunStart(r))?;
                self.compile_branches(g, false, None)?;
                self.emit(Inst::ScriptRunEnd(r))?;
                Ok(())
            }
            GroupKind::LookAhead { neg, atomic } => {
                let kind = if *neg {
                    LookKind::NegAhead
                } else if *atomic {
                    LookKind::PosAhead
                } else {
                    LookKind::NaAhead
                };
                self.compile_look(g, kind, None)
            }
            GroupKind::LookBehind { neg, atomic } => {
                let kind = if *neg {
                    LookKind::NegBehind
                } else if *atomic {
                    LookKind::PosBehind
                } else {
                    LookKind::NaBehind
                };
                let lens = self.lb.lookbehind_lengths(g);
                self.compile_look(g, kind, Some(&lens))
            }
            GroupKind::Scs(_) => {
                let groups = self.scs_groups(g);
                let list = self.list(groups);
                let reg = self.reg(3);
                let li = self.looks.len();
                self.looks.push(LookInfo {
                    kind: LookKind::Scs,
                    reg,
                    success_pc: NONE,
                });
                self.emit(Inst::ScsStart {
                    list,
                    reg,
                    cont: NONE,
                })?;
                self.look_stack.push(li);
                self.compile_branches(g, true, None)?;
                self.look_stack.pop();
                let end = self.emit(Inst::LookEnd {
                    kind: LookKind::Scs,
                    reg,
                    cont: NONE,
                })?;
                self.looks[li].success_pc = end + 1;
                Ok(())
            }
        }
    }

    fn scs_groups(&self, g: &Group) -> Vec<u32> {
        let GroupKind::Scs(refs) = &g.kind else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (r, _) in refs {
            match r {
                super::ast::RefTarget::Number(n) => out.push(*n),
                super::ast::RefTarget::Name(name) => {
                    for ng in &self.ast.names {
                        if &ng.name == name && !out.contains(&ng.number) {
                            out.push(ng.number);
                        }
                    }
                }
            }
        }
        out
    }

    fn compile_look(
        &mut self,
        g: &Group,
        kind: LookKind,
        lb: Option<&[(u32, u32)]>,
    ) -> Result<(), Error> {
        let reg = self.reg(2);
        let li = self.looks.len();
        self.looks.push(LookInfo {
            kind,
            reg,
            success_pc: NONE,
        });
        let start = self.emit(Inst::LookStart {
            kind,
            reg,
            cont: NONE,
        })?;
        self.look_stack.push(li);
        self.compile_branches(g, true, lb)?;
        self.look_stack.pop();
        let end = self.emit(Inst::LookEnd {
            kind,
            reg,
            cont: NONE,
        })?;
        let after = end + 1;
        self.looks[li].success_pc = after;
        if matches!(kind, LookKind::NegAhead | LookKind::NegBehind) {
            self.patch(start, after);
        }
        Ok(())
    }

    fn compile_cond(&mut self, c: &super::ast::Cond) -> Result<(), Error> {
        let mut assert_start: Option<(u32, LookKind, u32, usize)> = None;
        let test_inst = match &c.kind {
            CondKind::Group(_, groups) => {
                let list = self.list(groups.clone());
                Some(self.emit(Inst::Cond {
                    test: CondTest::Groups(list),
                    else_pc: NONE,
                })?)
            }
            CondKind::RName(_, groups) => {
                let list = self.list(groups.clone());
                Some(self.emit(Inst::Cond {
                    test: CondTest::RecurseGroups(list),
                    else_pc: NONE,
                })?)
            }
            CondKind::RNumber(_, rec, groups) => {
                let test = match rec {
                    Some(0) => CondTest::RecurseAny,
                    Some(n) => CondTest::RecurseGroup(*n),
                    None => CondTest::Groups(self.list(groups.clone())),
                };
                Some(self.emit(Inst::Cond {
                    test,
                    else_pc: NONE,
                })?)
            }
            CondKind::Define => Some(self.emit(Inst::Cond {
                test: CondTest::Const(false),
                else_pc: NONE,
            })?),
            CondKind::Bool(b) => Some(self.emit(Inst::Cond {
                test: CondTest::Const(*b),
                else_pc: NONE,
            })?),
            CondKind::Assert(g) => {
                let (neg, behind) = match g.kind {
                    GroupKind::LookAhead { neg, .. } => (neg, false),
                    GroupKind::LookBehind { neg, .. } => (neg, true),
                    _ => (false, false),
                };
                let kind = if neg {
                    LookKind::CondNeg
                } else {
                    LookKind::CondPos
                };
                let lens = if behind {
                    Some(self.lb.lookbehind_lengths(g))
                } else {
                    None
                };
                let reg = self.reg(2);
                let li = self.looks.len();
                self.looks.push(LookInfo {
                    kind,
                    reg,
                    success_pc: NONE,
                });
                let start = self.emit(Inst::LookStart {
                    kind,
                    reg,
                    cont: NONE,
                })?;
                self.look_stack.push(li);
                self.compile_branches(g, true, lens.as_deref())?;
                self.look_stack.pop();
                let end = self.emit(Inst::LookEnd {
                    kind,
                    reg,
                    cont: NONE,
                })?;
                assert_start = Some((start, kind, end, li));
                None
            }
        };
        let nb = c.branches.len();
        let yes_start = self.pc();
        if let Some(b) = c.branches.first() {
            self.compile_seq(b)?;
        }
        let mut jmp_end = None;
        if nb > 1 {
            jmp_end = Some(self.emit(Inst::Jmp(NONE))?);
        }
        let no_start = self.pc();
        if nb > 1 {
            self.compile_seq(&c.branches[1])?;
        }
        let end = self.pc();
        if let Some(j) = jmp_end {
            self.patch(j, end);
        }
        if let Some(t) = test_inst {
            self.patch(t, no_start);
        }
        if let Some((start, kind, lend, li)) = assert_start {
            if kind == LookKind::CondPos {
                self.patch(start, no_start);
                self.patch(lend, yes_start);
                self.looks[li].success_pc = yes_start;
            } else {
                self.patch(start, yes_start);
                self.patch(lend, no_start);
                self.looks[li].success_pc = no_start;
            }
        }
        Ok(())
    }

    fn repeat_mode(kind: RepKind) -> RepMode {
        match kind {
            RepKind::Greedy => RepMode::Greedy,
            RepKind::Lazy => RepMode::Lazy,
            RepKind::Possessive => RepMode::Possessive,
        }
    }

    fn compile_repeat(&mut self, r: &super::ast::Repeat, next: Option<&Node>) -> Result<(), Error> {
        let is_single = matches!(
            r.node,
            Node::Lit(_) | Node::Set(_) | Node::Dot | Node::AllAny | Node::AnyByte
        );
        if is_single {
            let max = r.max.unwrap_or(NONE);
            if max == 0 {
                return Ok(());
            }
            let item = self.single_item(&r.node).expect("item");
            let mut mode = Self::repeat_mode(r.kind);
            if mode == RepMode::Greedy
                && self
                    .possess
                    .contains(&(std::ptr::from_ref::<super::ast::Repeat>(r) as usize))
            {
                mode = RepMode::Possessive;
            }
            if r.min == 1 && max == 1 {
                self.emit(Inst::Item(item))?;
                return Ok(());
            }
            self.emit(Inst::Rep {
                item,
                min: r.min,
                max,
                mode,
            })?;
            return Ok(());
        }
        let mut max = r.max;
        if let Node::Group(g) = &r.node
            && matches!(
                g.kind,
                GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_)
            )
            && max.is_none()
        {
            max = Some(r.min + 1);
        }
        if r.min == 1 && max == Some(1) && r.kind != RepKind::Possessive {
            return self.compile_node(&r.node, next);
        }
        if let Node::Cond(c) = &r.node
            && matches!(c.kind, CondKind::Define | CondKind::Bool(false))
            && c.branches.len() < 2
        {
            return self.compile_node(&r.node, next);
        }
        if max == Some(0) {
            let j = self.emit(Inst::Jmp(NONE))?;
            self.compile_node(&r.node, None)?;
            let here = self.pc();
            self.patch(j, here);
            return Ok(());
        }
        if r.kind == RepKind::Possessive {
            let reg = self.reg(1);
            self.emit(Inst::AtomStart(reg))?;
            self.compile_repeat_body(&r.node, r.min, max, RepKind::Greedy)?;
            self.emit(Inst::AtomEnd(reg))?;
            return Ok(());
        }
        self.compile_repeat_body(&r.node, r.min, max, r.kind)
    }

    fn emit_loop(&mut self, node: &Node, lazy: bool) -> Result<(), Error> {
        let reg = if min_len(node) == 0 {
            self.reg(1)
        } else {
            NONE
        };
        let body = self.pc();
        if reg != NONE {
            self.emit(Inst::SetPos(reg))?;
        }
        self.compile_node(node, None)?;
        if lazy {
            self.emit(Inst::LoopMin { reg, body })?;
        } else {
            self.emit(Inst::LoopMax { reg, body })?;
        }
        Ok(())
    }

    fn compile_repeat_body(
        &mut self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        kind: RepKind,
    ) -> Result<(), Error> {
        let lazy = kind == RepKind::Lazy;
        match max {
            None => {
                if min == 0 {
                    let fork = self.emit(Inst::Fork {
                        other: NONE,
                        prefer_next: !lazy,
                    })?;
                    self.emit_loop(node, lazy)?;
                    let exit = self.pc();
                    self.patch(fork, exit);
                } else {
                    for _ in 1..min {
                        self.compile_node(node, None)?;
                    }
                    self.emit_loop(node, lazy)?;
                }
            }
            Some(mx) => {
                for _ in 0..min {
                    self.compile_node(node, None)?;
                }
                let mut forks = Vec::new();
                for _ in min..mx {
                    forks.push(self.emit(Inst::Fork {
                        other: NONE,
                        prefer_next: !lazy,
                    })?);
                    self.compile_node(node, None)?;
                }
                let end = self.pc();
                for f in forks {
                    self.patch(f, end);
                }
            }
        }
        Ok(())
    }
}
