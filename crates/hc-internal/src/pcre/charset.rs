use super::ucd;

#[derive(Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct CharSet {
    ranges: Vec<(u32, u32)>,
}

impl CharSet {
    pub fn new() -> CharSet {
        CharSet { ranges: Vec::new() }
    }

    pub fn from_ranges(mut v: Vec<(u32, u32)>) -> CharSet {
        v.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        for (a, b) in v {
            if let Some(last) = out.last_mut()
                && last.1.saturating_add(1) >= a
            {
                if b > last.1 {
                    last.1 = b;
                }
                continue;
            }
            out.push((a, b));
        }
        CharSet { ranges: out }
    }

    pub fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn len(&self) -> u64 {
        self.ranges.iter().map(|r| u64::from(r.1 - r.0) + 1).sum()
    }

    pub fn single_char(&self) -> Option<u32> {
        if self.ranges.len() == 1 && self.ranges[0].0 == self.ranges[0].1 {
            Some(self.ranges[0].0)
        } else {
            None
        }
    }

    pub fn union(&self, other: &CharSet) -> CharSet {
        let mut v = Vec::with_capacity(self.ranges.len() + other.ranges.len());
        v.extend_from_slice(&self.ranges);
        v.extend_from_slice(&other.ranges);
        CharSet::from_ranges(v)
    }

    pub fn negate(&self, max: u32) -> CharSet {
        let mut out = Vec::with_capacity(self.ranges.len() + 1);
        let mut next = 0u32;
        let mut done = false;
        for &(a, b) in &self.ranges {
            if a > max {
                break;
            }
            if a > next {
                out.push((next, a - 1));
            }
            if b >= max {
                done = true;
                break;
            }
            next = b + 1;
        }
        if !done && next <= max {
            out.push((next, max));
        }
        CharSet { ranges: out }
    }

    pub fn clamp(&self, max: u32) -> CharSet {
        let mut out = Vec::with_capacity(self.ranges.len());
        for &(a, b) in &self.ranges {
            if a > max {
                break;
            }
            out.push((a, b.min(max)));
        }
        CharSet { ranges: out }
    }

    pub fn intersect(&self, other: &CharSet) -> CharSet {
        let mut out = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.ranges.len() && j < other.ranges.len() {
            let (a1, b1) = self.ranges[i];
            let (a2, b2) = other.ranges[j];
            let lo = a1.max(a2);
            let hi = b1.min(b2);
            if lo <= hi {
                out.push((lo, hi));
            }
            if b1 < b2 {
                i += 1;
            } else {
                j += 1;
            }
        }
        CharSet { ranges: out }
    }

    pub fn subtract(&self, other: &CharSet, max: u32) -> CharSet {
        self.intersect(&other.negate(max))
    }

    pub fn xor(&self, other: &CharSet, max: u32) -> CharSet {
        self.subtract(other, max).union(&other.subtract(self, max))
    }

    pub fn case_closure(&self, mode: CaseMode) -> CharSet {
        let mut extra: Vec<(u32, u32)> = Vec::new();
        for &(a, b) in &self.ranges {
            for &(c, set_id) in ucd::case_map_range(a, b) {
                for &o in ucd::case_set_by_id(set_id) {
                    if mode.allows(c, o) {
                        extra.push((o, o));
                    }
                }
            }
            if mode.turkish {
                for c in [0x49u32, 0x69, 0x130, 0x131] {
                    if c >= a && c <= b {
                        let partner = match c {
                            0x69 => 0x130,
                            0x130 => 0x69,
                            0x49 => 0x131,
                            _ => 0x49,
                        };
                        extra.push((partner, partner));
                    }
                }
            }
        }
        if extra.is_empty() {
            return self.clone();
        }
        extra.extend_from_slice(&self.ranges);
        CharSet::from_ranges(extra).clamp(mode.max)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseMode {
    pub unicode: bool,
    pub restrict: bool,
    pub turkish: bool,
    pub max: u32,
}

impl CaseMode {
    fn allows(self, c: u32, o: u32) -> bool {
        if o > self.max {
            return false;
        }
        if !self.unicode && (c >= 128 || o >= 128) {
            return false;
        }
        if self.restrict && ((c < 128) != (o < 128)) {
            return false;
        }
        if self.turkish && is_turkish_i(c) && is_turkish_i(o) {
            return false;
        }
        true
    }
}

pub fn is_turkish_i(c: u32) -> bool {
    matches!(c, 0x49 | 0x69 | 0x130 | 0x131)
}

pub fn caseless_equivalents(c: u32, mode: CaseMode) -> Vec<u32> {
    let mut out = vec![c];
    if mode.turkish && is_turkish_i(c) {
        let p = match c {
            0x69 => 0x130,
            0x130 => 0x69,
            0x49 => 0x131,
            _ => 0x49,
        };
        if p <= mode.max {
            out.push(p);
        }
        out.sort_unstable();
        return out;
    }
    if let Some(set) = ucd::case_set(c) {
        for &o in set {
            if o != c && mode.allows(c, o) {
                out.push(o);
            }
        }
    }
    out.sort_unstable();
    out
}
