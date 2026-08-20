mod cli;
mod gitconfig;
mod ignore;
mod matcher;
mod search;
mod source;
mod walk;

#[cfg(all(target_os = "linux", not(test)))]
#[path = "../linuxdir.rs"]
mod linuxdir;

#[path = "../outbuf.rs"]
mod outbuf;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::cell::RefCell;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;

use cli::Cli;
use matcher::build_matcher;
use search::{Job, may_stop_early, report, search_buf, search_exists, selected};
use source::open_source;
use walk::for_each_path;

thread_local! {
    static SLOT: RefCell<(Vec<u8>, Vec<u8>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    let patterns: Vec<Vec<u8>> = if !cli.patterns.is_empty() || cli.pattern_file.is_some() {
        let mut ps: Vec<Vec<u8>> = cli
            .patterns
            .iter()
            .map(|p| p.as_os_str().as_bytes().to_vec())
            .collect();
        if let Some(f) = &cli.pattern_file {
            match fs::read(f) {
                Ok(data) if !data.is_empty() => {
                    let body = data.strip_suffix(b"\n").unwrap_or(&data);
                    ps.extend(
                        body.split(|&b| b == b'\n')
                            .map(|l| l.strip_suffix(b"\r").unwrap_or(l).to_vec()),
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("hgrep: {}: {e}", f.display());
                    return ExitCode::from(2);
                }
            }
        }
        ps
    } else if cli.operands.is_empty() {
        eprintln!("hgrep: no pattern given");
        return ExitCode::from(2);
    } else {
        vec![cli.operands.remove(0).into_vec()]
    };

    let matcher = match build_matcher(&cli, &patterns) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("hgrep: {e}");
            return ExitCode::from(2);
        }
    };

    let errors = AtomicBool::new(false);
    let found = AtomicBool::new(false);
    let show_name = if cli.no_filename {
        false
    } else {
        cli.with_filename || cli.recursive || cli.operands.len() > 1
    };
    let job = Job {
        matcher: &matcher,
        cli: &cli,
        show_name,
        emit_lines: !(cli.count || cli.files_with_matches || cli.files_without_match || cli.quiet),
    };

    if cli.operands.is_empty() {
        let mut buf = Vec::new();
        if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
            eprintln!("hgrep: (standard input): {e}");
            return ExitCode::from(2);
        }
        let mut out = Vec::new();
        let count = search_buf(&buf, &job, b"(standard input)", &mut out);
        report(&job, b"(standard input)", count, &mut out);
        let _ = io::stdout().lock().write_all(&out);
        return exit_code(selected(&cli, count), false);
    }

    let sink = Mutex::new(io::BufWriter::with_capacity(256 * 1024, io::stdout()));
    let early = may_stop_early(&cli);
    for_each_path(
        &cli.operands,
        cli.recursive,
        cli.gitignore,
        &errors,
        cli.no_messages,
        |path| process_path(path, &job, &sink, &found, &errors, early),
    );

    outbuf::finish(&sink);
    exit_code(
        found.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    )
}

fn process_path(
    path: &Path,
    job: &Job<'_>,
    sink: &Mutex<io::BufWriter<io::Stdout>>,
    found: &AtomicBool,
    errors: &AtomicBool,
    early: bool,
) {
    if job.cli.quiet && found.load(Ordering::Relaxed) {
        return;
    }
    SLOT.with(|slot| {
        let (read_buf, out) = &mut *slot.borrow_mut();
        out.clear();
        let name = path.as_os_str().as_bytes();
        let count = match stream_or_search(path, job, name, read_buf, out, errors, early) {
            Some(c) => c,
            None => return,
        };
        if selected(job.cli, count) {
            found.store(true, Ordering::Relaxed);
        }
        report(job, name, count, out);
        if !out.is_empty() {
            outbuf::push(sink, out, None);
        }
    });
}

fn stream_or_search(
    path: &Path,
    job: &Job<'_>,
    name: &[u8],
    read_buf: &mut Vec<u8>,
    out: &mut Vec<u8>,
    errors: &AtomicBool,
    early: bool,
) -> Option<u64> {
    if early
        && !job.cli.invert
        && !job.cli.count
        && let Some(overlap) = job.matcher.stream_overlap()
    {
        return match open_source(path, read_buf, true) {
            Ok(src) => {
                let count = search_exists(src.bytes(), job, overlap, || src.prefetch_from(0));
                drop(src);
                Some(count)
            }
            Err(e) => {
                errors.store(true, Ordering::Relaxed);
                if !job.cli.no_messages {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
                None
            }
        };
    }
    open_and_search(path, job, name, read_buf, out, errors, early)
}

fn open_and_search(
    path: &Path,
    job: &Job<'_>,
    name: &[u8],
    read_buf: &mut Vec<u8>,
    out: &mut Vec<u8>,
    errors: &AtomicBool,
    early: bool,
) -> Option<u64> {
    match open_source(path, read_buf, early) {
        Ok(src) => {
            let count = search_buf(src.bytes(), job, name, out);
            drop(src);
            Some(count)
        }
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !job.cli.no_messages {
                eprintln!("hgrep: {}: {e}", path.display());
            }
            None
        }
    }
}

fn exit_code(found: bool, errored: bool) -> ExitCode {
    if errored {
        ExitCode::from(2)
    } else if found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
