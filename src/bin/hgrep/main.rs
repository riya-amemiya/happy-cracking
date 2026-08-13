mod cli;
mod gitconfig;
mod ignore;
mod matcher;
mod search;
mod source;
mod walk;

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::Parser;
use rayon::prelude::*;

use cli::Cli;
use matcher::build_matcher;
use search::{Job, report, search_buf, selected};
use source::open_source;
use walk::collect_paths;

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
    let files = collect_paths(
        &cli.operands,
        cli.recursive,
        cli.gitignore,
        &errors,
        cli.no_messages,
    );
    let show_name = if cli.no_filename {
        false
    } else {
        cli.with_filename || files.len() > 1 || cli.recursive
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
        let mut body = Vec::new();
        let count = search_buf(&buf, &job, b"(standard input)", &mut body);
        let mut out = Vec::new();
        report(&job, b"(standard input)", count, body, &mut out);
        let _ = io::stdout().lock().write_all(&out);
        return exit_code(selected(&cli, count), false);
    }

    let sink = Mutex::new(io::BufWriter::with_capacity(256 * 1024, io::stdout()));
    files.par_iter().for_each(|path| {
        if cli.quiet && found.load(Ordering::Relaxed) {
            return;
        }
        let src = match open_source(path) {
            Ok(s) => s,
            Err(e) => {
                errors.store(true, Ordering::Relaxed);
                if !cli.no_messages {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
                return;
            }
        };
        let name = path.as_os_str().as_bytes();
        let mut body = Vec::new();
        let count = search_buf(src.bytes(), &job, name, &mut body);
        if selected(&cli, count) {
            found.store(true, Ordering::Relaxed);
        }
        let mut out = Vec::new();
        report(&job, name, count, body, &mut out);
        if !out.is_empty()
            && let Ok(mut w) = sink.lock()
        {
            let _ = w.write_all(&out);
        }
    });

    if let Ok(mut w) = sink.lock() {
        let _ = w.flush();
    }
    exit_code(
        found.load(Ordering::Relaxed),
        errors.load(Ordering::Relaxed),
    )
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
