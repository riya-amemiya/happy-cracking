mod args;
mod expr;
mod walk;

use std::io;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::hc_internal::outbuf;
use walk::WalkCfg;

pub fn run() -> ExitCode {
    finish(args::parse_args())
}

fn finish(parsed: Result<args::Outcome, String>) -> ExitCode {
    match parsed {
        Ok(args::Outcome::Help(name)) => {
            print!("{}", args::help_text(&name));
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("hfind: {msg}");
            ExitCode::from(2)
        }
        Ok(args::Outcome::Run(parsed)) => execute(parsed),
    }
}

fn execute(parsed: args::Parsed) -> ExitCode {
    let errors = AtomicBool::new(false);
    let sink = Mutex::new(io::BufWriter::with_capacity(256 * 1024, io::stdout()));
    let now = SystemTime::now();
    let cfg = WalkCfg {
        follow: parsed.follow,
        gitignore: parsed.gitignore,
        mindepth: parsed.mindepth,
        maxdepth: parsed.maxdepth,
        need_meta: expr::needs_meta(&parsed.expr),
    };
    walk::for_each(&parsed.roots, &cfg, &errors, |item| {
        expr::eval(&parsed.expr, item, now, &errors, &mut |bytes, nul| {
            emit(&sink, bytes, nul);
        });
    });
    outbuf::finish(&sink);
    if errors.load(Ordering::Relaxed) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn emit(sink: &Mutex<io::BufWriter<io::Stdout>>, bytes: &[u8], nul: bool) {
    outbuf::push(sink, bytes, Some(if nul { 0 } else { b'\n' }));
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
        let _ = args::parse_args();
    }

    #[test]
    fn finish_help_error_and_run() {
        assert_eq!(
            finish(Ok(args::Outcome::Help("hfind".into()))),
            ExitCode::SUCCESS
        );
        assert_eq!(finish(Err("bad".into())), ExitCode::from(2));
        let dir = scratch("run");
        fs::write(dir.join("a"), b"").unwrap();
        fn as_run(o: args::Outcome) -> Option<args::Parsed> {
            match o {
                args::Outcome::Run(p) => Some(p),
                args::Outcome::Help(_) => None,
            }
        }
        let ok = args::parse([dir.clone().into_os_string()], "hfind".into()).unwrap();
        assert!(as_run(args::Outcome::Help("x".into())).is_none());
        assert_eq!(execute(as_run(ok).unwrap()), ExitCode::SUCCESS);
        let missing =
            args::parse([OsString::from("/hfind-no-such-main-root")], "hfind".into()).unwrap();
        assert_eq!(execute(as_run(missing).unwrap()), ExitCode::from(1));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn emit_writes_and_ignores_poison() {
        let sink = Mutex::new(io::BufWriter::with_capacity(16, io::stdout()));
        emit(&sink, b"ok", false);
        emit(&sink, b"z", true);
        let _ = std::panic::catch_unwind(|| {
            let _g = sink.lock().unwrap();
            panic!("poison");
        });
        emit(&sink, b"x", false);
    }
}
