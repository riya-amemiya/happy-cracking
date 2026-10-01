use super::super::locale;
use super::ast::{Node, UnitSet};

const MAX_LITS: usize = 64;
const MAX_LEN: usize = 16;
const MAX_SET: u32 = 16;

#[derive(Clone, Debug)]
struct Lits {
    lits: Vec<Vec<u8>>,
    exact: bool,
}

fn set_units(s: &UnitSet, utf8: bool, eol: u8) -> Option<Vec<Vec<u8>>> {
    let mut count = 0u32;
    for &(lo, hi) in s.ranges() {
        count = count.saturating_add(hi - lo + 1);
        if count > MAX_SET {
            return None;
        }
    }
    let mut out = Vec::new();
    for &(lo, hi) in s.ranges() {
        for c in lo..=hi {
            if c >= locale::INVALID_BASE || c == u32::from(eol) {
                return None;
            }
            let mut b = Vec::new();
            locale::encode(utf8, c, &mut b);
            out.push(b);
        }
    }
    (!out.is_empty()).then_some(out)
}

fn cross(a: &Lits, b: &Lits) -> Lits {
    let mut lits = Vec::new();
    let mut exact = a.exact && b.exact;
    for x in &a.lits {
        for y in &b.lits {
            let mut v = x.clone();
            v.extend_from_slice(y);
            if v.len() > MAX_LEN {
                v.truncate(MAX_LEN);
                exact = false;
            }
            lits.push(v);
        }
    }
    lits.sort();
    lits.dedup();
    if lits.len() > MAX_LITS {
        return Lits {
            lits: a.lits.clone(),
            exact: false,
        };
    }
    Lits { lits, exact }
}

fn prefixes(node: &Node, utf8: bool, eol: u8) -> Option<Lits> {
    match node {
        Node::Empty | Node::Look(_) => Some(Lits {
            lits: vec![Vec::new()],
            exact: true,
        }),
        Node::Set(s) => set_units(s, utf8, eol).map(|lits| Lits { lits, exact: true }),
        Node::Group { node, .. } => prefixes(node, utf8, eol),
        Node::Concat(v) => {
            let mut acc = Lits {
                lits: vec![Vec::new()],
                exact: true,
            };
            for child in v {
                if !acc.exact {
                    break;
                }
                if let Some(c) = prefixes(child, utf8, eol) {
                    acc = cross(&acc, &c);
                } else {
                    acc.exact = false;
                    break;
                }
            }
            Some(acc)
        }
        Node::Alt(v) => {
            let mut lits = Vec::new();
            let mut exact = true;
            for child in v {
                let c = prefixes(child, utf8, eol)?;
                exact &= c.exact;
                lits.extend(c.lits);
            }
            lits.sort();
            lits.dedup();
            (lits.len() <= MAX_LITS).then_some(Lits { lits, exact })
        }
        Node::Repeat { node, min, max } => {
            if *min == 0 {
                return None;
            }
            let mut c = prefixes(node, utf8, eol)?;
            if !(*min == 1 && *max == Some(1)) {
                c.exact = false;
            }
            Some(c)
        }
        Node::Backref(_) | Node::Unsupported => None,
    }
}

#[must_use]
pub fn good_prefix(node: &Node, utf8: bool, eol: u8) -> bool {
    let Some(p) = prefixes(node, utf8, eol) else {
        return false;
    };
    let min = p.lits.iter().map(Vec::len).min().unwrap_or(0);
    min >= 2 || (min == 1 && p.lits.len() <= 3)
}

fn score(lits: &[Vec<u8>]) -> usize {
    let min = lits.iter().map(Vec::len).min().unwrap_or(0);
    if lits.is_empty() || min == 0 {
        return 0;
    }
    min * 64 - lits.len().min(63)
}

fn required(node: &Node, utf8: bool, eol: u8) -> Option<Vec<Vec<u8>>> {
    match node {
        Node::Set(s) => set_units(s, utf8, eol),
        Node::Group { node, .. } => required(node, utf8, eol),
        Node::Repeat { node, min, .. } if *min >= 1 => required(node, utf8, eol),
        Node::Alt(v) => {
            let mut all = Vec::new();
            for child in v {
                all.extend(required(child, utf8, eol)?);
            }
            all.sort();
            all.dedup();
            (all.len() <= MAX_LITS).then_some(all)
        }
        Node::Concat(v) => {
            let mut best: Option<Vec<Vec<u8>>> = None;
            let consider = |cand: Vec<Vec<u8>>, best: &mut Option<Vec<Vec<u8>>>| {
                if best.as_ref().is_none_or(|b| score(&cand) > score(b)) {
                    *best = Some(cand);
                }
            };
            let mut run: Option<Lits> = None;
            for child in v {
                let exact_lits = match child {
                    Node::Set(s) => set_units(s, utf8, eol).map(|lits| Lits { lits, exact: true }),
                    Node::Group { node, .. } => prefixes(node, utf8, eol).filter(|l| l.exact),
                    _ => None,
                };
                if let Some(l) = exact_lits {
                    run = Some(match run.take() {
                        None => l,
                        Some(r) => {
                            let c = cross(&r, &l);
                            if c.exact { c } else { r }
                        }
                    });
                } else {
                    if let Some(r) = run.take() {
                        consider(r.lits, &mut best);
                    }
                    if let Some(c) = required(child, utf8, eol) {
                        consider(c, &mut best);
                    }
                }
            }
            if let Some(r) = run.take() {
                consider(r.lits, &mut best);
            }
            best
        }
        _ => None,
    }
}

#[must_use]
pub fn required_literals(node: &Node, utf8: bool, eol: u8) -> Option<Vec<Vec<u8>>> {
    let lits = required(node, utf8, eol)?;
    let min = lits.iter().map(Vec::len).min().unwrap_or(0);
    (min >= 2 && lits.len() <= MAX_LITS).then_some(lits)
}

#[cfg(test)]
mod tests {
    use super::super::syntax;
    use super::*;

    fn node(p: &str) -> Node {
        syntax::parse(
            p.as_bytes(),
            syntax::Syntax {
                extended: true,
                icase: false,
                utf8: false,
            },
        )
        .unwrap()
        .node
    }

    #[test]
    fn literal_analysis() {
        assert!(good_prefix(&node("error"), false, b'\n'));
        assert!(!good_prefix(&node("[A-Z][a-z]+_"), false, b'\n'));
        assert!(good_prefix(&node("^#include"), false, b'\n'));
        assert_eq!(
            required_literals(&node("\\w+_t\\b"), false, b'\n'),
            Some(vec![b"_t".to_vec()])
        );
        assert_eq!(
            required_literals(&node("[A-Z][a-z]+_[a-z]+"), false, b'\n'),
            None
        );
        assert_eq!(
            required_literals(&node("x*(foo|bar)y*"), false, b'\n'),
            Some(vec![b"bar".to_vec(), b"foo".to_vec()])
        );
    }
}
