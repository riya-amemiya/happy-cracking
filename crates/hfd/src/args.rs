use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;

use super::expr::{self, Expr};
use super::walk::Follow;

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

pub(crate) fn argv() -> (Vec<OsString>, String) {
    let mut args = env::args_os();
    let argv0 = args.next();
    (args.collect(), bin_name(argv0.as_deref()))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Find,
    Fd,
}

const FIND_TOKENS: &[&str] = &[
    "-name",
    "-iname",
    "-path",
    "-ipath",
    "-regex",
    "-iregex",
    "-type",
    "-size",
    "-empty",
    "-mtime",
    "-mmin",
    "-newer",
    "-true",
    "-false",
    "-print",
    "-print0",
    "-maxdepth",
    "-mindepth",
    "-not",
    "-and",
    "-or",
    "(",
    ")",
    "!",
    "-amin",
    "-anewer",
    "-atime",
    "-cmin",
    "-cnewer",
    "-ctime",
    "-daystart",
    "-delete",
    "-depth",
    "-exec",
    "-execdir",
    "-executable",
    "-fls",
    "-follow",
    "-fprint",
    "-fprint0",
    "-fprintf",
    "-fstype",
    "-gid",
    "-group",
    "-ignore_readdir_race",
    "-ilname",
    "-inum",
    "-iwholename",
    "-links",
    "-lname",
    "-ls",
    "-mount",
    "-newermt",
    "-nogroup",
    "-noleaf",
    "-nouser",
    "-ok",
    "-okdir",
    "-perm",
    "-printf",
    "-prune",
    "-quit",
    "-readable",
    "-regextype",
    "-samefile",
    "-uid",
    "-used",
    "-user",
    "-wholename",
    "-writable",
    "-xdev",
    "-xtype",
];
const FIND_GLOBALS: &[&str] = &["-H", "-L", "-P", "--gitignore", "--no-ignore"];
const NEUTRAL: &[&str] = &["--help", "-h", "--version", "-V"];
const AMBIGUOUS: &[&str] = &["-a", "-o"];
const FD_LONG_VALUE: &[&str] = &[
    "and",
    "max-depth",
    "maxdepth",
    "min-depth",
    "mindepth",
    "exact-depth",
    "exclude",
    "type",
    "extension",
    "size",
    "changed-within",
    "change-newer-than",
    "newer",
    "changed-after",
    "changed-before",
    "change-older-than",
    "older",
    "owner",
    "format",
    "batch-size",
    "ignore-file",
    "color",
    "ignore-contain",
    "threads",
    "max-buffer-time",
    "max-results",
    "base-directory",
    "path-separator",
    "search-path",
];
const FD_LONG_FLAG: &[&str] = &[
    "hidden",
    "no-hidden",
    "no-ignore",
    "ignore",
    "no-ignore-vcs",
    "ignore-vcs",
    "no-require-git",
    "require-git",
    "no-ignore-parent",
    "no-global-ignore-file",
    "unrestricted",
    "case-sensitive",
    "ignore-case",
    "glob",
    "regex",
    "fixed-strings",
    "literal",
    "absolute-path",
    "relative-path",
    "list-details",
    "follow",
    "dereference",
    "no-follow",
    "full-path",
    "print0",
    "prune",
    "quiet",
    "has-results",
    "show-errors",
    "one-file-system",
    "mount",
    "xdev",
    "hyperlink",
    "hyper",
    "strip-cwd-prefix",
    "gen-completions",
];
const FD_SHORT_FLAG: &[u8] = b"HIusigFalLp0q1hV";
const FD_SHORT_VALUE: &[u8] = b"dEteSocjC";

enum FdToken {
    Flag,
    Value,
    Exec,
}

fn fd_token(tok: &[u8]) -> Option<FdToken> {
    if let Some(long) = tok.strip_prefix(b"--") {
        let (name, attached) = match long.iter().position(|&b| b == b'=') {
            Some(eq) => (&long[..eq], true),
            None => (long, false),
        };
        let name = std::str::from_utf8(name).ok()?;
        return if name == "exec" || name == "exec-batch" {
            Some(FdToken::Exec)
        } else if FD_LONG_VALUE.contains(&name) {
            Some(if attached {
                FdToken::Flag
            } else {
                FdToken::Value
            })
        } else if FD_LONG_FLAG.contains(&name) {
            Some(FdToken::Flag)
        } else {
            None
        };
    }
    let cluster = tok.strip_prefix(b"-").filter(|c| !c.is_empty())?;
    for (i, c) in cluster.iter().enumerate() {
        let last = i + 1 == cluster.len();
        if matches!(c, b'x' | b'X') {
            return Some(FdToken::Exec);
        }
        if FD_SHORT_VALUE.contains(c) {
            return Some(if last { FdToken::Value } else { FdToken::Flag });
        }
        if !FD_SHORT_FLAG.contains(c) {
            return None;
        }
    }
    Some(FdToken::Flag)
}

pub(crate) fn mode(args: &[OsString], name: &str) -> Mode {
    let mut fd_flag = false;
    let mut i = 0;
    while i < args.len() {
        let tok = args[i].as_encoded_bytes();
        i += 1;
        let text = std::str::from_utf8(tok).unwrap_or("");
        if FIND_TOKENS.contains(&text) {
            return Mode::Find;
        }
        if tok == b"--" {
            fd_flag = true;
            break;
        }
        if FIND_GLOBALS.contains(&text) || NEUTRAL.contains(&text) || AMBIGUOUS.contains(&text) {
            continue;
        }
        match fd_token(tok) {
            Some(FdToken::Flag) => fd_flag = true,
            Some(FdToken::Value) => {
                fd_flag = true;
                i += 1;
            }
            Some(FdToken::Exec) => {
                fd_flag = true;
                while i < args.len() && args[i].as_encoded_bytes() != b";" {
                    i += 1;
                }
                i += 1;
            }
            None => {}
        }
    }
    let base = name.strip_suffix(".exe").unwrap_or(name);
    if fd_flag || base.ends_with("fd") {
        Mode::Fd
    } else {
        Mode::Find
    }
}

fn bin_name(argv0: Option<&OsStr>) -> String {
    argv0
        .and_then(|a| Path::new(a).file_name())
        .and_then(|n| n.to_str())
        .map_or_else(|| env!("CARGO_PKG_NAME").to_string(), str::to_owned)
}

fn is_expr_start(tok: &OsStr) -> bool {
    let b = tok.as_encoded_bytes();
    matches!(b, b"(" | b"!" | b",") || b.first() == Some(&b'-')
}

pub(crate) fn parse<I>(args: I, name: String) -> Result<Outcome, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter().peekable();
    let mut follow = Follow::Never;
    let mut gitignore = true;
    loop {
        match args.peek().map(|s| s.as_encoded_bytes()) {
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
            Some(b"--no-ignore") => {
                gitignore = false;
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
  --gitignore    Skip gitignored paths (default; like fd)
  --no-ignore    Do not skip gitignored paths
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
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn bin_name_falls_back() {
        assert_eq!(bin_name(None), env!("CARGO_PKG_NAME"));
        assert_eq!(
            bin_name(Some(OsStr::from_bytes(b"/tmp/\xff"))),
            env!("CARGO_PKG_NAME")
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
        assert!(p.gitignore);
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
        assert!(help_text("hfind").contains("--no-ignore"));
        let off = parse([OsString::from("--no-ignore")], "hfind".into()).unwrap();
        assert!(matches!(off, Outcome::Run(ref p) if !p.gitignore));
    }

    fn mode_of(args: &[&str], name: &str) -> Mode {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        mode(&args, name)
    }

    #[test]
    fn disambiguation_table() {
        let find = [
            (&[][..], "hfind"),
            (&["src"][..], "hfind"),
            (&["-H", "src"][..], "hfind"),
            (&["--no-ignore", "src"][..], "hfind"),
            (&["--gitignore"][..], "hfind"),
            (&["--help"][..], "hfind"),
            (&["src", "-a"][..], "hfind"),
            (&["src", "-unknown"][..], "hfind"),
            (&[".", "-name", "x"][..], "hfd"),
            (&["-L", ".", "-type", "f"][..], "hfd"),
            (&["(", "-true", ")"][..], "hfd"),
            (&["!"][..], "hfd"),
            (&["src", "-o", "-print0"][..], "hfd"),
            (&["-H", "x", "-name", "y"][..], "/usr/bin/hfd"),
            (&["src", "-exec", "rm", "{}", ";"][..], "hfind"),
            (&["src"][..], "find"),
            (&["src"][..], "find.exe"),
            (&["src"][..], "fdx"),
            (&["-e", "rs", "-empty"][..], "hfd"),
        ];
        for (args, name) in find {
            assert_eq!(mode_of(args, name), Mode::Find, "{args:?} {name}");
        }
        let fd = [
            (&[][..], "hfd"),
            (&["src"][..], "hfd"),
            (&["src"][..], "fd"),
            (&["-a"][..], "hfd"),
            (&["-o", "root"][..], "hfd"),
            (&["-H"][..], "hfd"),
            (&["--gitignore"][..], "hfd"),
            (&["-e", "rs"][..], "hfind"),
            (&["-H", "-e", "rs"][..], "hfind"),
            (&["-HI"][..], "hfind"),
            (&["-tf"][..], "hfind"),
            (&["--exclude", "-name"][..], "hfind"),
            (&["--exclude=x", "src"][..], "hfind"),
            (&["--hidden"][..], "hfind"),
            (&["-x", "find", "{}", "-name", "x", ";"][..], "hfind"),
            (&["-X", "ls", "-type"][..], "hfind"),
            (&["--", "-name"][..], "hfind"),
            (&["-1"][..], "hfind"),
            (&["--exec", "echo"][..], "hfind"),
            (&["-unknown"][..], "hfd"),
            (&["--strip-cwd-prefix=never"][..], "hfind"),
            (&["--and", "-type"][..], "hfind"),
            (&["--gen-completions", "bash"][..], "hfind"),
            (&["src"][..], "myfd"),
            (&["src"][..], "fd.exe"),
        ];
        for (args, name) in fd {
            assert_eq!(mode_of(args, name), Mode::Fd, "{args:?} {name}");
        }
        assert!(matches!(fd_token(b"--max-depth"), Some(FdToken::Value)));
        assert!(fd_token(b"--nonsense").is_none());
        assert!(fd_token(b"-").is_none());
        assert!(matches!(fd_token(b"-Hd"), Some(FdToken::Value)));
        assert!(matches!(fd_token(b"-d2"), Some(FdToken::Flag)));
        assert!(matches!(fd_token(b"-HX"), Some(FdToken::Exec)));
    }
}
