use std::fmt::Write as _;

use super::super::locale::{INVALID_BASE, MAX_CHAR};
use super::ast::{Look, Node, UnitSet};

type Ranges = Vec<(u32, u32)>;

pub struct Translation {
    pub pattern: String,
    pub exact: bool,
}

#[derive(Clone, Copy)]
struct LookMode {
    ascii_words: bool,
    nl_quirk: bool,
}

struct Tr<'a> {
    root: &'a Node,
    utf8: bool,
    looks: LookMode,
    eol: u8,
    exact: bool,
    depth: usize,
}

#[must_use]
pub fn translate(node: &Node, utf8: bool, eol: u8) -> Translation {
    translate_with(node, utf8, eol, false, false)
}

#[must_use]
pub fn translate_with(
    node: &Node,
    utf8: bool,
    eol: u8,
    ascii_words: bool,
    nl_quirk: bool,
) -> Translation {
    let mut t = Tr {
        root: node,
        utf8,
        looks: LookMode {
            ascii_words,
            nl_quirk,
        },
        eol,
        exact: true,
        depth: 0,
    };
    let mut out = String::new();
    t.node(node, &mut out);
    Translation {
        pattern: out,
        exact: t.exact,
    }
}

fn push_unit(out: &mut String, utf8: bool, c: u32) {
    if c < 0x80 && (c as u8).is_ascii_alphanumeric() {
        out.push(c as u8 as char);
    } else if utf8 {
        let _ = write!(out, "\\x{{{c:X}}}");
    } else {
        let _ = write!(out, "\\x{c:02X}");
    }
}

fn push_class(out: &mut String, utf8: bool, ranges: &[(u32, u32)]) {
    out.push('[');
    for &(lo, hi) in ranges {
        push_unit(out, utf8, lo);
        if hi != lo {
            out.push('-');
            push_unit(out, utf8, hi);
        }
    }
    out.push(']');
}

impl Tr<'_> {
    fn never(&self, out: &mut String) {
        if self.utf8 {
            out.push_str("[^\\x{0}-\\x{10FFFF}]");
        } else {
            out.push_str("[^\\x00-\\xFF]");
        }
    }

    fn set(&mut self, set: &UnitSet, out: &mut String) {
        let set = set.without(u32::from(self.eol));
        let (chars, bytes): (Ranges, Ranges) = if self.utf8 {
            let chars: Vec<(u32, u32)> = set
                .ranges()
                .iter()
                .filter(|r| r.0 <= MAX_CHAR)
                .map(|&(lo, hi)| (lo, hi.min(MAX_CHAR)))
                .collect();
            let bytes: Vec<(u32, u32)> = set
                .ranges()
                .iter()
                .filter(|r| r.1 >= INVALID_BASE)
                .map(|&(lo, hi)| (lo.max(INVALID_BASE) - INVALID_BASE, hi - INVALID_BASE))
                .collect();
            (chars, bytes)
        } else {
            (set.ranges().to_vec(), Vec::new())
        };
        if chars.is_empty() && bytes.is_empty() {
            self.never(out);
            return;
        }
        if !bytes.is_empty() {
            self.exact = false;
            out.push_str("(?:");
        }
        if !chars.is_empty() {
            if let [(lo, hi)] = chars.as_slice()
                && lo == hi
            {
                push_unit(out, self.utf8, *lo);
            } else {
                push_class(out, self.utf8, &chars);
            }
        }
        if !bytes.is_empty() {
            if !chars.is_empty() {
                out.push('|');
            }
            out.push_str("(?-u:");
            push_class(out, false, &bytes);
            out.push_str("))");
        }
    }

    fn node(&mut self, node: &Node, out: &mut String) {
        match node {
            Node::Empty => out.push_str("(?:)"),
            Node::Unsupported => {
                self.exact = false;
                self.never(out);
            }
            Node::Set(s) => self.set(s, out),
            Node::Concat(v) => {
                out.push_str("(?:");
                for n in v {
                    self.node(n, out);
                }
                out.push(')');
            }
            Node::Alt(v) => {
                out.push_str("(?:");
                for (i, n) in v.iter().enumerate() {
                    if i > 0 {
                        out.push('|');
                    }
                    self.node(n, out);
                }
                out.push(')');
            }
            Node::Repeat { node, min, max } => {
                out.push_str("(?:");
                self.node(node, out);
                out.push(')');
                match (min, max) {
                    (0, None) => out.push('*'),
                    (1, None) => out.push('+'),
                    (0, Some(1)) => out.push('?'),
                    (m, None) => {
                        let _ = write!(out, "{{{m},}}");
                    }
                    (m, Some(x)) => {
                        let _ = write!(out, "{{{m},{x}}}");
                    }
                }
            }
            Node::Group { node, .. } => {
                out.push_str("(?:");
                self.node(node, out);
                out.push(')');
            }
            Node::Backref(i) => {
                self.exact = false;
                match self.root.find_group(*i) {
                    Some(g) if self.depth < 8 => {
                        self.depth += 1;
                        out.push_str("(?:");
                        self.node(g, out);
                        out.push(')');
                        self.depth -= 1;
                    }
                    _ => {
                        let any = UnitSet::universe(self.utf8);
                        out.push_str("(?:");
                        self.set(&any, out);
                        out.push_str(")*");
                    }
                }
            }
            Node::Look(l) => match l {
                Look::LineStart | Look::LineEnd if self.looks.nl_quirk => {
                    self.exact = false;
                    out.push_str("(?:)");
                }
                Look::LineStart | Look::BufStart => out.push_str("(?m:^)"),
                Look::LineEnd | Look::BufEnd => out.push_str("(?m:$)"),
                _ if self.utf8 && !self.looks.ascii_words => {
                    self.exact = false;
                    out.push_str("(?:)");
                }
                Look::WordStart => out.push_str("(?-u:\\b{start})"),
                Look::WordEnd => out.push_str("(?-u:\\b{end})"),
                Look::WordBoundary => out.push_str("(?-u:\\b)"),
                Look::NotWordBoundary => out.push_str("(?-u:\\B)"),
            },
        }
    }
}

#[must_use]
pub fn build(pattern: &str, utf8: bool, eol: u8) -> Option<regex::bytes::Regex> {
    regex::bytes::RegexBuilder::new(pattern)
        .unicode(utf8)
        .multi_line(true)
        .line_terminator(eol)
        .size_limit(1 << 28)
        .dfa_size_limit(1 << 28)
        .nest_limit(2000)
        .build()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_class_compiles() {
        let r = build("[^\\x{0}-\\x{10FFFF}]", true, b'\n').unwrap();
        assert!(!r.is_match(b"abc\xff"));
        let r = build("[^\\x00-\\xFF]", false, b'\n').unwrap();
        assert!(!r.is_match(b"abc\xff"));
        let r = build("(?:a|(?-u:[\\x80-\\xFF]))", true, b'\n').unwrap();
        assert!(r.is_match(b"\xe9"));
        let r = build("\\b{start}ab", false, b'\n').unwrap();
        assert!(r.is_match(b"x ab") && !r.is_match(b"xab"));
    }
}
