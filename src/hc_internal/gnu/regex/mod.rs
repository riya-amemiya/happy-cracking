mod ast;
mod dfa;
mod engine;
mod gnulib;
mod linedfa;
mod literals;
mod syntax;
mod translate;

#[cfg(test)]
mod gnu_suite;

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::Arc;

use memchr::{memchr, memrchr};
use regex::bytes::Regex;

use super::locale::{self, CharClass};
use ast::{Look, Node, UnitSet};
use engine::{Engine, Prefilter, TextMode};
use gnulib::Gre;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    Basic,
    Extended,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Anywhere,
    Words,
    Lines,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub dialect: Dialect,
    pub ignore_case: bool,
    pub scope: Scope,
    pub null_data: bool,
    pub utf8: bool,
}

impl Options {
    #[must_use]
    pub fn new(dialect: Dialect, utf8: bool) -> Self {
        Self {
            dialect,
            ignore_case: false,
            scope: Scope::Anywhere,
            null_data: false,
            utf8,
        }
    }

    fn eol(self) -> u8 {
        if self.null_data { 0 } else { b'\n' }
    }

    fn lines(self) -> bool {
        self.scope == Scope::Lines
    }

    fn words(self) -> bool {
        self.scope == Scope::Words
    }
}

#[derive(Clone, Debug)]
struct Origin {
    file: Option<String>,
    line: usize,
}

#[derive(Clone, Debug)]
pub struct PatternSet {
    opts: Options,
    patterns: Vec<Vec<u8>>,
    origins: Vec<Origin>,
    seen: HashSet<Vec<u8>>,
}

#[derive(Debug)]
pub struct Compiled {
    pub matcher: Matcher,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct Failure {
    pub messages: Vec<String>,
}

impl PatternSet {
    #[must_use]
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            patterns: Vec::new(),
            origins: Vec::new(),
            seen: HashSet::new(),
        }
    }

    fn push(&mut self, pattern: &[u8], origin: Origin) {
        if self.seen.insert(pattern.to_vec()) {
            self.patterns.push(pattern.to_vec());
            self.origins.push(origin);
        }
    }

    pub fn add_expression(&mut self, text: &[u8]) {
        for p in text.split(|&b| b == b'\n') {
            self.push(
                p,
                Origin {
                    file: None,
                    line: 0,
                },
            );
        }
    }

    pub fn add_operand(&mut self, text: &[u8]) {
        let skip = self.opts.dialect != Dialect::Fixed && text.starts_with(b"\\-");
        self.add_expression(&text[usize::from(skip)..]);
    }

    pub fn add_file(&mut self, name: &str, content: &[u8]) {
        if content.is_empty() {
            return;
        }
        let body = content.strip_suffix(b"\n").unwrap_or(content);
        for (i, p) in body.split(|&b| b == b'\n').enumerate() {
            self.push(
                p,
                Origin {
                    file: Some(name.to_string()),
                    line: i + 1,
                },
            );
        }
    }

    pub fn build(&self) -> Result<Compiled, Failure> {
        if self.patterns.is_empty() {
            return Ok(Compiled {
                matcher: Matcher::never(self.opts),
                warnings: Vec::new(),
            });
        }
        if self.opts.dialect != Dialect::Fixed
            && self.patterns.len() > 1
            && let Some(fixed) = self.try_fixed()
        {
            return Ok(Compiled {
                matcher: fixed.build_fixed(),
                warnings: Vec::new(),
            });
        }
        match self.opts.dialect {
            Dialect::Fixed => Ok(Compiled {
                matcher: self.build_fixed(),
                warnings: Vec::new(),
            }),
            _ => self.build_regex(),
        }
    }

    fn try_fixed(&self) -> Option<PatternSet> {
        let o = self.opts;
        let extended = o.dialect == Dialect::Extended;
        let keys = self.patterns.join(&b'\n');
        let mut out = Vec::with_capacity(keys.len());
        let mut q = 0;
        while q < keys.len() {
            match keys[q] {
                b'$' | b'*' | b'.' | b'[' | b'^' => return None,
                b'(' | b'+' | b'?' | b'{' | b'|' if extended => return None,
                b'\\' if q + 1 < keys.len() => match keys[q + 1] {
                    b'\n'
                    | b'B'
                    | b'S'
                    | b'W'
                    | b'\''
                    | b'<'
                    | b'b'
                    | b's'
                    | b'w'
                    | b'`'
                    | b'>'
                    | b'1'..=b'9' => return None,
                    b'(' | b'+' | b'?' | b'{' | b'|' | b')' if !extended => return None,
                    _ => q += 1,
                },
                _ => {}
            }
            let len = fgrep_charlen(&keys[q..], o.utf8, o.ignore_case)?;
            out.extend_from_slice(&keys[q..q + len]);
            q += len;
        }
        let mut fixed = PatternSet::new(Options {
            dialect: Dialect::Fixed,
            ..o
        });
        fixed.patterns = out.split(|&b| b == b'\n').map(<[u8]>::to_vec).collect();
        Some(fixed)
    }

    fn literal(&self, p: &[u8], regex_fold: bool) -> Node {
        let utf8 = self.opts.utf8;
        let mut items = Vec::new();
        let mut i = 0;
        while i < p.len() {
            let (u, len) = locale::decode(utf8, &p[i..]);
            i += len;
            let set = UnitSet::single(u);
            let set = if !self.opts.ignore_case {
                set
            } else if regex_fold {
                UnitSet::single(locale::to_upper(utf8, u)).upper_preimage(utf8)
            } else {
                set.dfa_fold(utf8)
            };
            items.push(Node::Set(set));
        }
        Node::concat(items)
    }

    fn word_wrap(inner: Node) -> Node {
        let nonword = UnitSet::from_ranges(CharClass::Alnum.ranges(false))
            .union(&UnitSet::single(u32::from(b'_')))
            .negate(false);
        Node::Concat(vec![
            Node::Alt(vec![
                Node::Look(Look::LineStart),
                Node::Set(nonword.clone()),
            ]),
            inner,
            Node::Alt(vec![Node::Set(nonword), Node::Look(Look::LineEnd)]),
        ])
    }

    fn build_fixed(&self) -> Matcher {
        let o = self.opts;
        let sel = Node::alt(
            self.patterns
                .iter()
                .map(|p| self.literal(p, false))
                .collect(),
        );
        let find = Node::alt(
            self.patterns
                .iter()
                .map(|p| self.literal(p, true))
                .collect(),
        );
        let sel_node = if o.lines() {
            Node::Concat(vec![
                Node::Look(Look::LineStart),
                sel.clone(),
                Node::Look(Look::LineEnd),
            ])
        } else if o.words() && !o.utf8 {
            Self::word_wrap(sel.clone())
        } else {
            sel.clone()
        };
        let mode = if o.lines() {
            Mode::Lines
        } else if o.words() {
            Mode::Words
        } else {
            Mode::Plain
        };
        let select = if o.words() && o.utf8 {
            Selector::regex_path(&sel, o, Mode::Words, true)
        } else {
            Selector::exact_or_confirm(&sel_node, &sel, o, mode)
        };
        Matcher::assemble(o, select, &find, true)
    }

    fn prefix(&self, idx: usize, msg: &str) -> String {
        match &self.origins[idx].file {
            Some(f) => format!("{}:{}: {msg}", f, self.origins[idx].line),
            None => msg.to_string(),
        }
    }

    fn gnu_set(&self, syn: syntax::Syntax) -> Option<GnuSet> {
        let o = self.opts;
        let (bk, free): (Vec<&Vec<u8>>, Vec<&Vec<u8>>) =
            self.patterns.iter().partition(|p| possible_backrefs(p));
        if bk.is_empty() {
            return None;
        }
        let compile = |p: &[u8]| {
            let parsed = syntax::parse(p, syn).ok()?;
            Gre::new(
                &parsed.node,
                parsed.groups,
                o.utf8,
                parsed.byte_mode,
                o.ignore_case,
            )
        };
        let mut res = Vec::with_capacity(bk.len() + 1);
        if !free.is_empty() {
            let joined: Vec<u8> = free
                .iter()
                .map(|p| p.as_slice())
                .collect::<Vec<_>>()
                .join(&b'\n');
            res.push(compile(&joined)?);
        }
        for p in bk {
            res.push(compile(p)?);
        }
        Some(GnuSet {
            res,
            newline_anchor: !o.null_data,
        })
    }

    fn build_regex(&self) -> Result<Compiled, Failure> {
        let o = self.opts;
        let extended = o.dialect == Dialect::Extended;
        let syn = syntax::Syntax {
            extended,
            icase: o.ignore_case,
            utf8: o.utf8,
        };
        let mut errors = Vec::new();
        let mut nodes = Vec::new();
        for (i, p) in self.patterns.iter().enumerate() {
            match syntax::parse(p, syn) {
                Ok(parsed) => nodes.push((parsed.node, parsed.groups, parsed.byte_mode)),
                Err(e) => errors.push(self.prefix(i, e.message())),
            }
        }
        if !errors.is_empty() {
            return Err(Failure { messages: errors });
        }
        let combined = self.patterns.join(&b'\n');
        let motif = if o.lines() {
            wrap(&combined, extended, b"^\\(", b"\\)$", b"^(", b")$")
        } else if o.words() {
            wrap(
                &combined,
                extended,
                b"\\(^\\|[^[:alnum:]_]\\)\\(",
                b"\\)\\([^[:alnum:]_]\\|$\\)",
                b"(^|[^[:alnum:]_])(",
                b")([^[:alnum:]_]|$)",
            )
        } else {
            combined
        };
        let parsed = dfa::parse(
            &motif,
            dfa::DfaSyntax {
                extended,
                icase: o.ignore_case,
                utf8: o.utf8,
            },
        );
        let warnings = parsed.warnings;
        let dfa_node = match parsed.result {
            Ok(n) => n,
            Err(e) => {
                let mut messages = warnings;
                messages.push(e);
                return Err(Failure { messages });
            }
        };
        let mut shift = 0;
        let mut alts = Vec::with_capacity(nodes.len());
        let mut byte = true;
        for (n, groups, b) in nodes {
            byte &= b;
            alts.push(n.shift_groups(shift));
            shift += groups;
        }
        let union = Node::alt(alts);
        let mode = if o.lines() {
            Mode::Lines
        } else if o.words() {
            Mode::Words
        } else {
            Mode::Plain
        };
        let select = if dfa::supported(&dfa_node, o.utf8) {
            Selector::exact_or_confirm(&dfa_node, &dfa_node, o, Mode::Plain)
        } else {
            let mut s = Selector::regex_path(&union, o, mode, byte);
            if union.has_backref() {
                s.gnu = self.gnu_set(syn).map(Arc::new);
            }
            let has_set = dfa_node.any(&|n| matches!(n, Node::Set(_)));
            let has_any = dfa_node.any(&|n| matches!(n, Node::Unsupported));
            if has_set && (has_any || o.utf8) {
                let sup = superset_node(&dfa_node, o.utf8);
                let t = translate::translate(&sup, o.utf8, o.eol());
                s.superset = translate::build(&t.pattern, o.utf8, o.eol());
            }
            s
        };
        Ok(Compiled {
            matcher: Matcher::assemble(o, select, &union, byte),
            warnings,
        })
    }
}

fn wrap(
    combined: &[u8],
    extended: bool,
    bre_open: &[u8],
    bre_close: &[u8],
    ere_open: &[u8],
    ere_close: &[u8],
) -> Vec<u8> {
    let (open, close) = if extended {
        (ere_open, ere_close)
    } else {
        (bre_open, bre_close)
    };
    let mut v = Vec::with_capacity(open.len() + combined.len() + close.len());
    v.extend_from_slice(open);
    v.extend_from_slice(combined);
    v.extend_from_slice(close);
    v
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Plain,
    Lines,
    Words,
}

#[derive(Clone, Debug)]
struct Selector {
    re: Option<Regex>,
    exact: bool,
    mode: Mode,
    engine: Option<Engine>,
    superset: Option<Regex>,
    fast_valid: Option<Regex>,
    fast_valid_dfa: Option<linedfa::LineDfa>,
    fast_ascii: Option<Regex>,
    dfa: Option<linedfa::LineDfa>,
    strategy: Strategy,
    gnu: Option<Arc<GnuSet>>,
}

#[derive(Clone, Debug)]
enum LitFinder {
    One(memchr::memmem::Finder<'static>),
    Many(Regex),
}

impl LitFinder {
    fn find(&self, buf: &[u8], pos: usize) -> Option<usize> {
        match self {
            Self::One(f) => f.find(&buf[pos..]).map(|i| pos + i),
            Self::Many(re) => re.find_at(buf, pos).map(|m| m.start()),
        }
    }
}

#[derive(Clone, Debug)]
enum Strategy {
    Regex,
    Dfa,
    Lits(LitFinder),
}

fn lit_finder(lits: &[Vec<u8>]) -> Option<LitFinder> {
    if let [one] = lits {
        return Some(LitFinder::One(
            memchr::memmem::Finder::new(one).into_owned(),
        ));
    }
    let mut pat = String::from("(?-u:");
    for (i, l) in lits.iter().enumerate() {
        if i > 0 {
            pat.push('|');
        }
        for b in l {
            let _ = write!(pat, "\\x{b:02X}");
        }
    }
    pat.push(')');
    regex::bytes::RegexBuilder::new(&pat)
        .unicode(false)
        .build()
        .ok()
        .map(LitFinder::Many)
}

fn exact_parts(node: &Node, o: Options) -> (Option<linedfa::LineDfa>, Strategy) {
    let eol = o.eol();
    let dfa = linedfa::LineDfa::build(node, o.utf8, eol);
    if literals::good_prefix(node, o.utf8, eol) {
        return (dfa, Strategy::Regex);
    }
    if let Some(f) = literals::required_literals(node, o.utf8, eol).and_then(|l| lit_finder(&l)) {
        return (dfa, Strategy::Lits(f));
    }
    let s = if dfa.is_some() {
        Strategy::Dfa
    } else {
        Strategy::Regex
    };
    (dfa, s)
}

fn any_unit(utf8: bool) -> UnitSet {
    if utf8 {
        UnitSet::from_ranges(vec![
            (0, locale::MAX_CHAR),
            (locale::INVALID_BASE + 0x80, locale::INVALID_BASE + 0xff),
        ])
    } else {
        UnitSet::universe(false)
    }
}

fn superset_node(n: &Node, utf8: bool) -> Node {
    let star = || Node::Repeat {
        node: Box::new(Node::Set(any_unit(utf8))),
        min: 0,
        max: None,
    };
    match n {
        Node::Unsupported | Node::Backref(_) => star(),
        Node::Look(
            Look::WordStart | Look::WordEnd | Look::WordBoundary | Look::NotWordBoundary,
        ) if utf8 => Node::Empty,
        Node::Concat(v) => Node::Concat(v.iter().map(|x| superset_node(x, utf8)).collect()),
        Node::Alt(v) => Node::Alt(v.iter().map(|x| superset_node(x, utf8)).collect()),
        Node::Repeat { node, min, max } => match node.as_ref() {
            Node::Unsupported => star(),
            inner => Node::Repeat {
                node: Box::new(superset_node(inner, utf8)),
                min: *min,
                max: *max,
            },
        },
        Node::Group { node, index } => Node::Group {
            node: Box::new(superset_node(node, utf8)),
            index: *index,
        },
        other => other.clone(),
    }
}

fn make_engine(node: &Node, o: Options, byte: bool) -> Option<Engine> {
    Engine::new(
        node.clone(),
        TextMode {
            utf8: o.utf8,
            icase: o.ignore_case,
            nl_quirk: o.null_data,
        },
        byte,
    )
    .ok()
}

impl Selector {
    fn exact_or_confirm(node: &Node, base: &Node, o: Options, mode: Mode) -> Self {
        let t = translate::translate(node, o.utf8, o.eol());
        if t.exact {
            let (dfa, strategy) = exact_parts(node, o);
            let need_re = dfa.is_none() || matches!(strategy, Strategy::Regex);
            let re = if need_re {
                translate::build(&t.pattern, o.utf8, o.eol())
            } else {
                None
            };
            if re.is_some() || !need_re {
                return Self {
                    re,
                    exact: true,
                    mode: Mode::Plain,
                    engine: None,
                    superset: None,
                    fast_valid: None,
                    fast_valid_dfa: None,
                    fast_ascii: None,
                    dfa,
                    strategy,
                    gnu: None,
                };
            }
        }
        let re = translate::build(&t.pattern, o.utf8, o.eol());
        let engine_node = if mode == Mode::Plain { node } else { base };
        Self {
            re,
            exact: false,
            mode,
            engine: make_engine(engine_node, o, true),
            superset: None,
            fast_valid: None,
            fast_valid_dfa: None,
            fast_ascii: None,
            dfa: None,
            strategy: Strategy::Regex,
            gnu: None,
        }
    }

    fn regex_path(union: &Node, o: Options, mode: Mode, byte: bool) -> Self {
        let q = nl_quirk(o, union);
        let t = translate::translate_with(union, o.utf8, o.eol(), false, q);
        if mode == Mode::Lines && t.exact {
            let wrapped = Node::Concat(vec![
                Node::Look(Look::LineStart),
                union.clone(),
                Node::Look(Look::LineEnd),
            ]);
            let tw = translate::translate_with(&wrapped, o.utf8, o.eol(), false, q);
            if let Some(re) = translate::build(&tw.pattern, o.utf8, o.eol()) {
                let (dfa, strategy) = exact_parts(&wrapped, o);
                return Self {
                    re: Some(re),
                    exact: true,
                    mode: Mode::Plain,
                    engine: None,
                    superset: None,
                    fast_valid: None,
                    fast_valid_dfa: None,
                    fast_ascii: None,
                    dfa,
                    strategy,
                    gnu: None,
                };
            }
        }
        let re = translate::build(&t.pattern, o.utf8, o.eol());
        let exact = mode == Mode::Plain && t.exact && re.is_some();
        let mut fv_dfa = None;
        let fast_valid = if mode == Mode::Words
            && o.utf8
            && t.exact
            && !union.can_be_empty()
            && !union.any(&|n| matches!(n, Node::Look(Look::BufEnd)))
        {
            let nonword = UnitSet::from_ranges(CharClass::Alnum.ranges(true))
                .union(&UnitSet::single(u32::from(b'_')))
                .negate(true);
            let wrapped = Node::Concat(vec![
                Node::Alt(vec![
                    Node::Look(Look::LineStart),
                    Node::Set(nonword.clone()),
                ]),
                union.clone(),
                Node::Alt(vec![Node::Set(nonword), Node::Look(Look::LineEnd)]),
            ]);
            let tw = translate::translate_with(&wrapped, o.utf8, o.eol(), false, q);
            fv_dfa = linedfa::LineDfa::build(&wrapped, o.utf8, o.eol());
            if tw.exact {
                translate::build(&tw.pattern, o.utf8, o.eol())
            } else {
                None
            }
        } else {
            None
        };
        let (exact_dfa, strategy) = if exact {
            exact_parts(union, o)
        } else {
            (None, Strategy::Regex)
        };
        let fast_ascii = if mode != Mode::Words
            && o.utf8
            && !t.exact
            && !union.has_backref()
            && !union.has_invalid_units()
        {
            let node = if mode == Mode::Lines {
                Node::Concat(vec![
                    Node::Look(Look::LineStart),
                    union.clone(),
                    Node::Look(Look::LineEnd),
                ])
            } else {
                union.clone()
            };
            let ta = translate::translate_with(&node, o.utf8, o.eol(), true, q);
            if ta.exact {
                translate::build(&ta.pattern, o.utf8, o.eol())
            } else {
                None
            }
        } else {
            None
        };
        Self {
            re,
            exact,
            mode,
            engine: if exact {
                None
            } else {
                make_engine(union, o, byte)
            },
            superset: None,
            fast_valid_dfa: fast_valid.as_ref().and(fv_dfa),
            fast_valid,
            fast_ascii,
            dfa: exact_dfa,
            strategy,
            gnu: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Matcher {
    utf8: bool,
    eol: u8,
    words: bool,
    select: Option<Selector>,
    finder: std::sync::OnceLock<Option<Engine>>,
    find_spec: Option<(Node, Options, bool)>,
}

impl Matcher {
    fn never(o: Options) -> Self {
        Self {
            utf8: o.utf8,
            eol: o.eol(),
            words: false,
            select: None,
            finder: std::sync::OnceLock::new(),
            find_spec: None,
        }
    }

    fn assemble(o: Options, select: Selector, find_node: &Node, byte: bool) -> Self {
        Self {
            utf8: o.utf8,
            eol: o.eol(),
            words: o.words(),
            select: Some(select),
            finder: std::sync::OnceLock::new(),
            find_spec: Some((find_node.clone(), o, byte)),
        }
    }

    fn finder(&self) -> Option<&Engine> {
        self.finder
            .get_or_init(|| {
                let (node, o, byte) = self.find_spec.as_ref()?;
                let t = translate::translate_with(node, o.utf8, o.eol(), false, nl_quirk(*o, node));
                let prefilter = translate::build(&t.pattern, o.utf8, o.eol())
                    .map(|re| Prefilter { re, exact: t.exact });
                make_engine(node, *o, *byte).map(|e| e.with_prefilter(prefilter))
            })
            .as_ref()
    }

    fn accept(&self, sel: &Selector, line: &[u8]) -> bool {
        if let Some(s) = &sel.superset
            && !s.is_match(line)
        {
            return false;
        }
        if sel.exact {
            return true;
        }
        if let Some(fv) = &sel.fast_valid {
            let hit = match &sel.fast_valid_dfa {
                Some(d) => d.is_match_line(line),
                None => fv.is_match(line),
            };
            if hit {
                return true;
            }
            if line.is_ascii() || std::str::from_utf8(line).is_ok() {
                return false;
            }
        }
        if let Some(fa) = &sel.fast_ascii
            && line.is_ascii()
        {
            return fa.is_match(line);
        }
        if let Some(g) = &sel.gnu {
            return g.select(line, sel.mode, self.utf8);
        }
        let Some(e) = &sel.engine else {
            return false;
        };
        match sel.mode {
            Mode::Plain => e.search(line, 0, line.len(), false).is_some(),
            Mode::Lines => e.longest_at(line, 0, line.len(), false) == Some(line.len()),
            Mode::Words => words_first(e, line, 0, self.utf8).is_some(),
        }
    }

    #[must_use]
    pub fn is_match(&self, line: &[u8]) -> bool {
        let Some(sel) = &self.select else {
            return false;
        };
        if let (Some(d), true) = (&sel.dfa, sel.exact) {
            if !d.is_match_line(line) {
                return false;
            }
        } else if let Some(re) = &sel.re
            && !re.is_match(line)
        {
            return false;
        }
        self.accept(sel, line)
    }

    fn line_bounds(&self, buf: &[u8], pos: usize, s: usize) -> (usize, usize) {
        let ls = memrchr(self.eol, &buf[pos..s]).map_or(pos, |i| pos + i + 1);
        let le = memchr(self.eol, &buf[s..]).map_or(buf.len(), |i| s + i);
        (ls, le)
    }

    fn candidate(&self, sel: &Selector, buf: &[u8], pos: usize) -> Option<(usize, usize, bool)> {
        let eol = self.eol;
        if let (Some(fv), None) = (&sel.fast_valid, &sel.fast_valid_dfa) {
            let hit = fv
                .shortest_match_at(buf, pos)
                .filter(|&s| !(s == buf.len() && (buf.is_empty() || buf[s - 1] == eol)));
            let hit_line = hit.map(|s| self.line_bounds(buf, pos, s));
            let region_end = hit_line.map_or(buf.len(), |(ls, _)| ls);
            let mut p = pos;
            while p < region_end {
                let region = &buf[p..region_end];
                if region.is_ascii() {
                    break;
                }
                let Err(e) = std::str::from_utf8(region) else {
                    break;
                };
                let bad = p + e.valid_up_to();
                let (ls, le) = self.line_bounds(buf, p, bad);
                if self.accept(sel, &buf[ls..le]) {
                    return Some((ls, le, true));
                }
                p = le + 1;
            }
            return hit_line.map(|(ls, le)| (ls, le, true));
        }
        if sel.exact {
            match (&sel.strategy, &sel.dfa) {
                (Strategy::Dfa, Some(d)) => {
                    let p = d.find(buf, pos)?;
                    let (ls, le) = self.line_bounds(buf, pos, p);
                    return Some((ls, le, false));
                }
                (Strategy::Lits(f), _) => {
                    let mut from = pos;
                    loop {
                        let p = f.find(buf, from)?;
                        let (ls, le) = self.line_bounds(buf, from, p);
                        let line = &buf[ls..le];
                        let ok = match (&sel.dfa, &sel.re) {
                            (Some(d), _) => d.is_match_line(line),
                            (None, Some(re)) => re.is_match(line),
                            (None, None) => true,
                        };
                        if ok {
                            return Some((ls, le, false));
                        }
                        if le >= buf.len() {
                            return None;
                        }
                        from = le + 1;
                    }
                }
                _ => {}
            }
        }
        if let Some(re) = &sel.re {
            let s = re.shortest_match_at(buf, pos)?;
            if s == buf.len() && (buf.is_empty() || buf[s - 1] == eol) {
                return None;
            }
            let (ls, le) = self.line_bounds(buf, pos, s);
            Some((ls, le, false))
        } else {
            if pos == buf.len() {
                return None;
            }
            let le = memchr(eol, &buf[pos..]).map_or(buf.len(), |i| pos + i);
            Some((pos, le, false))
        }
    }

    #[must_use]
    pub fn find_line(&self, buf: &[u8], at: usize) -> Option<(usize, usize)> {
        let sel = self.select.as_ref()?;
        let mut pos = at;
        while pos <= buf.len() {
            let (ls, le, accepted) = self.candidate(sel, buf, pos)?;
            let end = if le < buf.len() { le + 1 } else { le };
            let line = &buf[ls..le];
            let ok = if accepted {
                sel.superset.as_ref().is_none_or(|s| s.is_match(line))
            } else {
                self.accept(sel, line)
            };
            if ok {
                return Some((ls, end));
            }
            if end >= buf.len() {
                return None;
            }
            pos = end;
        }
        None
    }

    #[must_use]
    pub fn find_at(&self, line: &[u8], at: usize) -> Option<(usize, usize)> {
        if let Some(g) = self.select.as_ref().and_then(|s| s.gnu.as_ref()) {
            return g.find(line, at, self.words, self.utf8);
        }
        let e = self.finder()?;
        if self.words {
            words_first(e, line, at, self.utf8).map(|(s, l)| (s, s + l))
        } else {
            e.search(line, at, line.len(), false)
        }
    }

    #[must_use]
    pub fn match_spans(&self, line: &[u8]) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut cur = 0;
        while cur <= line.len() {
            let Some((b, e)) = self.find_at(line, cur) else {
                break;
            };
            if e == b {
                cur = b + 1;
            } else {
                spans.push((b, e));
                cur = e;
            }
        }
        spans
    }
}

fn wordchar_next(line: &[u8], pos: usize, utf8: bool) -> bool {
    pos < line.len() && locale::is_word_grep(utf8, locale::decode(utf8, &line[pos..]).0)
}

fn wordchar_prev(line: &[u8], pos: usize, utf8: bool) -> bool {
    if pos == 0 {
        return false;
    }
    let cur = pos - 1;
    let b = line[cur];
    if !utf8 || b < 0x80 {
        return locale::is_word_grep(utf8, u32::from(b));
    }
    let mut start = cur;
    if b & 0xc0 == 0x80 {
        for i in 1..=3.min(cur) {
            let lead = line[cur - i];
            if lead & 0xc0 != 0x80 {
                let long_enough = (!lead) >> (7 - i) == 0;
                if long_enough && locale::decode(true, &line[cur - i..]).0 < locale::INVALID_BASE {
                    start = cur - i;
                }
                break;
            }
        }
    }
    wordchar_next(line, start, utf8)
}

trait Searcher {
    fn find(&self, hay: &[u8], from: usize) -> Option<(usize, usize)>;
    fn anchored(&self, hay: &[u8], start: usize, limit: usize) -> Option<usize>;
}

impl Searcher for Engine {
    fn find(&self, hay: &[u8], from: usize) -> Option<(usize, usize)> {
        self.search(hay, from, hay.len(), false)
    }

    fn anchored(&self, hay: &[u8], start: usize, limit: usize) -> Option<usize> {
        self.longest_at(hay, start, limit, true)
    }
}

struct GreSearch<'a> {
    re: &'a Gre,
    newline_anchor: bool,
}

impl Searcher for GreSearch<'_> {
    fn find(&self, hay: &[u8], from: usize) -> Option<(usize, usize)> {
        self.re
            .search(hay, from, hay.len(), false, self.newline_anchor)
    }

    fn anchored(&self, hay: &[u8], start: usize, limit: usize) -> Option<usize> {
        self.re
            .match_at(hay, start, limit, true, self.newline_anchor)
    }
}

#[derive(Debug)]
struct GnuSet {
    res: Vec<Gre>,
    newline_anchor: bool,
}

impl GnuSet {
    fn searcher<'a>(&self, re: &'a Gre) -> GreSearch<'a> {
        GreSearch {
            re,
            newline_anchor: self.newline_anchor,
        }
    }

    fn select(&self, line: &[u8], mode: Mode, utf8: bool) -> bool {
        self.res.iter().any(|re| {
            let s = self.searcher(re);
            let Some((ms, me)) = s.find(line, 0) else {
                return false;
            };
            match mode {
                Mode::Plain => true,
                Mode::Lines => me - ms == line.len(),
                Mode::Words => {
                    words_from(&s, line, 0, (ms, me - ms), line.len() + 1, utf8).is_some()
                }
            }
        })
    }

    fn find(&self, line: &[u8], at: usize, words: bool, utf8: bool) -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize)> = None;
        for re in &self.res {
            let s = self.searcher(re);
            let Some((ms, me)) = s.find(line, at) else {
                continue;
            };
            let best_match = best.map_or(line.len() + 1, |b| b.0);
            if ms > best_match {
                continue;
            }
            let cand = if words {
                words_from(&s, line, at, (ms, me - ms), best_match, utf8)
            } else {
                Some((ms, me - ms))
            };
            if let Some((m, l)) = cand
                && best.is_none_or(|(bm, bl)| m < bm || (m == bm && l > bl))
            {
                best = Some((m, l));
            }
        }
        best.map(|(m, l)| (m, m + l))
    }
}

fn possible_backrefs(p: &[u8]) -> bool {
    let Some(last) = p.len().checked_sub(1) else {
        return false;
    };
    let mut i = 0;
    while i < last {
        if p[i] == b'\\' {
            if (b'1'..=b'9').contains(&p[i + 1]) {
                return true;
            }
            if p[i + 1] == b'\\' {
                i += 1;
            }
        }
        i += 1;
    }
    false
}

fn words_first(e: &dyn Searcher, line: &[u8], ptr: usize, utf8: bool) -> Option<(usize, usize)> {
    let (s, en) = e.find(line, ptr)?;
    words_from(e, line, ptr, (s, en - s), line.len() + 1, utf8)
}

fn words_from(
    e: &dyn Searcher,
    line: &[u8],
    ptr: usize,
    found: (usize, usize),
    best: usize,
    utf8: bool,
) -> Option<(usize, usize)> {
    let end1 = line.len();
    let (mut m, mut len) = found;
    while m <= best {
        if !wordchar_next(line, m + len, utf8) && !wordchar_prev(line, m, utf8) {
            return Some((m, len));
        }
        let mut shorter = 0;
        if len > 0 {
            len -= 1;
            if let Some(limit) = (m + len).checked_sub(ptr)
                && limit >= m
            {
                shorter = e.anchored(line, m, limit).map_or(0, |end| end - m);
            }
        }
        if shorter > 0 {
            len = shorter;
        } else {
            if m == end1 {
                return None;
            }
            m += 1;
            let (s, en) = e.find(line, m)?;
            m = s;
            len = en - s;
        }
    }
    None
}

fn fgrep_charlen(s: &[u8], utf8: bool, icase: bool) -> Option<usize> {
    let (u, len) = locale::decode(utf8, s);
    if u >= locale::INVALID_BASE {
        return None;
    }
    if !icase {
        return Some(len);
    }
    if len == 1 {
        let all_single = locale::case_folded_counterparts(utf8, u)
            .iter()
            .all(|&f| !utf8 || f < 0x80);
        return all_single.then_some(1);
    }
    if !locale::case_folded_counterparts(utf8, u).is_empty() {
        return None;
    }
    s[1..len]
        .iter()
        .all(|&b| locale::to_upper(utf8, u32::from(b)) == u32::from(b))
        .then_some(len)
}

fn nl_quirk(o: Options, node: &Node) -> bool {
    o.null_data && node.any(&|n| matches!(n, Node::Look(Look::LineStart | Look::LineEnd)))
}
