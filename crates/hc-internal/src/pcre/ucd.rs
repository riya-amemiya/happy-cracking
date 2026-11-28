use super::ucd_tables as t;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PropKind {
    Sc,
    Scx,
    Pc,
    Gc,
    Bidi,
    Bool,
    Any,
    Lamp,
    Alnum,
    PxSpace,
    Space,
    Ucnc,
    Word,
    PxGraph,
    PxPrint,
    PxPunct,
    PxXdigit,
}

pub const CC: u8 = 0;
pub const CF: u8 = 1;
pub const CN: u8 = 2;
pub const LL: u8 = 5;
pub const LT: u8 = 8;
pub const LU: u8 = 9;
pub const MN: u8 = 12;
pub const ND: u8 = 13;
pub const PC: u8 = 16;
pub const ZL: u8 = 27;
pub const ZP: u8 = 28;

pub const GB_LF: u8 = 1;
pub const GB_EXTEND: u8 = 3;
pub const GB_PREPEND: u8 = 4;
pub const GB_SPACING_MARK: u8 = 5;
pub const GB_L: u8 = 6;
pub const GB_V: u8 = 7;
pub const GB_T: u8 = 8;
pub const GB_LV: u8 = 9;
pub const GB_LVT: u8 = 10;
pub const GB_RI: u8 = 11;
pub const GB_OTHER: u8 = 12;
pub const GB_ZWJ: u8 = 13;
pub const GB_EXT_PICT: u8 = 14;

const fn gb_bit(v: u8) -> u32 {
    1u32 << v
}

const GB_ESZ: u32 = gb_bit(GB_EXTEND) | gb_bit(GB_SPACING_MARK) | gb_bit(GB_ZWJ);

pub static GB_TABLE: [u32; 15] = [
    gb_bit(GB_LF),
    0,
    0,
    GB_ESZ,
    GB_ESZ
        | gb_bit(GB_PREPEND)
        | gb_bit(GB_L)
        | gb_bit(GB_V)
        | gb_bit(GB_T)
        | gb_bit(GB_LV)
        | gb_bit(GB_LVT)
        | gb_bit(GB_OTHER)
        | gb_bit(GB_RI),
    GB_ESZ,
    GB_ESZ | gb_bit(GB_L) | gb_bit(GB_V) | gb_bit(GB_LV) | gb_bit(GB_LVT),
    GB_ESZ | gb_bit(GB_V) | gb_bit(GB_T),
    GB_ESZ | gb_bit(GB_T),
    GB_ESZ | gb_bit(GB_V) | gb_bit(GB_T),
    GB_ESZ | gb_bit(GB_T),
    gb_bit(GB_RI),
    GB_ESZ,
    GB_ESZ | gb_bit(GB_EXT_PICT),
    GB_ESZ,
];

pub const MAX_UNICODE: u32 = 0x10_ffff;

fn lookup<T: Copy>(table: &[(u32, u32, T)], c: u32) -> Option<T> {
    let idx = table.partition_point(|r| r.1 < c);
    match table.get(idx) {
        Some(r) if r.0 <= c => Some(r.2),
        _ => None,
    }
}

pub fn general_category(c: u32) -> u8 {
    lookup(t::GENERAL_CATEGORY, c).unwrap_or(CN)
}

pub fn gentype(category: u8) -> u8 {
    match category {
        0..=4 => 0,
        5..=9 => 1,
        10..=12 => 2,
        13..=15 => 3,
        16..=22 => 4,
        23..=26 => 5,
        _ => 6,
    }
}

pub const GT_C: u8 = 0;
pub const GT_L: u8 = 1;
pub const GT_N: u8 = 3;
pub const GT_P: u8 = 4;
pub const GT_S: u8 = 5;
pub const GT_Z: u8 = 6;

pub fn script(c: u32) -> u8 {
    lookup(t::SCRIPT, c).unwrap_or(t::SCRIPT_UNKNOWN)
}

pub fn script_extensions(c: u32) -> &'static [u8] {
    match lookup(t::SCRIPT_EXTENSIONS, c) {
        Some(i) => t::SCRIPT_EXTENSION_LISTS[usize::from(i)],
        None => &[],
    }
}

pub fn grapheme_break(c: u32) -> u8 {
    lookup(t::GRAPHEME_BREAK, c).unwrap_or(GB_OTHER)
}

pub fn case_set(c: u32) -> Option<&'static [u32]> {
    let map = t::CASE_FOLD_MAP;
    let idx = map.partition_point(|e| e.0 < c);
    match map.get(idx) {
        Some(e) if e.0 == c => Some(t::CASE_FOLD_SETS[usize::from(e.1)]),
        _ => None,
    }
}

pub fn case_map_range(lo: u32, hi: u32) -> &'static [(u32, u16)] {
    let map = t::CASE_FOLD_MAP;
    let a = map.partition_point(|e| e.0 < lo);
    let b = map.partition_point(|e| e.0 <= hi);
    &map[a..b]
}

pub fn case_set_by_id(id: u16) -> &'static [u32] {
    t::CASE_FOLD_SETS[usize::from(id)]
}

pub fn is_word_ucp(c: u32) -> bool {
    let cat = general_category(c);
    let g = gentype(cat);
    g == GT_L || g == GT_N || cat == MN || cat == PC
}

pub fn lookup_property_name(name: &[u8]) -> Option<(PropKind, u16)> {
    let names = t::PROPERTY_NAMES;
    let idx = names.partition_point(|e| e.0.as_bytes() < name);
    match names.get(idx) {
        Some(e) if e.0.as_bytes() == name => Some((e.1, e.2)),
        _ => None,
    }
}

fn push_range(out: &mut Vec<(u32, u32)>, a: u32, b: u32) {
    if let Some(last) = out.last_mut()
        && last.1.wrapping_add(1) >= a
    {
        if b > last.1 {
            last.1 = b;
        }
        return;
    }
    out.push((a, b));
}

fn filter_table<T: Copy>(table: &[(u32, u32, T)], mut f: impl FnMut(T) -> bool) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for &(a, b, v) in table {
        if f(v) {
            push_range(&mut out, a, b);
        }
    }
    out
}

fn category_filter(f: impl Fn(u8) -> bool) -> Vec<(u32, u32)> {
    filter_table(t::GENERAL_CATEGORY, f)
}

fn hspace_ranges() -> Vec<(u32, u32)> {
    vec![
        (0x09, 0x09),
        (0x20, 0x20),
        (0xa0, 0xa0),
        (0x1680, 0x1680),
        (0x180e, 0x180e),
        (0x2000, 0x200a),
        (0x202f, 0x202f),
        (0x205f, 0x205f),
        (0x3000, 0x3000),
    ]
}

fn vspace_ranges() -> Vec<(u32, u32)> {
    vec![(0x0a, 0x0d), (0x85, 0x85), (0x2028, 0x2029)]
}

pub fn hspace() -> Vec<(u32, u32)> {
    hspace_ranges()
}

pub fn vspace() -> Vec<(u32, u32)> {
    vspace_ranges()
}

fn merge(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    v.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
    for (a, b) in v {
        push_range(&mut out, a, b);
    }
    out
}

pub fn property_ranges(kind: PropKind, value: u16) -> Vec<(u32, u32)> {
    match kind {
        PropKind::Any => vec![(0, MAX_UNICODE)],
        PropKind::Lamp => category_filter(|c| c == LL || c == LU || c == LT),
        PropKind::Pc => category_filter(|c| u16::from(c) == value),
        PropKind::Gc => category_filter(|c| u16::from(gentype(c)) == value),
        PropKind::Sc => filter_table(t::SCRIPT, |s| u16::from(s) == value),
        PropKind::Scx => {
            let mut v = filter_table(t::SCRIPT, |s| u16::from(s) == value);
            for &(a, b, idx) in t::SCRIPT_EXTENSIONS {
                if t::SCRIPT_EXTENSION_LISTS[usize::from(idx)].contains(&(value as u8)) {
                    v.push((a, b));
                }
            }
            merge(v)
        }
        PropKind::Bidi => filter_table(t::BIDI_CLASS, |b| u16::from(b) == value),
        PropKind::Bool => t::BOOL_PROPERTIES[usize::from(value)].to_vec(),
        PropKind::Alnum => category_filter(|c| {
            let g = gentype(c);
            g == GT_L || g == GT_N
        }),
        PropKind::Space | PropKind::PxSpace => {
            let mut v = category_filter(|c| gentype(c) == GT_Z);
            v.extend(hspace_ranges());
            v.extend(vspace_ranges());
            merge(v)
        }
        PropKind::Word => category_filter(|c| {
            let g = gentype(c);
            g == GT_L || g == GT_N || c == MN || c == PC
        }),
        PropKind::Ucnc => vec![
            (0x24, 0x24),
            (0x40, 0x40),
            (0x60, 0x60),
            (0xa0, 0xd7ff),
            (0xe000, MAX_UNICODE),
        ],
        PropKind::PxGraph => {
            let v = category_filter(|c| {
                let g = gentype(c);
                g != GT_Z && (g != GT_C || c == CF)
            });
            subtract_points(v, &[(0x61c, 0x61c), (0x180e, 0x180e), (0x2066, 0x2069)])
        }
        PropKind::PxPrint => {
            let v = category_filter(|c| c != ZL && c != ZP && (gentype(c) != GT_C || c == CF));
            subtract_points(v, &[(0x61c, 0x61c), (0x2066, 0x2069)])
        }
        PropKind::PxPunct => {
            let mut v = category_filter(|c| gentype(c) == GT_P);
            for c in 0u32..128 {
                if gentype(general_category(c)) == GT_S {
                    v.push((c, c));
                }
            }
            merge(v)
        }
        PropKind::PxXdigit => vec![
            (0x30, 0x39),
            (0x41, 0x46),
            (0x61, 0x66),
            (0xff10, 0xff19),
            (0xff21, 0xff26),
            (0xff41, 0xff46),
        ],
    }
}

fn subtract_points(v: Vec<(u32, u32)>, remove: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(v.len() + remove.len());
    for (mut a, b) in v {
        for &(ra, rb) in remove {
            if rb < a || ra > b {
                continue;
            }
            if ra > a {
                out.push((a, ra - 1));
            }
            a = rb.saturating_add(1);
            if a > b {
                break;
            }
        }
        if a <= b {
            out.push((a, b));
        }
    }
    merge(out)
}

pub fn digit_set_zero(c: u32) -> Option<u32> {
    let table = t::GENERAL_CATEGORY;
    let idx = table.partition_point(|r| r.1 < c);
    let r = table.get(idx)?;
    if r.0 > c || r.2 != ND {
        return None;
    }
    Some(r.0 + (c - r.0) / 10 * 10)
}
