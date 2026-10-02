mod cli;
mod complete;
mod exec;
mod filter;
mod fsx;
mod help;
mod output;
mod smart;
mod walk;
mod when;

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::FromArgMatches;
use regex::bytes::{Regex, RegexBuilder};

use crate::hc_internal::walker::glob::GlobBuilder;
use cli::{ColorWhen, FileType, Flag, Flags, HyperlinkWhen, Opts};
use complete::Shell;
use exec::{CommandSet, FormatTemplate};
#[cfg(unix)]
use filter::OwnerFilter;
use filter::{FileTypes, SizeFilter, TimeFilter};
use output::LsColors;
use walk::Extensions;

fn bin() -> &'static str {
    super::prog()
}

fn print_error(msg: &str) {
    eprintln!("[{} error]: {msg}", bin());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exit {
    Success,
    HasResults(bool),
    GeneralError,
    KilledBySigint,
}

impl From<Exit> for ExitCode {
    fn from(code: Exit) -> Self {
        match code {
            Exit::Success | Exit::HasResults(true) => ExitCode::SUCCESS,
            Exit::HasResults(false) | Exit::GeneralError => ExitCode::from(1),
            Exit::KilledBySigint => {
                walk::reraise_interrupt();
                ExitCode::from(130)
            }
        }
    }
}

fn merge_exits(results: impl IntoIterator<Item = Exit>) -> Exit {
    if results
        .into_iter()
        .any(|e| !matches!(e, Exit::Success | Exit::HasResults(true)))
    {
        Exit::GeneralError
    } else {
        Exit::Success
    }
}

#[derive(Clone, Copy)]
enum Set {
    CaseSensitive,
    FullPath,
    IgnoreHidden,
    ReadFdignore,
    ReadParentIgnore,
    ReadVcsignore,
    RequireGit,
    ReadGlobalIgnore,
    FollowLinks,
    OneFileSystem,
    NullSeparator,
    Prune,
    Quiet,
    InteractiveTerminal,
    ShowErrors,
    StripCwdPrefix,
    Hyperlink,
}

struct Config {
    bits: u32,
    max_depth: Option<usize>,
    min_depth: Option<usize>,
    threads: usize,
    max_buffer_time: Option<Duration>,
    ls_colors: Option<LsColors>,
    file_types: Option<FileTypes>,
    extensions: Option<Extensions>,
    format: Option<FormatTemplate>,
    command: Option<CommandSet>,
    batch_size: usize,
    exclude_patterns: Vec<String>,
    ignore_files: Vec<PathBuf>,
    size_constraints: Vec<SizeFilter>,
    time_constraints: Vec<TimeFilter>,
    #[cfg(unix)]
    owner_constraint: Option<OwnerFilter>,
    path_separator: Option<String>,
    actual_path_separator: String,
    max_results: Option<usize>,
    ignore_contain: Vec<String>,
}

impl Config {
    fn is(&self, setting: Set) -> bool {
        self.bits & (1 << setting as u32) != 0
    }
}

#[cfg(unix)]
fn terminal_width() -> Option<usize> {
    use std::ffi::{c_int, c_ulong};
    #[repr(C)]
    struct WinSize {
        row: u16,
        col: u16,
        x: u16,
        y: u16,
    }
    unsafe extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }
    let request: c_ulong = if cfg!(any(target_os = "linux", target_os = "android")) {
        0x5413
    } else {
        0x4008_7468
    };
    [1, 2, 0].into_iter().find_map(|fd| {
        let mut ws = WinSize {
            row: 0,
            col: 0,
            x: 0,
            y: 0,
        };
        let ok = unsafe { ioctl(fd, request, &raw mut ws) } == 0;
        (ok && ws.col > 0 && ws.row > 0).then_some(usize::from(ws.col))
    })
}

#[cfg(not(unix))]
fn terminal_width() -> Option<usize> {
    None
}

fn help_width() -> usize {
    terminal_width()
        .or_else(|| env::var("COLUMNS").ok()?.parse().ok())
        .unwrap_or(100)
        .min(98)
}

pub(crate) fn run(args: Vec<OsString>) -> ExitCode {
    let name = bin();
    let width = help_width();
    let mut cmd = cli::command(bin()).term_width(width);
    let full = std::iter::once(OsString::from(name)).chain(args);
    let parsed = cmd.try_get_matches_from_mut(full).and_then(|matches| {
        let shell = matches
            .contains_id("gen_completions")
            .then(|| matches.get_one::<Shell>("gen_completions").copied());
        Opts::from_arg_matches(&matches)
            .map(|mut opts| {
                opts.flags = Flags::from_matches(&matches);
                (opts, shell)
            })
            .map_err(|e| e.format(&mut cmd))
    });
    let (opts, shell) = match parsed {
        Ok(opts) => opts,
        Err(e) => {
            if e.kind() == clap::error::ErrorKind::DisplayHelp {
                help::print(&e, width);
            } else {
                let _ = e.print();
            }
            return ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(2));
        }
    };
    if let Some(shell) = shell {
        let Some(shell) = shell.or_else(Shell::from_env) else {
            eprintln!("[{name} error]: Unable to get shell from environment");
            return ExitCode::from(1);
        };
        let stem = Path::new(name).file_stem().and_then(|s| s.to_str());
        let script = complete::generate(shell, cli::command(name), stem.unwrap_or("fd"));
        let _ = io::stdout().write_all(script.as_bytes());
        return ExitCode::SUCCESS;
    }
    match run_opts(opts) {
        Ok(code) => code.into(),
        Err(err) => {
            eprintln!("[{name} error]: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run_opts(opts: Opts) -> Result<Exit> {
    set_working_dir(&opts)?;
    let search_paths = opts.search_paths()?;
    if search_paths.is_empty() {
        bail!("No valid search paths given.");
    }
    ensure_search_pattern_is_not_a_path(&opts)?;
    let pattern_regexps = opts
        .exprs
        .iter()
        .flatten()
        .chain([&opts.pattern])
        .map(|pat| build_pattern_regex(pat, &opts))
        .collect::<Result<Vec<String>>>()?;
    let config = construct_config(opts, &pattern_regexps)?;
    ensure_use_hidden_option_for_leading_dot_pattern(&config, &pattern_regexps)?;
    let regexps = pattern_regexps
        .iter()
        .map(|pat| build_regex(pat, config.is(Set::CaseSensitive)).map(|re| (pat.is_empty(), re)))
        .collect::<Result<Vec<_>>>()?;
    let active: Vec<Regex> = regexps
        .into_iter()
        .filter_map(|(empty, re)| (!empty).then_some(re))
        .collect();
    walk::scan(&search_paths, &active, &config)
}

fn set_working_dir(opts: &Opts) -> Result<()> {
    if let Some(base_directory) = &opts.base_directory {
        if !fsx::is_existing_directory(base_directory) {
            return Err(anyhow!(
                "The '--base-directory' path '{}' is not a directory.",
                base_directory.to_string_lossy()
            ));
        }
        env::set_current_dir(base_directory).with_context(|| {
            format!(
                "Could not set '{}' as the current working directory",
                base_directory.to_string_lossy()
            )
        })?;
    }
    Ok(())
}

fn ensure_search_pattern_is_not_a_path(opts: &Opts) -> Result<()> {
    if !opts.flags.has(Flag::FullPath)
        && opts.pattern.contains(std::path::MAIN_SEPARATOR)
        && Path::new(&opts.pattern).is_dir()
    {
        Err(anyhow!(
            "The search pattern '{pattern}' contains a path-separation character ('{sep}') \
             and will not lead to any search results.\n\n\
             If you want to search for all files inside the '{pattern}' directory, use a match-all pattern:\n\n  \
             {name} . '{pattern}'\n\n\
             Instead, if you want your pattern to match the full file path, use:\n\n  \
             {name} --full-path '{pattern}'",
            pattern = opts.pattern,
            sep = std::path::MAIN_SEPARATOR,
            name = bin(),
        ))
    } else {
        Ok(())
    }
}

fn build_pattern_regex(pattern: &str, opts: &Opts) -> Result<String> {
    Ok(if opts.flags.has(Flag::Glob) && !pattern.is_empty() {
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| anyhow!("{e}"))?
            .regex()
            .to_owned()
    } else if opts.flags.has(Flag::FixedStrings) {
        regex::escape(pattern)
    } else {
        String::from(pattern)
    })
}

fn construct_config(mut opts: Opts, pattern_regexps: &[String]) -> Result<Config> {
    let flags = opts.flags;
    let case_sensitive = !flags.has(Flag::IgnoreCase)
        && (flags.has(Flag::CaseSensitive)
            || pattern_regexps
                .iter()
                .any(|pat| smart::pattern_has_uppercase_char(pat)));
    let path_separator = opts
        .path_separator
        .take()
        .or_else(fsx::default_path_separator);
    let actual_path_separator = path_separator
        .clone()
        .unwrap_or_else(|| std::path::MAIN_SEPARATOR.to_string());
    if cfg!(windows)
        && let Some(sep) = &path_separator
        && sep.len() > 1
    {
        bail!(
            "A path separator must be exactly one byte, but the given separator is {} bytes: '{}'.\n\
             In some shells on Windows, '/' is automatically expanded. Try to use '//' instead.",
            sep.len(),
            sep
        );
    }
    let size_constraints = std::mem::take(&mut opts.size);
    let time_constraints = extract_time_constraints(&opts)?;
    #[cfg(unix)]
    let owner_constraint = opts.owner.and_then(OwnerFilter::filter_ignore);
    let interactive_terminal = std::io::stdout().is_terminal();
    let colored_output = match opts.color {
        ColorWhen::Always => true,
        ColorWhen::Never => false,
        ColorWhen::Auto => {
            let no_color = env::var_os("NO_COLOR").is_some_and(|x| !x.is_empty());
            !no_color && interactive_terminal
        }
    };
    let ls_colors = colored_output.then(LsColors::from_env_or_default);
    let hyperlink = match opts.hyperlink {
        HyperlinkWhen::Always => true,
        HyperlinkWhen::Never => false,
        HyperlinkWhen::Auto => colored_output,
    };
    let command = match opts.exec.command.take() {
        Some(cmd) => Some(cmd),
        None if flags.has(Flag::ListDetails) => Some(
            CommandSet::new_batch([determine_ls_command(colored_output)?])
                .map_err(|e| anyhow!("{e}"))?,
        ),
        None => None,
    };
    let has_command = command.is_some();
    let extensions = opts
        .extensions
        .as_deref()
        .map(Extensions::new)
        .transpose()?;
    let unrestricted = opts.rg_alias_ignore();
    let no_ignore = flags.has(Flag::NoIgnore) || unrestricted;
    let null_separator = flags.has(Flag::NullSeparator);
    let strip_cwd_prefix = opts.strip_cwd_prefix(|| !(null_separator || has_command));
    let bits = [
        (Set::CaseSensitive, case_sensitive),
        (Set::FullPath, flags.has(Flag::FullPath)),
        (
            Set::IgnoreHidden,
            !(flags.has(Flag::Hidden) || unrestricted),
        ),
        (Set::ReadFdignore, !no_ignore),
        (
            Set::ReadVcsignore,
            !(no_ignore || flags.has(Flag::NoIgnoreVcs)),
        ),
        (Set::RequireGit, !flags.has(Flag::NoRequireGit)),
        (Set::ReadParentIgnore, !flags.has(Flag::NoIgnoreParent)),
        (
            Set::ReadGlobalIgnore,
            !(no_ignore || flags.has(Flag::NoGlobalIgnoreFile)),
        ),
        (Set::FollowLinks, flags.has(Flag::Follow)),
        (Set::OneFileSystem, flags.has(Flag::OneFileSystem)),
        (Set::NullSeparator, null_separator),
        (Set::Quiet, flags.has(Flag::Quiet)),
        (Set::Prune, flags.has(Flag::Prune)),
        (Set::Hyperlink, hyperlink),
        (Set::InteractiveTerminal, interactive_terminal),
        (Set::ShowErrors, flags.has(Flag::ShowErrors)),
        (Set::StripCwdPrefix, strip_cwd_prefix),
    ]
    .iter()
    .filter(|(_, on)| *on)
    .fold(0, |acc, (setting, _)| acc | (1 << *setting as u32));
    Ok(Config {
        bits,
        max_depth: opts.max_depth(),
        min_depth: opts.min_depth(),
        threads: opts.threads().get(),
        max_buffer_time: opts.max_buffer_time,
        ls_colors,
        file_types: opts.filetype.as_ref().map(|values| {
            let mut file_types = FileTypes::default();
            for &value in values {
                file_types.set(value);
                if value == FileType::Executable {
                    file_types.set(FileType::File);
                }
            }
            if file_types.has(FileType::Empty)
                && !(file_types.has(FileType::File) || file_types.has(FileType::Directory))
            {
                file_types.set(FileType::File);
                file_types.set(FileType::Directory);
            }
            file_types
        }),
        extensions,
        format: opts.format.as_deref().map(FormatTemplate::parse),
        command,
        batch_size: opts.batch_size,
        exclude_patterns: opts.exclude.iter().map(|p| format!("!{p}")).collect(),
        ignore_files: std::mem::take(&mut opts.ignore_file),
        size_constraints,
        time_constraints,
        #[cfg(unix)]
        owner_constraint,
        path_separator,
        actual_path_separator,
        max_results: opts.max_results(),
        ignore_contain: std::mem::take(&mut opts.ignore_contain),
    })
}

fn determine_ls_command(colored_output: bool) -> Result<Vec<&'static str>> {
    let gnu_ls = |command_name| {
        vec![
            command_name,
            "-l",
            "-h",
            "-d",
            if colored_output {
                "--color=always"
            } else {
                "--color=never"
            },
        ]
    };
    let probe = |name: &str| {
        std::process::Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    };
    if cfg!(unix) {
        if !cfg!(any(
            target_os = "macos",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd"
        )) {
            Ok(gnu_ls("ls"))
        } else if probe("gls") {
            Ok(gnu_ls("gls"))
        } else {
            let mut cmd = vec!["ls", "-l", "-h", "-d"];
            if !cfg!(any(target_os = "netbsd", target_os = "openbsd")) && colored_output {
                cmd.push("-G");
            }
            Ok(cmd)
        }
    } else if cfg!(windows) {
        if probe("ls") {
            Ok(gnu_ls("ls"))
        } else {
            Err(anyhow!(
                "'{} --list-details' is not supported on Windows unless GNU 'ls' is installed.",
                bin()
            ))
        }
    } else {
        Err(anyhow!(
            "'{} --list-details' is not supported on this platform.",
            bin()
        ))
    }
}

fn extract_time_constraints(opts: &Opts) -> Result<Vec<TimeFilter>> {
    let mut time_constraints = Vec::new();
    let invalid = |t: &str| {
        anyhow!(
            "'{t}' is not a valid date or duration. See '{} --help'.",
            bin()
        )
    };
    if let Some(t) = &opts.changed_within {
        time_constraints.push(TimeFilter::after(t).ok_or_else(|| invalid(t))?);
    }
    if let Some(t) = &opts.changed_before {
        time_constraints.push(TimeFilter::before(t).ok_or_else(|| invalid(t))?);
    }
    Ok(time_constraints)
}

fn ensure_use_hidden_option_for_leading_dot_pattern(
    config: &Config,
    pattern_regexps: &[String],
) -> Result<()> {
    if cfg!(unix)
        && config.is(Set::IgnoreHidden)
        && pattern_regexps
            .iter()
            .any(|pat| smart::pattern_matches_strings_with_leading_dot(pat))
    {
        Err(anyhow!(
            "The pattern(s) seems to only match files with a leading dot, but hidden files are \
            filtered by default. Consider adding -H/--hidden to search hidden files as well \
            or adjust your search pattern(s)."
        ))
    } else {
        Ok(())
    }
}

fn build_regex(pattern_regex: &str, case_sensitive: bool) -> Result<Regex> {
    RegexBuilder::new(pattern_regex)
        .case_insensitive(!case_sensitive)
        .dot_matches_new_line(true)
        .build()
        .map_err(|e| {
            anyhow!(
                "{e}\n\nNote: You can use the '--fixed-strings' option to search for a \
                 literal string instead of a regular expression. Alternatively, you can \
                 also use the '--glob' option to match on a glob pattern."
            )
        })
}
