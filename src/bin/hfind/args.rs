use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::expr::{self, Expr};
use crate::walk::Follow;

pub(crate) enum Outcome {
    Help(String),
    Run(Parsed),
}

pub(crate) struct Parsed {
    pub(crate) follow: Follow,
    pub(crate) gitignore: bool,
    pub(crate) roots: Vec<OsString>,
    pub(crate) expr: Expr,
    pub(crate) mindepth: usize,
    pub(crate) maxdepth: Option<usize>,
}

pub(crate) fn parse_args() -> Result<Outcome, String> {
    let mut args = env::args_os();
    let argv0 = args.next();
    parse(args, bin_name(argv0.as_deref()))
}

fn bin_name(argv0: Option<&OsStr>) -> String {
    argv0
        .and_then(|a| Path::new(a).file_name())
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| env!("CARGO_BIN_NAME").to_string())
}

fn is_expr_start(tok: &OsStr) -> bool {
    let b = tok.as_bytes();
    matches!(b, b"(" | b"!" | b",") || b.first() == Some(&b'-')
}

pub(crate) fn parse<I>(args: I, name: String) -> Result<Outcome, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter().peekable();
    let mut follow = Follow::Never;
    let mut gitignore = false;
    loop {
        match args.peek().map(|s| s.as_bytes()) {
            Some(b"-H") => {
                follow = Follow::Cli;
                args.next();
            }
            Some(b"-L") => {
                follow = Follow::Always;
                args.next();
            }
            Some(b"-P") => {
                follow = Follow::Never;
                args.next();
            }
            Some(b"--gitignore") => {
                gitignore = true;
                args.next();
            }
            Some(b"--help") => return Ok(Outcome::Help(name)),
            _ => break,
        }
    }
    let mut roots = Vec::new();
    while let Some(tok) = args.peek() {
        if is_expr_start(tok) {
            break;
        }
        roots.push(args.next().unwrap());
    }
    if roots.is_empty() {
        roots.push(OsString::from("."));
    }
    let tokens: Vec<OsString> = args.collect();
    let (expr, mindepth, maxdepth) = expr::parse(&tokens, follow)?;
    Ok(Outcome::Run(Parsed {
        follow,
        gitignore,
        roots,
        expr,
        mindepth,
        maxdepth,
    }))
}

pub(crate) fn help_text(name: &str) -> String {
    format!(
        "\
{name} [options] [path ...] [expression]

Global options:
  -H             Follow symbolic links on the command line only
  -L             Follow symbolic links
  -P             Never follow symbolic links (default)
  --gitignore    Skip paths excluded by .gitignore
  --help         Print help

Tests:
  -name PATTERN         Basename matches glob PATTERN
  -iname PATTERN        Like -name, ignore case
  -path PATTERN         Path matches glob PATTERN
  -ipath PATTERN        Like -path, ignore case
  -regex PATTERN        Path matches regular expression
  -iregex PATTERN       Like -regex, ignore case
  -type [fdl]           File is regular (f), directory (d), or symlink (l)
  -size [+-]N[cwbkMG]   File size, 512-byte blocks by default
  -empty                Empty file or directory
  -mtime [+-]N          Modified N*24 hours ago
  -mmin [+-]N           Modified N minutes ago
  -newer FILE           Modified more recently than FILE
  -true                 Always true
  -false                Always false

Actions:
  -print         Print path and a newline (default)
  -print0        Print path and a NUL

Global expression options:
  -maxdepth N    Descend at most N levels
  -mindepth N    Apply tests at levels >= N

Operators:
  ( ) ! -not  -a -and  -o -or
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_name_falls_back() {
        assert_eq!(bin_name(None), env!("CARGO_BIN_NAME"));
        assert_eq!(
            bin_name(Some(OsStr::from_bytes(b"/tmp/\xff"))),
            env!("CARGO_BIN_NAME")
        );
        assert_eq!(
            bin_name(Some(OsStr::new("/usr/bin/hfd"))),
            "hfd".to_string()
        );
    }

    fn as_run(o: Outcome) -> Option<Parsed> {
        match o {
            Outcome::Run(p) => Some(p),
            Outcome::Help(_) => None,
        }
    }

    fn as_help(o: Outcome) -> Option<String> {
        match o {
            Outcome::Help(n) => Some(n),
            Outcome::Run(_) => None,
        }
    }

    #[test]
    fn parse_globals_roots_help_and_expr_starts() {
        assert!(is_expr_start(OsStr::new("(")));
        assert!(is_expr_start(OsStr::new("!")));
        assert!(is_expr_start(OsStr::new(",")));
        assert!(is_expr_start(OsStr::new("-name")));
        assert!(!is_expr_start(OsStr::new("src")));
        assert!(as_help(parse(Vec::<OsString>::new(), "hfind".into()).unwrap()).is_none());
        let p = as_run(parse(Vec::<OsString>::new(), "hfind".into()).unwrap()).unwrap();
        assert_eq!(p.roots, [OsString::from(".")]);
        assert!(!p.gitignore);
        assert!(matches!(p.follow, Follow::Never));
        assert_eq!(
            as_help(parse([OsString::from("--help")], "hfd".into()).unwrap()).as_deref(),
            Some("hfd")
        );
        assert!(as_run(parse([OsString::from("--help")], "hfd".into()).unwrap()).is_none());
        let toks = [
            OsString::from("-H"),
            OsString::from("-L"),
            OsString::from("-P"),
            OsString::from("--gitignore"),
            OsString::from("foo"),
            OsString::from("bar"),
            OsString::from("-name"),
            OsString::from("x"),
        ];
        let multi = parse(toks, "hfind".into()).unwrap();
        let p = as_run(multi).unwrap();
        assert!(p.gitignore);
        assert!(matches!(p.follow, Follow::Never));
        assert_eq!(p.roots, [OsString::from("foo"), OsString::from("bar")]);
        let follow = parse(
            [
                OsString::from("-P"),
                OsString::from("-H"),
                OsString::from("-true"),
            ],
            "hfind".into(),
        )
        .unwrap();
        assert!(matches!(follow, Outcome::Run(ref p) if matches!(p.follow, Follow::Cli)));
        let always = parse(
            [OsString::from("-L"), OsString::from("-true")],
            "hfind".into(),
        )
        .unwrap();
        assert!(matches!(always, Outcome::Run(ref p) if matches!(p.follow, Follow::Always)));
        assert!(help_text("hfind").contains("hfind [options]"));
        assert!(help_text("hfd").contains("--gitignore"));
    }
}
