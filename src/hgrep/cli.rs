use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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
        long = "no-ignore",
        help = "Do not skip gitignored paths (hg/hrg directory search applies gitignore by default)"
    )]
    pub(crate) no_ignore: bool,
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

pub(crate) fn invoked_as_rg_style() -> bool {
    std::env::args_os()
        .next()
        .is_some_and(|argv0| is_rg_style_name(&argv0))
}

fn is_rg_style_name(argv0: &OsStr) -> bool {
    let name = Path::new(argv0).file_name();
    name == Some(OsStr::new("hg")) || name == Some(OsStr::new("hrg"))
}

pub(crate) fn apply_search_defaults(cli: &mut Cli, is_rg_style: bool, stdin_is_tty: bool) {
    if cli.no_ignore {
        cli.gitignore = false;
    }
    if !is_rg_style {
        return;
    }
    if cli.operands.is_empty() && (stdin_is_tty || cli.recursive) {
        cli.operands.push(OsString::from("."));
    }
    if cli.operands.is_empty() {
        return;
    }
    if cli.recursive || cli.operands.iter().any(|p| Path::new(p).is_dir()) {
        cli.recursive = true;
        if !cli.no_ignore {
            cli.gitignore = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn rg_style_names_are_hg_and_hrg() {
        assert!(is_rg_style_name(OsStr::new("hg")));
        assert!(is_rg_style_name(OsStr::new("/usr/bin/hrg")));
        assert!(!is_rg_style_name(OsStr::new("hgrep")));
        assert!(!is_rg_style_name(OsStr::new("hfind")));
    }

    #[test]
    fn hg_empty_tty_searches_dot_with_gitignore() {
        let mut cli = Cli::parse_from(["hg", "-e", "needle"]);
        apply_search_defaults(&mut cli, true, true);
        assert_eq!(cli.operands, [OsString::from(".")]);
        assert!(cli.recursive);
        assert!(cli.gitignore);
        assert!(!cli.no_ignore);
    }

    #[test]
    fn hg_empty_pipe_stays_on_stdin() {
        let mut cli = Cli::parse_from(["hg", "-e", "needle"]);
        apply_search_defaults(&mut cli, true, false);
        assert!(cli.operands.is_empty());
        assert!(!cli.recursive);
        assert!(!cli.gitignore);
    }

    #[test]
    fn hgrep_empty_tty_stays_on_stdin() {
        let mut cli = Cli::parse_from(["hgrep", "-e", "needle"]);
        apply_search_defaults(&mut cli, false, true);
        assert!(cli.operands.is_empty());
        assert!(!cli.recursive);
        assert!(!cli.gitignore);
    }

    #[test]
    fn hg_no_ignore_keeps_gitignore_off_on_a_directory() {
        let mut cli = Cli::parse_from(["hg", "--no-ignore", "-e", "needle", "."]);
        apply_search_defaults(&mut cli, true, false);
        assert!(cli.recursive);
        assert!(!cli.gitignore);
        assert!(cli.no_ignore);
    }
}
