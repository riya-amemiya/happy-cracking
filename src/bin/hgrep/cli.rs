use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgAction, Parser};

#[derive(Parser)]
#[command(
    name = "hgrep",
    about = "grep-compatible line matcher",
    disable_help_flag = true
)]
pub(crate) struct Cli {
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    pub(crate) patterns: Vec<OsString>,
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pub(crate) pattern_file: Option<PathBuf>,
    #[arg(short = 'F', long = "fixed-strings")]
    pub(crate) fixed: bool,
    #[arg(short = 'i', long = "ignore-case")]
    pub(crate) ignore_case: bool,
    #[arg(short = 'v', long = "invert-match")]
    pub(crate) invert: bool,
    #[arg(short = 'w', long = "word-regexp")]
    pub(crate) word: bool,
    #[arg(short = 'x', long = "line-regexp")]
    pub(crate) line_regexp: bool,
    #[arg(short = 'c', long = "count")]
    pub(crate) count: bool,
    #[arg(short = 'n', long = "line-number")]
    pub(crate) line_number: bool,
    #[arg(short = 'l', long = "files-with-matches")]
    pub(crate) files_with_matches: bool,
    #[arg(short = 'L', long = "files-without-match")]
    pub(crate) files_without_match: bool,
    #[arg(short = 'o', long = "only-matching")]
    pub(crate) only_matching: bool,
    #[arg(short = 'q', long = "quiet", alias = "silent")]
    pub(crate) quiet: bool,
    #[arg(short = 'r', long = "recursive", visible_short_alias = 'R')]
    pub(crate) recursive: bool,
    #[arg(
        long = "gitignore",
        requires = "recursive",
        help = "Skip paths excluded by .gitignore while walking (requires -r). Reads .gitignore files from the enclosing repository root down to each visited directory, then .git/info/exclude, then core.excludesFile, honors core.ignorecase, and always skips .git"
    )]
    pub(crate) gitignore: bool,
    #[arg(short = 'H', long = "with-filename")]
    pub(crate) with_filename: bool,
    #[arg(short = 'h', long = "no-filename")]
    pub(crate) no_filename: bool,
    #[arg(short = 'a', long = "text")]
    pub(crate) text: bool,
    #[arg(short = 's', long = "no-messages")]
    pub(crate) no_messages: bool,
    #[arg(short = 'm', long = "max-count", value_name = "NUM")]
    pub(crate) max_count: Option<u64>,
    #[arg(long = "help", action = ArgAction::Help, help = "Print help")]
    pub(crate) help: Option<bool>,
    #[arg(value_name = "PATTERN_OR_FILE")]
    pub(crate) operands: Vec<OsString>,
}
