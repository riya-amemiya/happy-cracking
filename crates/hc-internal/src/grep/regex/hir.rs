use std::collections::HashMap;
use std::fmt::Write as _;

use super::super::matcher::ByteSet;
use super::ast::{
    AssertKind, Ast, ClassAst, ClassItem, ClassSet, FLAG_I, FLAG_M, FLAG_R, FLAG_S, FLAG_U,
    GroupKind, Lit, PerlKind, SetOp,
};

const SPECIAL_K: u32 = 0x212A;
const SPECIAL_S: u32 = 0x17F;
const MAX_CHAR: u32 = 0x10_FFFF;

#[derive(Clone, Debug)]
pub(super) enum High {
    Bytes(u128),
    Chars(Vec<(u32, u32)>),
    Unknown,
}

#[derive(Clone, Debug)]
pub(super) struct CSet {
    pub(super) ascii: u128,
    pub(super) high: High,
}

fn ascii_range(lo: u8, hi: u8) -> u128 {
    (u32::from(lo)..=u32::from(hi)).fold(0, |acc, b| acc | (1u128 << b))
}

fn normalize(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (lo, hi) in ranges {
        if let Some(last) = out.last_mut()
            && lo <= last.1.saturating_add(1)
        {
            last.1 = last.1.max(hi);
            continue;
        }
        out.push((lo, hi));
    }
    out
}

fn complement_chars(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let universe = [(0x80u32, 0xD7FFu32), (0xE000, MAX_CHAR)];
    let mut out = vec![];
    for &(ulo, uhi) in &universe {
        let mut cur = ulo;
        for &(lo, hi) in ranges {
            if hi < cur || lo > uhi {
                continue;
            }
            if lo > cur {
                out.push((cur, lo - 1));
            }
            cur = hi.saturating_add(1);
            if cur > uhi {
                break;
            }
        }
        if cur <= uhi {
            out.push((cur, uhi));
        }
    }
    out
}

fn intersect_chars(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = vec![];
    for &(alo, ahi) in a {
        for &(blo, bhi) in b {
            let lo = alo.max(blo);
            let hi = ahi.min(bhi);
            if lo <= hi {
                out.push((lo, hi));
            }
        }
    }
    normalize(out)
}

fn chars_contains(ranges: &[(u32, u32)], c: u32) -> bool {
    ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi)
}

impl CSet {
    pub(super) fn empty(bytes: bool) -> CSet {
        CSet {
            ascii: 0,
            high: if bytes {
                High::Bytes(0)
            } else {
                High::Chars(vec![])
            },
        }
    }

    fn is_bytes(&self) -> bool {
        matches!(self.high, High::Bytes(_))
    }

    fn from_ascii(bytes: bool, ascii: u128) -> CSet {
        CSet {
            ascii,
            ..CSet::empty(bytes)
        }
    }

    fn from_range(bytes: bool, lo: u32, hi: u32) -> CSet {
        let mut set = CSet::empty(bytes);
        if lo > hi {
            return set;
        }
        if lo < 0x80 {
            set.ascii = ascii_range(lo as u8, hi.min(0x7F) as u8);
        }
        if hi >= 0x80 {
            let hlo = lo.max(0x80);
            if bytes {
                let mut mask = 0u128;
                for b in hlo..=hi.min(0xFF) {
                    mask |= 1u128 << (b - 0x80);
                }
                set.high = High::Bytes(mask);
            } else {
                set.high = High::Chars(normalize(vec![(hlo, hi)]));
            }
        }
        set
    }

    pub(super) fn is_empty_exact(&self) -> bool {
        self.ascii == 0
            && match &self.high {
                High::Bytes(m) => *m == 0,
                High::Chars(r) => r.is_empty(),
                High::Unknown => false,
            }
    }

    fn high_count(&self) -> Option<u64> {
        match &self.high {
            High::Bytes(m) => Some(u64::from(m.count_ones())),
            High::Chars(r) => Some(r.iter().map(|&(lo, hi)| u64::from(hi - lo + 1)).sum()),
            High::Unknown => None,
        }
    }

    fn singleton(&self) -> Option<Vec<u8>> {
        let count = u64::from(self.ascii.count_ones()) + self.high_count()?;
        if count != 1 {
            return None;
        }
        if self.ascii != 0 {
            return Some(vec![self.ascii.trailing_zeros() as u8]);
        }
        match &self.high {
            High::Bytes(m) => Some(vec![0x80 + m.trailing_zeros() as u8]),
            High::Chars(r) => {
                let c = char::from_u32(r[0].0)?;
                Some(c.encode_utf8(&mut [0; 4]).as_bytes().to_vec())
            }
            High::Unknown => None,
        }
    }

    fn union(&mut self, other: &CSet) {
        self.ascii |= other.ascii;
        self.high = match (&self.high, &other.high) {
            (High::Bytes(a), High::Bytes(b)) => High::Bytes(a | b),
            (High::Chars(a), High::Chars(b)) => {
                High::Chars(normalize(a.iter().chain(b.iter()).copied().collect()))
            }
            (High::Bytes(a), High::Chars(b)) | (High::Chars(b), High::Bytes(a)) if *a == 0 => {
                High::Chars(b.clone())
            }
            _ => High::Unknown,
        };
    }

    fn intersect(&mut self, other: &CSet) {
        self.ascii &= other.ascii;
        self.high = match (&self.high, &other.high) {
            (High::Bytes(a), High::Bytes(b)) => High::Bytes(a & b),
            (High::Chars(a), High::Chars(b)) => High::Chars(intersect_chars(a, b)),
            (High::Chars(a), _) | (_, High::Chars(a)) if a.is_empty() => High::Chars(vec![]),
            (High::Bytes(0), _) | (_, High::Bytes(0)) => High::Bytes(0),
            _ => High::Unknown,
        };
    }

    fn difference(&mut self, other: &CSet) {
        self.ascii &= !other.ascii;
        self.high = match (&self.high, &other.high) {
            (High::Bytes(a), High::Bytes(b)) => High::Bytes(a & !b),
            (High::Chars(a), High::Chars(b)) => {
                High::Chars(intersect_chars(a, &complement_chars(b)))
            }
            (High::Chars(a), _) if a.is_empty() => High::Chars(vec![]),
            (High::Bytes(0), _) => High::Bytes(0),
            (h, High::Chars(b)) if b.is_empty() => h.clone(),
            _ => High::Unknown,
        };
    }

    fn symmetric_difference(&mut self, other: &CSet) {
        self.ascii ^= other.ascii;
        self.high = match (&self.high, &other.high) {
            (High::Bytes(a), High::Bytes(b)) => High::Bytes(a ^ b),
            (High::Chars(a), High::Chars(b)) => {
                let only_a = intersect_chars(a, &complement_chars(b));
                let only_b = intersect_chars(b, &complement_chars(a));
                High::Chars(normalize(only_a.into_iter().chain(only_b).collect()))
            }
            _ => High::Unknown,
        };
    }

    fn negate(&mut self) {
        self.ascii = !self.ascii;
        self.high = match &self.high {
            High::Bytes(m) => High::Bytes(!m),
            High::Chars(r) => High::Chars(complement_chars(r)),
            High::Unknown => High::Unknown,
        };
    }

    fn fold(&mut self) {
        let letters_upper = ascii_range(b'A', b'Z');
        let letters_lower = ascii_range(b'a', b'z');
        let upper = self.ascii & letters_upper;
        let lower = self.ascii & letters_lower;
        self.ascii |= (upper << 32) | (lower >> 32);
        if self.is_bytes() {
            return;
        }
        let has = |set: &CSet, c: u8| set.ascii & (1u128 << c) != 0;
        let mut extra = vec![];
        if has(self, b'k') {
            extra.push((SPECIAL_K, SPECIAL_K));
        }
        if has(self, b's') {
            extra.push((SPECIAL_S, SPECIAL_S));
        }
        self.high = match &self.high {
            High::Chars(r) => {
                let mut chars: Vec<(u32, u32)> = r.clone();
                if chars_contains(r, SPECIAL_K) {
                    self.ascii |= (1u128 << b'k') | (1u128 << b'K');
                }
                if chars_contains(r, SPECIAL_S) {
                    self.ascii |= (1u128 << b's') | (1u128 << b'S');
                }
                let small: u64 = r.iter().map(|&(lo, hi)| u64::from(hi - lo + 1)).sum();
                if small > 256 {
                    High::Unknown
                } else {
                    for &(lo, hi) in r {
                        for cp in lo..=hi {
                            if let Some(c) = char::from_u32(cp) {
                                for v in simple_variants(c) {
                                    let v = v as u32;
                                    if v < 0x80 {
                                        self.ascii |= 1u128 << v;
                                    } else {
                                        chars.push((v, v));
                                    }
                                }
                            }
                        }
                    }
                    chars.extend(extra);
                    High::Chars(normalize(chars))
                }
            }
            High::Unknown => High::Unknown,
            High::Bytes(m) => High::Bytes(*m),
        };
    }

    fn contains_ascii(&self, b: u8) -> bool {
        b < 0x80 && self.ascii & (1u128 << b) != 0
    }

    fn remove_ascii(&mut self, b: u8) {
        if b < 0x80 {
            self.ascii &= !(1u128 << b);
        }
    }

    fn add_matching_bytes(&self, set: &mut ByteSet) {
        for b in 0..128u8 {
            if self.contains_ascii(b) {
                set.remove(b);
            }
        }
        match &self.high {
            High::Bytes(m) => {
                for i in 0..128u32 {
                    if m & (1u128 << i) != 0 {
                        set.remove(0x80 + i as u8);
                    }
                }
            }
            High::Chars(r) => {
                for &(lo, hi) in r {
                    remove_utf8_range(set, lo, hi);
                }
            }
            High::Unknown => {
                set.remove_all(0x80, 0xBF);
                set.remove_all(0xC2, 0xF4);
            }
        }
    }
}

fn simple_variants(c: char) -> Vec<char> {
    let mut out = vec![c];
    let mut push = |x: char| {
        if !out.contains(&x) {
            out.push(x);
        }
    };
    let single = |mut it: std::iter::Peekable<std::vec::IntoIter<char>>| {
        let first = it.next()?;
        if it.peek().is_some() {
            None
        } else {
            Some(first)
        }
    };
    let lower = single(c.to_lowercase().collect::<Vec<_>>().into_iter().peekable());
    let upper = single(c.to_uppercase().collect::<Vec<_>>().into_iter().peekable());
    if let Some(l) = lower {
        push(l);
        if let Some(u) = single(l.to_uppercase().collect::<Vec<_>>().into_iter().peekable()) {
            push(u);
        }
    }
    if let Some(u) = upper {
        push(u);
        if let Some(l) = single(u.to_lowercase().collect::<Vec<_>>().into_iter().peekable()) {
            push(l);
        }
    }
    out
}

fn utf8_lead_and_len(c: u32) -> (u8, usize) {
    let ch = char::from_u32(c).unwrap_or('\u{FFFD}');
    let mut buf = [0; 4];
    let s = ch.encode_utf8(&mut buf);
    (s.as_bytes()[0], s.len())
}

fn remove_utf8_range(set: &mut ByteSet, lo: u32, hi: u32) {
    let classes = [(0x80u32, 0x7FFu32), (0x800, 0xFFFF), (0x1_0000, MAX_CHAR)];
    for &(clo, chi) in &classes {
        let a = lo.max(clo);
        let b = hi.min(chi);
        if a > b {
            continue;
        }
        if b - a < 4096 {
            for cp in a..=b {
                if let Some(ch) = char::from_u32(cp) {
                    for &byte in ch.encode_utf8(&mut [0; 4]).as_bytes() {
                        set.remove(byte);
                    }
                }
            }
        } else {
            let (la, _) = utf8_lead_and_len(a);
            let (lb, _) = utf8_lead_and_len(if (0xD800..=0xDFFF).contains(&b) {
                0xD7FF
            } else {
                b
            });
            set.remove_all(la.min(lb), la.max(lb));
            set.remove_all(0x80, 0xBF);
        }
    }
}

impl PartialEq for CSet {
    fn eq(&self, other: &CSet) -> bool {
        if self.ascii != other.ascii {
            return false;
        }
        match (&self.high, &other.high) {
            (High::Bytes(a), High::Bytes(b)) => a == b,
            (High::Chars(a), High::Chars(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Look {
    Start,
    End,
    StartLf,
    EndLf,
    StartCrlf,
    EndCrlf,
    Word(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Hir {
    Empty,
    Literal(Vec<u8>),
    Class(CSet),
    Look(Look),
    Repetition {
        min: u32,
        max: Option<u32>,
        sub: Box<Hir>,
    },
    Capture(Box<Hir>),
    Concat(Vec<Hir>),
    Alternation(Vec<Hir>),
}

impl Hir {
    pub(super) fn fail() -> Hir {
        Hir::Class(CSet::empty(false))
    }

    pub(super) fn literal(bytes: Vec<u8>) -> Hir {
        if bytes.is_empty() {
            Hir::Empty
        } else {
            Hir::Literal(bytes)
        }
    }

    pub(super) fn class(set: CSet) -> Hir {
        if set.is_empty_exact() {
            return Hir::Class(set);
        }
        if let Some(bytes) = set.singleton() {
            return Hir::literal(bytes);
        }
        Hir::Class(set)
    }

    fn is_fail(&self) -> bool {
        matches!(self, Hir::Class(set) if set.is_empty_exact())
    }

    fn max_is_zero(&self) -> bool {
        match self {
            Hir::Empty | Hir::Look(_) => true,
            Hir::Literal(_) | Hir::Class(_) => false,
            Hir::Repetition { max, sub, .. } => *max == Some(0) || sub.max_is_zero(),
            Hir::Capture(sub) => sub.max_is_zero(),
            Hir::Concat(xs) | Hir::Alternation(xs) => xs.iter().all(Hir::max_is_zero),
        }
    }

    pub(super) fn repetition(mut min: u32, mut max: Option<u32>, sub: Hir) -> Hir {
        if sub.max_is_zero() && !sub.is_fail() {
            min = min.min(1);
            max = Some(max.map_or(1, |n| n.min(1)));
        }
        if min == 0 && max == Some(0) {
            return Hir::Empty;
        }
        if min == 1 && max == Some(1) {
            return sub;
        }
        Hir::Repetition {
            min,
            max,
            sub: Box::new(sub),
        }
    }

    pub(super) fn concat(subs: Vec<Hir>) -> Hir {
        let mut new = vec![];
        let mut prior: Option<Vec<u8>> = None;
        let push = |h: Hir, new: &mut Vec<Hir>, prior: &mut Option<Vec<u8>>| match h {
            Hir::Literal(bytes) => match prior {
                Some(p) => p.extend_from_slice(&bytes),
                None => *prior = Some(bytes),
            },
            Hir::Empty => {}
            other => {
                if let Some(p) = prior.take() {
                    new.push(Hir::literal(p));
                }
                new.push(other);
            }
        };
        for sub in subs {
            match sub {
                Hir::Concat(xs) => {
                    for x in xs {
                        push(x, &mut new, &mut prior);
                    }
                }
                other => push(other, &mut new, &mut prior),
            }
        }
        if let Some(p) = prior.take() {
            new.push(Hir::literal(p));
        }
        match new.len() {
            0 => Hir::Empty,
            1 => new.pop().unwrap(),
            _ => Hir::Concat(new),
        }
    }

    pub(super) fn alternation(subs: Vec<Hir>) -> Hir {
        let mut new = vec![];
        for sub in subs {
            match sub {
                Hir::Alternation(xs) => new.extend(xs),
                other => new.push(other),
            }
        }
        if new.is_empty() {
            return Hir::fail();
        }
        if new.len() == 1 {
            return new.pop().unwrap();
        }
        if let Some(chars) = singleton_chars(&new) {
            let mut set = CSet::empty(false);
            for c in chars {
                set.union(&CSet::from_range(false, c, c));
            }
            return Hir::class(set);
        }
        if let Some(bytes) = singleton_bytes(&new) {
            let mut set = CSet::empty(true);
            for b in bytes {
                set.union(&CSet::from_range(true, u32::from(b), u32::from(b)));
            }
            return Hir::class(set);
        }
        if let Some(set) = class_union(&new) {
            return Hir::class(set);
        }
        match lift_common_prefix(new) {
            Ok(hir) => hir,
            Err(unchanged) => Hir::Alternation(unchanged),
        }
    }
}

fn singleton_chars(hirs: &[Hir]) -> Option<Vec<u32>> {
    hirs.iter()
        .map(|h| match h {
            Hir::Literal(bytes) => {
                let s = std::str::from_utf8(bytes).ok()?;
                let mut it = s.chars();
                let c = it.next()?;
                if it.next().is_some() {
                    None
                } else {
                    Some(c as u32)
                }
            }
            _ => None,
        })
        .collect()
}

fn singleton_bytes(hirs: &[Hir]) -> Option<Vec<u8>> {
    hirs.iter()
        .map(|h| match h {
            Hir::Literal(bytes) if bytes.len() == 1 => Some(bytes[0]),
            _ => None,
        })
        .collect()
}

fn class_union(hirs: &[Hir]) -> Option<CSet> {
    let all_bytes = hirs
        .iter()
        .all(|h| matches!(h, Hir::Class(s) if s.is_bytes()));
    let mut out = CSet::empty(all_bytes);
    for h in hirs {
        let Hir::Class(set) = h else { return None };
        if !all_bytes && set.is_bytes() {
            if let High::Bytes(m) = set.high
                && m != 0
            {
                return None;
            }
            out.union(&CSet::from_ascii(false, set.ascii));
        } else {
            out.union(set);
        }
    }
    Some(out)
}

fn lift_common_prefix(hirs: Vec<Hir>) -> Result<Hir, Vec<Hir>> {
    if hirs.len() <= 1 {
        return Err(hirs);
    }
    let Hir::Concat(first) = &hirs[0] else {
        return Err(hirs);
    };
    let mut len = first.len();
    for h in &hirs[1..] {
        let Hir::Concat(xs) = h else { return Err(hirs) };
        len = first[..len]
            .iter()
            .zip(xs.iter())
            .take_while(|(x, y)| x == y)
            .count();
        if len == 0 {
            return Err(hirs);
        }
    }
    let mut prefix = vec![];
    let mut suffixes = vec![];
    for h in hirs {
        let Hir::Concat(mut xs) = h else {
            unreachable!()
        };
        suffixes.push(Hir::concat(xs.split_off(len)));
        if prefix.is_empty() {
            prefix = xs;
        }
    }
    prefix.push(Hir::alternation(suffixes));
    Ok(Hir::concat(prefix))
}

#[derive(Debug)]
pub(super) struct Edit {
    pub(super) span: (usize, usize),
    pub(super) text: String,
}

pub(super) struct Translator<'a> {
    pattern: &'a str,
    terminators: Vec<u8>,
    probes: HashMap<(String, bool), CSet>,
    pub(super) edits: Vec<Edit>,
}

fn term_text(terms: &[u8]) -> String {
    terms.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "\\x{b:02X}");
        out
    })
}

impl<'a> Translator<'a> {
    pub(super) fn new(pattern: &'a str, terminators: Vec<u8>) -> Translator<'a> {
        Translator {
            pattern,
            terminators,
            probes: HashMap::new(),
            edits: vec![],
        }
    }

    pub(super) fn translate(&mut self, ast: &Ast, flags: u8) -> Hir {
        let mut flags = flags;
        self.visit(ast, &mut flags)
    }

    fn visit(&mut self, ast: &Ast, flags: &mut u8) -> Hir {
        match ast {
            Ast::Empty => Hir::Empty,
            Ast::Flags(set) => {
                *flags = set.apply(*flags);
                Hir::Empty
            }
            Ast::Literal(lit) => self.literal(lit, *flags),
            Ast::Dot(span) => self.dot(*span, *flags),
            Ast::Assertion(kind) => Hir::Look(assertion(*kind, *flags)),
            Ast::Class { span, class } => {
                let set = self.class(class, *flags);
                if self.terminators.iter().any(|&t| set.contains_ascii(t)) {
                    let text = &self.pattern[span.0..span.1];
                    self.edits.push(Edit {
                        span: *span,
                        text: format!("[{text}&&[^{}]]", term_text(&self.terminators)),
                    });
                }
                Hir::class(set)
            }
            Ast::Repetition { min, max, ast, .. } => {
                let sub = self.visit(ast, flags);
                Hir::repetition(*min, *max, sub)
            }
            Ast::Group { kind, ast } => {
                let saved = *flags;
                if let GroupKind::NonCapture(set) = kind {
                    *flags = set.apply(*flags);
                }
                let sub = self.visit(ast, flags);
                *flags = saved;
                match kind {
                    GroupKind::Capture => Hir::Capture(Box::new(sub)),
                    GroupKind::NonCapture(_) => sub,
                }
            }
            Ast::Concat(asts) => {
                let subs = asts.iter().map(|a| self.visit(a, flags)).collect();
                Hir::concat(subs)
            }
            Ast::Alternation(asts) => {
                let subs = asts.iter().map(|a| self.visit(a, flags)).collect();
                Hir::alternation(subs)
            }
        }
    }

    fn literal(&mut self, lit: &Lit, flags: u8) -> Hir {
        let unicode = flags & FLAG_U != 0;
        let ci = flags & FLAG_I != 0;
        let raw_byte = !unicode && lit.hex_byte && (lit.c as u32) > 0x7F && (lit.c as u32) <= 0xFF;
        let hir = if raw_byte {
            Hir::literal(vec![lit.c as u32 as u8])
        } else if ci {
            let mut set = CSet::from_range(!unicode, lit.c as u32, lit.c as u32);
            set.fold();
            Hir::class(set)
        } else {
            Hir::literal(lit.c.encode_utf8(&mut [0; 4]).as_bytes().to_vec())
        };
        let hits = match &hir {
            Hir::Literal(bytes) => bytes.iter().any(|b| self.terminators.contains(b)),
            _ => false,
        };
        if hits {
            self.edits.push(Edit {
                span: lit.span,
                text: "[a&&b]".to_string(),
            });
        }
        hir
    }

    fn dot(&mut self, span: (usize, usize), flags: u8) -> Hir {
        let unicode = flags & FLAG_U != 0;
        let mut excluded: Vec<u8> = vec![];
        if flags & FLAG_S == 0 {
            if flags & FLAG_R != 0 {
                excluded.extend_from_slice(b"\r\n");
            } else {
                excluded.push(b'\n');
            }
        }
        let mut set = CSet::from_range(!unicode, 0, if unicode { MAX_CHAR } else { 0xFF });
        for &b in &excluded {
            set.remove_ascii(b);
        }
        if self.terminators.iter().any(|t| !excluded.contains(t)) {
            let mut all = excluded.clone();
            for &t in &self.terminators {
                if !all.contains(&t) {
                    all.push(t);
                }
            }
            self.edits.push(Edit {
                span,
                text: format!("[^{}]", term_text(&all)),
            });
        }
        Hir::class(set)
    }

    fn class(&mut self, class: &ClassAst, flags: u8) -> CSet {
        let unicode = flags & FLAG_U != 0;
        let ci = flags & FLAG_I != 0;
        match class {
            ClassAst::Perl { kind, negated } => perl_set(*kind, *negated, unicode),
            ClassAst::Unicode { text, negated } => {
                let mut set = self.probe(text, ci);
                if *negated {
                    set.negate();
                }
                set
            }
            ClassAst::Bracketed { negated, set } => {
                let mut out = self.class_set(set, unicode, ci);
                if ci {
                    out.fold();
                }
                if *negated {
                    out.negate();
                }
                out
            }
        }
    }

    fn class_set(&mut self, set: &ClassSet, unicode: bool, ci: bool) -> CSet {
        match set {
            ClassSet::Item(item) => self.class_item(item, unicode, ci),
            ClassSet::BinaryOp(op, lhs, rhs) => {
                let mut l = self.class_set(lhs, unicode, ci);
                let mut r = self.class_set(rhs, unicode, ci);
                if ci {
                    l.fold();
                    r.fold();
                }
                match op {
                    SetOp::Intersection => l.intersect(&r),
                    SetOp::Difference => l.difference(&r),
                    SetOp::SymmetricDifference => l.symmetric_difference(&r),
                }
                l
            }
        }
    }

    fn class_item(&mut self, item: &ClassItem, unicode: bool, ci: bool) -> CSet {
        let bytes = !unicode;
        match item {
            ClassItem::Literal(lit) => CSet::from_range(bytes, lit.c as u32, lit.c as u32),
            ClassItem::Range(a, b) => CSet::from_range(bytes, a.c as u32, b.c as u32),
            ClassItem::Ascii { name, negated } => {
                let mut set = CSet::from_ascii(bytes, ascii_class(name));
                if ci {
                    set.fold();
                }
                if *negated {
                    set.negate();
                }
                set
            }
            ClassItem::Unicode { text, negated } => {
                let mut set = self.probe(text, ci);
                if *negated {
                    set.negate();
                }
                set
            }
            ClassItem::Perl { kind, negated } => perl_set(*kind, *negated, unicode),
            ClassItem::Bracketed { negated, set } => {
                let mut out = self.class_set(set, unicode, ci);
                if ci {
                    out.fold();
                }
                if *negated {
                    out.negate();
                }
                out
            }
            ClassItem::Union(items) => {
                let mut out = CSet::empty(bytes);
                for x in items {
                    let s = self.class_item(x, unicode, ci);
                    out.union(&s);
                }
                out
            }
        }
    }

    fn probe(&mut self, text: &str, ci: bool) -> CSet {
        if let Some(set) = self.probes.get(&(text.to_string(), ci)) {
            return set.clone();
        }
        let set = probe_unicode_class(text, ci);
        self.probes.insert((text.to_string(), ci), set.clone());
        set
    }
}

fn probe_unicode_class(text: &str, ci: bool) -> CSet {
    let Ok(re) = regex::bytes::RegexBuilder::new(text)
        .unicode(true)
        .case_insensitive(ci)
        .build()
    else {
        return CSet {
            ascii: u128::MAX,
            high: High::Unknown,
        };
    };
    let hay: Vec<u8> = (0..128u8).collect();
    let ascii = re
        .find_iter(&hay)
        .filter(|m| m.len() == 1)
        .fold(0u128, |acc, m| acc | (1u128 << m.start()));
    let samples = "\u{80}\u{A0}\u{AA}\u{B5}\u{C0}\u{E9}\u{FF}\u{100}\u{17F}\u{391}\u{3B1}\u{3C3}\u{410}\u{430}\u{5D0}\u{627}\u{660}\u{905}\u{966}\u{E01}\u{1100}\u{2028}\u{2160}\u{212A}\u{3000}\u{3042}\u{30A2}\u{4E00}\u{AC00}\u{FB01}\u{FF10}\u{FF21}\u{FFFD}\u{10000}\u{1D400}\u{1F600}\u{20000}\u{E0001}\u{10FFFD}";
    let high = if re.is_match(samples.as_bytes()) {
        High::Unknown
    } else {
        High::Chars(vec![])
    };
    CSet { ascii, high }
}

fn ascii_class(name: &str) -> u128 {
    let r = ascii_range;
    match name {
        "alnum" => r(b'0', b'9') | r(b'A', b'Z') | r(b'a', b'z'),
        "alpha" => r(b'A', b'Z') | r(b'a', b'z'),
        "ascii" => r(0, 0x7F),
        "blank" => r(b'\t', b'\t') | r(b' ', b' '),
        "cntrl" => r(0, 0x1F) | r(0x7F, 0x7F),
        "digit" => r(b'0', b'9'),
        "graph" => r(b'!', b'~'),
        "lower" => r(b'a', b'z'),
        "print" => r(b' ', b'~'),
        "punct" => r(b'!', b'/') | r(b':', b'@') | r(b'[', b'`') | r(b'{', b'~'),
        "space" => r(b'\t', b'\r') | r(b' ', b' '),
        "upper" => r(b'A', b'Z'),
        "word" => r(b'0', b'9') | r(b'A', b'Z') | r(b'a', b'z') | r(b'_', b'_'),
        _ => r(b'0', b'9') | r(b'A', b'F') | r(b'a', b'f'),
    }
}

fn perl_set(kind: PerlKind, negated: bool, unicode: bool) -> CSet {
    let ascii = match kind {
        PerlKind::Digit => ascii_class("digit"),
        PerlKind::Space => ascii_class("space"),
        PerlKind::Word => ascii_class("word"),
    };
    let mut set = CSet {
        ascii,
        high: if unicode {
            High::Unknown
        } else {
            High::Bytes(0)
        },
    };
    if negated {
        set.negate();
    }
    set
}

fn assertion(kind: AssertKind, flags: u8) -> Look {
    let multi_line = flags & FLAG_M != 0;
    let crlf = flags & FLAG_R != 0;
    match kind {
        AssertKind::StartLine if multi_line => {
            if crlf {
                Look::StartCrlf
            } else {
                Look::StartLf
            }
        }
        AssertKind::EndLine if multi_line => {
            if crlf {
                Look::EndCrlf
            } else {
                Look::EndLf
            }
        }
        AssertKind::StartLine | AssertKind::StartText => Look::Start,
        AssertKind::EndLine | AssertKind::EndText => Look::End,
        _ => Look::Word(0),
    }
}

pub(super) fn ban_check(hir: &Hir, byte: u8) -> bool {
    match hir {
        Hir::Empty | Hir::Look(_) => false,
        Hir::Literal(bytes) => bytes.contains(&byte),
        Hir::Class(set) => {
            let count = set
                .high_count()
                .map(|h| u64::from(set.ascii.count_ones()) + h);
            count == Some(1) && set.contains_ascii(byte)
        }
        Hir::Repetition { sub, .. } | Hir::Capture(sub) => ban_check(sub, byte),
        Hir::Concat(xs) | Hir::Alternation(xs) => xs.iter().any(|x| ban_check(x, byte)),
    }
}

pub(super) fn strip(hir: Hir, byte: u8) -> Result<Hir, u8> {
    Ok(match hir {
        Hir::Empty => Hir::Empty,
        Hir::Literal(bytes) => {
            if bytes.contains(&byte) {
                return Err(byte);
            }
            Hir::literal(bytes)
        }
        Hir::Class(mut set) => {
            if set.is_empty_exact() {
                return Ok(Hir::Class(set));
            }
            set.remove_ascii(byte);
            if set.is_empty_exact() {
                return Err(byte);
            }
            Hir::class(set)
        }
        Hir::Look(look) => Hir::Look(look),
        Hir::Repetition { min, max, sub } => Hir::repetition(min, max, strip(*sub, byte)?),
        Hir::Capture(sub) => Hir::Capture(Box::new(strip(*sub, byte)?)),
        Hir::Concat(xs) => Hir::concat(
            xs.into_iter()
                .map(|x| strip(x, byte))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Hir::Alternation(xs) => Hir::alternation(
            xs.into_iter()
                .map(|x| strip(x, byte))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    })
}

pub(super) fn has_anchor_haystack(hir: &Hir) -> bool {
    match hir {
        Hir::Look(Look::Start | Look::End) => true,
        Hir::Empty | Hir::Literal(_) | Hir::Class(_) | Hir::Look(_) => false,
        Hir::Repetition { sub, .. } | Hir::Capture(sub) => has_anchor_haystack(sub),
        Hir::Concat(xs) | Hir::Alternation(xs) => xs.iter().any(has_anchor_haystack),
    }
}

pub(super) fn non_matching_bytes(hir: &Hir) -> ByteSet {
    let mut set = ByteSet::full();
    remove_matching(hir, &mut set);
    set
}

fn remove_matching(hir: &Hir, set: &mut ByteSet) {
    match hir {
        Hir::Empty | Hir::Look(Look::Word(_)) => {}
        Hir::Look(Look::Start | Look::End | Look::StartLf | Look::EndLf) => set.remove(b'\n'),
        Hir::Look(Look::StartCrlf | Look::EndCrlf) => {
            set.remove(b'\r');
            set.remove(b'\n');
        }
        Hir::Literal(bytes) => {
            for &b in bytes {
                set.remove(b);
            }
        }
        Hir::Class(cls) => cls.add_matching_bytes(set),
        Hir::Repetition { sub, .. } | Hir::Capture(sub) => remove_matching(sub, set),
        Hir::Concat(xs) | Hir::Alternation(xs) => {
            for x in xs {
                remove_matching(x, set);
            }
        }
    }
}

pub(super) fn apply_edits(pattern: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|e| e.span.0);
    let mut out = String::with_capacity(pattern.len() + edits.len() * 16);
    let mut last = 0;
    for edit in edits {
        if edit.span.0 < last {
            continue;
        }
        out.push_str(&pattern[last..edit.span.0]);
        out.push_str(&edit.text);
        last = edit.span.1;
    }
    out.push_str(&pattern[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::super::ast::{FLAG_M, FLAG_U, Parser};
    use super::*;

    fn hir(pattern: &str, flags: u8) -> Hir {
        let ast = Parser::new(pattern, false, false).parse().ok().unwrap();
        Translator::new(pattern, vec![]).translate(&ast, flags)
    }

    fn sparse(set: &ByteSet) -> Vec<u8> {
        (0..=255u8).filter(|&b| set.contains(b)).collect()
    }

    fn sparse_except(except: &[u8]) -> Vec<u8> {
        (0..=255u8).filter(|b| !except.contains(b)).collect()
    }

    fn extract(pattern: &str) -> ByteSet {
        non_matching_bytes(&hir(pattern, FLAG_U))
    }

    #[test]
    fn non_matching_dot() {
        assert_eq!(
            sparse(&extract(".")),
            vec![
                b'\n', 192, 193, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255
            ]
        );
        assert_eq!(
            sparse(&extract("(?s).")),
            vec![
                192, 193, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255
            ]
        );
        assert_eq!(sparse(&extract("(?-u).")), vec![b'\n']);
        assert_eq!(sparse(&extract("(?s-u).")), Vec::<u8>::new());
    }

    #[test]
    fn non_matching_literal() {
        assert_eq!(sparse(&extract("a")), sparse_except(b"a"));
        assert_eq!(
            sparse(&extract("\u{2603}")),
            sparse_except(&[0xE2, 0x98, 0x83])
        );
        assert_eq!(sparse(&extract(r"\xFF")), sparse_except(&[0xC3, 0xBF]));
        assert_eq!(sparse(&extract(r"(?-u)\xFF")), sparse_except(&[0xFF]));
    }

    #[test]
    fn non_matching_anchor() {
        for p in [r"^", r"$", r"\A", r"\z", r"(?m)^", r"(?m)$"] {
            assert_eq!(sparse(&extract(p)), sparse_except(b"\n"), "{p}");
        }
    }

    fn ban(pattern: &str, byte: u8) -> bool {
        ban_check(&hir(pattern, FLAG_U), byte)
    }

    #[test]
    fn ban_various() {
        for p in [
            r"\x00",
            r"a\x00",
            r"\x00b",
            r"a\x00b",
            r"\x00|ab",
            r"ab|\x00",
            r"\x00?",
            r"(\x00)",
            r"[\x00]",
            r"[^[^\x00]]",
        ] {
            assert!(ban(p, 0), "{p}");
        }
        assert!(!ban(r"[^\x00]", 0));
        assert!(!ban(r"[\x00a]", 0));
    }

    fn strip_err(pattern: &str, terms: &[u8]) -> bool {
        let mut h = hir(pattern, FLAG_U | FLAG_M);
        for &t in terms {
            match strip(h, t) {
                Ok(x) => h = x,
                Err(_) => return true,
            }
        }
        false
    }

    #[test]
    fn strip_various() {
        assert!(!strip_err(r"[a\n]", b"\n"));
        assert!(!strip_err(r"[a\n]", b"a"));
        assert!(!strip_err(r"[a\n]", b"\r\n"));
        assert!(!strip_err(r"[a\r]", b"\r\n"));
        assert!(!strip_err(r"[a\r\n]", b"\r\n"));
        assert!(!strip_err(r"(?-u)\s", b"a"));
        assert!(!strip_err(r"(?-u)\s", b"\n"));
        for p in [
            r"\n",
            r"abc\n",
            r"\nabc",
            r"abc\nxyz",
            r"\x0A",
            r"\u000A",
            r"\U0000000A",
            r"\u{A}",
            "\n",
        ] {
            assert!(strip_err(p, b"\n"), "{p:?}");
        }
        assert!(!strip_err(r"a|\n", b"\n"));
        assert!(strip_err(r"[\r\n]", b"\r\n"));
        assert!(!strip_err(r"\n{0}", b"\n"));
    }
}
