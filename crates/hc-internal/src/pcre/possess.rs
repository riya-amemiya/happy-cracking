use std::collections::HashSet;

use super::ast::{Ast, Group, GroupKind, Newline, Node, RepKind, Repeat};
use super::charset::CharSet;
use super::compile::lit_equivalents;

enum Cont<'n> {
    End,
    Opaque,
    Seq(&'n [Node], &'n Cont<'n>),
}

pub struct Possess<'a> {
    ast: &'a Ast,
    max: u32,
    recursion: bool,
    out: HashSet<usize>,
}

pub fn analyze(ast: &Ast) -> HashSet<usize> {
    if ast.no_auto_possess {
        return HashSet::new();
    }
    let mut p = Possess {
        ast,
        max: if ast.utf { 0x10_ffff } else { 0xff },
        recursion: ast.root.branches.iter().any(|b| has_recurse(b)),
        out: HashSet::new(),
    };
    for b in &ast.root.branches {
        p.walk_seq(b, &Cont::End);
    }
    p.out
}

fn has_recurse(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Recurse(_) => true,
        Node::Group(g) => g.branches.iter().any(|b| has_recurse(b)),
        Node::Repeat(r) => has_recurse(std::slice::from_ref(&r.node)),
        Node::Cond(c) => {
            c.branches.iter().any(|b| has_recurse(b))
                || matches!(&c.kind, super::ast::CondKind::Assert(g) if g.branches.iter().any(|b| has_recurse(b)))
        }
        _ => false,
    })
}

impl Possess<'_> {
    fn item_set(&self, n: &Node) -> Option<CharSet> {
        match n {
            Node::Lit(l) => Some(CharSet::from_ranges(
                lit_equivalents(self.ast, *l)
                    .iter()
                    .map(|&c| (c, c))
                    .collect(),
            )),
            Node::Set(s) => Some((**s).clone()),
            Node::AllAny => Some(CharSet::from_ranges(vec![(0, self.max)])),
            Node::Dot => {
                let nl: Vec<(u32, u32)> = match self.ast.newline {
                    Newline::Lf => vec![(0x0a, 0x0a)],
                    Newline::Cr => vec![(0x0d, 0x0d)],
                    Newline::Nul => vec![(0, 0)],
                    Newline::AnyCrLf => vec![(0x0a, 0x0a), (0x0d, 0x0d)],
                    Newline::Any => vec![(0x0a, 0x0d), (0x85, 0x85), (0x2028, 0x2029)],
                    Newline::CrLf => vec![],
                };
                Some(CharSet::from_ranges(nl).negate(self.max))
            }
            _ => None,
        }
    }

    fn first_of(&self, n: &Node) -> Option<(CharSet, bool)> {
        match n {
            Node::Lit(_) | Node::Set(_) | Node::Dot | Node::AllAny => {
                Some((self.item_set(n)?, false))
            }
            Node::Repeat(r) => {
                if r.max == Some(0) {
                    return Some((CharSet::new(), true));
                }
                let (s, e) = match &r.node {
                    Node::Group(g) => self.group_first(g)?,
                    other => (self.item_set(other)?, false),
                };
                Some((s, e || r.min == 0))
            }
            Node::Group(g) => self.group_first(g),
            _ => None,
        }
    }

    fn group_first(&self, g: &Group) -> Option<(CharSet, bool)> {
        if !matches!(
            g.kind,
            GroupKind::NonCapture | GroupKind::Capture(_) | GroupKind::Atomic
        ) {
            return None;
        }
        let mut acc = CharSet::new();
        let mut empty = false;
        for b in &g.branches {
            let (s, e) = self.seq_first(b)?;
            acc = acc.union(&s);
            empty |= e;
        }
        Some((acc, empty))
    }

    fn seq_first(&self, nodes: &[Node]) -> Option<(CharSet, bool)> {
        let mut acc = CharSet::new();
        for n in nodes {
            let (s, e) = self.first_of(n)?;
            acc = acc.union(&s);
            if !e {
                return Some((acc, false));
            }
        }
        Some((acc, true))
    }

    fn follow(&self, rest: &[Node], cont: &Cont<'_>) -> Option<CharSet> {
        let (s, e) = self.seq_first(rest)?;
        if !e {
            return Some(s);
        }
        match cont {
            Cont::End => Some(s),
            Cont::Opaque => None,
            Cont::Seq(r2, c2) => self.follow(r2, c2).map(|t| s.union(&t)),
        }
    }

    fn walk_seq(&mut self, nodes: &[Node], cont: &Cont<'_>) {
        for (i, n) in nodes.iter().enumerate() {
            let rest = &nodes[i + 1..];
            match n {
                Node::Repeat(r) => self.walk_repeat(r, rest, cont),
                Node::Group(g) => self.walk_group(g, rest, cont),
                Node::Cond(c) => {
                    if let super::ast::CondKind::Assert(g) = &c.kind {
                        for b in &g.branches {
                            self.walk_seq(b, &Cont::Opaque);
                        }
                    }
                    for b in &c.branches {
                        self.walk_seq(b, &Cont::Opaque);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_repeat(&mut self, r: &Repeat, rest: &[Node], cont: &Cont<'_>) {
        match &r.node {
            Node::Lit(_) | Node::Set(_) | Node::Dot | Node::AllAny => {
                if r.kind != RepKind::Greedy {
                    return;
                }
                let Some(mine) = self.item_set(&r.node) else {
                    return;
                };
                if let Some(f) = self.follow(rest, cont)
                    && mine.intersect(&f).is_empty()
                {
                    self.out.insert(std::ptr::from_ref::<Repeat>(r) as usize);
                }
            }
            Node::Group(g) => {
                if r.min == 1 && r.max == Some(1) && r.kind != RepKind::Possessive {
                    self.walk_group(g, rest, cont);
                } else {
                    for b in &g.branches {
                        self.walk_seq(b, &Cont::Opaque);
                    }
                }
            }
            Node::Cond(c) => {
                for b in &c.branches {
                    self.walk_seq(b, &Cont::Opaque);
                }
            }
            _ => {}
        }
    }

    fn walk_group(&mut self, g: &Group, rest: &[Node], cont: &Cont<'_>) {
        match g.kind {
            GroupKind::NonCapture => {
                let inner = Cont::Seq(rest, cont);
                for b in &g.branches {
                    self.walk_seq(b, &inner);
                }
            }
            GroupKind::Capture(_) if !self.recursion => {
                let inner = Cont::Seq(rest, cont);
                for b in &g.branches {
                    self.walk_seq(b, &inner);
                }
            }
            GroupKind::Atomic | GroupKind::LookAhead { atomic: true, .. } => {
                for b in &g.branches {
                    self.walk_seq(b, &Cont::End);
                }
            }
            _ => {
                for b in &g.branches {
                    self.walk_seq(b, &Cont::Opaque);
                }
            }
        }
    }
}
