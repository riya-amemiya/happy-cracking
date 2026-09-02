use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Parser)]
#[command(
    name = "hgrep",
    about = "grep-compatible line matcher",
    disable_help_flag = true
)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Cli {
    #[arg(
        short = 'e',
        long = "regexp",
        value_name = "PATTERN",
        help = "Match this pattern; repeatable"
    )]
    pub(crate) patterns: Vec<OsString>,
    #[arg(
        short = 'f',
        long = "file",
        value_name = "FILE",
        help = "Read patterns from FILE, one per line"
    )]
    pub(crate) pattern_file: Option<PathBuf>,
    #[arg(
        short = 'F',
        long = "fixed-strings",
        help = "Treat patterns as literal strings"
    )]
    pub(crate) fixed: bool,
    #[arg(short = 'i', long = "ignore-case", help = "Ignore case distinctions")]
    pub(crate) ignore_case: bool,
    #[arg(short = 'v', long = "invert-match", help = "Select non-matching lines")]
    pub(crate) invert: bool,
    #[arg(short = 'w', long = "word-regexp", help = "Match only whole words")]
    pub(crate) word: bool,
    #[arg(short = 'x', long = "line-regexp", help = "Match only whole lines")]
    pub(crate) line_regexp: bool,
    #[arg(short = 'c', long = "count", help = "Print a match count per file")]
    pub(crate) count: bool,
    #[arg(
        short = 'n',
        long = "line-number",
        help = "Prefix each line with its line number"
    )]
    pub(crate) line_number: bool,
    #[arg(
        short = 'l',
        long = "files-with-matches",
        help = "Print only names of files with matches"
    )]
    pub(crate) files_with_matches: bool,
    #[arg(
        short = 'L',
        long = "files-without-match",
        help = "Print only names of files with no matches"
    )]
    pub(crate) files_without_match: bool,
    #[arg(
        short = 'o',
        long = "only-matching",
        help = "Print only the matched part of each line"
    )]
    pub(crate) only_matching: bool,
    #[arg(
        short = 'q',
        long = "quiet",
        alias = "silent",
        help = "Suppress normal output; exit status only"
    )]
    pub(crate) quiet: bool,
    #[arg(
        short = 'r',
        long = "recursive",
        visible_short_alias = 'R',
        help = "Recurse into directories"
    )]
    pub(crate) recursive: bool,
    #[arg(
        long = "gitignore",
        requires = "recursive",
        help = "Skip paths excluded by .gitignore while walking (requires -r). Reads .gitignore files from the enclosing repository root down to each visited directory, then .git/info/exclude, then core.excludesFile, honors core.ignorecase, and always skips .git"
    )]
    pub(crate) gitignore: bool,
    #[arg(
        short = 'H',
        long = "with-filename",
        help = "Prefix each match with the file name"
    )]
    pub(crate) with_filename: bool,
    #[arg(
        short = 'h',
        long = "no-filename",
        help = "Do not prefix matches with the file name"
    )]
    pub(crate) no_filename: bool,
    #[arg(short = 'a', long = "text", help = "Treat binary files as text")]
    pub(crate) text: bool,
    #[arg(
        short = 's',
        long = "no-messages",
        help = "Suppress error messages about missing or unreadable files"
    )]
    pub(crate) no_messages: bool,
    #[arg(
        short = 'm',
        long = "max-count",
        value_name = "NUM",
        help = "Stop after NUM matching lines per file"
    )]
    pub(crate) max_count: Option<u64>,
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    pub(crate) help: Option<bool>,
    #[arg(
        value_name = "PATTERN_OR_FILE",
        help = "Pattern then files, or files only when -e or -f is set"
    )]
    pub(crate) operands: Vec<OsString>,
}
