use super::super::locale::{self, INVALID_BASE, MAX_CHAR};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitSet {
    ranges: Vec<(u32, u32)>,
    alt: bool,
}

impl UnitSet {
    #[must_use]
    pub fn single(c: u32) -> Self {
        Self {
            ranges: vec![(c, c)],
            alt: false,
        }
    }

    #[must_use]
    pub fn from_ranges(mut ranges: Vec<(u32, u32)>) -> Self {
        ranges.retain(|&(lo, hi)| lo <= hi);
        ranges.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (lo, hi) in ranges {
            match out.last_mut() {
                Some(last) if lo <= last.1.saturating_add(1) => last.1 = last.1.max(hi),
                _ => out.push((lo, hi)),
            }
        }
        Self {
            ranges: out,
            alt: false,
        }
    }

    #[must_use]
    pub fn from_units(units: &[u32]) -> Self {
        Self::from_ranges(units.iter().map(|&c| (c, c)).collect())
    }

    #[must_use]
    pub fn universe(utf8: bool) -> Self {
        if utf8 {
            Self {
                ranges: vec![(0, MAX_CHAR)],
                alt: false,
            }
        } else {
            Self {
                ranges: vec![(0, 0xff)],
                alt: false,
            }
        }
    }

    #[must_use]
    pub fn with_alt(mut self, alt: bool) -> Self {
        self.alt = alt;
        self
    }

    #[must_use]
    pub fn alt(&self) -> bool {
        self.alt
    }

    #[must_use]
    pub fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    #[must_use]
    pub fn contains(&self, c: u32) -> bool {
        locale::in_ranges(&self.ranges, c)
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut v = self.ranges.clone();
        v.extend_from_slice(&other.ranges);
        Self::from_ranges(v)
    }

    #[must_use]
    pub fn complement_in(&self, universe: &Self) -> Self {
        let mut out = Vec::new();
        for &(ulo, uhi) in &universe.ranges {
            let mut next = ulo;
            for &(lo, hi) in &self.ranges {
                if hi < next || lo > uhi {
                    continue;
                }
                if lo > next {
                    out.push((next, lo - 1));
                }
                next = hi.saturating_add(1);
                if next > uhi {
                    break;
                }
            }
            if next <= uhi {
                out.push((next, uhi));
            }
        }
        Self::from_ranges(out)
    }

    #[must_use]
    pub fn negate(&self, utf8: bool) -> Self {
        self.complement_in(&Self::universe(utf8))
    }

    #[must_use]
    pub fn without(&self, c: u32) -> Self {
        Self::single(c).complement_in(self)
    }

    #[must_use]
    pub fn has_invalid_units(&self) -> bool {
        self.ranges
            .last()
            .is_some_and(|&(_, hi)| hi >= INVALID_BASE)
    }

    #[must_use]
    pub fn upper_preimage(&self, utf8: bool) -> Self {
        let mut add = Vec::new();
        let mut remove = Vec::new();
        for (c, u) in locale::upper_changing(utf8) {
            if self.contains(u) {
                add.push((c, c));
            } else if self.contains(c) {
                remove.push((c, c));
            }
        }
        let base = if remove.is_empty() {
            self.clone()
        } else {
            Self::from_ranges(remove).complement_in(self)
        };
        if add.is_empty() {
            base
        } else {
            base.union(&Self::from_ranges(add))
        }
    }

    #[must_use]
    pub fn dfa_fold(&self, utf8: bool) -> Self {
        let mut v = self.ranges.clone();
        for &(lo, hi) in &self.ranges {
            for c in lo..=hi.min(lo.saturating_add(0x1_0000)) {
                v.extend(dfa_fold_char(utf8, c).into_iter().map(|f| (f, f)));
            }
        }
        Self::from_ranges(v)
    }
}

#[must_use]
pub fn dfa_fold_char(utf8: bool, c: u32) -> Vec<u32> {
    if utf8 {
        let mut v = vec![c];
        v.extend(locale::case_folded_counterparts(true, c));
        v
    } else {
        let u = locale::to_upper(false, c);
        (0..=0xffu32)
            .filter(|&d| d == c || locale::to_upper(false, d) == u)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    LineStart,
    LineEnd,
    BufStart,
    BufEnd,
    WordStart,
    WordEnd,
    WordBoundary,
    NotWordBoundary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Empty,
    Set(UnitSet),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
    Group {
        node: Box<Node>,
        index: usize,
    },
    Backref(usize),
    Look(Look),
    Unsupported,
}

impl Node {
    #[must_use]
    pub fn concat(mut nodes: Vec<Node>) -> Node {
        nodes.retain(|n| *n != Node::Empty);
        match nodes.len() {
            0 => Node::Empty,
            1 => nodes.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(nodes),
        }
    }

    #[must_use]
    pub fn alt(mut nodes: Vec<Node>) -> Node {
        if nodes.len() == 1 {
            nodes.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alt(nodes)
        }
    }

    pub fn walk(&self, f: &mut dyn FnMut(&Node)) {
        f(self);
        match self {
            Node::Concat(v) | Node::Alt(v) => v.iter().for_each(|n| n.walk(f)),
            Node::Repeat { node, .. } | Node::Group { node, .. } => node.walk(f),
            _ => {}
        }
    }

    #[must_use]
    pub fn any(&self, pred: &dyn Fn(&Node) -> bool) -> bool {
        let mut found = false;
        self.walk(&mut |n| found |= pred(n));
        found
    }

    #[must_use]
    pub fn has_backref(&self) -> bool {
        self.any(&|n| matches!(n, Node::Backref(_)))
    }

    #[must_use]
    pub fn has_word_look(&self) -> bool {
        self.any(&|n| {
            matches!(
                n,
                Node::Look(
                    Look::WordStart | Look::WordEnd | Look::WordBoundary | Look::NotWordBoundary
                )
            )
        })
    }

    #[must_use]
    pub fn has_invalid_units(&self) -> bool {
        self.any(&|n| matches!(n, Node::Set(s) if s.has_invalid_units()))
    }

    #[must_use]
    pub fn max_group(&self) -> usize {
        let mut m = 0;
        self.walk(&mut |n| {
            if let Node::Group { index, .. } = n {
                m = m.max(*index);
            }
        });
        m
    }

    #[must_use]
    pub fn shift_groups(self, by: usize) -> Node {
        if by == 0 {
            return self;
        }
        match self {
            Node::Concat(v) => Node::Concat(v.into_iter().map(|n| n.shift_groups(by)).collect()),
            Node::Alt(v) => Node::Alt(v.into_iter().map(|n| n.shift_groups(by)).collect()),
            Node::Repeat { node, min, max } => Node::Repeat {
                node: Box::new(node.shift_groups(by)),
                min,
                max,
            },
            Node::Group { node, index } => Node::Group {
                node: Box::new(node.shift_groups(by)),
                index: index + by,
            },
            Node::Backref(i) => Node::Backref(i + by),
            other => other,
        }
    }

    #[must_use]
    pub fn find_group(&self, index: usize) -> Option<&Node> {
        match self {
            Node::Group { node, index: i } if *i == index => Some(node),
            Node::Concat(v) | Node::Alt(v) => v.iter().find_map(|n| n.find_group(index)),
            Node::Repeat { node, .. } | Node::Group { node, .. } => node.find_group(index),
            _ => None,
        }
    }

    #[must_use]
    pub fn can_be_empty(&self) -> bool {
        match self {
            Node::Empty | Node::Look(_) | Node::Backref(_) => true,
            Node::Set(_) | Node::Unsupported => false,
            Node::Concat(v) => v.iter().all(Node::can_be_empty),
            Node::Alt(v) => v.iter().any(Node::can_be_empty),
            Node::Repeat { node, min, .. } => *min == 0 || node.can_be_empty(),
            Node::Group { node, .. } => node.can_be_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_algebra() {
        let s = UnitSet::from_ranges(vec![(5, 10), (1, 3), (4, 4), (20, 30)]);
        assert_eq!(s.ranges(), &[(1, 10), (20, 30)]);
        let n = s.negate(false);
        assert_eq!(n.ranges(), &[(0, 0), (11, 19), (31, 255)]);
        assert_eq!(s.without(5).ranges(), &[(1, 4), (6, 10), (20, 30)]);
        let up = UnitSet::single(u32::from(b'S')).upper_preimage(true);
        assert!(up.contains(u32::from(b's')) && up.contains(0x17f));
        let raw = UnitSet::single(u32::from(b'a')).upper_preimage(true);
        assert_eq!(raw.ranges(), &[] as &[(u32, u32)]);
        let folded = UnitSet::single(u32::from(b'k')).dfa_fold(true);
        assert!(folded.contains(u32::from(b'K')));
    }
}
