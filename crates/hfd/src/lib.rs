mod args;
mod expr;
mod fd;
mod walk;

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use walk::WalkCfg;

static PROG: OnceLock<String> = OnceLock::new();

pub(crate) fn prog() -> &'static str {
    PROG.get().map_or("hfind", String::as_str)
}

pub fn run() -> ExitCode {
    let (argv, name) = args::argv();
    let _ = PROG.set(name.clone());
    match args::mode(&argv, &name) {
        args::Mode::Find => finish(args::parse(argv, name)),
        args::Mode::Fd => fd::run(argv),
    }
}

fn finish(parsed: Result<args::Outcome, String>) -> ExitCode {
    match parsed {
        Ok(args::Outcome::Help(name)) => {
            print!("{}", args::help_text(&name));
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{}: {msg}", prog());
            ExitCode::from(2)
        }
        Ok(args::Outcome::Run(parsed)) => execute(&parsed),
    }
}

fn execute(parsed: &args::Parsed) -> ExitCode {
    let errors = AtomicBool::new(false);
    let mut out = io::BufWriter::with_capacity(256 * 1024, io::stdout().lock());
    let now = SystemTime::now();
    let cfg = WalkCfg {
        follow: parsed.follow,
        gitignore: parsed.gitignore,
        mindepth: parsed.mindepth,
        maxdepth: parsed.maxdepth,
        need_meta: expr::needs_meta(&parsed.expr),
    };
    let visit = |item: &walk::Item<'_>, sink: &mut walk::Sink<'_>| {
        expr::eval(&parsed.expr, item, now, sink);
    };
    let _ = walk::for_each(&parsed.roots, &cfg, &errors, &mut out, &visit);
    let _ = out.flush();
    if errors.load(Ordering::Relaxed) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_main_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_args_reads_process_argv() {
        let (argv, name) = args::argv();
        let _ = args::mode(&argv, &name);
    }

    #[test]
    fn finish_help_error_and_run() {
        fn as_run(o: args::Outcome) -> Option<args::Parsed> {
            match o {
                args::Outcome::Run(p) => Some(p),
                args::Outcome::Help(_) => None,
            }
        }
        assert_eq!(
            finish(Ok(args::Outcome::Help("hfind".into()))),
            ExitCode::SUCCESS
        );
        assert_eq!(finish(Err("bad".into())), ExitCode::from(2));
        let dir = scratch("run");
        fs::write(dir.join("a"), b"").unwrap();
        let ok = args::parse([dir.clone().into_os_string()], "hfind".into()).unwrap();
        assert!(as_run(args::Outcome::Help("x".into())).is_none());
        assert_eq!(execute(&as_run(ok).unwrap()), ExitCode::SUCCESS);
        let missing =
            args::parse([OsString::from("/hfind-no-such-main-root")], "hfind".into()).unwrap();
        assert_eq!(execute(&as_run(missing).unwrap()), ExitCode::from(1));
        fs::remove_dir_all(&dir).unwrap();
    }
}
