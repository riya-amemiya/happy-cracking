use super::charset::CharSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newline {
    Cr,
    Lf,
    CrLf,
    Any,
    AnyCrLf,
    Nul,
}

#[derive(Clone, Debug)]
pub enum Node {
    Lit(Lit),
    Set(Box<CharSet>),
    Dot,
    AllAny,
    AnyByte,
    Assert(Assert),
    Group(Box<Group>),
    Repeat(Box<Repeat>),
    BackRef(Box<BackRef>),
    Recurse(Box<Recurse>),
    Cond(Box<Cond>),
    Verb(Verb),
    SetSom,
    Newline,
    ExtUni,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lit {
    pub c: u32,
    pub fold: bool,
    pub restrict: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assert {
    Circ,
    CircM,
    Doll,
    DollM,
    Sod,
    Eod,
    EodN,
    Som,
    WordB { ucp: bool, neg: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupKind {
    Top,
    NonCapture,
    Capture(u32),
    Atomic,
    LookAhead { neg: bool, atomic: bool },
    LookBehind { neg: bool, atomic: bool },
    ScriptRun,
    Scs(Vec<(RefTarget, usize)>),
}

#[derive(Clone, Debug)]
pub struct Group {
    pub kind: GroupKind,
    pub branches: Vec<Vec<Node>>,
    pub offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepKind {
    Greedy,
    Lazy,
    Possessive,
}

#[derive(Clone, Debug)]
pub struct Repeat {
    pub node: Node,
    pub min: u32,
    pub max: Option<u32>,
    pub kind: RepKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefTarget {
    Number(u32),
    Name(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct BackRef {
    pub target: RefTarget,
    pub groups: Vec<u32>,
    pub fold: bool,
    pub restrict: bool,
    pub offset: usize,
}

#[derive(Clone, Debug)]
pub struct Recurse {
    pub target: RefTarget,
    pub group: u32,
    pub offset: usize,
    pub returns: Vec<(RefTarget, usize)>,
    pub ret_groups: Vec<u32>,
}

#[derive(Clone, Debug)]
pub enum CondKind {
    Group(RefTarget, Vec<u32>),
    RName(Vec<u8>, Vec<u32>),
    RNumber(Vec<u8>, Option<u32>, Vec<u32>),
    Define,
    Bool(bool),
    Assert(Box<Group>),
}

#[derive(Clone, Debug)]
pub struct Cond {
    pub kind: CondKind,
    pub branches: Vec<Vec<Node>>,
    pub offset: usize,
    pub name_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    Accept,
    Fail,
    Commit(bool),
    Prune(bool),
    Skip,
    SkipArg(Vec<u8>),
    Then(bool),
    Mark(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct NamedGroup {
    pub name: Vec<u8>,
    pub number: u32,
    pub isdup: bool,
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Ast {
    pub root: Group,
    pub capture_count: u32,
    pub names: Vec<NamedGroup>,
    pub utf: bool,
    pub ucp: bool,
    pub newline: Newline,
    pub bsr_anycrlf: bool,
    pub turkish: bool,
    pub match_limit: u32,
    pub depth_limit: u32,
    pub heap_limit: u32,
    pub notempty: bool,
    pub notempty_atstart: bool,
    pub no_auto_possess: bool,
    pub no_start_opt: bool,
    pub no_jit: bool,
    pub no_dotstar_anchor: bool,
    pub has_crorlf: bool,
    pub has_then: bool,
    pub has_skip_arg: bool,
    pub dupcap_used: bool,
    pub max_lookbehind: u32,
    pub dollar_endonly: bool,
}
