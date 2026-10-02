use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::hc_internal::grep::matcher::LineTerminator;
use crate::hc_internal::grep::printer::termcolor::WriteColor;
use crate::hc_internal::grep::printer::{
    ColorSpecs, HyperlinkConfig, HyperlinkEnvironment, HyperlinkFormat, JSON, JSONBuilder,
    PathPrinterBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder, SummaryKind,
    UserColorSpec, default_color_specs,
};
use crate::hc_internal::grep::searcher::{BinaryDetection, MmapChoice, Searcher, SearcherBuilder};
use crate::hc_internal::walker::overrides::{Override, OverrideBuilder};
use crate::hc_internal::walker::types::{Types, TypesBuilder};
use crate::hc_internal::walker::{WalkBuilder, tree_threads};

use super::flags::{
    BinaryMode, BoundaryMode, BufferMode, CaseMode, ColorChoice, ContextMode, DevAction, DirAction,
    EncodingMode, EngineChoice, GnuBinaryFiles, GnuMatcher, LowArgs, MmapMode, Mode, Personality,
    SearchMode, Separator, SortMode, SortModeKind, TypeChange,
};
use super::gnu_search::{GnuPrinter, GnuPrinterConfig, ListFiles};
use super::haystack::HaystackBuilder;
use super::matchers::{PatternMatcher, build_gnu, build_pcre2, build_rust};
use super::patterns::{self, Patterns, StdinState};
use super::worker::{Printer, SearchWorker, SearchWorkerBuilder};
use crate::hc_internal::gnu::colors::{Colors as GnuColors, term_allows_color};
use crate::hc_internal::gnu::exclude::Excludes;
use crate::hc_internal::gnu::output::{BinaryFiles, OutputConfig};
use crate::hc_internal::gnu::regex::Dialect;

#[derive(Debug)]
pub(crate) struct HiArgsError(pub(crate) String);

impl<T: std::fmt::Display> From<T> for HiArgsError {
    fn from(err: T) -> HiArgsError {
        HiArgsError(err.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Recursion {
    Smart,
    Gnu { deref: bool },
}

#[derive(Clone, Debug)]
pub(crate) struct Paths {
    pub(crate) operands: Vec<PathBuf>,
    pub(crate) has_implicit_path: bool,
    pub(crate) is_one_file: bool,
    pub(crate) operand_count: usize,
}

impl Paths {
    fn is_only_stdin(&self) -> bool {
        self.operands.len() == 1 && self.operands[0] == Path::new("-")
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BinaryPair {
    pub(crate) explicit: BinaryDetection,
    pub(crate) implicit: BinaryDetection,
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct GnuSettings {
    pub(crate) printer: bool,
    pub(crate) dir_action: Option<DirAction>,
    pub(crate) dev_action: Option<DevAction>,
    pub(crate) excludes: Excludes,
    pub(crate) label: Option<Vec<u8>>,
    pub(crate) binary_files: GnuBinaryFiles,
    pub(crate) utf8_locale: bool,
    pub(crate) filename_option: Option<bool>,
    pub(crate) initial_tab: bool,
    pub(crate) colors: Option<GnuColors>,
    pub(crate) context_set: bool,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct HiArgs {
    pub(crate) personality: Personality,
    pub(crate) mode: Mode,
    patterns: Patterns,
    pub(crate) paths: Paths,
    binary: BinaryPair,
    boundary: Option<BoundaryMode>,
    buffer: BufferMode,
    byte_offset: bool,
    case: CaseMode,
    pub(crate) color: ColorChoice,
    colors: ColorSpecs,
    column: bool,
    context: ContextMode,
    context_separator: Option<Vec<u8>>,
    crlf: bool,
    cwd: PathBuf,
    dfa_size_limit: Option<usize>,
    encoding: Option<&'static crate::hc_internal::encoding::Encoding>,
    bom_sniffing: bool,
    engine: EngineChoice,
    field_context_separator: Vec<u8>,
    field_match_separator: Vec<u8>,
    pub(crate) file_separator: Option<Vec<u8>>,
    fixed_strings: bool,
    follow: bool,
    globs: Override,
    heading: bool,
    hidden: bool,
    hyperlink_config: HyperlinkConfig,
    ignore_file_case_insensitive: bool,
    ignore_file: Vec<PathBuf>,
    include_zero: bool,
    invert_match: bool,
    pub(crate) is_terminal_stdout: bool,
    line_number: bool,
    max_columns: Option<u64>,
    max_columns_preview: bool,
    max_count: Option<u64>,
    max_depth: Option<usize>,
    max_filesize: Option<u64>,
    mmap_choice: MmapChoice,
    multiline: bool,
    multiline_dotall: bool,
    no_ignore_dot: bool,
    no_ignore_exclude: bool,
    no_ignore_files: bool,
    no_ignore_global: bool,
    no_ignore_parent: bool,
    no_ignore_vcs: bool,
    no_require_git: bool,
    no_unicode: bool,
    null_data: bool,
    one_file_system: bool,
    only_matching: bool,
    path_separator: Option<u8>,
    path_terminator: Option<u8>,
    pre: Option<PathBuf>,
    pre_globs: Override,
    pub(crate) quiet: bool,
    pub(crate) quit_after_match: bool,
    regex_size_limit: Option<usize>,
    replace: Option<Vec<u8>>,
    search_zip: bool,
    sort: Option<SortMode>,
    stats: Option<Stats>,
    stop_on_nonmatch: bool,
    pub(crate) threads: usize,
    file_threads: usize,
    trim: bool,
    types: Types,
    vimgrep: bool,
    with_filename: bool,
    gnu_matcher: Option<GnuMatcher>,
    pub(crate) recursion: Recursion,
    pub(crate) gnu: GnuSettings,
    dev_null_output: bool,
    gitignore: bool,
}

fn current_dir() -> Result<PathBuf, HiArgsError> {
    match std::env::current_dir() {
        Ok(cwd) => Ok(cwd),
        Err(err) => match std::env::var_os("PWD") {
            Some(pwd) if !pwd.is_empty() => Ok(PathBuf::from(pwd)),
            _ => Err(HiArgsError(format!(
                "failed to get current working directory: {err}\n\
                 did your CWD get deleted?"
            ))),
        },
    }
}

#[cfg(unix)]
fn stdout_is_dev_null() -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let Ok(fd) = std::io::stdout().as_fd().try_clone_to_owned() else {
        return false;
    };
    let Ok(out) = std::fs::File::from(fd).metadata() else {
        return false;
    };
    if !out.file_type().is_char_device() {
        return false;
    }
    std::fs::metadata("/dev/null")
        .is_ok_and(|null| null.dev() == out.dev() && null.ino() == out.ino())
}

#[cfg(not(unix))]
fn stdout_is_dev_null() -> bool {
    false
}

pub(crate) fn is_readable_stdin() -> bool {
    if std::io::stdin().is_terminal() {
        return false;
    }
    stdin_kind_is_readable()
}

#[cfg(unix)]
fn stdin_kind_is_readable() -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::FileTypeExt;
    let Ok(fd) = std::io::stdin().as_fd().try_clone_to_owned() else {
        return false;
    };
    let Ok(md) = std::fs::File::from(fd).metadata() else {
        return false;
    };
    let ft = md.file_type();
    ft.is_file() || ft.is_fifo() || ft.is_socket()
}

#[cfg(not(unix))]
fn stdin_kind_is_readable() -> bool {
    true
}

pub(crate) fn utf8_locale() -> bool {
    static UTF8: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *UTF8.get_or_init(|| crate::hc_internal::gnu::locale::Locale::from_env().utf8)
}

fn gnu_dialect_selected(low: &LowArgs, personality: Personality) -> bool {
    low.engine == EngineChoice::Default
        && match low.gnu_matcher {
            Some(GnuMatcher::Grep | GnuMatcher::Egrep | GnuMatcher::Fgrep) => true,
            Some(GnuMatcher::Perl) => false,
            None => personality == Personality::Grep,
        }
}

fn should_colorize_env() -> bool {
    match std::env::var_os("TERM") {
        None => false,
        Some(term) => term != "dumb",
    }
}

fn resolve_color(low: &LowArgs, personality: Personality, is_tty: bool) -> ColorChoice {
    let default = match personality {
        Personality::Grep => ColorChoice::Never,
        Personality::Rg => ColorChoice::Auto,
    };
    match low.color.unwrap_or(default) {
        ColorChoice::Auto => {
            let env_ok = should_colorize_env() && std::env::var_os("NO_COLOR").is_none();
            if is_tty && (personality == Personality::Grep && should_colorize_env() || env_ok) {
                ColorChoice::Always
            } else {
                ColorChoice::Never
            }
        }
        other => other,
    }
}

impl HiArgs {
    pub(crate) fn from_low_args(
        mut low: LowArgs,
        personality: Personality,
    ) -> Result<HiArgs, HiArgsError> {
        if let Some(sort) = low.sort {
            sort_supported(sort)?;
        }
        if let Mode::Search(ref mut mode) = low.mode {
            match *mode {
                SearchMode::CountMatches if low.invert_match => *mode = SearchMode::Count,
                SearchMode::Count if low.only_matching && personality == Personality::Rg => {
                    *mode = SearchMode::CountMatches;
                }
                _ => {}
            }
        }
        let is_terminal_stdout = std::io::stdout().is_terminal();
        let cwd = current_dir()?;
        let gnu_dialect = gnu_dialect_selected(&low, personality);
        let mut stdin = StdinState { consumed: false };
        let mut patterns = patterns::from_low_args(&mut low, personality, gnu_dialect, &mut stdin)
            .map_err(|e| HiArgsError(e.0))?;
        if patterns.gnu_empty_source && !gnu_dialect {
            low.invert_match = !low.invert_match;
            low.boundary = None;
            patterns.list = vec![Vec::new()];
            patterns.gnu_empty_source = false;
        }
        let recursion = if low.directories == Some(DirAction::Recurse) {
            Recursion::Gnu {
                deref: low.deref_recursive,
            }
        } else {
            Recursion::Smart
        };
        if low.gitignore && recursion == Recursion::Smart {
            return Err(HiArgsError(
                "--gitignore requires -r/--recursive (or -R or --directories=recurse)".into(),
            ));
        }
        let paths = paths_from_low(&mut low, personality, recursion, &stdin)?;
        let binary = binary_from_low(&low, personality);
        let colors = color_specs(&low)?;
        let hyperlink_config = hyperlink_config(&low)?;
        let stats = stats_of(&low);
        let types = types_of(&low)?;
        let globs = globs_of(&cwd, &low)?;
        let pre_globs = pre_globs_of(&cwd, &low)?;
        let color = resolve_color(&low, personality, is_terminal_stdout);
        let column = low.column.unwrap_or(low.vimgrep);
        let heading = match (low.heading, personality) {
            (None, Personality::Rg) => !low.vimgrep && is_terminal_stdout,
            (None, Personality::Grep) | (Some(false), _) => false,
            (Some(true), _) => !low.vimgrep,
        };
        let path_terminator = low.null.then_some(b'\0');
        let quit_after_match = stats.is_none() && low.quiet;
        let threads = if low.sort.is_some() || paths.is_one_file {
            1
        } else if let Some(threads) = low.threads {
            threads
        } else {
            std::thread::available_parallelism().map_or(1, |n| tree_threads(n.get()))
        };
        let with_filename = low
            .with_filename
            .unwrap_or(low.vimgrep || !paths.is_one_file);
        let context_separator = match low.context_separator.clone() {
            None => Some(b"--".to_vec()),
            Some(Separator::Disabled) => None,
            Some(Separator::Escaped(bytes) | Separator::Literal(bytes)) => Some(bytes),
        };
        let file_separator = match low.mode {
            Mode::Search(SearchMode::Standard) => {
                if heading {
                    Some(Vec::new())
                } else if let ContextMode::Limited(limited) = low.context {
                    let (before, after) = limited.get();
                    if before > 0 || after > 0 {
                        context_separator.clone()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        let line_number = low.line_number.unwrap_or_else(|| {
            if low.quiet || personality == Personality::Grep {
                return false;
            }
            let Mode::Search(search_mode) = low.mode else {
                return false;
            };
            match search_mode {
                SearchMode::FilesWithMatches
                | SearchMode::FilesWithoutMatch
                | SearchMode::Count
                | SearchMode::CountMatches => false,
                SearchMode::Json => true,
                SearchMode::Standard => {
                    (is_terminal_stdout && !paths.is_only_stdin()) || column || low.vimgrep
                }
            }
        });
        let mmap_choice = match low.mmap {
            MmapMode::Auto => {
                if paths.operands.len() <= 10 && paths.operands.iter().all(|p| p.is_file()) {
                    MmapChoice::auto()
                } else {
                    MmapChoice::never()
                }
            }
            MmapMode::AlwaysTryMmap => MmapChoice::auto(),
            MmapMode::Never => MmapChoice::never(),
        };
        let (encoding, bom_sniffing) = match &low.encoding {
            None | Some(EncodingMode::Auto) => (None, true),
            Some(EncodingMode::Disabled) => (None, false),
            Some(EncodingMode::Label(deferred)) => {
                match crate::hc_internal::encoding::Encoding::for_label_no_replacement(
                    deferred.value.as_bytes(),
                ) {
                    Some(enc) => (Some(enc), true),
                    None => {
                        return Err(HiArgsError(format!(
                            "error parsing flag {}: grep config error: unknown encoding: {}",
                            deferred.flag, deferred.value
                        )));
                    }
                }
            }
        };
        let gnu = GnuSettings {
            printer: personality == Personality::Grep && !low.rg_only_output,
            dir_action: low.directories,
            dev_action: low.devices,
            excludes: excludes_of(&low.excludes, utf8_locale())?,
            label: low
                .label
                .as_deref()
                .map(|l| super::flags::os_bytes(l).to_vec()),
            binary_files: low.gnu_binary_files.unwrap_or(GnuBinaryFiles::Binary),
            utf8_locale: utf8_locale(),
            filename_option: low.with_filename,
            initial_tab: low.initial_tab,
            colors: gnu_colors(personality, &low, is_terminal_stdout),
            context_set: matches!(low.context, ContextMode::Limited(l) if l.is_set()),
        };
        let file_separator = if gnu.printer { None } else { file_separator };
        let include_zero = low.include_zero.unwrap_or(personality == Personality::Grep);
        Ok(HiArgs {
            personality,
            mode: low.mode,
            patterns,
            paths,
            binary,
            boundary: low.boundary,
            buffer: low.buffer,
            byte_offset: low.byte_offset,
            case: low.case,
            color,
            colors,
            column,
            context: low.context,
            context_separator,
            crlf: low.crlf,
            cwd,
            dfa_size_limit: low.dfa_size_limit,
            encoding,
            bom_sniffing,
            engine: low.engine,
            field_context_separator: low
                .field_context_separator
                .clone()
                .unwrap_or_else(|| b"-".to_vec()),
            field_match_separator: low
                .field_match_separator
                .clone()
                .unwrap_or_else(|| b":".to_vec()),
            file_separator,
            fixed_strings: low.fixed_strings,
            follow: low.follow,
            globs,
            heading,
            hidden: low.hidden,
            hyperlink_config,
            ignore_file_case_insensitive: low.ignore_file_case_insensitive,
            ignore_file: low.ignore_file.clone(),
            include_zero,
            invert_match: low.invert_match,
            is_terminal_stdout,
            line_number,
            max_columns: low.max_columns,
            max_columns_preview: low.max_columns_preview,
            max_count: low.max_count,
            max_depth: low.max_depth,
            max_filesize: low.max_filesize,
            mmap_choice,
            multiline: low.multiline,
            multiline_dotall: low.multiline_dotall,
            no_ignore_dot: low.no_ignore_dot,
            no_ignore_exclude: low.no_ignore_exclude,
            no_ignore_files: low.no_ignore_files,
            no_ignore_global: low.no_ignore_global,
            no_ignore_parent: low.no_ignore_parent,
            no_ignore_vcs: low.no_ignore_vcs,
            no_require_git: low.no_require_git,
            no_unicode: low.no_unicode,
            null_data: low.null_data,
            one_file_system: low.one_file_system,
            only_matching: low.only_matching,
            path_separator: low.path_separator,
            path_terminator,
            pre: low.pre.clone(),
            pre_globs,
            quiet: low.quiet,
            quit_after_match,
            regex_size_limit: low.regex_size_limit,
            replace: low.replace.clone(),
            search_zip: low.search_zip,
            sort: low.sort,
            stats,
            stop_on_nonmatch: low.stop_on_nonmatch,
            threads,
            file_threads: if threads > 1 {
                1
            } else {
                low.threads.unwrap_or_else(|| {
                    std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
                })
            },
            trim: low.trim,
            types,
            vimgrep: low.vimgrep,
            with_filename,
            gnu_matcher: low.gnu_matcher,
            recursion,
            gnu,
            dev_null_output: stdout_is_dev_null(),
            gitignore: low.gitignore,
        })
    }

    pub(crate) fn gnu_fast_walk(&self) -> Option<bool> {
        let Recursion::Gnu { deref } = self.recursion else {
            return None;
        };
        let plain = !self.gitignore
            && self.max_depth.is_none()
            && self.max_filesize.is_none()
            && !self.one_file_system
            && self.globs.is_empty()
            && self.types.is_empty();
        plain.then_some(deref || self.follow)
    }

    pub(crate) fn explicit_files_only(&self) -> bool {
        self.threads > 1
            && !self.paths.has_implicit_path
            && self
                .paths
                .operands
                .iter()
                .all(|p| p.as_os_str() != "-" && !p.is_dir())
    }

    pub(crate) fn has_implicit_path(&self) -> bool {
        self.paths.has_implicit_path
    }

    pub(crate) fn haystack_builder(&self) -> HaystackBuilder {
        let dev_action = match (self.gnu.dev_action, self.recursion) {
            (None, Recursion::Gnu { deref: true }) if self.personality == Personality::Grep => {
                Some(DevAction::Read)
            }
            (action, _) => action,
        };
        HaystackBuilder::new(self.paths.has_implicit_path, dev_action, self.recursion)
    }

    pub(crate) fn uses_gnu_dialect(&self) -> bool {
        self.engine == EngineChoice::Default
            && match self.gnu_matcher {
                Some(GnuMatcher::Grep | GnuMatcher::Egrep | GnuMatcher::Fgrep) => true,
                Some(GnuMatcher::Perl) => false,
                None => self.personality == Personality::Grep,
            }
    }

    pub(crate) fn matcher(&self) -> Result<PatternMatcher, HiArgsError> {
        if self.uses_gnu_dialect() {
            return build_gnu(&self.gnu_spec()).map_err(HiArgsError);
        }
        match self.engine {
            EngineChoice::Default => build_rust(&self.rust_spec()).map_err(HiArgsError),
            EngineChoice::Pcre2 => build_pcre2(&self.pcre_spec()).map_err(HiArgsError),
            EngineChoice::Auto => match build_rust(&self.rust_spec()) {
                Ok(m) => Ok(m),
                Err(rust_err) => match build_pcre2(&self.pcre_spec()) {
                    Ok(m) => Ok(m),
                    Err(pcre_err) => {
                        let divider = "~".repeat(79);
                        Err(HiArgsError(format!(
                            "regex could not be compiled with either the default \
                             regex engine or with PCRE2.\n\n\
                             default regex engine error:\n\
                             {divider}\n\
                             {rust_err}\n\
                             {divider}\n\n\
                             PCRE2 regex engine error:\n{pcre_err}"
                        )))
                    }
                },
            },
        }
    }

    fn gnu_spec(&self) -> super::matchers::GnuSpec<'_> {
        super::matchers::GnuSpec {
            sources: &self.patterns.gnu_sources,
            dialect: match self.gnu_matcher {
                Some(GnuMatcher::Egrep) => Dialect::Extended,
                Some(GnuMatcher::Fgrep) => Dialect::Fixed,
                _ if self.fixed_strings => Dialect::Fixed,
                _ => Dialect::Basic,
            },
            case: self.case,
            boundary: self.boundary,
            null_data: self.null_data,
            utf8: self.gnu.utf8_locale && !self.no_unicode,
        }
    }

    fn rust_spec(&self) -> super::matchers::RustSpec<'_> {
        super::matchers::RustSpec {
            patterns: &self.patterns.list,
            fixed_strings: self.fixed_strings,
            case: self.case,
            boundary: self.boundary,
            no_unicode: self.no_unicode,
            multiline: self.multiline,
            multiline_dotall: self.multiline_dotall,
            crlf: self.crlf,
            null_data: self.null_data,
            regex_size_limit: self.regex_size_limit,
            dfa_size_limit: self.dfa_size_limit,
            ban_nul: !(self.binary.explicit == BinaryDetection::none()
                && self.binary.implicit == BinaryDetection::none()),
        }
    }

    fn pcre_spec(&self) -> super::matchers::PcreSpec<'_> {
        super::matchers::PcreSpec {
            patterns: &self.patterns.list,
            fixed_strings: self.fixed_strings,
            case: self.case,
            boundary: self.boundary,
            no_unicode: self.no_unicode
                || (self.personality == Personality::Grep && !self.gnu.utf8_locale),
            multiline: self.multiline,
            multiline_dotall: self.multiline_dotall,
            crlf: self.crlf,
            gnu: self.personality == Personality::Grep,
        }
    }

    pub(crate) fn matches_possible(&self) -> bool {
        if self.uses_gnu_dialect() || self.personality == Personality::Grep {
            let list = &self.patterns.list;
            let none = self.patterns.gnu_empty_source;
            let keys_empty = list.is_empty() || (list.len() == 1 && list[0].is_empty());
            let invert = self.invert_match ^ none;
            let bounded = self.boundary.is_some() && !none;
            let hopeless = self.max_count == Some(0) || (keys_empty && invert && !bounded);
            let listing_nonmatching = self.mode == Mode::Search(SearchMode::FilesWithoutMatch)
                && !self.quiet
                && !(self.gnu.printer && self.dev_null_output);
            return !hopeless || listing_nonmatching;
        }
        if self.patterns.list.is_empty() && !self.invert_match {
            return false;
        }
        self.max_count != Some(0)
    }

    pub(crate) fn path_printer_builder(&self) -> PathPrinterBuilder {
        let mut builder = PathPrinterBuilder::new();
        builder
            .color_specs(self.colors.clone())
            .hyperlink(self.hyperlink_config.clone())
            .separator(self.path_separator)
            .terminator(self.path_terminator.unwrap_or(b'\n'));
        builder
    }

    pub(crate) fn printer<W: WriteColor>(&self, search_mode: SearchMode, wtr: W) -> Printer<W> {
        if self.gnu.printer {
            return Printer::Gnu(GnuPrinter::new(self.gnu_printer_config(search_mode), wtr));
        }
        let summary_kind = if self.quiet {
            match search_mode {
                SearchMode::FilesWithoutMatch => SummaryKind::QuietWithoutMatch,
                _ => SummaryKind::QuietWithMatch,
            }
        } else {
            match search_mode {
                SearchMode::FilesWithMatches => SummaryKind::PathWithMatch,
                SearchMode::FilesWithoutMatch => SummaryKind::PathWithoutMatch,
                SearchMode::Count => SummaryKind::Count,
                SearchMode::CountMatches => SummaryKind::CountMatches,
                SearchMode::Json => return Printer::Json(self.printer_json(wtr)),
                SearchMode::Standard => return Printer::Standard(self.printer_standard(wtr)),
            }
        };
        Printer::Summary(self.printer_summary(wtr, summary_kind))
    }

    fn gnu_printer_config(&self, search_mode: SearchMode) -> GnuPrinterConfig {
        let exit_on_match = self.quiet;
        let dev_null = !exit_on_match && self.dev_null_output;
        let list_files = match search_mode {
            _ if exit_on_match || dev_null => ListFiles::None,
            SearchMode::FilesWithMatches => ListFiles::Matching,
            SearchMode::FilesWithoutMatch => ListFiles::NonMatching,
            _ => ListFiles::None,
        };
        let stops = exit_on_match || dev_null || list_files != ListFiles::None;
        let count_matches = !stops && search_mode == SearchMode::Count;
        let done_on_match = stops && self.max_count.is_none();
        let mut output = OutputConfig::new(
            if self.null_data { b'\0' } else { b'\n' },
            self.gnu.utf8_locale,
        );
        output.prefix.line_number = self.line_number;
        output.prefix.byte_offset = self.byte_offset;
        output.layout.only_matching = self.only_matching;
        output.layout.initial_tab = self.gnu.initial_tab;
        output.layout.null_after_name = self.path_terminator.is_some();
        output.colors.clone_from(&self.gnu.colors);
        output.invert = self.invert_match;
        output.binary_files = match self.gnu.binary_files {
            GnuBinaryFiles::Binary => BinaryFiles::Binary,
            GnuBinaryFiles::Text => BinaryFiles::Text,
            GnuBinaryFiles::WithoutMatch => BinaryFiles::WithoutMatch,
        };
        let (before, after) = match self.context {
            ContextMode::Limited(limited) => limited.get(),
            ContextMode::Passthru => (0, 0),
        };
        GnuPrinterConfig {
            output,
            filename_option: self.gnu.filename_option,
            operand_count: self.paths.operand_count,
            list_files,
            count_matches,
            out_quiet: count_matches || done_on_match || exit_on_match,
            done_on_match,
            context: self.gnu.context_set,
            before,
            after,
            group_separator: self.context_separator.clone(),
            max_count: self.max_count.unwrap_or(u64::MAX),
            line_buffered: self.buffer == BufferMode::Line,
            defer_lead: false,
            file_threads: self.file_threads,
        }
    }

    fn printer_json<W: std::io::Write>(&self, wtr: W) -> JSON<W> {
        JSONBuilder::new()
            .pretty(false)
            .always_begin_end(false)
            .replacement(self.replace.clone())
            .build(wtr)
    }

    fn printer_standard<W: WriteColor>(&self, wtr: W) -> Standard<W> {
        let mut builder = StandardBuilder::new();
        builder
            .byte_offset(self.byte_offset)
            .color_specs(self.colors.clone())
            .column(self.column)
            .heading(self.heading)
            .hyperlink(self.hyperlink_config.clone())
            .max_columns_preview(self.max_columns_preview)
            .max_columns(self.max_columns)
            .only_matching(self.only_matching)
            .path(self.with_filename)
            .path_terminator(self.path_terminator)
            .per_match_one_line(true)
            .per_match(self.vimgrep)
            .replacement(self.replace.clone())
            .separator_context(self.context_separator.clone())
            .separator_field_context(self.field_context_separator.clone())
            .separator_field_match(self.field_match_separator.clone())
            .separator_path(self.path_separator)
            .stats(self.stats.is_some())
            .trim_ascii(self.trim);
        if self.threads == 1 {
            builder.separator_search(self.file_separator.clone());
        }
        builder.build(wtr)
    }

    fn printer_summary<W: WriteColor>(&self, wtr: W, kind: SummaryKind) -> Summary<W> {
        SummaryBuilder::new()
            .color_specs(self.colors.clone())
            .exclude_zero(!self.include_zero)
            .hyperlink(self.hyperlink_config.clone())
            .kind(kind)
            .path(self.with_filename)
            .path_terminator(self.path_terminator)
            .separator_field(b":".to_vec())
            .separator_path(self.path_separator)
            .stats(self.stats.is_some())
            .build(wtr)
    }

    pub(crate) fn search_worker<W: WriteColor>(
        &self,
        matcher: PatternMatcher,
        searcher: Searcher,
        printer: Printer<W>,
    ) -> Result<SearchWorker<W>, HiArgsError> {
        let mut builder = SearchWorkerBuilder::new();
        builder
            .preprocessor(self.pre.clone())
            .map_err(|e| HiArgsError(e.to_string()))?
            .preprocessor_globs(self.pre_globs.clone())
            .search_zip(self.search_zip)
            .binary_detection_explicit(self.binary.explicit.clone())
            .binary_detection_implicit(self.binary.implicit.clone())
            .gnu(
                self.personality == Personality::Grep,
                self.gnu.label.clone(),
            )
            .gnu_decode(self.encoding.filter(|_| self.gnu.printer).map(|enc| {
                crate::hc_internal::encoding::DecodeOptions {
                    encoding: Some(enc),
                    bom_sniffing: self.bom_sniffing,
                }
            }))
            .dev_null(self.dev_null_status());
        Ok(builder.build(matcher, searcher, printer))
    }

    fn dev_null_status(&self) -> Option<bool> {
        let Mode::Search(mode) = self.mode else {
            return None;
        };
        if !self.dev_null_output {
            return None;
        }
        match self.personality {
            Personality::Grep => Some(false),
            Personality::Rg => Some(mode == SearchMode::FilesWithoutMatch),
        }
    }

    pub(crate) fn searcher(&self) -> Searcher {
        let line_term = if self.crlf {
            LineTerminator::crlf()
        } else if self.null_data {
            LineTerminator::byte(b'\0')
        } else {
            LineTerminator::byte(b'\n')
        };
        let mut builder = SearcherBuilder::new();
        builder
            .max_matches(self.max_count)
            .line_terminator(line_term)
            .invert_match(self.invert_match)
            .line_number(self.line_number || self.gnu.printer)
            .multi_line(self.multiline)
            .memory_map(self.mmap_choice.clone())
            .stop_on_nonmatch(self.stop_on_nonmatch)
            .bom_sniffing(self.bom_sniffing);
        match self.context {
            ContextMode::Passthru => {
                builder.passthru(true);
            }
            ContextMode::Limited(limited) => {
                let (before, after) = limited.get();
                builder.before_context(before);
                builder.after_context(after);
            }
        }
        if let Some(enc) = self.encoding {
            builder.transcoder(Some(super::worker::transcoder(
                Some(enc),
                self.bom_sniffing,
            )));
        } else if self.bom_sniffing {
            builder.transcoder(Some(super::worker::transcoder(None, true)));
        }
        builder.build()
    }

    pub(crate) fn sort<'a, I>(
        &self,
        haystacks: I,
    ) -> Box<dyn Iterator<Item = super::haystack::Haystack> + 'a>
    where
        I: Iterator<Item = super::haystack::Haystack> + 'a,
    {
        let Some(sort) = self.sort else {
            return Box::new(haystacks);
        };
        let get = match sort.kind {
            SortModeKind::Path if !sort.reverse => return Box::new(haystacks),
            SortModeKind::Path => {
                let mut list: Vec<_> = haystacks.collect();
                list.sort_by(|a, b| b.path().cmp(a.path()));
                return Box::new(list.into_iter());
            }
            SortModeKind::LastModified => std::fs::Metadata::modified,
            SortModeKind::LastAccessed => std::fs::Metadata::accessed,
            SortModeKind::Created => std::fs::Metadata::created,
        };
        let mut with_times: Vec<_> = haystacks
            .map(|h| {
                let time = h.path().metadata().and_then(|m| get(&m)).ok();
                (h, time)
            })
            .collect();
        with_times.sort_by(|(_, t1), (_, t2)| {
            let ordering = match (t1, t2) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if sort.reverse {
                ordering.reverse()
            } else {
                ordering
            }
        });
        Box::new(with_times.into_iter().map(|(h, _)| h))
    }

    pub(crate) fn stats(&self) -> Option<Stats> {
        self.stats.clone()
    }

    pub(crate) fn stdout_line_buffered(&self) -> bool {
        match self.buffer {
            BufferMode::Auto => self.is_terminal_stdout,
            BufferMode::Line => true,
            BufferMode::Block => false,
        }
    }

    pub(crate) fn types(&self) -> &Types {
        &self.types
    }

    pub(crate) fn walk_builder_for(&self, roots: &[PathBuf]) -> WalkBuilder {
        let mut builder = WalkBuilder::new(&roots[0]);
        for path in roots.iter().skip(1) {
            builder.add(path);
        }
        builder
            .max_depth(self.max_depth)
            .max_filesize(self.max_filesize)
            .threads(self.threads)
            .same_file_system(self.one_file_system)
            .skip_stdout(matches!(self.mode, Mode::Search(_)) && !self.gnu.printer)
            .overrides(self.globs.clone())
            .types(self.types.clone())
            .current_dir(&self.cwd);
        match self.recursion {
            Recursion::Gnu { deref } => {
                builder
                    .standard_filters(false)
                    .hidden(false)
                    .follow_links(deref || self.follow);
                let excludes = (self.gnu.excludes.has_file_patterns()
                    || self.gnu.excludes.has_dir_patterns())
                .then(|| self.gnu.excludes.clone());
                let git = self.gitignore.then(super::gitfilter::GitFilter::new);
                if excludes.is_some() || git.is_some() {
                    let exclude = excludes.map(exclude_filter);
                    builder.filter_entry(move |dent| {
                        exclude.as_ref().is_none_or(|f| f(dent))
                            && git.as_ref().is_none_or(|g| g.keep(dent))
                    });
                }
            }
            Recursion::Smart => {
                if !self.no_ignore_files {
                    for path in &self.ignore_file {
                        if let Some(err) = builder.add_ignore(path) {
                            super::messages::ignore_message(&err.to_string());
                        }
                    }
                }
                builder
                    .follow_links(self.follow)
                    .hidden(!self.hidden)
                    .parents(!self.no_ignore_parent)
                    .ignore(!self.no_ignore_dot)
                    .git_global(!self.no_ignore_vcs && !self.no_ignore_global)
                    .git_ignore(!self.no_ignore_vcs)
                    .git_exclude(!self.no_ignore_vcs && !self.no_ignore_exclude)
                    .require_git(!self.no_require_git)
                    .ignore_case_insensitive(self.ignore_file_case_insensitive);
                if !self.no_ignore_dot {
                    builder.add_custom_ignore_filename(".rgignore");
                }
                if self.gnu.excludes.has_file_patterns() || self.gnu.excludes.has_dir_patterns() {
                    builder.filter_entry(exclude_filter(self.gnu.excludes.clone()));
                }
            }
        }
        if let Some(sort) = self.sort
            && !sort.reverse
            && sort.kind == SortModeKind::Path
        {
            builder.sort_by_file_name(Ord::cmp);
        }
        builder
    }
}

fn excludes_of(rules: &[super::flags::ExcludeRule], utf8: bool) -> Result<Excludes, HiArgsError> {
    use super::flags::ExcludeRule;
    let mut excludes = Excludes::new(utf8);
    for rule in rules {
        match rule {
            ExcludeRule::Include(glob) => excludes.add_include(super::flags::os_bytes(glob)),
            ExcludeRule::Exclude(glob) => excludes.add_exclude(super::flags::os_bytes(glob)),
            ExcludeRule::ExcludeDir(glob) => excludes.add_exclude_dir(super::flags::os_bytes(glob)),
            ExcludeRule::ExcludeFrom(path) => match std::fs::read(path) {
                Ok(content) => excludes.add_exclude_from(&content),
                Err(err) => {
                    return Err(HiArgsError(format!(
                        "{}: {}",
                        path.display(),
                        super::messages::strerror(&err)
                    )));
                }
            },
        }
    }
    Ok(excludes)
}

fn exclude_filter(
    excludes: Excludes,
) -> impl Fn(&crate::hc_internal::walker::DirEntry) -> bool + Send + Sync + 'static {
    move |dent| {
        if dent.is_stdin() || dent.depth() == 0 {
            return true;
        }
        let name = super::flags::os_bytes(dent.file_name());
        let is_dir = dent
            .file_type()
            .is_some_and(crate::hc_internal::walker::FileType::is_dir)
            || (dent.path_is_symlink() && dent.path().is_dir());
        if is_dir {
            !excludes.skip_dir(name, false)
        } else {
            !excludes.skip_file(name, false)
        }
    }
}

fn gnu_colors(personality: Personality, low: &LowArgs, is_tty: bool) -> Option<GnuColors> {
    if personality != Personality::Grep || low.rg_only_output {
        return None;
    }
    let on = match low.color {
        None | Some(ColorChoice::Never) => false,
        Some(ColorChoice::Always | ColorChoice::Ansi) => true,
        Some(ColorChoice::Auto) => {
            !low.quiet
                && is_tty
                && !stdout_is_dev_null()
                && term_allows_color(std::env::var("TERM").ok().as_deref())
        }
    };
    if !on {
        return None;
    }
    let (colors, warning) = GnuColors::from_env();
    if let Some(warning) = warning {
        super::messages::eprint_locked(&warning);
    }
    Some(colors)
}

fn sort_supported(sort: SortMode) -> Result<(), HiArgsError> {
    let probe =
        |label: &str, get: fn(&std::fs::Metadata) -> std::io::Result<std::time::SystemTime>| {
            let md = std::env::current_exe()
                .and_then(|p| p.metadata())
                .and_then(|m| get(&m));
            match md {
                Ok(_) => Ok(()),
                Err(err) => Err(HiArgsError(format!(
                    "sorting by {label} isn't supported: {err}"
                ))),
            }
        };
    match sort.kind {
        SortModeKind::Path => Ok(()),
        SortModeKind::LastModified => probe("last modified", std::fs::Metadata::modified),
        SortModeKind::LastAccessed => probe("last accessed", std::fs::Metadata::accessed),
        SortModeKind::Created => probe("creation time", std::fs::Metadata::created),
    }
}

fn paths_from_low(
    low: &mut LowArgs,
    personality: Personality,
    recursion: Recursion,
    stdin: &StdinState,
) -> Result<Paths, HiArgsError> {
    let paths: Vec<PathBuf> = low.positional.drain(..).map(PathBuf::from).collect();
    if stdin.consumed && paths.iter().any(|p| p == Path::new("-")) {
        return Err(HiArgsError(
            "error: attempted to read patterns from stdin while also searching stdin".into(),
        ));
    }
    let operand_count = paths.len();
    if !paths.is_empty() {
        let is_one_file = paths.len() == 1 && (paths[0] == Path::new("-") || !paths[0].is_dir());
        return Ok(Paths {
            operands: paths,
            has_implicit_path: false,
            is_one_file,
            operand_count,
        });
    }
    let search = matches!(low.mode, Mode::Search(_));
    let use_cwd = match (personality, recursion) {
        (Personality::Grep, Recursion::Gnu { .. }) if low.recursive_given => true,
        (Personality::Grep, _) => !search || stdin.consumed || std::io::stdin().is_terminal(),
        (Personality::Rg, _) => !search || stdin.consumed || !is_readable_stdin(),
    };
    let (path, is_one_file) = if use_cwd {
        let path = if recursion == Recursion::Smart {
            PathBuf::from("./")
        } else {
            PathBuf::from(".")
        };
        (path, false)
    } else {
        (PathBuf::from("-"), true)
    };
    Ok(Paths {
        operands: vec![path],
        has_implicit_path: true,
        is_one_file,
        operand_count: 0,
    })
}

fn binary_from_low(low: &LowArgs, personality: Personality) -> BinaryPair {
    let none = low.binary == BinaryMode::AsText
        || low.null_data
        || low.gnu_binary_files == Some(GnuBinaryFiles::Text);
    if none {
        return BinaryPair {
            explicit: BinaryDetection::none(),
            implicit: BinaryDetection::none(),
        };
    }
    if personality == Personality::Grep && !low.rg_only_output {
        let detection = if low.gnu_binary_files == Some(GnuBinaryFiles::WithoutMatch) {
            BinaryDetection::quit(b'\0')
        } else {
            BinaryDetection::convert(b'\0')
        };
        return BinaryPair {
            explicit: detection.clone(),
            implicit: detection,
        };
    }
    let convert = low.binary == BinaryMode::SearchAndSuppress;
    BinaryPair {
        explicit: BinaryDetection::convert(b'\0'),
        implicit: if convert {
            BinaryDetection::convert(b'\0')
        } else {
            BinaryDetection::quit(b'\0')
        },
    }
}

fn color_specs(low: &LowArgs) -> Result<ColorSpecs, HiArgsError> {
    let mut specs = default_color_specs();
    for deferred in &low.colors {
        let spec: UserColorSpec = deferred
            .value
            .parse()
            .map_err(|err| HiArgsError(format!("error parsing flag {}: {err}", deferred.flag)))?;
        specs.push(spec);
    }
    Ok(ColorSpecs::new(&specs))
}

fn hyperlink_config(low: &LowArgs) -> Result<HyperlinkConfig, HiArgsError> {
    let mut env = HyperlinkEnvironment::new();
    if let Some(host) = super::hostname::hostname(low.hostname_bin.as_deref()) {
        env.host(Some(host));
    }
    if cfg!(unix)
        && let Some(distro) = std::env::var_os("WSL_DISTRO_NAME").and_then(|d| d.into_string().ok())
    {
        env.wsl_prefix(Some(format!("wsl$/{distro}")));
    }
    let format = match &low.hyperlink_format {
        None => HyperlinkFormat::empty(),
        Some(deferred) => deferred.value.parse().map_err(|err| {
            HiArgsError(format!(
                "error parsing flag {}: invalid hyperlink format: {err}",
                deferred.flag
            ))
        })?,
    };
    Ok(HyperlinkConfig::new(env, format))
}

fn stats_of(low: &LowArgs) -> Option<Stats> {
    match low.mode {
        Mode::Search(SearchMode::Json) => Some(Stats::new()),
        Mode::Search(_) if low.stats => Some(Stats::new()),
        _ => None,
    }
}

fn types_of(low: &LowArgs) -> Result<Types, HiArgsError> {
    let mut builder = TypesBuilder::new();
    builder.add_defaults();
    for change in &low.type_changes {
        match change {
            TypeChange::Clear { name } => {
                builder.clear(name);
            }
            TypeChange::Add { def } => {
                builder.add_def(def)?;
            }
            TypeChange::Select { name } => {
                builder.select(name);
            }
            TypeChange::Negate { name } => {
                builder.negate(name);
            }
        }
    }
    Ok(builder.build()?)
}

fn globs_of(cwd: &Path, low: &LowArgs) -> Result<Override, HiArgsError> {
    if low.globs.is_empty() && low.iglobs.is_empty() {
        return Ok(Override::empty());
    }
    let mut builder = OverrideBuilder::new(cwd);
    if low.glob_case_insensitive {
        builder.case_insensitive(true);
    }
    for glob in &low.globs {
        builder.add(glob)?;
    }
    builder.case_insensitive(true);
    for glob in &low.iglobs {
        builder.add(glob)?;
    }
    Ok(builder.build()?)
}

fn pre_globs_of(cwd: &Path, low: &LowArgs) -> Result<Override, HiArgsError> {
    if low.pre_glob.is_empty() {
        return Ok(Override::empty());
    }
    let mut builder = OverrideBuilder::new(cwd);
    for glob in &low.pre_glob {
        builder.add(glob)?;
    }
    Ok(builder.build()?)
}
