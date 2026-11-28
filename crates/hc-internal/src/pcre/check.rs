use std::collections::HashMap;

use super::ast::{Ast, CondKind, Group, GroupKind, Node, RefTarget, Verb};
use super::error::Error;

const LOOKBEHIND_MAX: i64 = 65535;
const MAX_VARLOOKBEHIND: i64 = 255;
const MAX_GROUP_NUMBER: u32 = 65535;

pub struct Ctx<'a> {
    ast_names: &'a [super::ast::NamedGroup],
    capture_count: u32,
    utf: bool,
    dupcap: bool,
    groups: HashMap<u32, &'a Group>,
    cache: HashMap<u32, Option<(i64, i64)>>,
    loops: u32,
    err: Option<(u32, Option<usize>)>,
    max_lookbehind: i64,
    small_ref_offset: [Option<usize>; 10],
}

impl<'a> Ctx<'a> {
    pub fn new(ast: &'a Ast, small_ref_offset: [Option<usize>; 10]) -> Ctx<'a> {
        let mut groups: HashMap<u32, &'a Group> = HashMap::new();
        collect_groups(&ast.root, &mut groups);
        Ctx {
            ast_names: &ast.names,
            capture_count: ast.capture_count,
            utf: ast.utf,
            dupcap: ast.dupcap_used,
            groups,
            cache: HashMap::new(),
            loops: 0,
            err: None,
            max_lookbehind: 0,
            small_ref_offset,
        }
    }

    pub fn lookbehind_lengths(&mut self, g: &Group) -> Vec<(u32, u32)> {
        self.loops = 0;
        self.err = None;
        let mut out = Vec::new();
        for b in &g.branches {
            match branch_length(b, self, &mut Vec::new(), &mut Vec::new()) {
                Ok(Some((a, b))) => out.push((a as u32, b as u32)),
                _ => out.push((0, 0)),
            }
        }
        out
    }
}

fn find_name<'a>(
    names: &'a [super::ast::NamedGroup],
    name: &[u8],
) -> Option<&'a super::ast::NamedGroup> {
    names.iter().find(|n| n.name == name)
}

fn name_groups(names: &[super::ast::NamedGroup], name: &[u8]) -> Vec<u32> {
    let mut v: Vec<u32> = names
        .iter()
        .filter(|n| n.name == name)
        .map(|n| n.number)
        .collect();
    v.dedup();
    v
}

pub fn check(
    ast: &mut Ast,
    patlen: usize,
    has_lookbehind: bool,
    small_ref_offset: &[Option<usize>; 10],
) -> Result<(), Error> {
    let mut max_lb: i64 = 0;
    if has_lookbehind {
        let mut ctx = Ctx::new(ast, *small_ref_offset);
        let mut ancestors = Vec::new();
        if let Err(()) = scan_lookbehinds(&ast.root, &mut ctx, &mut ancestors) {
            let (code, off) = ctx.err.unwrap_or((25, Some(0)));
            return Err(Error::compile(code, off.unwrap_or(0)));
        }
        max_lb = ctx.max_lookbehind;
    }
    let names = ast.names.clone();
    let cc = ast.capture_count;
    let mut st = PassState {
        assert_depth: 0,
        patlen,
        names: &names,
        capture_count: cc,
        small: *small_ref_offset,
    };
    first_pass(&mut ast.root.branches, &mut st)?;
    second_pass(&ast.root.branches)?;
    let wb = has_wordb_or_sod(&ast.root);
    ast.max_lookbehind = (max_lb as u32).max(u32::from(wb));
    Ok(())
}

fn collect_groups<'a>(g: &'a Group, out: &mut HashMap<u32, &'a Group>) {
    if let GroupKind::Capture(n) = g.kind {
        out.entry(n).or_insert(g);
    }
    for b in &g.branches {
        for n in b {
            collect_node_groups(n, out);
        }
    }
}

fn collect_node_groups<'a>(n: &'a Node, out: &mut HashMap<u32, &'a Group>) {
    match n {
        Node::Group(g) => collect_groups(g, out),
        Node::Repeat(r) => collect_node_groups(&r.node, out),
        Node::Cond(c) => {
            if let CondKind::Assert(g) = &c.kind {
                collect_groups(g, out);
            }
            for b in &c.branches {
                for n in b {
                    collect_node_groups(n, out);
                }
            }
        }
        _ => {}
    }
}

fn has_wordb_or_sod(g: &Group) -> bool {
    g.branches.iter().any(|b| b.iter().any(node_has_wordb))
}

fn node_has_wordb(n: &Node) -> bool {
    match n {
        Node::Assert(super::ast::Assert::WordB { .. } | super::ast::Assert::Sod) => true,
        Node::Group(g) => has_wordb_or_sod(g),
        Node::Repeat(r) => node_has_wordb(&r.node),
        Node::Cond(c) => {
            (matches!(&c.kind, CondKind::Assert(g) if has_wordb_or_sod(g)))
                || c.branches.iter().any(|b| b.iter().any(node_has_wordb))
        }
        _ => false,
    }
}

fn is_lookbehind(g: &Group) -> bool {
    matches!(g.kind, GroupKind::LookBehind { .. })
}

fn scan_lookbehinds(g: &Group, ctx: &mut Ctx, ancestors: &mut Vec<u32>) -> Result<(), ()> {
    if is_lookbehind(g) {
        set_lookbehind_lengths(g, ctx, ancestors, &mut Vec::new())?;
        return Ok(());
    }
    let pushed = if let GroupKind::Capture(n) = g.kind {
        ancestors.push(n);
        true
    } else {
        false
    };
    for b in &g.branches {
        for n in b {
            scan_node_lookbehinds(n, ctx, ancestors)?;
        }
    }
    if pushed {
        ancestors.pop();
    }
    Ok(())
}

fn scan_node_lookbehinds(n: &Node, ctx: &mut Ctx, ancestors: &mut Vec<u32>) -> Result<(), ()> {
    match n {
        Node::Group(g) => scan_lookbehinds(g, ctx, ancestors),
        Node::Repeat(r) => scan_node_lookbehinds(&r.node, ctx, ancestors),
        Node::Cond(c) => {
            if let CondKind::Assert(g) = &c.kind {
                scan_lookbehinds(g, ctx, ancestors)?;
            }
            for b in &c.branches {
                for n in b {
                    scan_node_lookbehinds(n, ctx, ancestors)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn set_lookbehind_lengths(
    g: &Group,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<Vec<(u32, u32)>, ()> {
    let offset = g.offset;
    let mut lens = Vec::new();
    let mut variable = false;
    let mut maxlength: i64 = 0;
    for b in &g.branches {
        let Ok(Some((min, max))) = branch_length(b, ctx, ancestors, recurses) else {
            match &mut ctx.err {
                None => ctx.err = Some((25, Some(offset))),
                Some((_, off)) if off.is_none() => *off = Some(offset),
                _ => {}
            }
            return Err(());
        };
        if min != max {
            variable = true;
        }
        if max > maxlength {
            maxlength = max;
        }
        if max > ctx.max_lookbehind {
            ctx.max_lookbehind = max;
        }
        lens.push((min as u32, max as u32));
    }
    if variable && maxlength > MAX_VARLOOKBEHIND {
        ctx.err = Some((100, Some(offset)));
        return Err(());
    }
    Ok(lens)
}

fn fail(ctx: &mut Ctx, code: u32, offset: Option<usize>) -> Result<Option<(i64, i64)>, ()> {
    if ctx.err.is_none() {
        ctx.err = Some((code, offset));
    }
    Err(())
}

fn branch_length(
    nodes: &[Node],
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<Option<(i64, i64)>, ()> {
    ctx.loops += 1;
    if ctx.loops > 2001 {
        return fail(ctx, 35, None);
    }
    let mut min: i64 = 0;
    let mut max: i64 = 0;
    for n in nodes {
        if let Node::Verb(Verb::Accept | Verb::Fail) = n {
            break;
        }
        let Some((imin, imax)) = item_length(n, ctx, ancestors, recurses)? else {
            return Ok(None);
        };
        max += imax;
        if max > LOOKBEHIND_MAX {
            return fail(ctx, 87, None);
        }
        min += imin;
    }
    Ok(Some((min, max)))
}

fn group_length(
    g: &Group,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<Option<(i64, i64)>, ()> {
    let gnum = if let GroupKind::Capture(n) = g.kind {
        Some(n)
    } else {
        None
    };
    if let Some(n) = gnum
        && !ctx.dupcap
        && let Some(cached) = ctx.cache.get(&n)
    {
        return Ok(*cached);
    }
    if let Some(n) = gnum {
        ancestors.push(n);
    }
    let mut gmin = i64::MAX;
    let mut gmax: i64 = -1;
    let mut fixed = true;
    for b in &g.branches {
        let r = branch_length(b, ctx, ancestors, recurses);
        match r {
            Ok(Some((a, b))) => {
                gmin = gmin.min(a);
                gmax = gmax.max(b);
            }
            Ok(None) => {
                fixed = false;
                break;
            }
            Err(()) => {
                if gnum.is_some() {
                    ancestors.pop();
                }
                return Err(());
            }
        }
    }
    if gnum.is_some() {
        ancestors.pop();
    }
    let res = if fixed { Some((gmin, gmax)) } else { None };
    if let Some(n) = gnum {
        ctx.cache.insert(n, res);
    }
    Ok(res)
}

fn ref_length(
    group: u32,
    offset: Option<usize>,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<Option<(i64, i64)>, ()> {
    if group > ctx.capture_count {
        return fail(ctx, 15, offset);
    }
    if group == 0 {
        return Ok(None);
    }
    if ancestors.contains(&group) || recurses.contains(&group) {
        return Ok(None);
    }
    let Some(&g) = ctx.groups.get(&group) else {
        return Ok(None);
    };
    recurses.push(group);
    let mut fresh_ancestors: Vec<u32> = Vec::new();
    let saved = std::mem::take(ancestors);
    let r = group_length(g, ctx, &mut fresh_ancestors, recurses);
    *ancestors = saved;
    recurses.pop();
    r
}

fn item_length(
    n: &Node,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<Option<(i64, i64)>, ()> {
    match n {
        Node::Assert(_) | Node::SetSom | Node::Verb(_) => Ok(Some((0, 0))),
        Node::Lit(_) | Node::Set(_) | Node::Dot | Node::AllAny => Ok(Some((1, 1))),
        Node::AnyByte => {
            if ctx.utf {
                return fail(ctx, 36, None);
            }
            Ok(Some((1, 1)))
        }
        Node::Newline => Ok(Some((1, 2))),
        Node::ExtUni => Ok(None),
        Node::Group(g) => match &g.kind {
            GroupKind::LookAhead { .. } | GroupKind::Scs(_) => {
                scan_inner_lookbehinds(g, ctx, ancestors, recurses)?;
                Ok(Some((0, 0)))
            }
            GroupKind::LookBehind { .. } => {
                set_lookbehind_lengths(g, ctx, ancestors, recurses)?;
                Ok(Some((0, 0)))
            }
            _ => group_length(g, ctx, ancestors, recurses),
        },
        Node::Repeat(r) => {
            if let Node::Group(g) = &r.node
                && matches!(g.kind, GroupKind::LookAhead { .. } | GroupKind::Scs(_))
            {
                scan_inner_lookbehinds(g, ctx, ancestors, recurses)?;
                return Ok(Some((0, 0)));
            }
            let Some((imin, imax)) = item_length(&r.node, ctx, ancestors, recurses)? else {
                return Ok(None);
            };
            let Some(rmax) = r.max else {
                return Ok(None);
            };
            if imax != 0 && rmax != 0 && (i64::from(i32::MAX)) / imax < i64::from(rmax) - 1 {
                return fail(ctx, 87, None);
            }
            Ok(Some((imin * i64::from(r.min), imax * i64::from(rmax))))
        }
        Node::BackRef(br) => {
            if ctx.dupcap {
                return Ok(None);
            }
            match &br.target {
                RefTarget::Number(num) => {
                    let off = if *num < 10 {
                        ctx.small_ref_offset[*num as usize]
                    } else {
                        Some(br.offset)
                    };
                    ref_length(*num, off, ctx, ancestors, recurses)
                }
                RefTarget::Name(name) => match find_name(ctx.ast_names, name) {
                    None => fail(ctx, 15, Some(br.offset)),
                    Some(ng) => {
                        if ng.isdup {
                            return Ok(None);
                        }
                        let num = ng.number;
                        ref_length(num, Some(br.offset), ctx, ancestors, recurses)
                    }
                },
            }
        }
        Node::Recurse(rc) => {
            let num = match &rc.target {
                RefTarget::Number(n) => *n,
                RefTarget::Name(name) => match find_name(ctx.ast_names, name) {
                    None => return fail(ctx, 15, Some(rc.offset)),
                    Some(ng) => ng.number,
                },
            };
            ref_length(num, Some(rc.offset), ctx, ancestors, recurses)
        }
        Node::Cond(c) => {
            if let CondKind::Define = c.kind {
                return Ok(Some((0, 0)));
            }
            if let CondKind::Assert(g) = &c.kind {
                if is_lookbehind(g) {
                    set_lookbehind_lengths(g, ctx, ancestors, recurses)?;
                } else {
                    scan_inner_lookbehinds(g, ctx, ancestors, recurses)?;
                }
            }
            let mut gmin = i64::MAX;
            let mut gmax: i64 = -1;
            for b in &c.branches {
                let Some((a, bb)) = branch_length(b, ctx, ancestors, recurses)? else {
                    return Ok(None);
                };
                gmin = gmin.min(a);
                gmax = gmax.max(bb);
            }
            if c.branches.len() == 1 {
                gmin = gmin.min(0);
            }
            Ok(Some((gmin, gmax)))
        }
    }
}

fn scan_inner_lookbehinds(
    g: &Group,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<(), ()> {
    for b in &g.branches {
        for n in b {
            scan_inner_node(n, ctx, ancestors, recurses)?;
        }
    }
    Ok(())
}

fn scan_inner_node(
    n: &Node,
    ctx: &mut Ctx,
    ancestors: &mut Vec<u32>,
    recurses: &mut Vec<u32>,
) -> Result<(), ()> {
    match n {
        Node::Group(g) => {
            if is_lookbehind(g) {
                set_lookbehind_lengths(g, ctx, ancestors, recurses)?;
                Ok(())
            } else {
                scan_inner_lookbehinds(g, ctx, ancestors, recurses)
            }
        }
        Node::Repeat(r) => scan_inner_node(&r.node, ctx, ancestors, recurses),
        Node::Cond(c) => {
            if let CondKind::Assert(g) = &c.kind {
                scan_inner_node(&Node::Group(g.clone()), ctx, ancestors, recurses)?;
            }
            for b in &c.branches {
                for n in b {
                    scan_inner_node(n, ctx, ancestors, recurses)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

struct PassState<'a> {
    assert_depth: u32,
    patlen: usize,
    names: &'a [super::ast::NamedGroup],
    capture_count: u32,
    small: [Option<usize>; 10],
}

fn first_pass(branches: &mut [Vec<Node>], st: &mut PassState) -> Result<(), Error> {
    for b in branches.iter_mut() {
        for n in b.iter_mut() {
            first_pass_node(n, st)?;
        }
    }
    Ok(())
}

fn first_pass_group(g: &mut Group, st: &mut PassState) -> Result<(), Error> {
    let is_assert = matches!(
        g.kind,
        GroupKind::LookAhead { .. } | GroupKind::LookBehind { .. } | GroupKind::Scs(_)
    );
    if let GroupKind::Scs(refs) = &g.kind {
        resolve_capture_list(refs, st)?;
    }
    if is_assert {
        st.assert_depth += 1;
    }
    let r = first_pass(&mut g.branches, st);
    if is_assert {
        st.assert_depth -= 1;
    }
    r
}

fn first_pass_node(n: &mut Node, st: &mut PassState) -> Result<(), Error> {
    match n {
        Node::Group(g) => first_pass_group(g, st),
        Node::Repeat(r) => first_pass_node(&mut r.node, st),
        Node::SetSom => {
            if st.assert_depth > 0 {
                return Err(Error::compile(99, st.patlen));
            }
            Ok(())
        }
        Node::BackRef(br) => {
            match &br.target {
                RefTarget::Number(num) => {
                    let off = if *num < 10 {
                        st.small[*num as usize].unwrap_or(br.offset)
                    } else {
                        br.offset
                    };
                    if *num > st.capture_count {
                        return Err(Error::compile(15, off));
                    }
                    br.groups = vec![*num];
                }
                RefTarget::Name(name) => {
                    let gs = name_groups(st.names, name);
                    if gs.is_empty() {
                        return Err(Error::compile(15, br.offset));
                    }
                    br.groups = gs;
                }
            }
            Ok(())
        }
        Node::Recurse(rc) => {
            match &rc.target {
                RefTarget::Number(num) => {
                    if *num > st.capture_count {
                        return Err(Error::compile(15, rc.offset));
                    }
                    rc.group = *num;
                }
                RefTarget::Name(name) => match find_name(st.names, name) {
                    None => return Err(Error::compile(15, rc.offset)),
                    Some(ng) => rc.group = ng.number,
                },
            }
            rc.ret_groups = resolve_capture_list(&rc.returns, st)?;
            Ok(())
        }
        Node::Cond(c) => {
            let name_offset = c.name_offset;
            match &mut c.kind {
                CondKind::Group(target, groups) => match target {
                    RefTarget::Number(num) => {
                        if *num > st.capture_count {
                            return Err(Error::compile(15, name_offset));
                        }
                        *groups = vec![*num];
                    }
                    RefTarget::Name(name) => {
                        let gs = name_groups(st.names, name);
                        if gs.is_empty() {
                            return Err(Error::compile(15, name_offset));
                        }
                        *groups = gs;
                    }
                },
                CondKind::RName(name, groups) => {
                    let gs = name_groups(st.names, name);
                    if gs.is_empty() {
                        return Err(Error::compile(15, name_offset));
                    }
                    *groups = gs;
                }
                CondKind::RNumber(name, rec, groups) => {
                    let gs = name_groups(st.names, name);
                    if gs.is_empty() {
                        let mut num: u32 = 0;
                        for (i, d) in name.iter().enumerate().skip(1) {
                            num = num * 10 + u32::from(d - b'0');
                            if num > MAX_GROUP_NUMBER {
                                return Err(Error::compile(61, name_offset + i));
                            }
                        }
                        if num > st.capture_count {
                            return Err(Error::compile(15, name_offset));
                        }
                        *rec = Some(num);
                    } else {
                        *groups = gs;
                    }
                }
                CondKind::Assert(g) => first_pass_group(g, st)?,
                CondKind::Define | CondKind::Bool(_) => {}
            }
            first_pass(&mut c.branches, st)
        }
        _ => Ok(()),
    }
}

fn second_pass(branches: &[Vec<Node>]) -> Result<(), Error> {
    for b in branches {
        for n in b {
            second_pass_node(n)?;
        }
    }
    Ok(())
}

fn second_pass_node(n: &Node) -> Result<(), Error> {
    match n {
        Node::Group(g) => second_pass(&g.branches),
        Node::Repeat(r) => second_pass_node(&r.node),
        Node::Cond(c) => {
            if let CondKind::Assert(g) = &c.kind {
                second_pass(&g.branches)?;
            }
            second_pass(&c.branches)?;
            match c.kind {
                CondKind::Define => {
                    if c.branches.len() > 1 {
                        return Err(Error::compile(54, c.offset));
                    }
                }
                _ => {
                    if c.branches.len() > 2 {
                        return Err(Error::compile(27, c.offset));
                    }
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn resolve_capture_list(refs: &[(RefTarget, usize)], st: &PassState) -> Result<Vec<u32>, Error> {
    let mut out = Vec::new();
    for (r, off) in refs {
        match r {
            RefTarget::Number(n) => {
                if *n > st.capture_count {
                    return Err(Error::compile(15, *off));
                }
                out.push(*n);
            }
            RefTarget::Name(name) => {
                let gs = name_groups(st.names, name);
                if gs.is_empty() {
                    return Err(Error::compile(15, *off));
                }
                out.extend(gs);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}
