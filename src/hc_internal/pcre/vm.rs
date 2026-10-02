use super::ast::Newline;
use super::compile::LookInfo;
use super::error;
use super::prog::{AssertOp, CondTest, Inst, Item, LookKind, NONE, Prog, RepMode, VerbOp};
use super::ucd;

pub const UNSET: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    NoMatch,
    Commit(u32),
    Prune(u32),
    Skip(usize, u32),
    SkipArg(u32, u32),
    Then(u32, u32),
}

impl Code {
    fn recurse(self) -> Option<u32> {
        match self {
            Code::NoMatch => None,
            Code::Commit(r)
            | Code::Prune(r)
            | Code::Skip(_, r)
            | Code::SkipArg(_, r)
            | Code::Then(_, r) => Some(r),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Bt {
    Choice {
        pc: u32,
        pos: usize,
        trail: u32,
    },
    Alt {
        pc: u32,
        pos: usize,
        trail: u32,
        then_all: bool,
    },
    ThenBarrier {
        bound: u32,
    },
    RepGreedy {
        pc: u32,
        min_pos: usize,
        pos: usize,
        trail: u32,
        step: u8,
        want: u16,
    },
    RepLazy {
        pc: u32,
        rep_pc: u32,
        pos: usize,
        count: u32,
        trail: u32,
    },
    VRev {
        pc: u32,
        pos: usize,
        remaining: u32,
        trail: u32,
    },
    Verb {
        code: Code,
    },
    Mark {
        name: u32,
        pos: usize,
    },
    PosLook,
    NegLook {
        pc: u32,
        pos: usize,
        trail: u32,
    },
    Call {
        group: u32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct CallFrame {
    group: u32,
    rets: u32,
    ret_pc: u32,
    snap: u32,
    pos: usize,
    hwm: usize,
    bt_height: u32,
}

#[derive(Clone, Copy, Debug)]
enum Trail {
    Reg(u32, usize),
    Som(usize),
    End(usize),
    CallPush,
    CallPop(CallFrame),
    Regs(u32),
}

#[derive(Default)]
pub struct Scratch {
    bt: Vec<Bt>,
    trail: Vec<Trail>,
    regs: Vec<usize>,
    calls: Vec<CallFrame>,
    arena: Vec<usize>,
}

impl Scratch {
    pub fn new() -> Scratch {
        Scratch::default()
    }
}

pub enum Outcome {
    Match(usize, usize),
    Fail(Code),
    Error(i32),
}

pub enum Stop {
    Code(Code),
    Error(i32),
}

pub struct Vm<'a> {
    prog: &'a Prog,
    looks: &'a [LookInfo],
    hay: &'a [u8],
    end: usize,
    floor: usize,
    limit_end: usize,
    noteol: bool,
    pub notbol: bool,
    lax: bool,
    start_offset: usize,
    som: usize,
    s: &'a mut Scratch,
    steps: u64,
    limit: u64,
    depth_limit: usize,
    heap_frames: usize,
    hwm: usize,
    pub skip_arg_count: u32,
    pub ignore_skip_arg: u32,
}

#[inline]
fn is_cont(b: u8) -> bool {
    b & 0xc0 == 0x80
}

#[inline]
pub fn decode(h: &[u8], i: usize, end: usize) -> Option<(u32, usize)> {
    let b0 = h[i];
    if b0 < 0x80 {
        return Some((u32::from(b0), 1));
    }
    decode_slow(h, i, end)
}

#[inline(never)]
pub fn decode_slow(h: &[u8], i: usize, end: usize) -> Option<(u32, usize)> {
    let b0 = h[i];
    let avail = end - i;
    match b0 {
        0xc2..=0xdf => {
            if avail < 2 || !is_cont(h[i + 1]) {
                return None;
            }
            Some((
                (u32::from(b0) & 0x1f) << 6 | (u32::from(h[i + 1]) & 0x3f),
                2,
            ))
        }
        0xe0..=0xef => {
            if avail < 3 {
                return None;
            }
            let b1 = h[i + 1];
            let ok1 = match b0 {
                0xe0 => (0xa0..=0xbf).contains(&b1),
                0xed => (0x80..=0x9f).contains(&b1),
                _ => is_cont(b1),
            };
            if !ok1 || !is_cont(h[i + 2]) {
                return None;
            }
            Some((
                (u32::from(b0) & 0x0f) << 12
                    | (u32::from(b1) & 0x3f) << 6
                    | (u32::from(h[i + 2]) & 0x3f),
                3,
            ))
        }
        0xf0..=0xf4 => {
            if avail < 4 {
                return None;
            }
            let b1 = h[i + 1];
            let ok1 = match b0 {
                0xf0 => (0x90..=0xbf).contains(&b1),
                0xf4 => (0x80..=0x8f).contains(&b1),
                _ => is_cont(b1),
            };
            if !ok1 || !is_cont(h[i + 2]) || !is_cont(h[i + 3]) {
                return None;
            }
            Some((
                (u32::from(b0) & 0x07) << 18
                    | (u32::from(b1) & 0x3f) << 12
                    | (u32::from(h[i + 2]) & 0x3f) << 6
                    | (u32::from(h[i + 3]) & 0x3f),
                4,
            ))
        }
        _ => None,
    }
}

#[inline]
pub fn prev_char(h: &[u8], pos: usize, utf: bool) -> Option<(u32, usize)> {
    if pos == 0 {
        return None;
    }
    if !utf {
        return Some((u32::from(h[pos - 1]), pos - 1));
    }
    let b = h[pos - 1];
    if b < 0x80 {
        return Some((u32::from(b), pos - 1));
    }
    let mut s = pos - 1;
    while s > 0 && pos - s < 4 && is_cont(h[s]) {
        s -= 1;
    }
    match decode(h, s, pos) {
        Some((c, len)) if s + len == pos => Some((c, s)),
        _ => None,
    }
}

pub fn next_boundary(h: &[u8], pos: usize, utf: bool) -> usize {
    if utf
        && pos < h.len()
        && let Some((_, len)) = decode(h, pos, h.len())
    {
        return pos + len;
    }
    pos + 1
}

fn is_word_ascii(c: u32) -> bool {
    c < 128 && {
        let b = c as u8;
        b.is_ascii_alphanumeric() || b == b'_'
    }
}

impl<'a> Vm<'a> {
    pub fn new(
        prog: &'a Prog,
        looks: &'a [LookInfo],
        hay: &'a [u8],
        start_offset: usize,
        s: &'a mut Scratch,
    ) -> Vm<'a> {
        let limit = u64::from(prog.match_limit.min(10_000_000));
        let depth_limit = prog.depth_limit.min(10_000_000) as usize;
        let heap_frames = if prog.heap_limit == u32::MAX {
            usize::MAX
        } else {
            let frame = 128 + 16 * prog.ncap;
            (prog.heap_limit as usize).saturating_mul(1024) / frame
        };
        s.regs.clear();
        s.regs.resize(prog.nregs, UNSET);
        s.bt.clear();
        s.trail.clear();
        s.calls.clear();
        s.arena.clear();
        Vm {
            prog,
            looks,
            hay,
            end: hay.len(),
            floor: 0,
            limit_end: hay.len(),
            noteol: false,
            notbol: false,
            lax: prog.interp,
            start_offset,
            som: 0,
            s,
            steps: 0,
            limit,
            depth_limit,
            heap_frames,
            hwm: 0,
            skip_arg_count: 0,
            ignore_skip_arg: 0,
        }
    }

    pub fn set_bounds(&mut self, floor: usize, end: usize, noteol: bool) {
        self.floor = floor;
        self.limit_end = end;
        self.noteol = noteol;
    }

    #[inline]
    fn dec(&self, pos: usize, end: usize) -> Option<(u32, usize)> {
        let b = self.hay[pos];
        if b < 0x80 {
            return Some((u32::from(b), 1));
        }
        self.dslow(pos, end)
    }

    #[inline]
    fn dslow(&self, pos: usize, end: usize) -> Option<(u32, usize)> {
        match decode_slow(self.hay, pos, end) {
            None if self.lax && is_cont(self.hay[pos]) => Some((u32::from(self.hay[pos]), 1)),
            r => r,
        }
    }

    fn prev_at(&self, pos: usize) -> Option<(u32, usize)> {
        if !self.lax {
            return prev_char(self.hay, pos, self.prog.utf);
        }
        if pos == 0 {
            return None;
        }
        let mut s = pos - 1;
        while s > 0 && is_cont(self.hay[s]) {
            s -= 1;
        }
        self.dec(s, self.hay.len()).map(|(c, _)| (c, s))
    }

    fn true_end(&self) -> usize {
        if self.end == self.limit_end {
            self.hay.len()
        } else {
            self.end
        }
    }

    fn reset(&mut self, start: usize) {
        self.s.bt.clear();
        self.s.trail.clear();
        self.s.calls.clear();
        self.s.arena.clear();
        self.som = start;
        self.end = self.limit_end;
        self.steps = 0;
        self.hwm = start;
        self.skip_arg_count = 0;
    }

    pub fn run_end(&self, item: Item, pos: usize) -> usize {
        self.rep_scan(item, pos, NONE).0
    }

    pub fn regs(&self) -> &[usize] {
        &self.s.regs
    }

    #[inline]
    fn set_reg(&mut self, r: u32, v: usize) {
        let old = self.s.regs[r as usize];
        self.s.trail.push(Trail::Reg(r, old));
        self.s.regs[r as usize] = v;
    }

    fn set_end(&mut self, v: usize) {
        self.s.trail.push(Trail::End(self.end));
        self.end = v;
    }

    fn undo(&mut self, to: u32) {
        let to = to as usize;
        while self.s.trail.len() > to {
            match self.s.trail.pop().expect("trail") {
                Trail::Reg(r, v) => self.s.regs[r as usize] = v,
                Trail::Som(v) => self.som = v,
                Trail::End(v) => self.end = v,
                Trail::CallPush => {
                    self.s.calls.pop();
                }
                Trail::CallPop(f) => self.s.calls.push(f),
                Trail::Regs(idx) => {
                    let n = self.s.regs.len();
                    let idx = idx as usize;
                    let (regs, arena) = (&mut self.s.regs, &self.s.arena);
                    regs.copy_from_slice(&arena[idx..idx + n]);
                }
            }
        }
    }

    #[inline]
    fn trail_len(&self) -> u32 {
        self.s.trail.len() as u32
    }

    #[inline]
    fn push(&mut self, b: Bt) -> Result<(), i32> {
        self.steps += 1;
        if self.steps > self.limit {
            return Err(error::MATCH_LIMIT);
        }
        let depth = self.s.bt.len();
        if depth >= self.depth_limit {
            return Err(error::DEPTH_LIMIT);
        }
        if depth >= self.heap_frames {
            return Err(error::HEAP_LIMIT);
        }
        self.s.bt.push(b);
        Ok(())
    }

    #[inline]
    fn is_newline(&self, pos: usize) -> Option<usize> {
        let h = self.hay;
        let end = self.end;
        if pos >= end {
            return None;
        }
        let b = h[pos];
        match self.prog.newline {
            Newline::Lf => (b == b'\n').then_some(1),
            Newline::Cr => (b == b'\r').then_some(1),
            Newline::Nul => (b == 0).then_some(1),
            Newline::CrLf => (b == b'\r' && pos + 1 < end && h[pos + 1] == b'\n').then_some(2),
            Newline::AnyCrLf => match b {
                b'\n' => Some(1),
                b'\r' => Some(if pos + 1 < end && h[pos + 1] == b'\n' {
                    2
                } else {
                    1
                }),
                _ => None,
            },
            Newline::Any => match b {
                0x0a..=0x0c => Some(1),
                b'\r' => Some(if pos + 1 < end && h[pos + 1] == b'\n' {
                    2
                } else {
                    1
                }),
                0x85 if !self.prog.utf => Some(1),
                0xc2 if self.prog.utf => (pos + 1 < end && h[pos + 1] == 0x85).then_some(2),
                0xe2 if self.prog.utf => (pos + 2 < end
                    && h[pos + 1] == 0x80
                    && (h[pos + 2] == 0xa8 || h[pos + 2] == 0xa9))
                    .then_some(3),
                _ => None,
            },
        }
    }

    #[inline]
    fn was_newline(&self, pos: usize) -> bool {
        let h = self.hay;
        if pos == 0 {
            return false;
        }
        let b = h[pos - 1];
        match self.prog.newline {
            Newline::Lf => b == b'\n',
            Newline::Cr => b == b'\r',
            Newline::Nul => b == 0,
            Newline::CrLf => b == b'\n' && pos >= 2 && h[pos - 2] == b'\r',
            Newline::AnyCrLf => b == b'\n' || b == b'\r',
            Newline::Any => match self.prev_at(pos) {
                Some((c, _)) => matches!(c, 0x0a..=0x0d | 0x85 | 0x2028 | 0x2029),
                None => false,
            },
        }
    }

    #[inline]
    fn match_item(&self, item: Item, pos: usize) -> Option<usize> {
        let h = self.hay;
        let end = self.end;
        match item {
            Item::Byte(b) => (pos < end && h[pos] == b).then_some(pos + 1),
            Item::Byte2(a, b) => (pos < end && (h[pos] == a || h[pos] == b)).then_some(pos + 1),
            Item::Bytes(start, len) => {
                let len = len as usize;
                if end - pos < len {
                    return None;
                }
                if start & 0x8000_0000 != 0 {
                    let s = (start & 0x7fff_ffff) as usize;
                    let pat = &self.prog.pool[s..s + len];
                    let sub = &h[pos..pos + len];
                    if sub
                        .iter()
                        .zip(pat)
                        .all(|(&x, &y)| x.to_ascii_lowercase() == y)
                    {
                        Some(pos + len)
                    } else {
                        None
                    }
                } else {
                    let s = start as usize;
                    (h[pos..pos + len] == self.prog.pool[s..s + len]).then_some(pos + len)
                }
            }
            Item::Class(id) => {
                if pos >= end {
                    return None;
                }
                let m = &self.prog.classes[id as usize];
                let b = h[pos];
                if !self.prog.utf || b < 0x80 {
                    return m.bytes[b as usize].then_some(pos + 1);
                }
                let (c, len) = self.dslow(pos, end)?;
                m.matches_high(c).then_some(pos + len)
            }
            Item::Any => {
                if pos >= end || self.is_newline(pos).is_some() {
                    return None;
                }
                self.any_char(pos)
            }
            Item::AllAny => {
                if pos >= end {
                    return None;
                }
                self.any_char(pos)
            }
            Item::AnyByte => (pos < end).then_some(pos + 1),
        }
    }

    #[inline]
    fn any_char(&self, pos: usize) -> Option<usize> {
        if !self.prog.utf {
            return Some(pos + 1);
        }
        let b = self.hay[pos];
        if b < 0x80 {
            return Some(pos + 1);
        }
        self.dslow(pos, self.end).map(|(_, l)| pos + l)
    }

    fn rep_scan(&self, item: Item, mut pos: usize, max: u32) -> (usize, u32) {
        let h = self.hay;
        let end = self.end;
        let lim = if max == NONE {
            end
        } else {
            end.min(pos.saturating_add(max as usize))
        };
        let mut count: u32 = 0;
        match item {
            Item::Byte(b) => {
                let start = pos;
                while pos < lim && h[pos] == b {
                    pos += 1;
                }
                count = (pos - start) as u32;
            }
            Item::Byte2(a, b) => {
                let start = pos;
                while pos < lim && (h[pos] == a || h[pos] == b) {
                    pos += 1;
                }
                count = (pos - start) as u32;
            }
            Item::Class(id) if !self.prog.utf => {
                let m = &self.prog.classes[id as usize];
                let start = pos;
                while pos < lim && m.bytes[h[pos] as usize] {
                    pos += 1;
                }
                count = (pos - start) as u32;
            }
            Item::Class(id) => {
                let m = &self.prog.classes[id as usize];
                while pos < end && count < max {
                    let b = h[pos];
                    if b < 0x80 {
                        if !m.bytes[b as usize] {
                            break;
                        }
                        pos += 1;
                    } else {
                        match self.dslow(pos, end) {
                            Some((c, l)) if m.matches_high(c) => pos += l,
                            _ => break,
                        }
                    }
                    count += 1;
                }
            }
            Item::AnyByte => {
                count = (lim - pos) as u32;
                pos = lim;
            }
            Item::AllAny if !self.prog.utf => {
                count = (lim - pos) as u32;
                pos = lim;
            }
            Item::Any if !self.prog.utf && self.prog.newline == Newline::Lf => {
                let stop = memchr::memchr(b'\n', &h[pos..lim]).map_or(lim, |i| pos + i);
                count = (stop - pos) as u32;
                pos = stop;
            }
            Item::Any if self.prog.newline == Newline::Lf => {
                while pos < end && count < max {
                    let b = h[pos];
                    if b < 0x80 {
                        if b == b'\n' {
                            break;
                        }
                        pos += 1;
                    } else {
                        match self.dslow(pos, end) {
                            Some((_, l)) => pos += l,
                            None => break,
                        }
                    }
                    count += 1;
                }
            }
            _ => {
                while count < max {
                    match self.match_item(item, pos) {
                        Some(np) => pos = np,
                        None => break,
                    }
                    count += 1;
                }
            }
        }
        (pos, count)
    }

    fn rep_step(item: Item, utf: bool) -> u8 {
        match item {
            Item::Byte(_) | Item::Byte2(..) | Item::AnyByte => 1,
            Item::Bytes(_, len) => len as u8,
            Item::Class(_) | Item::Any | Item::AllAny => u8::from(!utf),
        }
    }

    fn want_byte(&self, pc: usize) -> u16 {
        match self.prog.insts.get(pc) {
            Some(Inst::Item(Item::Byte(b))) => u16::from(*b),
            Some(Inst::Item(Item::Bytes(start, _))) if start & 0x8000_0000 == 0 => {
                u16::from(self.prog.pool[*start as usize])
            }
            _ => u16::MAX,
        }
    }

    fn word_at(c: u32, ucp: bool) -> bool {
        if c < 128 {
            is_word_ascii(c)
        } else {
            ucp && ucd::is_word_ucp(c)
        }
    }

    fn word_boundary(&self, pos: usize, ucp: bool) -> Option<bool> {
        let h = self.hay;
        let prev_fast = if pos <= self.floor {
            Some(false)
        } else if h[pos - 1] < 0x80 {
            Some(is_word_ascii(u32::from(h[pos - 1])))
        } else {
            None
        };
        let cur_fast = if pos >= self.end {
            Some(false)
        } else if h[pos] < 0x80 {
            Some(is_word_ascii(u32::from(h[pos])))
        } else {
            None
        };
        if let (Some(p), Some(c)) = (prev_fast, cur_fast) {
            return Some(p != c);
        }
        let utf = self.prog.utf;
        let prev = if pos <= self.floor {
            Some(false)
        } else {
            self.prev_at(pos).map(|(c, _)| Self::word_at(c, ucp))
        };
        let cur = if pos >= self.end {
            Some(false)
        } else if utf {
            self.dec(pos, self.end).map(|(c, _)| Self::word_at(c, ucp))
        } else {
            Some(Self::word_at(u32::from(self.hay[pos]), ucp))
        };
        match (prev, cur) {
            (None, None) => None,
            (p, c) => Some(p.unwrap_or(false) != c.unwrap_or(false)),
        }
    }

    fn check_assert(&self, op: AssertOp, pos: usize) -> bool {
        let end = self.end;
        match op {
            AssertOp::Circ => pos == 0 && !self.notbol,
            AssertOp::Sod => pos == 0,
            AssertOp::CircM => {
                if pos == 0 {
                    return !self.notbol;
                }
                if pos == end {
                    return false;
                }
                self.was_newline(pos)
            }
            AssertOp::Doll | AssertOp::EodN => {
                if op == AssertOp::Doll && self.noteol {
                    return false;
                }
                let te = self.true_end();
                if pos == te {
                    return true;
                }
                if op == AssertOp::Doll && self.prog.dollar_endonly {
                    return false;
                }
                match self.is_newline(pos) {
                    Some(len) => pos + len == te,
                    None => false,
                }
            }
            AssertOp::DollM => {
                if pos >= end {
                    !self.noteol
                } else {
                    self.is_newline(pos).is_some()
                }
            }
            AssertOp::Eod => pos == self.true_end(),
            AssertOp::Som => pos == self.start_offset,
            AssertOp::WordB => self.word_boundary(pos, false) == Some(true),
            AssertOp::NotWordB => self.word_boundary(pos, false) == Some(false),
            AssertOp::UcpWordB => self.word_boundary(pos, true) == Some(true),
            AssertOp::UcpNotWordB => self.word_boundary(pos, true) == Some(false),
        }
    }

    fn newline_seq(&self, pos: usize) -> Option<usize> {
        let h = self.hay;
        let end = self.end;
        if pos >= end {
            return None;
        }
        let b = h[pos];
        let anycrlf = self.prog.bsr_anycrlf;
        match b {
            b'\r' => Some(if pos + 1 < end && h[pos + 1] == b'\n' {
                pos + 2
            } else {
                pos + 1
            }),
            b'\n' => Some(pos + 1),
            0x0b | 0x0c if !anycrlf => Some(pos + 1),
            _ if anycrlf => None,
            0x85 if !self.prog.utf => Some(pos + 1),
            _ if self.prog.utf && b >= 0x80 => match self.dslow(pos, end) {
                Some((0x85 | 0x2028 | 0x2029, l)) => Some(pos + l),
                _ => None,
            },
            _ => None,
        }
    }

    fn char_at(&self, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.end {
            return None;
        }
        if self.prog.utf {
            self.dec(pos, self.end)
        } else {
            Some((u32::from(self.hay[pos]), 1))
        }
    }

    fn ext_uni(&self, pos: usize) -> Option<usize> {
        let (c, len) = self.char_at(pos)?;
        let mut lgb = ucd::grapheme_break(c);
        let mut p = pos + len;
        let mut was_ep_zwj = false;
        let mut left = pos;
        while p < self.end {
            let Some((c2, l2)) = self.char_at(p) else {
                break;
            };
            let rgb = ucd::grapheme_break(c2);
            if ucd::GB_TABLE[lgb as usize] & (1u32 << rgb) == 0 {
                break;
            }
            if lgb == ucd::GB_ZWJ && rgb == ucd::GB_EXT_PICT && !was_ep_zwj {
                break;
            }
            if lgb == ucd::GB_RI && rgb == ucd::GB_RI && (!self.prog.invalid_ok || !self.prog.jit) {
                let mut count = 0u32;
                let mut q = left;
                while q > self.floor
                    && let Some((pc, s)) = self.prev_at(q)
                {
                    if ucd::grapheme_break(pc) != ucd::GB_RI {
                        break;
                    }
                    count += 1;
                    q = s;
                }
                if count & 1 != 0 {
                    break;
                }
            }
            was_ep_zwj = lgb == ucd::GB_EXT_PICT && rgb == ucd::GB_ZWJ;
            if rgb != ucd::GB_EXTEND || lgb != ucd::GB_EXT_PICT {
                lgb = rgb;
            }
            left = p;
            p += l2;
        }
        Some(p)
    }

    fn backref(&self, list: u32, fold: bool, restrict: bool, pos: usize) -> Option<usize> {
        let regs = &self.s.regs;
        let mut found: Option<(usize, usize)> = None;
        for &g in &self.prog.lists[list as usize] {
            let s = regs[2 * g as usize];
            let e = regs[2 * g as usize + 1];
            if s != UNSET && e != UNSET {
                found = Some((s, e));
                break;
            }
        }
        let (s, e) = found?;
        let h = self.hay;
        let len = e.saturating_sub(s);
        if !fold {
            if self.end - pos < len {
                return None;
            }
            return (h[s..e] == h[pos..pos + len]).then_some(pos + len);
        }
        if !self.prog.utf {
            if self.end - pos < len {
                return None;
            }
            for i in 0..len {
                let a = u32::from(h[s + i]);
                let b = u32::from(h[pos + i]);
                if a != b && !self.fold_eq(a, b, restrict) {
                    return None;
                }
            }
            return Some(pos + len);
        }
        let mut p = s;
        let mut q = pos;
        while p < e {
            let (a, la) = self.dec(p, e)?;
            if q >= self.end {
                return None;
            }
            let (b, lb) = self.dec(q, self.end)?;
            if a != b && !self.fold_eq(a, b, restrict) {
                return None;
            }
            p += la;
            q += lb;
        }
        Some(q)
    }

    fn fold_eq(&self, a: u32, b: u32, restrict: bool) -> bool {
        let mode = super::charset::CaseMode {
            unicode: self.prog.utf || self.prog.ucp,
            restrict,
            turkish: self.prog.turkish,
            max: if self.prog.utf { 0x10_ffff } else { 0xff },
        };
        super::charset::caseless_equivalents(a, mode).contains(&b)
    }

    fn cond_test(&self, test: CondTest) -> bool {
        match test {
            CondTest::Const(b) => b,
            CondTest::Groups(list) => self.prog.lists[list as usize].iter().any(|&g| {
                self.s.regs[2 * g as usize] != UNSET && self.s.regs[2 * g as usize + 1] != UNSET
            }),
            CondTest::RecurseAny => !self.s.calls.is_empty(),
            CondTest::RecurseGroup(g) => self.s.calls.last().is_some_and(|f| f.group == g),
            CondTest::RecurseGroups(list) => self
                .s
                .calls
                .last()
                .is_some_and(|f| self.prog.lists[list as usize].contains(&f.group)),
        }
    }

    fn current_recurse(&self) -> u32 {
        self.s.calls.last().map_or(NONE, |f| f.group)
    }

    fn do_return(&mut self) -> u32 {
        let f = *self.s.calls.last().expect("call");
        self.s.calls.pop();
        self.s.trail.push(Trail::CallPop(f));
        let n = self.s.regs.len();
        let idx = self.s.arena.len();
        self.s.arena.extend_from_slice(&self.s.regs);
        self.s.trail.push(Trail::Regs(idx as u32));
        let snap = f.snap as usize;
        let (regs, arena) = (&mut self.s.regs, &self.s.arena);
        regs.copy_from_slice(&arena[snap..snap + n]);
        if f.rets != NONE {
            for &g in &self.prog.lists[f.rets as usize] {
                let g = g as usize * 2;
                regs[g] = arena[idx + g];
                regs[g + 1] = arena[idx + g + 1];
            }
        }
        f.ret_pc
    }

    fn step_back(&self, pos: usize, n: u32) -> Option<usize> {
        if !self.prog.utf {
            return (pos >= self.floor + n as usize).then(|| pos - n as usize);
        }
        let mut p = pos;
        for _ in 0..n {
            let (_, s) = self.prev_at(p)?;
            if s < self.floor {
                return None;
            }
            p = s;
        }
        Some(p)
    }

    fn script_run(&self, start: usize, end: usize) -> bool {
        let mut p = start;
        let Some((c0, l0)) = self.char_at(p) else {
            return true;
        };
        if p + l0 >= end {
            return true;
        }
        let mut c = c0;
        p += l0;
        let full = super::ucd_tables::SCRIPT_COUNT;
        let mut require_state = 0u8;
        let mut require_map: Vec<bool> = vec![false; full];
        let mut require_digit: Option<u32> = None;
        let han = super::ucd_tables::SCRIPT_HAN;
        let hira = super::ucd_tables::SCRIPT_HIRAGANA;
        let kata = super::ucd_tables::SCRIPT_KATAKANA;
        let bopo = super::ucd_tables::SCRIPT_BOPOMOFO;
        let hang = super::ucd_tables::SCRIPT_HANGUL;
        let special = |s: u8| -> Option<u8> {
            if s == han {
                Some(1)
            } else if s == hira || s == kata {
                Some(2)
            } else if s == bopo {
                Some(3)
            } else if s == hang {
                Some(4)
            } else {
                None
            }
        };
        loop {
            let script = ucd::script(c);
            if script == super::ucd_tables::SCRIPT_UNKNOWN {
                return false;
            }
            let scx = ucd::script_extensions(c);
            let common = super::ucd_tables::SCRIPT_COMMON;
            let inherited = super::ucd_tables::SCRIPT_INHERITED;
            if !scx.is_empty() || (script != inherited && script != common) {
                let mut map = vec![false; full];
                for &s in scx {
                    map[s as usize] = true;
                }
                if script != common && script != inherited {
                    map[script as usize] = true;
                }
                match require_state {
                    0 => {
                        if let Some(st) = special(script) {
                            require_state = st;
                        } else {
                            require_map.copy_from_slice(&map);
                            require_state = 5;
                        }
                    }
                    1 => {
                        if script != han {
                            let mut chs = 0u8;
                            if map[bopo as usize] {
                                chs |= 1;
                            }
                            if map[hira as usize] {
                                chs |= 2;
                            }
                            if map[kata as usize] {
                                chs |= 4;
                            }
                            if map[hang as usize] {
                                chs |= 8;
                            }
                            if chs == 0 {
                                return false;
                            }
                            if chs == 1 {
                                require_state = 3;
                            } else if chs == 6 {
                                require_state = 2;
                            }
                        }
                    }
                    2 => {
                        if !(map[han as usize] || map[hira as usize] || map[kata as usize]) {
                            return false;
                        }
                    }
                    3 => {
                        if !(map[han as usize] || map[bopo as usize]) {
                            return false;
                        }
                    }
                    4 => {
                        if !(map[han as usize] || map[hang as usize]) {
                            return false;
                        }
                    }
                    _ => {
                        if !require_map.iter().zip(&map).any(|(a, b)| *a && *b) {
                            return false;
                        }
                        match special(script) {
                            Some(st) => require_state = st,
                            None => {
                                for (a, b) in require_map.iter_mut().zip(&map) {
                                    *a = *a && *b;
                                }
                            }
                        }
                    }
                }
            }
            if let Some(z) = ucd::digit_set_zero(c) {
                match require_digit {
                    None => require_digit = Some(z),
                    Some(d) if d != z => return false,
                    _ => {}
                }
            }
            if p >= end {
                return true;
            }
            let Some((c2, l2)) = self.char_at(p) else {
                return true;
            };
            c = c2;
            p += l2;
        }
    }

    pub fn run(&mut self, start: usize) -> Outcome {
        self.reset(start);
        let mut pc: usize = 0;
        let mut pos = start;
        let insts = &self.prog.insts[..];
        'run: loop {
            let code: Code = 'fail: {
                match insts[pc] {
                    Inst::Match => {
                        if let Some(f) = self.s.calls.last()
                            && f.group == 0
                        {
                            pc = self.do_return() as usize;
                            continue 'run;
                        }
                        if pos == self.som
                            && (self.prog.notempty
                                || (self.prog.notempty_atstart && self.som == self.start_offset))
                        {
                            break 'fail Code::NoMatch;
                        }
                        return Outcome::Match(self.som, pos);
                    }
                    Inst::Item(item) => match self.match_item(item, pos) {
                        Some(np) => {
                            pos = np;
                            pc += 1;
                        }
                        None => break 'fail Code::NoMatch,
                    },
                    Inst::Rep {
                        item,
                        min,
                        max,
                        mode,
                    } => {
                        let (p1, c1) = self.rep_scan(item, pos, min);
                        if c1 < min {
                            break 'fail Code::NoMatch;
                        }
                        let rest = if max == NONE { NONE } else { max - min };
                        match mode {
                            RepMode::Possessive => {
                                let (p2, _) = self.rep_scan(item, p1, rest);
                                pos = p2;
                                pc += 1;
                            }
                            RepMode::Greedy => {
                                let (p2, _) = self.rep_scan(item, p1, rest);
                                if p2 > p1 {
                                    let step = Self::rep_step(item, self.prog.utf);
                                    let want = self.want_byte(pc + 1);
                                    let trail = self.trail_len();
                                    let e = Bt::RepGreedy {
                                        pc: (pc + 1) as u32,
                                        min_pos: p1,
                                        pos: p2,
                                        trail,
                                        step,
                                        want,
                                    };
                                    if let Err(e) = self.push(e) {
                                        return Outcome::Error(e);
                                    }
                                }
                                pos = p2;
                                pc += 1;
                            }
                            RepMode::Lazy => {
                                if rest > 0 {
                                    let trail = self.trail_len();
                                    let e = Bt::RepLazy {
                                        pc: (pc + 1) as u32,
                                        rep_pc: pc as u32,
                                        pos: p1,
                                        count: min,
                                        trail,
                                    };
                                    if let Err(e) = self.push(e) {
                                        return Outcome::Error(e);
                                    }
                                }
                                pos = p1;
                                pc += 1;
                            }
                        }
                    }
                    Inst::Newline => match self.newline_seq(pos) {
                        Some(np) => {
                            pos = np;
                            pc += 1;
                        }
                        None => break 'fail Code::NoMatch,
                    },
                    Inst::ExtUni => match self.ext_uni(pos) {
                        Some(np) => {
                            pos = np;
                            pc += 1;
                        }
                        None => break 'fail Code::NoMatch,
                    },
                    Inst::Assert(op) => {
                        if self.check_assert(op, pos) {
                            pc += 1;
                        } else {
                            break 'fail Code::NoMatch;
                        }
                    }
                    Inst::Jmp(t) => pc = t as usize,
                    Inst::Fork { other, prefer_next } => {
                        let trail = self.trail_len();
                        let (alt, go) = if prefer_next {
                            (other, pc + 1)
                        } else {
                            ((pc + 1) as u32, other as usize)
                        };
                        if let Err(e) = self.push(Bt::Choice {
                            pc: alt,
                            pos,
                            trail,
                        }) {
                            return Outcome::Error(e);
                        }
                        pc = go;
                    }
                    Inst::Alt { next, then_all } => {
                        let trail = self.trail_len();
                        if let Err(e) = self.push(Bt::Alt {
                            pc: next,
                            pos,
                            trail,
                            then_all,
                        }) {
                            return Outcome::Error(e);
                        }
                        pc += 1;
                    }
                    Inst::ThenBarrier { bound } => {
                        self.s.bt.push(Bt::ThenBarrier { bound });
                        pc += 1;
                    }
                    Inst::SetPos(r) | Inst::ScriptRunStart(r) => {
                        self.set_reg(r, pos);
                        pc += 1;
                    }
                    Inst::LoopMax { reg, body } => {
                        if reg != NONE && self.s.regs[reg as usize] == pos {
                            pc += 1;
                        } else {
                            let trail = self.trail_len();
                            if let Err(e) = self.push(Bt::Choice {
                                pc: (pc + 1) as u32,
                                pos,
                                trail,
                            }) {
                                return Outcome::Error(e);
                            }
                            pc = body as usize;
                        }
                    }
                    Inst::LoopMin { reg, body } => {
                        if reg != NONE && self.s.regs[reg as usize] == pos {
                            pc += 1;
                        } else {
                            let trail = self.trail_len();
                            if let Err(e) = self.push(Bt::Choice {
                                pc: body,
                                pos,
                                trail,
                            }) {
                                return Outcome::Error(e);
                            }
                            pc += 1;
                        }
                    }
                    Inst::CapOpen(n) => {
                        let r = (self.prog.ncap * 2) as u32 + n;
                        self.set_reg(r, pos);
                        pc += 1;
                    }
                    Inst::CapClose(n) => {
                        if let Some(f) = self.s.calls.last()
                            && f.group == n
                        {
                            pc = self.do_return() as usize;
                            continue 'run;
                        }
                        let open = self.s.regs[self.prog.ncap * 2 + n as usize];
                        self.set_reg(2 * n, open);
                        self.set_reg(2 * n + 1, pos);
                        pc += 1;
                    }
                    Inst::AtomStart(r) => {
                        let h = self.s.bt.len();
                        self.set_reg(r, h);
                        pc += 1;
                    }
                    Inst::AtomEnd(r) => {
                        let h = self.s.regs[r as usize];
                        self.s.bt.truncate(h);
                        pc += 1;
                    }
                    Inst::LookStart { kind, reg, cont } => {
                        self.set_reg(reg, pos);
                        let h = self.s.bt.len();
                        self.set_reg(reg + 1, h);
                        let trail = self.trail_len();
                        let entry = match kind {
                            LookKind::NegAhead
                            | LookKind::NegBehind
                            | LookKind::CondPos
                            | LookKind::CondNeg => Bt::NegLook {
                                pc: cont,
                                pos,
                                trail,
                            },
                            _ => Bt::PosLook,
                        };
                        if let Err(e) = self.push(entry) {
                            return Outcome::Error(e);
                        }
                        pc += 1;
                    }
                    Inst::LookEnd {
                        kind, reg, cont, ..
                    } => {
                        let saved = self.s.regs[reg as usize];
                        let height = self.s.regs[reg as usize + 1];
                        if pos > self.hwm {
                            self.hwm = pos;
                        }
                        match kind {
                            LookKind::PosAhead | LookKind::PosBehind => {
                                self.s.bt.truncate(height);
                                pos = saved;
                                pc += 1;
                            }
                            LookKind::NaAhead | LookKind::NaBehind => {
                                pos = saved;
                                pc += 1;
                            }
                            LookKind::NegAhead | LookKind::NegBehind => {
                                self.s.bt.truncate(height);
                                break 'fail Code::NoMatch;
                            }
                            LookKind::CondPos | LookKind::CondNeg => {
                                self.s.bt.truncate(height);
                                pos = saved;
                                pc = cont as usize;
                            }
                            LookKind::Scs => {
                                let old_end = self.s.regs[reg as usize + 2];
                                self.set_end(old_end);
                                pos = saved;
                                pc += 1;
                            }
                        }
                    }
                    Inst::Reverse(n) => match self.step_back(pos, n) {
                        Some(p) => {
                            pos = p;
                            pc += 1;
                        }
                        None => break 'fail Code::NoMatch,
                    },
                    Inst::VReverse { min, max } => {
                        let mut back = 0u32;
                        let mut p = pos;
                        while back < max {
                            match self.step_back(p, 1) {
                                Some(q) => {
                                    p = q;
                                    back += 1;
                                }
                                None => break,
                            }
                        }
                        if back < min {
                            break 'fail Code::NoMatch;
                        }
                        if back > min {
                            let trail = self.trail_len();
                            let e = Bt::VRev {
                                pc: (pc + 1) as u32,
                                pos: p,
                                remaining: back - min,
                                trail,
                            };
                            if let Err(e) = self.push(e) {
                                return Outcome::Error(e);
                            }
                        }
                        pos = p;
                        pc += 1;
                    }
                    Inst::CheckPos(reg) => {
                        if pos == self.s.regs[reg as usize] {
                            pc += 1;
                        } else {
                            break 'fail Code::NoMatch;
                        }
                    }
                    Inst::BackRef {
                        list,
                        fold,
                        restrict,
                    } => match self.backref(list, fold, restrict, pos) {
                        Some(np) => {
                            pos = np;
                            pc += 1;
                        }
                        None => break 'fail Code::NoMatch,
                    },
                    Inst::Call {
                        group,
                        target,
                        rets,
                    } => {
                        if pos > self.hwm {
                            self.hwm = pos;
                        }
                        if let Some(f) = self.s.calls.iter().rev().find(|f| f.group == group)
                            && f.pos == pos
                            && f.hwm == self.hwm
                        {
                            return Outcome::Error(if self.prog.jit {
                                error::JIT_STACK_LIMIT
                            } else {
                                error::RECURSE_LOOP
                            });
                        }
                        let snap = self.s.arena.len() as u32;
                        self.s.arena.extend_from_slice(&self.s.regs);
                        let frame = CallFrame {
                            group,
                            rets,
                            ret_pc: (pc + 1) as u32,
                            snap,
                            pos,
                            hwm: self.hwm,
                            bt_height: self.s.bt.len() as u32,
                        };
                        self.s.calls.push(frame);
                        self.s.trail.push(Trail::CallPush);
                        if let Err(e) = self.push(Bt::Call { group }) {
                            return Outcome::Error(e);
                        }
                        pc = target as usize;
                    }
                    Inst::Cond { test, else_pc } => {
                        if self.cond_test(test) {
                            pc += 1;
                        } else {
                            pc = else_pc as usize;
                        }
                    }
                    Inst::Verb(v) => {
                        let rec = self.current_recurse();
                        let entry = match v {
                            VerbOp::Commit => Some(Bt::Verb {
                                code: Code::Commit(rec),
                            }),
                            VerbOp::Prune => Some(Bt::Verb {
                                code: Code::Prune(rec),
                            }),
                            VerbOp::Skip => Some(Bt::Verb {
                                code: Code::Skip(pos, rec),
                            }),
                            VerbOp::SkipArg(name) => {
                                self.skip_arg_count += 1;
                                if self.skip_arg_count <= self.ignore_skip_arg {
                                    None
                                } else {
                                    Some(Bt::Verb {
                                        code: Code::SkipArg(name, rec),
                                    })
                                }
                            }
                            VerbOp::Then => Some(Bt::Verb {
                                code: Code::Then(pc as u32, rec),
                            }),
                            VerbOp::Mark(name) => Some(Bt::Mark { name, pos }),
                        };
                        if let Some(e) = entry {
                            self.s.bt.push(e);
                        }
                        pc += 1;
                    }
                    Inst::Accept { closes, look } => {
                        if self.s.calls.is_empty() {
                            for &g in &self.prog.lists[closes as usize] {
                                let open = self.s.regs[self.prog.ncap * 2 + g as usize];
                                self.set_reg(2 * g, open);
                                self.set_reg(2 * g + 1, pos);
                            }
                        }
                        let look_inner = look != NONE && {
                            let h = self.s.regs[self.looks[look as usize].reg as usize + 1];
                            h != UNSET
                                && self.s.calls.last().is_none_or(|f| h > f.bt_height as usize)
                        };
                        if look_inner {
                            let info = &self.looks[look as usize];
                            let reg = info.reg as usize;
                            let kind = info.kind;
                            let success = info.success_pc as usize;
                            let saved = self.s.regs[reg];
                            let height = self.s.regs[reg + 1];
                            self.s.bt.truncate(height);
                            match kind {
                                LookKind::NegAhead | LookKind::NegBehind => {
                                    break 'fail Code::NoMatch;
                                }
                                LookKind::Scs => {
                                    let old_end = self.s.regs[reg + 2];
                                    self.set_end(old_end);
                                    pos = saved;
                                    pc = success;
                                }
                                _ => {
                                    pos = saved;
                                    pc = success;
                                }
                            }
                            continue 'run;
                        }
                        if let Some(&f) = self.s.calls.last() {
                            self.s.bt.truncate(f.bt_height as usize);
                            pc = self.do_return() as usize;
                            continue 'run;
                        }
                        if pos == self.som
                            && (self.prog.notempty
                                || (self.prog.notempty_atstart && self.som == self.start_offset))
                        {
                            break 'fail Code::NoMatch;
                        }
                        return Outcome::Match(self.som, pos);
                    }
                    Inst::Fail => break 'fail Code::NoMatch,
                    Inst::SetSom => {
                        self.s.trail.push(Trail::Som(self.som));
                        self.som = pos;
                        pc += 1;
                    }
                    Inst::ScriptRunEnd(r) => {
                        let s = self.s.regs[r as usize];
                        if self.script_run(s, pos) {
                            pc += 1;
                        } else {
                            break 'fail Code::NoMatch;
                        }
                    }
                    Inst::ScsStart { list, reg, .. } => {
                        let mut found = None;
                        for &g in &self.prog.lists[list as usize] {
                            let s = self.s.regs[2 * g as usize];
                            let e = self.s.regs[2 * g as usize + 1];
                            if s != UNSET && e != UNSET {
                                found = Some((s, e));
                                break;
                            }
                        }
                        let Some((s, e)) = found else {
                            break 'fail Code::NoMatch;
                        };
                        self.set_reg(reg, pos);
                        let h = self.s.bt.len();
                        self.set_reg(reg + 1, h);
                        let end = self.end;
                        self.set_reg(reg + 2, end);
                        self.set_end(e);
                        if let Err(err) = self.push(Bt::PosLook) {
                            return Outcome::Error(err);
                        }
                        pos = s;
                        pc += 1;
                    }
                }
                continue 'run;
            };
            if pos > self.hwm {
                self.hwm = pos;
            }
            match self.backtrack(code) {
                Ok((npc, npos)) => {
                    pc = npc as usize;
                    pos = npos;
                }
                Err(Stop::Code(c)) => {
                    self.undo(0);
                    return Outcome::Fail(c);
                }
                Err(Stop::Error(e)) => return Outcome::Error(e),
            }
        }
    }

    fn backtrack(&mut self, mut code: Code) -> Result<(u32, usize), Stop> {
        loop {
            let Some(top) = self.s.bt.pop() else {
                return Err(Stop::Code(code));
            };
            match top {
                Bt::Choice { pc, pos, trail } => {
                    if code == Code::NoMatch {
                        self.undo(trail);
                        return Ok((pc, pos));
                    }
                }
                Bt::Alt {
                    pc,
                    pos,
                    trail,
                    then_all,
                } => {
                    let catch = match code {
                        Code::NoMatch => true,
                        Code::Then(tpc, _) => then_all || tpc < pc,
                        _ => false,
                    };
                    if catch {
                        self.undo(trail);
                        return Ok((pc, pos));
                    }
                }
                Bt::ThenBarrier { bound } => {
                    if let Code::Then(tpc, _) = code
                        && tpc < bound
                    {
                        code = Code::NoMatch;
                    }
                }
                Bt::RepGreedy {
                    pc,
                    min_pos,
                    pos,
                    trail,
                    step,
                    want,
                } => {
                    if code != Code::NoMatch {
                        continue;
                    }
                    self.steps += 1;
                    if self.steps > self.limit {
                        return Err(Stop::Error(error::MATCH_LIMIT));
                    }
                    let mut np = if step == 0 {
                        let mut p = pos - 1;
                        while p > min_pos && is_cont(self.hay[p]) {
                            p -= 1;
                        }
                        p
                    } else {
                        pos - step as usize
                    };
                    if want != u16::MAX && step <= 1 {
                        let b = want as u8;
                        match memchr::memrchr(b, &self.hay[min_pos..=np]) {
                            Some(i) => np = min_pos + i,
                            None => continue,
                        }
                    }
                    if np > min_pos {
                        self.s.bt.push(Bt::RepGreedy {
                            pc,
                            min_pos,
                            pos: np,
                            trail,
                            step,
                            want,
                        });
                    }
                    self.undo(trail);
                    return Ok((pc, np));
                }
                Bt::RepLazy {
                    pc,
                    rep_pc,
                    pos,
                    count,
                    trail,
                } => {
                    if code != Code::NoMatch {
                        continue;
                    }
                    let Inst::Rep { item, max, .. } = self.prog.insts[rep_pc as usize] else {
                        continue;
                    };
                    self.undo(trail);
                    let Some(np) = self.match_item(item, pos) else {
                        continue;
                    };
                    let count = count + 1;
                    self.steps += 1;
                    if self.steps > self.limit {
                        return Err(Stop::Error(error::MATCH_LIMIT));
                    }
                    if max == NONE || count < max {
                        self.s.bt.push(Bt::RepLazy {
                            pc,
                            rep_pc,
                            pos: np,
                            count,
                            trail,
                        });
                    }
                    return Ok((pc, np));
                }
                Bt::VRev {
                    pc,
                    pos,
                    remaining,
                    trail,
                } => {
                    if code != Code::NoMatch {
                        continue;
                    }
                    self.steps += 1;
                    if self.steps > self.limit {
                        return Err(Stop::Error(error::MATCH_LIMIT));
                    }
                    let np = next_boundary(self.hay, pos, self.prog.utf);
                    if remaining > 1 {
                        self.s.bt.push(Bt::VRev {
                            pc,
                            pos: np,
                            remaining: remaining - 1,
                            trail,
                        });
                    }
                    self.undo(trail);
                    return Ok((pc, np));
                }
                Bt::Verb { code: c } => {
                    if code == Code::NoMatch {
                        code = c;
                    }
                }
                Bt::Mark { name, pos } => {
                    if let Code::SkipArg(n, rec) = code
                        && self.prog.names[n as usize] == self.prog.names[name as usize]
                    {
                        code = Code::Skip(pos, rec);
                    }
                }
                Bt::PosLook => {
                    if matches!(code, Code::NoMatch | Code::Then(..)) {
                        code = Code::NoMatch;
                    }
                }
                Bt::NegLook { pc, pos, trail } => {
                    if matches!(
                        code,
                        Code::NoMatch
                            | Code::Then(..)
                            | Code::Commit(_)
                            | Code::Prune(_)
                            | Code::Skip(..)
                    ) {
                        self.undo(trail);
                        return Ok((pc, pos));
                    }
                }
                Bt::Call { group } => {
                    if code.recurse() == Some(group) {
                        code = Code::NoMatch;
                    }
                }
            }
        }
    }
}
