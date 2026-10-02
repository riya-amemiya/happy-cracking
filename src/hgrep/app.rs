use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::hc_internal::grep::printer::Stats;
use crate::hc_internal::walker::WalkState;

use super::doc;
use super::flags::{
    ArgError, DirAction, GenerateMode, LowArgs, Mode, Personality, SearchMode, Special,
    config_args, parse_args,
};
use super::hiargs::HiArgs;
use super::messages::{self, eprint_locked, err_message};
use super::out::{self, BufferWriter};

pub(crate) struct Invocation {
    pub(crate) personality: Personality,
    pub(crate) prog: String,
    pub(crate) argv0: String,
    pub(crate) implied: Vec<OsString>,
}

pub(crate) fn invocation(argv0: Option<&OsStr>) -> Invocation {
    let argv0_text =
        argv0.map_or_else(|| "hgrep".to_string(), |a| a.to_string_lossy().into_owned());
    let base = Path::new(&argv0_text)
        .file_name()
        .map_or_else(|| "hgrep".to_string(), |n| n.to_string_lossy().into_owned());
    let stem = base.strip_suffix(".exe").unwrap_or(&base).to_string();
    let (personality, implied) = match stem.as_str() {
        "hg" | "hrg" | "rg" => (Personality::Rg, Vec::new()),
        "egrep" | "hegrep" => (Personality::Grep, vec![OsString::from("-E")]),
        "fgrep" | "hfgrep" => (Personality::Grep, vec![OsString::from("-F")]),
        name if name.ends_with("rg") => (Personality::Rg, Vec::new()),
        _ => (Personality::Grep, Vec::new()),
    };
    Invocation {
        personality,
        prog: stem,
        argv0: argv0_text,
        implied,
    }
}

fn locale_quote(text: &str) -> String {
    if super::hiargs::utf8_locale() {
        format!("\u{2018}{text}\u{2019}")
    } else {
        format!("'{text}'")
    }
}

fn report_arg_error(inv: &Invocation, err: ArgError) -> ExitCode {
    let mut stderr = io::stderr().lock();
    match err {
        ArgError::Rg(msg) | ArgError::GnuDie(msg) => {
            let _ = writeln!(stderr, "{}: {msg}", inv.prog);
            ExitCode::from(2)
        }
        ArgError::GnuUsage(msg) => {
            let _ = writeln!(stderr, "{}: {msg}", inv.argv0);
            let _ = stderr.write_all(doc::gnu_usage_hint(&inv.prog).as_bytes());
            ExitCode::from(2)
        }
        ArgError::GnuArgmatch {
            value,
            option,
            choices,
            ambiguous,
        } => {
            let _ = writeln!(
                stderr,
                "{}: {} argument {} for {}",
                inv.prog,
                if ambiguous { "ambiguous" } else { "invalid" },
                locale_quote(&value),
                locale_quote(option)
            );
            let _ = writeln!(stderr, "Valid arguments are:");
            for choice in choices {
                let _ = writeln!(stderr, "  - {}", locale_quote(choice));
            }
            let _ = stderr.write_all(doc::gnu_usage_hint(&inv.prog).as_bytes());
            ExitCode::from(1)
        }
    }
}

fn parse_with_config(inv: &Invocation, cli: &[OsString]) -> Result<LowArgs, ArgError> {
    let posix = std::env::var_os("POSIXLY_CORRECT").is_some();
    let mut args = inv.implied.clone();
    args.extend_from_slice(cli);
    let low = parse_args(inv.personality, &args, posix)?;
    if inv.personality == Personality::Grep || low.special.is_some() || low.no_config {
        return Ok(low);
    }
    let mut warnings = Vec::new();
    let config = config_args(&mut |w| warnings.push(w));
    for warning in warnings {
        eprint_locked(&warning);
    }
    if config.is_empty() {
        return Ok(low);
    }
    let mut all = config;
    all.extend(args);
    parse_args(inv.personality, &all, posix)
}

fn special(inv: &Invocation, special: Special) -> ExitCode {
    let (text, code) = match (inv.personality, special) {
        (Personality::Grep, Special::HelpShort | Special::HelpLong) => {
            (doc::gnu_help(&inv.prog), 0)
        }
        (Personality::Grep, Special::VersionShort | Special::VersionLong) => {
            (doc::gnu_version(&inv.prog), 0)
        }
        (_, Special::HelpShort) => (doc::help_short(&inv.prog), 0),
        (_, Special::HelpLong) => (doc::help_long(&inv.prog), 0),
        (_, Special::VersionShort) => (doc::version_short(&inv.prog), 0),
        (_, Special::VersionLong) => (doc::version_long(&inv.prog), 0),
        (_, Special::VersionPcre2) => {
            let (text, available) = doc::pcre2_version();
            (text, u8::from(!available))
        }
    };
    let mut stdout = io::stdout().lock();
    let body = if inv.personality == Personality::Grep {
        text
    } else {
        format!("{}\n", text.trim_end())
    };
    if let Err(err) = stdout
        .write_all(body.as_bytes())
        .and_then(|()| stdout.flush())
        && err.kind() != io::ErrorKind::BrokenPipe
    {
        return ExitCode::from(2);
    }
    ExitCode::from(code)
}

#[must_use]
pub(crate) fn run() -> ExitCode {
    let mut os_args = std::env::args_os();
    let inv = invocation(os_args.next().as_deref());
    messages::set_prog(&inv.prog);
    super::set_personality(inv.personality);
    let cli: Vec<OsString> = os_args.collect();
    let low = match parse_with_config(&inv, &cli) {
        Ok(low) => low,
        Err(err) => return report_arg_error(&inv, err),
    };
    messages::set_messages(!low.no_messages);
    messages::set_ignore_messages(!low.no_ignore_messages);
    if inv.personality == Personality::Grep && low.gnu_invalid_color {
        let _ = io::stdout()
            .lock()
            .write_all(doc::gnu_help(&inv.prog).as_bytes());
        return ExitCode::SUCCESS;
    }
    if inv.personality == Personality::Grep && low.unix_byte_offsets {
        let _ = io::stderr()
            .lock()
            .write_all(doc::gnu_usage_hint(&inv.prog).as_bytes());
        return ExitCode::from(2);
    }
    if let Some(mode) = low.special {
        return special(&inv, mode);
    }
    let gnu_needs_pattern = inv.personality == Personality::Grep
        && matches!(low.mode, Mode::Search(_))
        && low.patterns.is_empty()
        && low.positional.is_empty();
    if gnu_needs_pattern {
        let _ = io::stderr()
            .lock()
            .write_all(doc::gnu_usage_hint(&inv.prog).as_bytes());
        return ExitCode::from(2);
    }
    let args = match HiArgs::from_low_args(low, inv.personality) {
        Ok(args) => args,
        Err(err) => {
            eprint_locked(&err.0);
            return ExitCode::from(2);
        }
    };
    match dispatch(&args) {
        Ok(code) => code,
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(err) => {
            eprint_locked(&err.to_string());
            ExitCode::from(2)
        }
    }
}

fn roots(args: &HiArgs) -> Vec<PathBuf> {
    args.paths
        .operands
        .iter()
        .filter(|path| {
            if path.as_os_str() == "-" || !path.is_dir() {
                return true;
            }
            match args.gnu.dir_action {
                Some(DirAction::Skip) => false,
                Some(DirAction::Read) => {
                    err_message(&format!("{}: Is a directory", path.display()));
                    false
                }
                Some(DirAction::Recurse) | None => true,
            }
        })
        .cloned()
        .collect()
}

fn dispatch(args: &HiArgs) -> io::Result<ExitCode> {
    let matched = match args.mode {
        Mode::Search(_) if !args.matches_possible() => false,
        Mode::Search(mode) if args.gnu.printer => super::ordered::search(args, mode)?,
        Mode::Search(mode) if args.personality == Personality::Rg && args.explicit_files_only() => {
            super::ordered::search(args, mode)?
        }
        Mode::Search(mode) => {
            let roots = roots(args);
            if roots.is_empty() {
                false
            } else if args.threads == 1 {
                search(args, mode, &roots)?
            } else {
                search_parallel(args, mode, &roots)?
            }
        }
        Mode::Files => {
            let roots = roots(args);
            if roots.is_empty() {
                false
            } else if args.threads == 1 {
                files(args, &roots)?
            } else {
                files_parallel(args, &roots)?
            }
        }
        Mode::Types => return types(args),
        Mode::Generate(mode) => return Ok(generate(args, mode)),
    };
    Ok(if matched && (args.quiet || !messages::errored()) {
        ExitCode::SUCCESS
    } else if messages::errored() {
        ExitCode::from(2)
    } else {
        ExitCode::from(1)
    })
}

fn nothing_searched(args: &HiArgs) {
    if args.has_implicit_path() && args.personality == Personality::Rg {
        err_message(
            "No files were searched, which means ripgrep probably applied a filter you didn't expect.\n\
             Running with --debug will show why files are being skipped.",
        );
    }
}

pub(crate) fn print_stats<W: Write>(
    mode: SearchMode,
    stats: &Stats,
    started: Instant,
    mut wtr: W,
) -> io::Result<()> {
    let elapsed = started.elapsed();
    if mode == SearchMode::Json {
        let summary = serde_json::json!({
            "type": "summary",
            "data": {
                "stats": stats,
                "elapsed_total": {
                    "secs": elapsed.as_secs(),
                    "nanos": elapsed.subsec_nanos(),
                    "human": format!("{:0.6}s", elapsed.as_secs_f64()),
                },
            }
        });
        serde_json::to_writer(&mut wtr, &summary)?;
        wtr.write_all(b"\n")
    } else {
        write!(
            wtr,
            "\n{} matches\n{} matched lines\n{} files contained matches\n{} files searched\n{} bytes printed\n{} bytes searched\n{:0.6} seconds spent searching\n{:0.6} seconds total\n",
            stats.matches(),
            stats.matched_lines(),
            stats.searches_with_match(),
            stats.searches(),
            stats.bytes_printed(),
            stats.bytes_searched(),
            stats.elapsed().as_secs_f64(),
            elapsed.as_secs_f64(),
        )
    }
}

fn search(args: &HiArgs, mode: SearchMode, roots: &[PathBuf]) -> io::Result<bool> {
    let started = Instant::now();
    let builder = args.haystack_builder();
    let unsorted = args
        .walk_builder_for(roots)
        .build()
        .filter_map(|result| builder.build_from_result(result));
    let haystacks = args.sort(unsorted);
    let compiled = args.matcher().map_err(|e| io::Error::other(e.0))?;
    let stdout = out::stdout(
        args.color != super::flags::ColorChoice::Never,
        args.stdout_line_buffered(),
    );
    let mut worker = args
        .search_worker(compiled, args.searcher(), args.printer(mode, stdout))
        .map_err(|e| io::Error::other(e.0))?;
    let mut matched = false;
    let mut searched = false;
    let mut stats = args.stats();
    for haystack in haystacks {
        searched = true;
        let result = match worker.search(&haystack) {
            Ok(result) => result,
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => break,
            Err(err) => {
                err_message(&super::format_error(&format!(
                    "{}: {err}",
                    haystack.path().display()
                )));
                continue;
            }
        };
        if let Some(fatal) = &result.fatal {
            let _ = worker.printer().get_mut().flush();
            eprint_locked(fatal);
            std::process::exit(2);
        }
        matched = matched || result.has_match;
        if let Some(message) = result.after_message {
            worker.printer().get_mut().flush()?;
            eprint_locked(&message);
        }
        if let (Some(total), Some(one)) = (stats.as_mut(), result.stats.as_ref()) {
            *total += one;
        }
        if matched && args.quit_after_match {
            break;
        }
    }
    if !searched {
        nothing_searched(args);
    }
    if let Some(stats) = &stats {
        let _ = print_stats(mode, stats, started, worker.printer().get_mut());
    }
    worker.printer().get_mut().flush()?;
    Ok(matched)
}

fn search_parallel(args: &HiArgs, mode: SearchMode, roots: &[PathBuf]) -> io::Result<bool> {
    let started = Instant::now();
    let builder = args.haystack_builder();
    let bufwtr = BufferWriter::new(
        args.color != super::flags::ColorChoice::Never,
        args.stdout_line_buffered(),
        args.file_separator.clone(),
    );
    let stats = args.stats().map(Mutex::new);
    let matched = AtomicBool::new(false);
    let searched = AtomicBool::new(false);
    let compiled = args.matcher().map_err(|e| io::Error::other(e.0))?;
    let worker = args
        .search_worker(
            compiled,
            args.searcher(),
            args.printer(mode, bufwtr.buffer()),
        )
        .map_err(|e| io::Error::other(e.0))?;
    args.walk_builder_for(roots).build_parallel().run(|| {
        let bufwtr = &bufwtr;
        let stats = &stats;
        let matched = &matched;
        let searched = &searched;
        let builder = &builder;
        let mut worker = worker.clone();
        Box::new(move |result| {
            let Some(haystack) = builder.build_from_result(result) else {
                return WalkState::Continue;
            };
            searched.store(true, Ordering::Relaxed);
            worker.printer().get_mut().clear();
            let result = match worker.search(&haystack) {
                Ok(result) => result,
                Err(err) => {
                    err_message(&super::format_error(&format!(
                        "{}: {err}",
                        haystack.path().display()
                    )));
                    return WalkState::Continue;
                }
            };
            if let Some(fatal) = &result.fatal {
                let _ = bufwtr.print(worker.printer().get_mut());
                let _ = bufwtr.flush();
                eprint_locked(fatal);
                std::process::exit(2);
            }
            if result.has_match {
                matched.store(true, Ordering::Relaxed);
            }
            if let (Some(locked), Some(one)) = (stats.as_ref(), result.stats.as_ref())
                && let Ok(mut total) = locked.lock()
            {
                *total += one;
            }
            if let Err(err) = bufwtr.print_with(
                worker.printer().get_mut(),
                result.lead_separator.as_deref(),
                result.gnu_used,
            ) {
                if err.kind() == io::ErrorKind::BrokenPipe {
                    return WalkState::Quit;
                }
                err_message(&format!("{}: {err}", haystack.path().display()));
            }
            if let Some(message) = result.after_message {
                let _ = bufwtr.flush();
                eprint_locked(&message);
            }
            if matched.load(Ordering::Relaxed) && args.quit_after_match {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });
    if !searched.load(Ordering::Relaxed) {
        nothing_searched(args);
    }
    if let Some(locked) = &stats
        && let Ok(stats) = locked.lock()
    {
        let mut buffer = bufwtr.buffer();
        let _ = print_stats(mode, &stats, started, &mut buffer);
        let _ = bufwtr.print(&buffer);
    }
    bufwtr.flush()?;
    Ok(matched.load(Ordering::Relaxed))
}

fn files(args: &HiArgs, roots: &[PathBuf]) -> io::Result<bool> {
    let builder = args.haystack_builder();
    let unsorted = args
        .walk_builder_for(roots)
        .build()
        .filter_map(|result| builder.build_from_result(result));
    let haystacks = args.sort(unsorted);
    let mut matched = false;
    let mut printer = args.path_printer_builder().build(out::stdout(
        args.color != super::flags::ColorChoice::Never,
        args.stdout_line_buffered(),
    ));
    for haystack in haystacks {
        matched = true;
        if args.quit_after_match {
            break;
        }
        if let Err(err) = printer.write(haystack.path()) {
            if err.kind() == io::ErrorKind::BrokenPipe {
                break;
            }
            return Err(err);
        }
    }
    Ok(matched)
}

fn files_parallel(args: &HiArgs, roots: &[PathBuf]) -> io::Result<bool> {
    let builder = args.haystack_builder();
    let matched = AtomicBool::new(false);
    let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
    let color = args.color != super::flags::ColorChoice::Never;
    let line_buffered = args.stdout_line_buffered();
    let mut printer = args
        .path_printer_builder()
        .build(out::stdout(color, line_buffered));
    let print_thread = std::thread::spawn(move || -> io::Result<()> {
        for path in rx {
            printer.write(&path)?;
        }
        printer.get_mut().flush()
    });
    args.walk_builder_for(roots).build_parallel().run(|| {
        let builder = &builder;
        let matched = &matched;
        let tx = tx.clone();
        Box::new(move |result| {
            let Some(haystack) = builder.build_from_result(result) else {
                return WalkState::Continue;
            };
            matched.store(true, Ordering::Relaxed);
            if args.quit_after_match {
                return WalkState::Quit;
            }
            match tx.send(haystack.path().to_path_buf()) {
                Ok(()) => WalkState::Continue,
                Err(_) => WalkState::Quit,
            }
        })
    });
    drop(tx);
    match print_thread.join() {
        Ok(Err(err)) if err.kind() != io::ErrorKind::BrokenPipe => return Err(err),
        _ => {}
    }
    Ok(matched.load(Ordering::Relaxed))
}

fn types(args: &HiArgs) -> io::Result<ExitCode> {
    let mut stdout = out::stdout(false, args.stdout_line_buffered());
    let mut count = 0usize;
    for def in args.types().definitions() {
        count += 1;
        stdout.write_all(def.name().as_bytes())?;
        stdout.write_all(b": ")?;
        stdout.write_all(def.globs().join(", ").as_bytes())?;
        stdout.write_all(b"\n")?;
    }
    stdout.flush()?;
    Ok(ExitCode::from(u8::from(count == 0)))
}

fn generate(args: &HiArgs, mode: GenerateMode) -> ExitCode {
    let prog = messages::prog();
    let text = match mode {
        GenerateMode::Man => doc::man_page(prog),
        GenerateMode::CompleteBash => doc::complete_bash(prog),
        GenerateMode::CompleteZsh => doc::complete_zsh(prog),
        GenerateMode::CompleteFish => doc::complete_fish(prog),
        GenerateMode::CompletePowerShell => doc::complete_powershell(prog),
    };
    let _ = args;
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", text.trim_end());
    ExitCode::SUCCESS
}
