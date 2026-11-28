use super::ast::Newline;
use super::charset::CharSet;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct ClassMatcher {
    pub bytes: [bool; 256],
    pub high: Box<[(u32, u32)]>,
    pub high_all: bool,
    pub high_none: bool,
}

impl ClassMatcher {
    pub fn new(set: &CharSet, utf: bool) -> ClassMatcher {
        let mut bytes = [false; 256];
        let limit = if utf { 128u32 } else { 256 };
        let mut high = Vec::new();
        for &(a, b) in set.ranges() {
            let mut c = a;
            while c <= b && c < limit {
                bytes[c as usize] = true;
                c += 1;
            }
            if utf && b >= 128 {
                high.push((a.max(128), b));
            }
        }
        let high_all = utf && high.len() == 1 && high[0].0 <= 128 && high[0].1 >= 0x10_ffff;
        let high_all = high_all
            || (utf
                && high.len() == 2
                && high[0].0 <= 128
                && high[0].1 >= 0xd7ff
                && high[1].0 <= 0xe000
                && high[1].1 >= 0x10_ffff);
        let high_none = high.is_empty();
        ClassMatcher {
            bytes,
            high: high.into_boxed_slice(),
            high_all,
            high_none,
        }
    }

    #[inline]
    pub fn matches_high(&self, c: u32) -> bool {
        if self.high_all {
            return true;
        }
        if self.high_none {
            return false;
        }
        let h = &self.high;
        let idx = h.partition_point(|r| r.1 < c);
        idx < h.len() && h[idx].0 <= c
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssertOp {
    Circ,
    CircM,
    Doll,
    DollM,
    Sod,
    Eod,
    EodN,
    Som,
    WordB,
    NotWordB,
    UcpWordB,
    UcpNotWordB,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Byte(u8),
    Byte2(u8, u8),
    Bytes(u32, u32),
    Class(u32),
    Any,
    AllAny,
    AnyByte,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepMode {
    Greedy,
    Lazy,
    Possessive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookKind {
    PosAhead,
    NegAhead,
    NaAhead,
    PosBehind,
    NegBehind,
    NaBehind,
    CondPos,
    CondNeg,
    Scs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbOp {
    Commit,
    Prune,
    Skip,
    SkipArg(u32),
    Then,
    Mark(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CondTest {
    Groups(u32),
    RecurseAny,
    RecurseGroup(u32),
    RecurseGroups(u32),
    Const(bool),
}

#[derive(Clone, Debug)]
pub enum Inst {
    Match,
    Item(Item),
    Rep {
        item: Item,
        min: u32,
        max: u32,
        mode: RepMode,
    },
    Newline,
    ExtUni,
    Assert(AssertOp),
    Jmp(u32),
    Fork {
        other: u32,
        prefer_next: bool,
    },
    Alt {
        next: u32,
        then_all: bool,
    },
    ThenBarrier {
        bound: u32,
    },
    SetPos(u32),
    LoopMax {
        reg: u32,
        body: u32,
    },
    LoopMin {
        reg: u32,
        body: u32,
    },
    CapOpen(u32),
    CapClose(u32),
    AtomStart(u32),
    AtomEnd(u32),
    LookStart {
        kind: LookKind,
        reg: u32,
        cont: u32,
    },
    LookEnd {
        kind: LookKind,
        reg: u32,
        cont: u32,
    },
    Reverse(u32),
    VReverse {
        min: u32,
        max: u32,
    },
    CheckPos(u32),
    BackRef {
        list: u32,
        fold: bool,
        restrict: bool,
    },
    Call {
        group: u32,
        target: u32,
        rets: u32,
    },
    Cond {
        test: CondTest,
        else_pc: u32,
    },
    Verb(VerbOp),
    Accept {
        closes: u32,
        look: u32,
    },
    Fail,
    SetSom,
    ScriptRunStart(u32),
    ScriptRunEnd(u32),
    ScsStart {
        list: u32,
        reg: u32,
        cont: u32,
    },
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Prog {
    pub insts: Vec<Inst>,
    pub classes: Vec<ClassMatcher>,
    pub pool: Vec<u8>,
    pub lists: Vec<Vec<u32>>,
    pub names: Vec<Vec<u8>>,
    pub nregs: usize,
    pub ncap: usize,
    pub utf: bool,
    pub ucp: bool,
    pub invalid_ok: bool,
    pub jit: bool,
    pub interp: bool,
    pub newline: Newline,
    pub bsr_anycrlf: bool,
    pub has_crorlf: bool,
    pub notempty: bool,
    pub notempty_atstart: bool,
    pub match_limit: u32,
    pub depth_limit: u32,
    pub heap_limit: u32,
    pub max_lookbehind: u32,
    pub turkish: bool,
    pub dollar_endonly: bool,
}
