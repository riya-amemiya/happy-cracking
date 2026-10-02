use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use super::escape::{debug_os, unescape};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Personality {
    Grep,
    Rg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Arity {
    Switch,
    Value,
    Optional,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Id {
    Regexp,
    File,
    AfterContext,
    BeforeContext,
    Binary,
    BlockBuffered,
    ByteOffset,
    CaseSensitive,
    Color,
    Colors,
    Column,
    Context,
    ContextSeparator,
    Count,
    CountMatches,
    Crlf,
    Debug,
    DfaSizeLimit,
    Encoding,
    Engine,
    FieldContextSeparator,
    FieldMatchSeparator,
    Files,
    FilesWithMatches,
    FilesWithoutMatch,
    FixedStrings,
    Follow,
    Generate,
    Glob,
    GlobCaseInsensitive,
    Heading,
    Help,
    Hidden,
    HostnameBin,
    HyperlinkFormat,
    IGlob,
    IgnoreCase,
    IgnoreFile,
    IgnoreFileCaseInsensitive,
    IncludeZero,
    InvertMatch,
    Json,
    LineBuffered,
    LineNumber,
    LineNumberNo,
    LineRegexp,
    MaxColumns,
    MaxColumnsPreview,
    MaxCount,
    MaxDepth,
    MaxFilesize,
    Mmap,
    Multiline,
    MultilineDotall,
    NoConfig,
    NoIgnore,
    NoIgnoreDot,
    NoIgnoreExclude,
    NoIgnoreFiles,
    NoIgnoreGlobal,
    NoIgnoreMessages,
    NoIgnoreParent,
    NoIgnoreVcs,
    NoMessages,
    NoRequireGit,
    NoUnicode,
    Null,
    NullData,
    OneFileSystem,
    OnlyMatching,
    PathSeparator,
    Passthru,
    Pcre2,
    Pcre2Version,
    Pre,
    PreGlob,
    Pretty,
    Quiet,
    RegexSizeLimit,
    Replace,
    SearchZip,
    SmartCase,
    Sort,
    Sortr,
    Stats,
    StopOnNonmatch,
    Text,
    Threads,
    Trace,
    Trim,
    Type,
    TypeNot,
    TypeAdd,
    TypeClear,
    TypeList,
    Unrestricted,
    Version,
    Vimgrep,
    WithFilename,
    WithFilenameNo,
    WordRegexp,
    AutoHybridRegex,
    NoPcre2Unicode,
    SortFiles,
    BasicRegexp,
    ExtendedRegexp,
    PerlRegexp,
    Matcher,
    Recursive,
    DerefRecursive,
    Directories,
    Devices,
    Include,
    Exclude,
    ExcludeFrom,
    ExcludeDir,
    BinaryFiles,
    BinaryWithoutMatch,
    Label,
    InitialTab,
    NoIgnoreCase,
    UnixByteOffsets,
    DosBinary,
    GroupSeparator,
    NoGroupSeparator,
    GitIgnore,
}

struct Def {
    id: Id,
    long: &'static str,
    short: Option<u8>,
    negated: Option<&'static str>,
    aliases: &'static [&'static str],
    arity: Arity,
}

const fn d(
    id: Id,
    long: &'static str,
    short: Option<u8>,
    negated: Option<&'static str>,
    aliases: &'static [&'static str],
    arity: Arity,
) -> Def {
    Def {
        id,
        long,
        short,
        negated,
        aliases,
        arity,
    }
}

const S: Arity = Arity::Switch;
const V: Arity = Arity::Value;

const DEFS: &[Def] = &[
    d(Id::Regexp, "regexp", Some(b'e'), None, &[], V),
    d(Id::File, "file", Some(b'f'), None, &[], V),
    d(Id::AfterContext, "after-context", Some(b'A'), None, &[], V),
    d(
        Id::BeforeContext,
        "before-context",
        Some(b'B'),
        None,
        &[],
        V,
    ),
    d(Id::Binary, "binary", None, Some("no-binary"), &[], S),
    d(
        Id::BlockBuffered,
        "block-buffered",
        None,
        Some("no-block-buffered"),
        &[],
        S,
    ),
    d(
        Id::ByteOffset,
        "byte-offset",
        Some(b'b'),
        Some("no-byte-offset"),
        &[],
        S,
    ),
    d(
        Id::CaseSensitive,
        "case-sensitive",
        Some(b's'),
        None,
        &[],
        S,
    ),
    d(Id::Color, "color", None, None, &[], V),
    d(Id::Colors, "colors", None, None, &[], V),
    d(Id::Column, "column", None, Some("no-column"), &[], S),
    d(Id::Context, "context", Some(b'C'), None, &[], V),
    d(
        Id::ContextSeparator,
        "context-separator",
        None,
        Some("no-context-separator"),
        &[],
        V,
    ),
    d(Id::Count, "count", Some(b'c'), None, &[], S),
    d(Id::CountMatches, "count-matches", None, None, &[], S),
    d(Id::Crlf, "crlf", None, Some("no-crlf"), &[], S),
    d(Id::Debug, "debug", None, None, &[], S),
    d(Id::DfaSizeLimit, "dfa-size-limit", None, None, &[], V),
    d(
        Id::Encoding,
        "encoding",
        Some(b'E'),
        Some("no-encoding"),
        &[],
        V,
    ),
    d(Id::Engine, "engine", None, None, &[], V),
    d(
        Id::FieldContextSeparator,
        "field-context-separator",
        None,
        None,
        &[],
        V,
    ),
    d(
        Id::FieldMatchSeparator,
        "field-match-separator",
        None,
        None,
        &[],
        V,
    ),
    d(Id::Files, "files", None, None, &[], S),
    d(
        Id::FilesWithMatches,
        "files-with-matches",
        Some(b'l'),
        None,
        &[],
        S,
    ),
    d(
        Id::FilesWithoutMatch,
        "files-without-match",
        None,
        None,
        &[],
        S,
    ),
    d(
        Id::FixedStrings,
        "fixed-strings",
        Some(b'F'),
        Some("no-fixed-strings"),
        &["fixed-regexp"],
        S,
    ),
    d(Id::Follow, "follow", Some(b'L'), Some("no-follow"), &[], S),
    d(Id::Generate, "generate", None, None, &[], V),
    d(Id::Glob, "glob", Some(b'g'), None, &[], V),
    d(
        Id::GlobCaseInsensitive,
        "glob-case-insensitive",
        None,
        Some("no-glob-case-insensitive"),
        &[],
        S,
    ),
    d(Id::Heading, "heading", None, Some("no-heading"), &[], S),
    d(Id::Help, "help", Some(b'h'), None, &[], S),
    d(Id::Hidden, "hidden", Some(b'.'), Some("no-hidden"), &[], S),
    d(Id::HostnameBin, "hostname-bin", None, None, &[], V),
    d(Id::HyperlinkFormat, "hyperlink-format", None, None, &[], V),
    d(Id::IGlob, "iglob", None, None, &[], V),
    d(Id::IgnoreCase, "ignore-case", Some(b'i'), None, &[], S),
    d(Id::IgnoreFile, "ignore-file", None, None, &[], V),
    d(
        Id::IgnoreFileCaseInsensitive,
        "ignore-file-case-insensitive",
        None,
        Some("no-ignore-file-case-insensitive"),
        &[],
        S,
    ),
    d(
        Id::IncludeZero,
        "include-zero",
        None,
        Some("no-include-zero"),
        &[],
        S,
    ),
    d(
        Id::InvertMatch,
        "invert-match",
        Some(b'v'),
        Some("no-invert-match"),
        &[],
        S,
    ),
    d(Id::Json, "json", None, Some("no-json"), &[], S),
    d(
        Id::LineBuffered,
        "line-buffered",
        None,
        Some("no-line-buffered"),
        &[],
        S,
    ),
    d(Id::LineNumber, "line-number", Some(b'n'), None, &[], S),
    d(Id::LineNumberNo, "no-line-number", Some(b'N'), None, &[], S),
    d(Id::LineRegexp, "line-regexp", Some(b'x'), None, &[], S),
    d(Id::MaxColumns, "max-columns", Some(b'M'), None, &[], V),
    d(
        Id::MaxColumnsPreview,
        "max-columns-preview",
        None,
        Some("no-max-columns-preview"),
        &[],
        S,
    ),
    d(Id::MaxCount, "max-count", Some(b'm'), None, &[], V),
    d(
        Id::MaxDepth,
        "max-depth",
        Some(b'd'),
        None,
        &["maxdepth"],
        V,
    ),
    d(Id::MaxFilesize, "max-filesize", None, None, &[], V),
    d(Id::Mmap, "mmap", None, Some("no-mmap"), &[], S),
    d(
        Id::Multiline,
        "multiline",
        Some(b'U'),
        Some("no-multiline"),
        &[],
        S,
    ),
    d(
        Id::MultilineDotall,
        "multiline-dotall",
        None,
        Some("no-multiline-dotall"),
        &[],
        S,
    ),
    d(Id::NoConfig, "no-config", None, None, &[], S),
    d(Id::NoIgnore, "no-ignore", None, Some("ignore"), &[], S),
    d(
        Id::NoIgnoreDot,
        "no-ignore-dot",
        None,
        Some("ignore-dot"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreExclude,
        "no-ignore-exclude",
        None,
        Some("ignore-exclude"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreFiles,
        "no-ignore-files",
        None,
        Some("ignore-files"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreGlobal,
        "no-ignore-global",
        None,
        Some("ignore-global"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreMessages,
        "no-ignore-messages",
        None,
        Some("ignore-messages"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreParent,
        "no-ignore-parent",
        None,
        Some("ignore-parent"),
        &[],
        S,
    ),
    d(
        Id::NoIgnoreVcs,
        "no-ignore-vcs",
        None,
        Some("ignore-vcs"),
        &[],
        S,
    ),
    d(
        Id::NoMessages,
        "no-messages",
        None,
        Some("messages"),
        &[],
        S,
    ),
    d(
        Id::NoRequireGit,
        "no-require-git",
        None,
        Some("require-git"),
        &[],
        S,
    ),
    d(Id::NoUnicode, "no-unicode", None, Some("unicode"), &[], S),
    d(Id::Null, "null", Some(b'0'), None, &[], S),
    d(Id::NullData, "null-data", None, None, &[], S),
    d(
        Id::OneFileSystem,
        "one-file-system",
        None,
        Some("no-one-file-system"),
        &[],
        S,
    ),
    d(Id::OnlyMatching, "only-matching", Some(b'o'), None, &[], S),
    d(Id::PathSeparator, "path-separator", None, None, &[], V),
    d(Id::Passthru, "passthru", None, None, &["passthrough"], S),
    d(Id::Pcre2, "pcre2", Some(b'P'), Some("no-pcre2"), &[], S),
    d(Id::Pcre2Version, "pcre2-version", None, None, &[], S),
    d(Id::Pre, "pre", None, Some("no-pre"), &[], V),
    d(Id::PreGlob, "pre-glob", None, None, &[], V),
    d(Id::Pretty, "pretty", Some(b'p'), None, &[], S),
    d(Id::Quiet, "quiet", Some(b'q'), None, &["silent"], S),
    d(Id::RegexSizeLimit, "regex-size-limit", None, None, &[], V),
    d(Id::Replace, "replace", Some(b'r'), None, &[], V),
    d(
        Id::SearchZip,
        "search-zip",
        Some(b'z'),
        Some("no-search-zip"),
        &[],
        S,
    ),
    d(Id::SmartCase, "smart-case", Some(b'S'), None, &[], S),
    d(Id::Sort, "sort", None, None, &[], V),
    d(Id::Sortr, "sortr", None, None, &[], V),
    d(Id::Stats, "stats", None, Some("no-stats"), &[], S),
    d(Id::StopOnNonmatch, "stop-on-nonmatch", None, None, &[], S),
    d(Id::Text, "text", Some(b'a'), Some("no-text"), &[], S),
    d(Id::Threads, "threads", Some(b'j'), None, &[], V),
    d(Id::Trace, "trace", None, None, &[], S),
    d(Id::Trim, "trim", None, Some("no-trim"), &[], S),
    d(Id::Type, "type", Some(b't'), None, &[], V),
    d(Id::TypeNot, "type-not", Some(b'T'), None, &[], V),
    d(Id::TypeAdd, "type-add", None, None, &[], V),
    d(Id::TypeClear, "type-clear", None, None, &[], V),
    d(Id::TypeList, "type-list", None, None, &[], S),
    d(Id::Unrestricted, "unrestricted", Some(b'u'), None, &[], S),
    d(Id::Version, "version", Some(b'V'), None, &[], S),
    d(Id::Vimgrep, "vimgrep", None, None, &[], S),
    d(Id::WithFilename, "with-filename", Some(b'H'), None, &[], S),
    d(Id::WithFilenameNo, "no-filename", Some(b'I'), None, &[], S),
    d(Id::WordRegexp, "word-regexp", Some(b'w'), None, &[], S),
    d(
        Id::AutoHybridRegex,
        "auto-hybrid-regex",
        None,
        Some("no-auto-hybrid-regex"),
        &[],
        S,
    ),
    d(
        Id::NoPcre2Unicode,
        "no-pcre2-unicode",
        None,
        Some("pcre2-unicode"),
        &[],
        S,
    ),
    d(
        Id::SortFiles,
        "sort-files",
        None,
        Some("no-sort-files"),
        &[],
        S,
    ),
    d(Id::BasicRegexp, "basic-regexp", None, None, &[], S),
    d(Id::ExtendedRegexp, "extended-regexp", None, None, &[], S),
    d(Id::PerlRegexp, "perl-regexp", None, None, &[], S),
    d(Id::Recursive, "recursive", None, None, &[], S),
    d(
        Id::DerefRecursive,
        "dereference-recursive",
        None,
        None,
        &[],
        S,
    ),
    d(Id::Directories, "directories", None, None, &[], V),
    d(Id::Devices, "devices", None, None, &[], V),
    d(Id::Include, "include", None, None, &[], V),
    d(Id::Exclude, "exclude", None, None, &[], V),
    d(Id::ExcludeFrom, "exclude-from", None, None, &[], V),
    d(Id::ExcludeDir, "exclude-dir", None, None, &[], V),
    d(Id::BinaryFiles, "binary-files", None, None, &[], V),
    d(Id::Label, "label", None, None, &[], V),
    d(Id::InitialTab, "initial-tab", None, None, &[], S),
    d(Id::NoIgnoreCase, "no-ignore-case", None, None, &[], S),
    d(Id::GroupSeparator, "group-separator", None, None, &[], V),
    d(
        Id::NoGroupSeparator,
        "no-group-separator",
        None,
        None,
        &[],
        S,
    ),
    d(Id::GitIgnore, "gitignore", None, None, &[], S),
];

const GREP_ONLY_LONG: &[(&str, Id)] = &[("colour", Id::Color)];

const GNU_LONG: &[&str] = &[
    "basic-regexp",
    "extended-regexp",
    "fixed-regexp",
    "fixed-strings",
    "perl-regexp",
    "after-context",
    "before-context",
    "binary-files",
    "byte-offset",
    "context",
    "color",
    "colour",
    "count",
    "devices",
    "directories",
    "exclude",
    "exclude-from",
    "exclude-dir",
    "file",
    "files-with-matches",
    "files-without-match",
    "group-separator",
    "help",
    "include",
    "ignore-case",
    "no-ignore-case",
    "initial-tab",
    "label",
    "line-buffered",
    "line-number",
    "line-regexp",
    "max-count",
    "no-filename",
    "no-group-separator",
    "no-messages",
    "null",
    "null-data",
    "only-matching",
    "quiet",
    "recursive",
    "dereference-recursive",
    "regexp",
    "invert-match",
    "silent",
    "text",
    "binary",
    "version",
    "with-filename",
    "word-regexp",
];

const GREP_SWITCHES_FOR_LONG_CONFLICTS: &[(&str, Id)] = &[("binary", Id::DosBinary)];

#[cfg(test)]
pub(crate) fn long_flag_names() -> Vec<&'static str> {
    long_entries(Personality::Rg)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

#[cfg(test)]
pub(crate) fn gnu_long_flag_names() -> &'static [&'static str] {
    GNU_LONG
}

pub(crate) fn grep_only_long_names() -> impl Iterator<Item = &'static str> {
    DEFS.iter()
        .skip_while(|def| def.id != Id::BasicRegexp)
        .map(|def| def.long)
}

#[cfg(test)]
pub(crate) fn grep_short_letters() -> Vec<u8> {
    (b'0'..=b'z')
        .chain(std::iter::once(b'.'))
        .filter(|&ch| ch.is_ascii_digit() || grep_short(ch).is_some() || ch == b'V')
        .collect()
}

fn def_of(id: Id) -> &'static Def {
    DEFS.iter()
        .find(|def| def.id == id)
        .expect("every flag id has a definition")
}

fn grep_short(ch: u8) -> Option<Id> {
    Some(match ch {
        b'A' => Id::AfterContext,
        b'B' => Id::BeforeContext,
        b'C' => Id::Context,
        b'D' => Id::Devices,
        b'E' => Id::ExtendedRegexp,
        b'F' => Id::FixedStrings,
        b'G' => Id::BasicRegexp,
        b'H' => Id::WithFilename,
        b'I' => Id::BinaryWithoutMatch,
        b'L' => Id::FilesWithoutMatch,
        b'M' => Id::MaxColumns,
        b'N' => Id::LineNumberNo,
        b'P' => Id::PerlRegexp,
        b'R' => Id::DerefRecursive,
        b'S' => Id::SmartCase,
        b'T' => Id::InitialTab,
        b'U' => Id::DosBinary,
        b'V' => Id::Version,
        b'X' => Id::Matcher,
        b'Z' => Id::Null,
        b'a' => Id::Text,
        b'b' => Id::ByteOffset,
        b'c' => Id::Count,
        b'd' => Id::Directories,
        b'e' => Id::Regexp,
        b'f' => Id::File,
        b'g' => Id::Glob,
        b'h' => Id::WithFilenameNo,
        b'i' | b'y' => Id::IgnoreCase,
        b'j' => Id::Threads,
        b'l' => Id::FilesWithMatches,
        b'm' => Id::MaxCount,
        b'n' => Id::LineNumber,
        b'o' => Id::OnlyMatching,
        b'p' => Id::Pretty,
        b'q' => Id::Quiet,
        b'r' => Id::Recursive,
        b's' => Id::NoMessages,
        b't' => Id::Type,
        b'u' => Id::UnixByteOffsets,
        b'v' => Id::InvertMatch,
        b'w' => Id::WordRegexp,
        b'x' => Id::LineRegexp,
        b'z' => Id::NullData,
        b'.' => Id::Hidden,
        _ => return None,
    })
}

fn rg_short(ch: u8) -> Option<Id> {
    if let Some(def) = DEFS.iter().find(|def| def.short == Some(ch)) {
        return Some(match def.id {
            Id::MaxDepth => Id::Directories,
            other => other,
        });
    }
    Some(match ch {
        b'D' => Id::Devices,
        b'G' => Id::BasicRegexp,
        b'R' => Id::DerefRecursive,
        b'X' => Id::Matcher,
        b'Z' => Id::Null,
        b'y' => Id::IgnoreCase,
        _ => return None,
    })
}

fn arity(id: Id, personality: Personality) -> Arity {
    match id {
        Id::Color if personality == Personality::Grep => Arity::Optional,
        Id::Matcher => Arity::Value,
        Id::BinaryWithoutMatch | Id::UnixByteOffsets | Id::DosBinary => Arity::Switch,
        _ => def_of(id).arity,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Standard,
    Negated,
}

#[derive(Clone, Debug)]
struct Lookup {
    id: Id,
    kind: Kind,
    display: String,
}

fn long_entries(personality: Personality) -> Vec<(&'static str, Id, Kind)> {
    let mut out = Vec::with_capacity(DEFS.len() * 2);
    for def in DEFS {
        out.push((def.long, def.id, Kind::Standard));
        out.extend(def.aliases.iter().map(|&a| (a, def.id, Kind::Standard)));
        if let Some(neg) = def.negated {
            out.push((neg, def.id, Kind::Negated));
        }
    }
    out.extend(
        GREP_ONLY_LONG
            .iter()
            .map(|&(name, id)| (name, id, Kind::Standard)),
    );
    if personality == Personality::Grep {
        for &(name, id) in GREP_SWITCHES_FOR_LONG_CONFLICTS {
            out.retain(|entry| entry.0 != name);
            out.push((name, id, Kind::Standard));
        }
    }
    out
}

enum LongMatch {
    Found(Lookup),
    Unknown,
    Ambiguous(Vec<&'static str>),
}

fn find_long(name: &str, personality: Personality) -> LongMatch {
    let entries = long_entries(personality);
    if let Some(&(long, id, kind)) = entries.iter().find(|e| e.0 == name) {
        return LongMatch::Found(Lookup {
            id,
            kind,
            display: format!("--{long}"),
        });
    }
    if personality != Personality::Grep || name.is_empty() {
        return LongMatch::Unknown;
    }
    let mut gnu: Vec<&(&str, Id, Kind)> = entries
        .iter()
        .filter(|e| GNU_LONG.contains(&e.0) && e.0.starts_with(name))
        .collect();
    gnu.sort_by_key(|e| GNU_LONG.iter().position(|g| *g == e.0));
    let candidates: Vec<&(&str, Id, Kind)> = if gnu.is_empty() {
        entries.iter().filter(|e| e.0.starts_with(name)).collect()
    } else {
        gnu
    };
    let Some(first) = candidates.first() else {
        return LongMatch::Unknown;
    };
    if candidates.iter().all(|e| e.1 == first.1 && e.2 == first.2) {
        return LongMatch::Found(Lookup {
            id: first.1,
            kind: first.2,
            display: format!("--{}", first.0),
        });
    }
    LongMatch::Ambiguous(candidates.iter().map(|e| e.0).collect())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Special {
    #[default]
    HelpShort,
    HelpLong,
    VersionShort,
    VersionLong,
    VersionPcre2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SearchMode {
    Standard,
    FilesWithMatches,
    FilesWithoutMatch,
    Count,
    CountMatches,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GenerateMode {
    Man,
    CompleteBash,
    CompleteZsh,
    CompleteFish,
    CompletePowerShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Search(SearchMode),
    Files,
    Types,
    Generate(GenerateMode),
}

impl Default for Mode {
    fn default() -> Mode {
        Mode::Search(SearchMode::Standard)
    }
}

impl Mode {
    fn update(&mut self, new: Mode) {
        if matches!(*self, Mode::Search(_)) || !matches!(new, Mode::Search(_)) {
            *self = new;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BinaryMode {
    #[default]
    Auto,
    SearchAndSuppress,
    AsText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryMode {
    Line,
    Word,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BufferMode {
    #[default]
    Auto,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CaseMode {
    #[default]
    Sensitive,
    Insensitive,
    Smart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorChoice {
    Never,
    Auto,
    Always,
    Ansi,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ContextLimited {
    before: Option<usize>,
    after: Option<usize>,
    both: Option<usize>,
}

impl ContextLimited {
    pub(crate) fn get(&self) -> (usize, usize) {
        let base = self.both.unwrap_or(0);
        (self.before.unwrap_or(base), self.after.unwrap_or(base))
    }

    pub(crate) fn is_set(&self) -> bool {
        self.before.is_some() || self.after.is_some() || self.both.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextMode {
    Passthru,
    Limited(ContextLimited),
}

impl Default for ContextMode {
    fn default() -> ContextMode {
        ContextMode::Limited(ContextLimited::default())
    }
}

impl ContextMode {
    fn limited(&mut self) -> &mut ContextLimited {
        if *self == ContextMode::Passthru {
            *self = ContextMode::Limited(ContextLimited::default());
        }
        match self {
            ContextMode::Limited(limited) => limited,
            ContextMode::Passthru => unreachable!("passthru was replaced"),
        }
    }

    fn set_before(&mut self, lines: usize) {
        self.limited().before = Some(lines);
    }

    fn set_after(&mut self, lines: usize) {
        self.limited().after = Some(lines);
    }

    fn set_both(&mut self, lines: usize) {
        self.limited().both = Some(lines);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EncodingMode {
    Auto,
    Label(Deferred),
    Disabled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum EngineChoice {
    #[default]
    Default,
    Auto,
    Pcre2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GnuMatcher {
    Grep,
    Egrep,
    Fgrep,
    Perl,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LoggingMode {
    #[default]
    Debug,
    Trace,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MmapMode {
    #[default]
    Auto,
    AlwaysTryMmap,
    Never,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PatternSource {
    Regexp(OsString),
    File(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SortModeKind {
    Path,
    LastModified,
    LastAccessed,
    Created,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SortMode {
    pub(crate) reverse: bool,
    pub(crate) kind: SortModeKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeChange {
    Clear { name: String },
    Add { def: String },
    Select { name: String },
    Negate { name: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DirAction {
    Read,
    Skip,
    Recurse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DevAction {
    Read,
    Skip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GnuBinaryFiles {
    Binary,
    Text,
    WithoutMatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExcludeRule {
    Include(OsString),
    Exclude(OsString),
    ExcludeFrom(PathBuf),
    ExcludeDir(OsString),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Separator {
    Escaped(Vec<u8>),
    Literal(Vec<u8>),
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Deferred {
    pub(crate) flag: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct LowArgs {
    pub(crate) special: Option<Special>,
    pub(crate) mode: Mode,
    pub(crate) positional: Vec<OsString>,
    pub(crate) patterns: Vec<PatternSource>,
    pub(crate) binary: BinaryMode,
    pub(crate) boundary: Option<BoundaryMode>,
    pub(crate) buffer: BufferMode,
    pub(crate) byte_offset: bool,
    pub(crate) case: CaseMode,
    pub(crate) color: Option<ColorChoice>,
    pub(crate) colors: Vec<Deferred>,
    pub(crate) column: Option<bool>,
    pub(crate) context: ContextMode,
    pub(crate) context_separator: Option<Separator>,
    pub(crate) crlf: bool,
    pub(crate) dfa_size_limit: Option<usize>,
    pub(crate) encoding: Option<EncodingMode>,
    pub(crate) engine: EngineChoice,
    pub(crate) field_context_separator: Option<Vec<u8>>,
    pub(crate) field_match_separator: Option<Vec<u8>>,
    pub(crate) fixed_strings: bool,
    pub(crate) follow: bool,
    pub(crate) glob_case_insensitive: bool,
    pub(crate) globs: Vec<String>,
    pub(crate) heading: Option<bool>,
    pub(crate) hidden: bool,
    pub(crate) hostname_bin: Option<PathBuf>,
    pub(crate) hyperlink_format: Option<Deferred>,
    pub(crate) iglobs: Vec<String>,
    pub(crate) ignore_file: Vec<PathBuf>,
    pub(crate) ignore_file_case_insensitive: bool,
    pub(crate) include_zero: Option<bool>,
    pub(crate) invert_match: bool,
    pub(crate) line_number: Option<bool>,
    pub(crate) logging: Option<LoggingMode>,
    pub(crate) max_columns: Option<u64>,
    pub(crate) max_columns_preview: bool,
    pub(crate) max_count: Option<u64>,
    pub(crate) max_depth: Option<usize>,
    pub(crate) max_filesize: Option<u64>,
    pub(crate) mmap: MmapMode,
    pub(crate) multiline: bool,
    pub(crate) multiline_dotall: bool,
    pub(crate) no_config: bool,
    pub(crate) no_ignore_dot: bool,
    pub(crate) no_ignore_exclude: bool,
    pub(crate) no_ignore_files: bool,
    pub(crate) no_ignore_global: bool,
    pub(crate) no_ignore_messages: bool,
    pub(crate) no_ignore_parent: bool,
    pub(crate) no_ignore_vcs: bool,
    pub(crate) no_messages: bool,
    pub(crate) no_require_git: bool,
    pub(crate) no_unicode: bool,
    pub(crate) null: bool,
    pub(crate) null_data: bool,
    pub(crate) one_file_system: bool,
    pub(crate) only_matching: bool,
    pub(crate) path_separator: Option<u8>,
    pub(crate) pre: Option<PathBuf>,
    pub(crate) pre_glob: Vec<String>,
    pub(crate) quiet: bool,
    pub(crate) regex_size_limit: Option<usize>,
    pub(crate) replace: Option<Vec<u8>>,
    pub(crate) search_zip: bool,
    pub(crate) sort: Option<SortMode>,
    pub(crate) stats: bool,
    pub(crate) stop_on_nonmatch: bool,
    pub(crate) threads: Option<usize>,
    pub(crate) trim: bool,
    pub(crate) type_changes: Vec<TypeChange>,
    pub(crate) unrestricted: usize,
    pub(crate) vimgrep: bool,
    pub(crate) with_filename: Option<bool>,
    pub(crate) gnu_matcher: Option<GnuMatcher>,
    pub(crate) directories: Option<DirAction>,
    pub(crate) recursive_given: bool,
    pub(crate) deref_recursive: bool,
    pub(crate) devices: Option<DevAction>,
    pub(crate) excludes: Vec<ExcludeRule>,
    pub(crate) gnu_binary_files: Option<GnuBinaryFiles>,
    pub(crate) label: Option<OsString>,
    pub(crate) initial_tab: bool,
    pub(crate) unix_byte_offsets: bool,
    pub(crate) gnu_invalid_color: bool,
    pub(crate) rg_only_output: bool,
    pub(crate) gitignore: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ArgError {
    Rg(String),
    GnuUsage(String),
    GnuDie(String),
    GnuArgmatch {
        value: String,
        option: &'static str,
        choices: &'static [&'static str],
        ambiguous: bool,
    },
}

pub(crate) type ArgResult<T> = Result<T, ArgError>;

#[cfg(unix)]
pub(crate) fn os_bytes(value: &OsStr) -> &[u8] {
    value.as_bytes()
}

#[cfg(not(unix))]
pub(crate) fn os_bytes(value: &OsStr) -> &[u8] {
    value.as_encoded_bytes()
}

#[cfg(unix)]
fn bytes_os(bytes: Vec<u8>) -> OsString {
    OsString::from_vec(bytes)
}

#[cfg(not(unix))]
fn bytes_os(bytes: Vec<u8>) -> OsString {
    OsString::from(String::from_utf8_lossy(&bytes).into_owned())
}

struct Ctx<'a> {
    personality: Personality,
    display: &'a str,
}

impl Ctx<'_> {
    fn rg_err(&self, msg: impl std::fmt::Display) -> ArgError {
        ArgError::Rg(format!("error parsing flag {}: {msg}", self.display))
    }

    fn str<'v>(&self, value: &'v OsStr) -> ArgResult<&'v str> {
        value
            .to_str()
            .ok_or_else(|| self.rg_err("value is not valid UTF-8"))
    }

    fn string(&self, value: &OsStr) -> ArgResult<String> {
        self.str(value).map(str::to_owned)
    }

    fn number<T: std::str::FromStr>(&self, value: &OsStr) -> ArgResult<T>
    where
        T::Err: std::fmt::Display,
    {
        self.str(value)?
            .parse()
            .map_err(|e| self.rg_err(format!("value is not a valid number: {e}")))
    }

    fn human_u64(&self, value: &OsStr) -> ArgResult<u64> {
        parse_human_readable_size(self.str(value)?)
            .map_err(|e| self.rg_err(format!("invalid size: {e}")))
    }

    fn human_usize(&self, value: &OsStr) -> ArgResult<usize> {
        usize::try_from(self.human_u64(value)?).map_err(|_| self.rg_err("size is too big"))
    }

    fn gnu_context(&self, value: &OsStr) -> ArgResult<usize> {
        if self.personality == Personality::Rg {
            return self.number(value);
        }
        parse_gnu_context(value).ok_or_else(|| {
            ArgError::GnuDie(format!(
                "{}: invalid context length argument",
                String::from_utf8_lossy(os_bytes(value))
            ))
        })
    }
}

fn parse_gnu_context(value: &OsStr) -> Option<usize> {
    let text = value.to_str()?;
    let digits = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(
        digits
            .parse::<u128>()
            .map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX)),
    )
}

fn parse_gnu_max_count(value: &OsStr) -> ArgResult<Option<u64>> {
    let invalid = || ArgError::GnuDie("invalid max count".into());
    let text = value
        .to_str()
        .ok_or_else(invalid)?
        .trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    if negative {
        return Ok(None);
    }
    Ok(Some(digits.parse::<u64>().unwrap_or(u64::MAX)))
}

pub(crate) fn parse_human_readable_size(size: &str) -> Result<u64, String> {
    let digits_end = size.bytes().take_while(u8::is_ascii_digit).count();
    let digits = &size[..digits_end];
    if digits.is_empty() {
        return Err(format!(
            "invalid format for size '{size}', which should be a non-empty \
             sequence of digits followed by an optional 'K', 'M' or 'G' suffix"
        ));
    }
    let value: u64 = digits
        .parse()
        .map_err(|e| format!("invalid integer found in size '{size}': {e}"))?;
    let shift = match &size[digits_end..] {
        "" => return Ok(value),
        "K" => 10,
        "M" => 20,
        "G" => 30,
        _ => {
            return Err(format!(
                "invalid format for size '{size}', which should be a non-empty \
                 sequence of digits followed by an optional 'K', 'M' or 'G' suffix"
            ));
        }
    };
    value
        .checked_mul(1 << shift)
        .ok_or_else(|| format!("size too big in '{size}'"))
}

fn set_gnu_matcher(low: &mut LowArgs, ctx: &Ctx<'_>, m: GnuMatcher) -> ArgResult<()> {
    if ctx.personality == Personality::Grep
        && let Some(prev) = low.gnu_matcher
        && prev != m
    {
        return Err(ArgError::GnuDie("conflicting matchers specified".into()));
    }
    low.gnu_matcher = Some(m);
    match m {
        GnuMatcher::Fgrep => {
            low.fixed_strings = true;
            low.engine = EngineChoice::Default;
        }
        GnuMatcher::Perl => {
            low.fixed_strings = false;
            low.engine = EngineChoice::Pcre2;
        }
        GnuMatcher::Grep | GnuMatcher::Egrep => {
            low.fixed_strings = false;
            low.engine = EngineChoice::Default;
        }
    }
    Ok(())
}

const DIRECTORIES_CHOICES: &[&str] = &["read", "recurse", "skip"];
const BINARY_FILES_CHOICES: &[&str] = &["binary", "text", "without-match"];

fn rg_mark(low: &mut LowArgs, ctx: &Ctx<'_>) {
    if ctx.personality == Personality::Grep {
        low.rg_only_output = true;
    }
}

fn update(
    low: &mut LowArgs,
    ctx: &Ctx<'_>,
    id: Id,
    switch: bool,
    value: Option<&OsStr>,
) -> ArgResult<()> {
    let val = || value.expect("value flags carry a value");
    match id {
        Id::AfterContext => low.context.set_after(ctx.gnu_context(val())?),
        Id::BeforeContext => low.context.set_before(ctx.gnu_context(val())?),
        Id::Context => low.context.set_both(ctx.gnu_context(val())?),
        Id::AutoHybridRegex => {
            low.engine = if switch {
                EngineChoice::Auto
            } else {
                EngineChoice::Default
            };
        }
        Id::Binary => {
            low.binary = if switch {
                BinaryMode::SearchAndSuppress
            } else {
                BinaryMode::Auto
            };
        }
        Id::BlockBuffered => {
            low.buffer = if switch {
                BufferMode::Block
            } else {
                BufferMode::Auto
            };
        }
        Id::LineBuffered => {
            low.buffer = if switch {
                BufferMode::Line
            } else {
                BufferMode::Auto
            };
        }
        Id::ByteOffset => low.byte_offset = switch,
        Id::CaseSensitive | Id::NoIgnoreCase => low.case = CaseMode::Sensitive,
        Id::IgnoreCase => low.case = CaseMode::Insensitive,
        Id::SmartCase => low.case = CaseMode::Smart,
        Id::Color => {
            low.color = Some(match value {
                None => ColorChoice::Auto,
                Some(v) if ctx.personality == Personality::Grep => {
                    match v.to_str().map(str::to_ascii_lowercase).as_deref() {
                        Some("always" | "yes" | "force") => ColorChoice::Always,
                        Some("never" | "no" | "none") => ColorChoice::Never,
                        Some("auto" | "tty" | "if-tty") => ColorChoice::Auto,
                        Some("ansi") => ColorChoice::Ansi,
                        _ => {
                            low.gnu_invalid_color = true;
                            ColorChoice::Never
                        }
                    }
                }
                Some(v) => match ctx.str(v)? {
                    "never" => ColorChoice::Never,
                    "auto" => ColorChoice::Auto,
                    "always" => ColorChoice::Always,
                    "ansi" => ColorChoice::Ansi,
                    unk => return Err(ctx.rg_err(format!("choice '{unk}' is unrecognized"))),
                },
            });
        }
        Id::Colors => {
            rg_mark(low, ctx);
            low.colors.push(Deferred {
                flag: ctx.display.to_string(),
                value: ctx.string(val())?,
            });
        }
        Id::Column => {
            rg_mark(low, ctx);
            low.column = Some(switch);
        }
        Id::ContextSeparator => {
            low.context_separator = Some(match value {
                None => Separator::Disabled,
                Some(v) => Separator::Escaped(unescape(ctx.str(v).map_err(|_| {
                    ctx.rg_err(
                        "separator must be valid UTF-8 (use escape sequences \
                         to provide a separator that is not valid UTF-8)",
                    )
                })?)),
            });
        }
        Id::GroupSeparator => {
            low.context_separator = Some(Separator::Literal(os_bytes(val()).to_vec()));
        }
        Id::NoGroupSeparator => low.context_separator = Some(Separator::Disabled),
        Id::GitIgnore => low.gitignore = true,
        Id::Count => low.mode.update(Mode::Search(SearchMode::Count)),
        Id::CountMatches => {
            rg_mark(low, ctx);
            low.mode.update(Mode::Search(SearchMode::CountMatches));
        }
        Id::Crlf => {
            low.crlf = switch;
            if switch {
                low.null_data = false;
            }
        }
        Id::Debug => low.logging = Some(LoggingMode::Debug),
        Id::Trace => low.logging = Some(LoggingMode::Trace),
        Id::DfaSizeLimit => low.dfa_size_limit = Some(ctx.human_usize(val())?),
        Id::RegexSizeLimit => low.regex_size_limit = Some(ctx.human_usize(val())?),
        Id::Encoding => {
            low.encoding = Some(match value {
                None => EncodingMode::Auto,
                Some(v) => match ctx.str(v)? {
                    "auto" => EncodingMode::Auto,
                    "none" => EncodingMode::Disabled,
                    label => EncodingMode::Label(Deferred {
                        flag: ctx.display.to_string(),
                        value: label.to_string(),
                    }),
                },
            });
        }
        Id::Engine => {
            low.engine = match ctx.str(val())? {
                "default" => EngineChoice::Default,
                "pcre2" => EngineChoice::Pcre2,
                "auto" => EngineChoice::Auto,
                other => {
                    return Err(ctx.rg_err(format!("unrecognized regex engine '{other}'")));
                }
            };
        }
        Id::FieldContextSeparator | Id::FieldMatchSeparator => {
            rg_mark(low, ctx);
            let bytes = unescape(ctx.str(val()).map_err(|_| {
                ctx.rg_err(
                    "separator must be valid UTF-8 (use escape sequences \
                     to provide a separator that is not valid UTF-8)",
                )
            })?);
            if id == Id::FieldContextSeparator {
                low.field_context_separator = Some(bytes);
            } else {
                low.field_match_separator = Some(bytes);
            }
        }
        Id::File => low.patterns.push(PatternSource::File(PathBuf::from(val()))),
        Id::Regexp => {
            if ctx.personality == Personality::Rg {
                ctx.str(val())?;
            }
            low.patterns
                .push(PatternSource::Regexp(val().to_os_string()));
        }
        Id::Files => low.mode.update(Mode::Files),
        Id::FilesWithMatches => low.mode.update(Mode::Search(SearchMode::FilesWithMatches)),
        Id::FilesWithoutMatch => low.mode.update(Mode::Search(SearchMode::FilesWithoutMatch)),
        Id::FixedStrings => {
            if switch && ctx.personality == Personality::Grep {
                set_gnu_matcher(low, ctx, GnuMatcher::Fgrep)?;
            }
            if !switch && low.gnu_matcher == Some(GnuMatcher::Fgrep) {
                low.gnu_matcher = None;
            }
            low.fixed_strings = switch;
        }
        Id::BasicRegexp => set_gnu_matcher(low, ctx, GnuMatcher::Grep)?,
        Id::ExtendedRegexp => set_gnu_matcher(low, ctx, GnuMatcher::Egrep)?,
        Id::PerlRegexp => set_gnu_matcher(low, ctx, GnuMatcher::Perl)?,
        Id::Pcre2 => {
            if switch && ctx.personality == Personality::Grep {
                set_gnu_matcher(low, ctx, GnuMatcher::Perl)?;
            }
            low.engine = if switch {
                EngineChoice::Pcre2
            } else {
                EngineChoice::Default
            };
        }
        Id::Matcher => {
            let m = match os_bytes(val()) {
                b"grep" => GnuMatcher::Grep,
                b"egrep" | b"awk" | b"gawk" | b"posixawk" => GnuMatcher::Egrep,
                b"fgrep" => GnuMatcher::Fgrep,
                b"perl" => GnuMatcher::Perl,
                other => {
                    return Err(ArgError::GnuDie(format!(
                        "invalid matcher {}",
                        String::from_utf8_lossy(other)
                    )));
                }
            };
            set_gnu_matcher(low, ctx, m)?;
        }
        Id::Follow => low.follow = switch,
        Id::Generate => {
            let mode = match ctx.str(val())? {
                "man" => GenerateMode::Man,
                "complete-bash" => GenerateMode::CompleteBash,
                "complete-zsh" => GenerateMode::CompleteZsh,
                "complete-fish" => GenerateMode::CompleteFish,
                "complete-powershell" => GenerateMode::CompletePowerShell,
                unk => return Err(ctx.rg_err(format!("choice '{unk}' is unrecognized"))),
            };
            low.mode.update(Mode::Generate(mode));
        }
        Id::Glob => low.globs.push(ctx.string(val())?),
        Id::IGlob => low.iglobs.push(ctx.string(val())?),
        Id::GlobCaseInsensitive => low.glob_case_insensitive = switch,
        Id::Heading => {
            rg_mark(low, ctx);
            low.heading = Some(switch);
        }
        Id::Help | Id::Version | Id::DosBinary => {}
        Id::Hidden => low.hidden = switch,
        Id::HostnameBin => {
            let path = PathBuf::from(val());
            low.hostname_bin = (!path.as_os_str().is_empty()).then_some(path);
        }
        Id::HyperlinkFormat => {
            rg_mark(low, ctx);
            low.hyperlink_format = Some(Deferred {
                flag: ctx.display.to_string(),
                value: ctx.string(val())?,
            });
        }
        Id::IgnoreFile => low.ignore_file.push(PathBuf::from(val())),
        Id::IgnoreFileCaseInsensitive => low.ignore_file_case_insensitive = switch,
        Id::IncludeZero => {
            rg_mark(low, ctx);
            low.include_zero = Some(switch);
        }
        Id::InvertMatch => low.invert_match = switch,
        Id::Json => {
            rg_mark(low, ctx);
            if switch {
                low.mode.update(Mode::Search(SearchMode::Json));
            } else if low.mode == Mode::Search(SearchMode::Json) {
                low.mode.update(Mode::Search(SearchMode::Standard));
            }
        }
        Id::LineNumber => low.line_number = Some(true),
        Id::LineNumberNo => low.line_number = Some(false),
        Id::LineRegexp => low.boundary = Some(BoundaryMode::Line),
        Id::WordRegexp => low.boundary = Some(BoundaryMode::Word),
        Id::MaxColumns => {
            rg_mark(low, ctx);
            let max: u64 = ctx.number(val())?;
            low.max_columns = (max != 0).then_some(max);
        }
        Id::MaxColumnsPreview => {
            rg_mark(low, ctx);
            low.max_columns_preview = switch;
        }
        Id::MaxCount => {
            if ctx.personality == Personality::Grep {
                low.max_count = parse_gnu_max_count(val())?;
            } else {
                low.max_count = Some(ctx.number(val())?);
            }
        }
        Id::MaxDepth => low.max_depth = Some(ctx.number(val())?),
        Id::Directories => {
            let raw = val();
            let text = raw.to_str().unwrap_or("");
            if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
                low.max_depth = Some(ctx.number(raw)?);
            } else {
                let candidates: Vec<&str> = DIRECTORIES_CHOICES
                    .iter()
                    .copied()
                    .filter(|choice| !text.is_empty() && choice.starts_with(text))
                    .collect();
                let exact = DIRECTORIES_CHOICES.iter().copied().find(|c| *c == text);
                let chosen = exact.or(match candidates.as_slice() {
                    [only] => Some(*only),
                    _ => None,
                });
                let action = match chosen {
                    Some("read") => DirAction::Read,
                    Some("skip") => DirAction::Skip,
                    Some(_) => DirAction::Recurse,
                    None if ctx.personality == Personality::Rg && ctx.display == "-d" => {
                        let _: usize = ctx.number(raw)?;
                        unreachable!("non-numeric values fail to parse")
                    }
                    None => {
                        return Err(ArgError::GnuArgmatch {
                            value: String::from_utf8_lossy(os_bytes(raw)).into_owned(),
                            option: "--directories",
                            choices: DIRECTORIES_CHOICES,
                            ambiguous: candidates.len() > 1,
                        });
                    }
                };
                if action == DirAction::Recurse {
                    low.recursive_given = true;
                }
                low.directories = Some(action);
            }
        }
        Id::Recursive => {
            low.directories = Some(DirAction::Recurse);
            low.recursive_given = true;
        }
        Id::DerefRecursive => {
            low.directories = Some(DirAction::Recurse);
            low.recursive_given = true;
            low.deref_recursive = true;
        }
        Id::Devices => {
            low.devices = Some(match os_bytes(val()) {
                b"read" => DevAction::Read,
                b"skip" => DevAction::Skip,
                _ => return Err(ArgError::GnuDie("unknown devices method".into())),
            });
        }
        Id::Include => low
            .excludes
            .push(ExcludeRule::Include(val().to_os_string())),
        Id::Exclude => low
            .excludes
            .push(ExcludeRule::Exclude(val().to_os_string())),
        Id::ExcludeFrom => low
            .excludes
            .push(ExcludeRule::ExcludeFrom(PathBuf::from(val()))),
        Id::ExcludeDir => low
            .excludes
            .push(ExcludeRule::ExcludeDir(val().to_os_string())),
        Id::BinaryFiles => {
            low.gnu_binary_files = Some(match os_bytes(val()) {
                b"binary" => GnuBinaryFiles::Binary,
                b"text" => GnuBinaryFiles::Text,
                b"without-match" => GnuBinaryFiles::WithoutMatch,
                _ if ctx.personality == Personality::Grep => {
                    return Err(ArgError::GnuDie("unknown binary-files type".into()));
                }
                other => {
                    return Err(ArgError::GnuArgmatch {
                        value: String::from_utf8_lossy(other).into_owned(),
                        option: "--binary-files",
                        choices: BINARY_FILES_CHOICES,
                        ambiguous: false,
                    });
                }
            });
        }
        Id::BinaryWithoutMatch => low.gnu_binary_files = Some(GnuBinaryFiles::WithoutMatch),
        Id::Label => low.label = Some(val().to_os_string()),
        Id::InitialTab => low.initial_tab = true,
        Id::UnixByteOffsets => low.unix_byte_offsets = true,
        Id::MaxFilesize => low.max_filesize = Some(ctx.human_u64(val())?),
        Id::Mmap => {
            low.mmap = if switch {
                MmapMode::AlwaysTryMmap
            } else {
                MmapMode::Never
            };
        }
        Id::Multiline => {
            rg_mark(low, ctx);
            low.multiline = switch;
            if switch {
                low.stop_on_nonmatch = false;
            }
        }
        Id::MultilineDotall => low.multiline_dotall = switch,
        Id::NoConfig => low.no_config = true,
        Id::NoIgnore => {
            low.no_ignore_dot = switch;
            low.no_ignore_exclude = switch;
            low.no_ignore_global = switch;
            low.no_ignore_parent = switch;
            low.no_ignore_vcs = switch;
        }
        Id::NoIgnoreDot => low.no_ignore_dot = switch,
        Id::NoIgnoreExclude => low.no_ignore_exclude = switch,
        Id::NoIgnoreFiles => low.no_ignore_files = switch,
        Id::NoIgnoreGlobal => low.no_ignore_global = switch,
        Id::NoIgnoreMessages => low.no_ignore_messages = switch,
        Id::NoIgnoreParent => low.no_ignore_parent = switch,
        Id::NoIgnoreVcs => low.no_ignore_vcs = switch,
        Id::NoMessages => low.no_messages = switch,
        Id::NoPcre2Unicode | Id::NoUnicode => low.no_unicode = switch,
        Id::NoRequireGit => low.no_require_git = switch,
        Id::Null => low.null = true,
        Id::NullData => {
            low.crlf = false;
            low.null_data = true;
        }
        Id::OneFileSystem => low.one_file_system = switch,
        Id::OnlyMatching => low.only_matching = true,
        Id::PathSeparator => {
            rg_mark(low, ctx);
            let s = ctx.string(val())?;
            let raw = unescape(&s);
            low.path_separator = match raw.len() {
                0 => None,
                1 => Some(raw[0]),
                len => {
                    return Err(ctx.rg_err(format!(
                        "A path separator must be exactly one byte, but \
                         the given separator is {len} bytes: {s}\n\
                         In some shells on Windows '/' is automatically \
                         expanded. Use '//' instead."
                    )));
                }
            };
        }
        Id::Passthru => {
            rg_mark(low, ctx);
            low.context = ContextMode::Passthru;
        }
        Id::Pcre2Version => low.special = Some(Special::VersionPcre2),
        Id::Pre => {
            low.pre = match value {
                None => None,
                Some(v) => {
                    let path = PathBuf::from(v);
                    (!path.as_os_str().is_empty()).then_some(path)
                }
            };
            if low.pre.is_some() {
                low.search_zip = false;
            }
        }
        Id::PreGlob => low.pre_glob.push(ctx.string(val())?),
        Id::Pretty => {
            rg_mark(low, ctx);
            low.color = Some(ColorChoice::Always);
            low.heading = Some(true);
            low.line_number = Some(true);
        }
        Id::Quiet => low.quiet = true,
        Id::Replace => {
            rg_mark(low, ctx);
            low.replace = Some(ctx.string(val())?.into_bytes());
        }
        Id::SearchZip => {
            low.search_zip = switch;
            if switch {
                low.pre = None;
            }
        }
        Id::Sort | Id::Sortr => {
            let kind = match ctx.str(val())? {
                "none" => {
                    low.sort = None;
                    return Ok(());
                }
                "path" => SortModeKind::Path,
                "modified" => SortModeKind::LastModified,
                "accessed" => SortModeKind::LastAccessed,
                "created" => SortModeKind::Created,
                unk => return Err(ctx.rg_err(format!("choice '{unk}' is unrecognized"))),
            };
            low.sort = Some(SortMode {
                reverse: id == Id::Sortr,
                kind,
            });
        }
        Id::SortFiles => {
            low.sort = switch.then_some(SortMode {
                reverse: false,
                kind: SortModeKind::Path,
            });
        }
        Id::Stats => {
            rg_mark(low, ctx);
            low.stats = switch;
        }
        Id::StopOnNonmatch => {
            low.stop_on_nonmatch = true;
            low.multiline = false;
        }
        Id::Text => {
            low.binary = if switch {
                BinaryMode::AsText
            } else {
                BinaryMode::Auto
            };
            if switch {
                low.gnu_binary_files = Some(GnuBinaryFiles::Text);
            } else if low.gnu_binary_files == Some(GnuBinaryFiles::Text) {
                low.gnu_binary_files = None;
            }
        }
        Id::Threads => {
            let threads: usize = ctx.number(val())?;
            low.threads = (threads != 0).then_some(threads);
        }
        Id::Trim => {
            rg_mark(low, ctx);
            low.trim = switch;
        }
        Id::Type => low.type_changes.push(TypeChange::Select {
            name: ctx.string(val())?,
        }),
        Id::TypeNot => low.type_changes.push(TypeChange::Negate {
            name: ctx.string(val())?,
        }),
        Id::TypeAdd => low.type_changes.push(TypeChange::Add {
            def: ctx.string(val())?,
        }),
        Id::TypeClear => low.type_changes.push(TypeChange::Clear {
            name: ctx.string(val())?,
        }),
        Id::TypeList => low.mode.update(Mode::Types),
        Id::Unrestricted => {
            low.unrestricted = low.unrestricted.saturating_add(1);
            if low.unrestricted > 3 {
                return Err(ctx.rg_err("flag can only be repeated up to 3 times"));
            }
            match low.unrestricted {
                1 => update(low, ctx, Id::NoIgnore, true, None)?,
                2 => low.hidden = true,
                _ => low.binary = BinaryMode::SearchAndSuppress,
            }
        }
        Id::Vimgrep => {
            rg_mark(low, ctx);
            low.vimgrep = true;
        }
        Id::WithFilename => low.with_filename = Some(true),
        Id::WithFilenameNo => low.with_filename = Some(false),
    }
    Ok(())
}

struct Parser<'a> {
    personality: Personality,
    args: &'a [OsString],
    index: usize,
    posix_correct: bool,
}

fn missing_value(ctx: &Ctx<'_>, option: &str) -> ArgError {
    match ctx.personality {
        Personality::Rg => ArgError::Rg(format!(
            "missing value for flag {}: missing argument for option '{option}'",
            ctx.display
        )),
        Personality::Grep => {
            if let Some(long) = option.strip_prefix("--") {
                ArgError::GnuUsage(format!("option '--{long}' requires an argument"))
            } else {
                ArgError::GnuUsage(format!(
                    "option requires an argument -- '{}'",
                    option.trim_start_matches('-')
                ))
            }
        }
    }
}

impl Parser<'_> {
    fn next_arg(&mut self) -> Option<&OsString> {
        let arg = self.args.get(self.index)?;
        self.index += 1;
        Some(arg)
    }

    fn run(&mut self, low: &mut LowArgs) -> ArgResult<()> {
        let mut finished = false;
        while let Some(arg) = self.next_arg() {
            let arg = arg.clone();
            let bytes = os_bytes(&arg);
            if finished || bytes == b"-" || bytes.first() != Some(&b'-') || bytes.len() < 2 {
                low.positional.push(arg);
                if self.posix_correct && self.personality == Personality::Grep {
                    finished = true;
                }
                continue;
            }
            if bytes == b"--" {
                finished = true;
                continue;
            }
            if bytes.starts_with(b"--") {
                self.long(low, &bytes[2..])?;
                continue;
            }
            self.shorts(low, &bytes[1..])?;
        }
        Ok(())
    }

    fn finish_digits(&self, low: &mut LowArgs, digits: &str) -> ArgResult<()> {
        let ctx = Ctx {
            personality: Personality::Grep,
            display: "-C",
        };
        let value = OsString::from(digits);
        let lines = ctx.gnu_context(&value).map_err(|err| {
            if self.personality == Personality::Rg {
                ArgError::Rg(format!(
                    "error parsing flag -C: value is not a valid number: {digits}"
                ))
            } else {
                err
            }
        })?;
        low.context.set_both(lines);
        Ok(())
    }

    fn long(&mut self, low: &mut LowArgs, body: &[u8]) -> ArgResult<()> {
        let (name_bytes, inline) = match body.iter().position(|&b| b == b'=') {
            Some(at) => (&body[..at], Some(&body[at + 1..])),
            None => (body, None),
        };
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        if self.personality == Personality::Rg && inline.is_none() {
            match name.as_str() {
                "help" => {
                    low.special = Some(Special::HelpLong);
                    return Ok(());
                }
                "version" => {
                    low.special = Some(Special::VersionLong);
                    return Ok(());
                }
                _ => {}
            }
        }
        let lookup = match find_long(&name, self.personality) {
            LongMatch::Found(lookup) => lookup,
            LongMatch::Unknown => {
                return Err(match self.personality {
                    Personality::Rg => {
                        let mut msg = format!("unrecognized flag --{name}");
                        if let Some(similar) = suggest(&name) {
                            msg = format!("{msg}\n\n{similar}");
                        }
                        ArgError::Rg(msg)
                    }
                    Personality::Grep => {
                        let shown = match inline {
                            Some(v) => format!("--{name}={}", String::from_utf8_lossy(v)),
                            None => format!("--{name}"),
                        };
                        ArgError::GnuUsage(format!("unrecognized option '{shown}'"))
                    }
                });
            }
            LongMatch::Ambiguous(names) => {
                let list: Vec<String> = names.iter().map(|n| format!(" '--{n}'")).collect();
                return Err(ArgError::GnuUsage(format!(
                    "option '--{name}' is ambiguous; possibilities:{}",
                    list.concat()
                )));
            }
        };
        if self.personality == Personality::Grep && inline.is_none() {
            match lookup.id {
                Id::Help => {
                    low.special.get_or_insert(Special::HelpLong);
                    return Ok(());
                }
                Id::Version => {
                    low.special = Some(Special::VersionLong);
                    return Ok(());
                }
                _ => {}
            }
        }
        let ctx = Ctx {
            personality: self.personality,
            display: &lookup.display,
        };
        if lookup.kind == Kind::Negated {
            if let Some(v) = inline {
                return Err(unexpected_value(self.personality, &lookup.display, v));
            }
            return update(low, &ctx, lookup.id, false, None);
        }
        match arity(lookup.id, self.personality) {
            Arity::Switch => {
                if let Some(v) = inline {
                    return Err(unexpected_value(self.personality, &lookup.display, v));
                }
                update(low, &ctx, lookup.id, true, None)
            }
            Arity::Optional => {
                let value = inline.map(|v| bytes_os(v.to_vec()));
                update(low, &ctx, lookup.id, true, value.as_deref())
            }
            Arity::Value => {
                let value = match inline {
                    Some(v) => bytes_os(v.to_vec()),
                    None => match self.next_arg() {
                        Some(next) => next.clone(),
                        None => return Err(missing_value(&ctx, &lookup.display)),
                    },
                };
                update(low, &ctx, lookup.id, true, Some(&value))
            }
        }
    }

    fn shorts(&mut self, low: &mut LowArgs, cluster: &[u8]) -> ArgResult<()> {
        let mut pos = 0usize;
        let mut digits = String::new();
        while pos < cluster.len() {
            let ch = cluster[pos];
            pos += 1;
            if self.personality == Personality::Rg && ch == b'=' && pos > 1 {
                let previous = char::from(cluster[pos - 2]);
                return Err(ArgError::Rg(format!(
                    "invalid CLI arguments: unexpected argument for option '-{previous}': {}",
                    debug_os(&cluster[pos..])
                )));
            }
            let is_digit = ch.is_ascii_digit()
                && (self.personality == Personality::Grep || ch != b'0' || !digits.is_empty());
            if is_digit {
                if digits == "0" {
                    digits.clear();
                }
                digits.push(char::from(ch));
                continue;
            }
            if !digits.is_empty() {
                self.finish_digits(low, &digits)?;
                digits.clear();
            }
            if self.personality == Personality::Rg {
                match ch {
                    b'h' => {
                        low.special = Some(Special::HelpShort);
                        continue;
                    }
                    b'V' => {
                        low.special = Some(Special::VersionShort);
                        continue;
                    }
                    _ => {}
                }
            } else if ch == b'V' {
                low.special = Some(Special::VersionShort);
                continue;
            }
            let lookup = match self.personality {
                Personality::Grep => grep_short(ch),
                Personality::Rg => rg_short(ch),
            };
            let Some(id) = lookup else {
                return Err(unknown_short(self.personality, cluster, pos - 1));
            };
            let display = format!("-{}", char::from(ch));
            let ctx = Ctx {
                personality: self.personality,
                display: &display,
            };
            match arity(id, self.personality) {
                Arity::Switch | Arity::Optional => update(low, &ctx, id, true, None)?,
                Arity::Value => {
                    let rest = &cluster[pos..];
                    let rest = if self.personality == Personality::Rg {
                        rest.strip_prefix(b"=").unwrap_or(rest)
                    } else {
                        rest
                    };
                    let value = if rest.is_empty() {
                        match self.next_arg() {
                            Some(next) => next.clone(),
                            None => return Err(missing_value(&ctx, &display)),
                        }
                    } else {
                        bytes_os(rest.to_vec())
                    };
                    update(low, &ctx, id, true, Some(&value))?;
                    return Ok(());
                }
            }
        }
        if digits.is_empty() {
            Ok(())
        } else {
            self.finish_digits(low, &digits)
        }
    }
}

fn unknown_short(personality: Personality, cluster: &[u8], at: usize) -> ArgError {
    match personality {
        Personality::Rg => {
            let ch = std::str::from_utf8(&cluster[at..])
                .ok()
                .and_then(|s| s.chars().next())
                .or_else(|| {
                    let end = (at + 4).min(cluster.len());
                    let tail = &cluster[at..end];
                    (1..=tail.len())
                        .find_map(|n| std::str::from_utf8(&tail[..n]).ok())
                        .and_then(|s| s.chars().next())
                })
                .unwrap_or('\u{FFFD}');
            ArgError::Rg(format!("unrecognized flag -{ch}"))
        }
        Personality::Grep => ArgError::GnuUsage(format!(
            "invalid option -- '{}'",
            String::from_utf8_lossy(&cluster[at..=at])
        )),
    }
}

fn unexpected_value(personality: Personality, display: &str, value: &[u8]) -> ArgError {
    match personality {
        Personality::Rg => ArgError::Rg(format!(
            "invalid CLI arguments: unexpected argument for option '{display}': {}",
            debug_os(value)
        )),
        Personality::Grep => {
            ArgError::GnuUsage(format!("option '{display}' doesn't allow an argument"))
        }
    }
}

fn ngrams(name: &str) -> BTreeSet<Vec<u8>> {
    let bytes = name.as_bytes();
    match bytes.len() {
        0 => BTreeSet::from([b"!!!".to_vec()]),
        1 => BTreeSet::from([vec![bytes[0], b'!', b'!']]),
        2 => BTreeSet::from([vec![bytes[0], bytes[1], b'!']]),
        _ => bytes.windows(3).map(<[u8]>::to_vec).collect(),
    }
}

fn jaccard(a: &BTreeSet<Vec<u8>>, b: &BTreeSet<Vec<u8>>) -> f64 {
    let union = a.union(b).count() as f64;
    let inter = a.intersection(b).count() as f64;
    inter / union
}

fn suggest(unrecognized: &str) -> Option<String> {
    let given = ngrams(unrecognized);
    let similar: Vec<String> = DEFS
        .iter()
        .take_while(|def| def.id != Id::BasicRegexp)
        .flat_map(|def| {
            std::iter::once(def.long)
                .chain(def.negated)
                .chain(def.aliases.iter().copied())
        })
        .filter(|name| jaccard(&given, &ngrams(name)) >= 0.4)
        .map(|name| format!("--{name}"))
        .collect();
    (!similar.is_empty())
        .then(|| format!("similar flags that are available: {}", similar.join(", ")))
}

pub(crate) fn parse_args(
    personality: Personality,
    args: &[OsString],
    posix_correct: bool,
) -> ArgResult<LowArgs> {
    let mut low = LowArgs::default();
    Parser {
        personality,
        args,
        index: 0,
        posix_correct,
    }
    .run(&mut low)?;
    Ok(low)
}

pub(crate) fn config_args(warn: &mut dyn FnMut(String)) -> Vec<OsString> {
    let Some(path) = std::env::var_os("RIPGREP_CONFIG_PATH") else {
        return Vec::new();
    };
    if path.is_empty() {
        return Vec::new();
    }
    let path = PathBuf::from(path);
    let data = match std::fs::read(&path) {
        Ok(data) => data,
        Err(err) => {
            warn(format!(
                "failed to read the file specified in RIPGREP_CONFIG_PATH: {}: {err}",
                path.display()
            ));
            return Vec::new();
        }
    };
    parse_config(&data, &path, warn)
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &bytes[start..end.max(start)]
}

fn parse_config(
    data: &[u8],
    path: &std::path::Path,
    warn: &mut dyn FnMut(String),
) -> Vec<OsString> {
    let mut out = Vec::new();
    for (index, line) in data.split_inclusive(|&b| b == b'\n').enumerate() {
        let line = trim_ascii(line);
        if line.is_empty() || line[0] == b'#' {
            continue;
        }
        if cfg!(unix) {
            out.push(bytes_os(line.to_vec()));
        } else {
            match std::str::from_utf8(line) {
                Ok(text) => out.push(OsString::from(text)),
                Err(err) => warn(format!("{}:{}: {err}", path.display(), index + 1)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rg(args: &[&str]) -> ArgResult<LowArgs> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse_args(Personality::Rg, &args, false)
    }

    fn gnu(args: &[&str]) -> ArgResult<LowArgs> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse_args(Personality::Grep, &args, false)
    }

    #[test]
    fn conflicting_shorts_follow_personality() {
        let g = gnu(&["-r", "-h", "-L", "-s", "-E", "-z", "-I", "-T", "-u", "x"]).unwrap();
        assert_eq!(g.directories, Some(DirAction::Recurse));
        assert_eq!(g.with_filename, Some(false));
        assert_eq!(g.mode, Mode::Search(SearchMode::FilesWithoutMatch));
        assert!(g.no_messages);
        assert_eq!(g.gnu_matcher, Some(GnuMatcher::Egrep));
        assert!(g.null_data);
        assert_eq!(g.gnu_binary_files, Some(GnuBinaryFiles::WithoutMatch));
        assert!(g.initial_tab);
        assert!(g.unix_byte_offsets);
        let r = rg(&[
            "-r", "X", "-L", "-s", "-E", "utf-16le", "-z", "-I", "-T", "rust", "-u", "x",
        ])
        .unwrap();
        assert_eq!(r.replace.as_deref(), Some(&b"X"[..]));
        assert!(r.follow);
        assert_eq!(r.case, CaseMode::Sensitive);
        assert_eq!(
            r.encoding,
            Some(EncodingMode::Label(Deferred {
                flag: "-E".into(),
                value: "utf-16le".into()
            }))
        );
        assert!(r.search_zip);
        assert_eq!(r.with_filename, Some(false));
        assert_eq!(
            r.type_changes,
            vec![TypeChange::Negate {
                name: "rust".into()
            }]
        );
        assert_eq!(r.unrestricted, 1);
        assert_eq!(rg(&["-h"]).unwrap().special, Some(Special::HelpShort));
    }

    #[test]
    fn digits_are_context_in_both() {
        let g = gnu(&["-12", "x"]).unwrap();
        assert_eq!(
            g.context,
            ContextMode::Limited(ContextLimited {
                before: None,
                after: None,
                both: Some(12)
            })
        );
        let r = rg(&["-3", "x"]).unwrap();
        assert_eq!(
            r.context,
            ContextMode::Limited(ContextLimited {
                before: None,
                after: None,
                both: Some(3)
            })
        );
        let z = rg(&["-0", "x"]).unwrap();
        assert!(z.null);
        let mixed = gnu(&["-5n", "x"]).unwrap();
        assert_eq!(mixed.line_number, Some(true));
        assert_eq!(
            mixed.context,
            ContextMode::Limited(ContextLimited {
                before: None,
                after: None,
                both: Some(5)
            })
        );
    }

    #[test]
    fn short_values_attach_like_each_tool() {
        let r = rg(&["-e=beta", "f"]).unwrap();
        assert_eq!(r.patterns, vec![PatternSource::Regexp("beta".into())]);
        let g = gnu(&["-e=beta", "f"]).unwrap();
        assert_eq!(g.patterns, vec![PatternSource::Regexp("=beta".into())]);
        let r = rg(&["-A3", "x"]).unwrap();
        assert_eq!(
            r.context,
            ContextMode::Limited(ContextLimited {
                before: None,
                after: Some(3),
                both: None
            })
        );
    }

    #[test]
    fn errors_use_each_tool_wording() {
        assert_eq!(
            rg(&["-n=3"]).unwrap_err(),
            ArgError::Rg(
                "invalid CLI arguments: unexpected argument for option '-n': \"3\"".into()
            )
        );
        assert_eq!(
            rg(&["--count=x"]).unwrap_err(),
            ArgError::Rg(
                "invalid CLI arguments: unexpected argument for option '--count': \"x\"".into()
            )
        );
        assert_eq!(
            rg(&["-e"]).unwrap_err(),
            ArgError::Rg("missing value for flag -e: missing argument for option '-e'".into())
        );
        assert_eq!(
            rg(&["-A", "x"]).unwrap_err(),
            ArgError::Rg(
                "error parsing flag -A: value is not a valid number: invalid digit found in string"
                    .into()
            )
        );
        assert_eq!(
            rg(&["--nope"]).unwrap_err(),
            ArgError::Rg("unrecognized flag --nope".into())
        );
        assert_eq!(
            rg(&["-k"]).unwrap_err(),
            ArgError::Rg("unrecognized flag -k".into())
        );
        assert_eq!(
            gnu(&["-k"]).unwrap_err(),
            ArgError::GnuUsage("invalid option -- 'k'".into())
        );
        assert_eq!(
            gnu(&["-e"]).unwrap_err(),
            ArgError::GnuUsage("option requires an argument -- 'e'".into())
        );
        assert_eq!(
            gnu(&["--nope"]).unwrap_err(),
            ArgError::GnuUsage("unrecognized option '--nope'".into())
        );
        assert_eq!(
            gnu(&["-A", "x"]).unwrap_err(),
            ArgError::GnuDie("x: invalid context length argument".into())
        );
        assert_eq!(
            gnu(&["--max", "beta"]).unwrap_err(),
            ArgError::GnuDie("invalid max count".into())
        );
        assert_eq!(
            gnu(&["-E", "-F", "x"]).unwrap_err(),
            ArgError::GnuDie("conflicting matchers specified".into())
        );
    }

    #[test]
    fn gnu_long_abbreviations_resolve() {
        let g = gnu(&["--colo=always", "x"]).unwrap();
        assert_eq!(g.color, Some(ColorChoice::Always));
        let g = gnu(&["--color", "x"]).unwrap();
        assert_eq!(g.color, Some(ColorChoice::Auto));
        assert_eq!(g.positional, vec![OsString::from("x")]);
        assert!(
            matches!(gnu(&["--no"]).unwrap_err(), ArgError::GnuUsage(m) if m.contains("is ambiguous"))
        );
        let r = rg(&["--color", "always", "x"]).unwrap();
        assert_eq!(r.color, Some(ColorChoice::Always));
    }

    #[test]
    fn rg_suggests_similar_flags() {
        let err = rg(&["--no-ignore-vc"]).unwrap_err();
        assert!(
            matches!(err, ArgError::Rg(m) if m.contains("similar flags that are available: ") && m.contains("--no-ignore-vcs"))
        );
    }

    #[test]
    fn unrestricted_stacks_up_to_three() {
        let r = rg(&["-uuu", "x"]).unwrap();
        assert!(r.no_ignore_vcs && r.hidden);
        assert_eq!(r.binary, BinaryMode::SearchAndSuppress);
        assert!(rg(&["-uuuu", "x"]).is_err());
    }

    #[test]
    fn human_sizes_match_rg() {
        assert_eq!(parse_human_readable_size("2M"), Ok(2 << 20));
        assert_eq!(parse_human_readable_size("10"), Ok(10));
        assert!(parse_human_readable_size("K").is_err());
        assert!(parse_human_readable_size("2X").is_err());
    }

    #[test]
    fn config_lines_are_trimmed_and_commented() {
        let mut warnings = Vec::new();
        let got = parse_config(
            b"# Test\n--context=0\n   --smart-case\n-u\n\n\n   # --bar\n--foo\n",
            std::path::Path::new("cfg"),
            &mut |w| warnings.push(w),
        );
        assert!(warnings.is_empty());
        assert_eq!(
            got,
            vec![
                OsString::from("--context=0"),
                OsString::from("--smart-case"),
                OsString::from("-u"),
                OsString::from("--foo")
            ]
        );
    }
}
