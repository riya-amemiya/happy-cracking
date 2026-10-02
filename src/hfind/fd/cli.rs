use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::anyhow;
use clap::{
    Arg, ArgAction, ArgGroup, ArgMatches, Command, CommandFactory, Parser, ValueEnum,
    error::ErrorKind, value_parser,
};

use super::complete::Shell;
use super::exec::CommandSet;
#[cfg(unix)]
use super::filter::OwnerFilter;
use super::filter::SizeFilter;
use super::{fsx, print_error};
use crate::hc_internal::walker::tree_threads;

#[derive(Parser)]
#[command(
    name = "fd",
    version,
    about = "A program to find entries in your filesystem",
    after_long_help = "Bugs can be reported on GitHub: https://github.com/sharkdp/fd/issues",
    max_term_width = 98,
    args_override_self = true,
    group(ArgGroup::new("execs").args(["exec", "exec_batch", "list_details"]).conflicts_with_all([
            "max_results", "quiet", "max_one_result"])),
)]
pub(super) struct Opts {
    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'H',
        help = "Search hidden files and directories",
        long_help = "Include hidden directories and files in the search results (default: hidden \
                     files and directories are skipped). Files and directories are considered to \
                     be hidden if their name starts with a `.` sign (dot). Any files or \
                     directories that are ignored due to the rules described by --no-ignore are \
                     still ignored unless otherwise specified. The flag can be overridden with \
                     --no-hidden."
    )]
    hidden: (),

    #[arg(long, overrides_with = "hidden", hide = true, action = ArgAction::SetTrue, help = "Overrides --hidden")]
    no_hidden: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'I',
        help = "Do not respect .(git|fd)ignore files",
        long_help = "Show search results from files and directories that would otherwise be \
                     ignored by '.gitignore', '.ignore', '.fdignore', or the global ignore file, \
                     The flag can be overridden with --ignore."
    )]
    no_ignore: (),

    #[arg(long, overrides_with = "no_ignore", hide = true, action = ArgAction::SetTrue, help = "Overrides --no-ignore")]
    ignore: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        hide_short_help = true,
        help = "Do not respect .gitignore files",
        long_help = "Show search results from files and directories that would otherwise be \
                     ignored by '.gitignore' files. The flag can be overridden with --ignore-vcs."
    )]
    no_ignore_vcs: (),

    #[arg(long, overrides_with = "no_ignore_vcs", hide = true, action = ArgAction::SetTrue, help = "Overrides --no-ignore-vcs")]
    ignore_vcs: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        overrides_with = "require_git",
        hide_short_help = true,
        help = "Do not require a git repository to respect gitignores. By default, fd will only \
                respect global gitignore rules, .gitignore rules, and local exclude rules if fd \
                detects that you are searching inside a git repository. This flag allows you to \
                relax this restriction such that fd will respect all git related ignore rules \
                regardless of whether you're searching in a git repository or not",
        long_help = "Do not require a git repository to respect gitignores. By default, fd will \
                     only respect global gitignore rules, .gitignore rules, and local exclude \
                     rules if fd detects that you are searching inside a git repository. This \
                     flag allows you to relax this restriction such that fd will respect all git \
                     related ignore rules regardless of whether you're searching in a git \
                     repository or not.\n\nThis flag can be disabled with --require-git."
    )]
    no_require_git: (),

    #[arg(long, overrides_with = "no_require_git", hide = true, action = ArgAction::SetTrue, help = "Overrides --no-require-git")]
    require_git: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        hide_short_help = true,
        help = "Do not respect .(git|fd)ignore files in parent directories",
        long_help = "Show search results from files and directories that would otherwise be \
                     ignored by '.gitignore', '.ignore', or '.fdignore' files in parent \
                     directories."
    )]
    no_ignore_parent: (),

    #[arg(action = ArgAction::SetTrue, long, hide = true, help = "Do not respect the global ignore file")]
    no_global_ignore_file: (),

    #[arg(
        long = "unrestricted",
        short = 'u',
        overrides_with_all(["ignore", "no_hidden"]),
        action(ArgAction::Count),
        hide_short_help = true,
        help = "Unrestricted search, alias for '--no-ignore --hidden'",
        long_help = "Perform an unrestricted search, including ignored and hidden files. This is \
                     an alias for '--no-ignore --hidden'."
    )]
    rg_alias_hidden_ignore: u8,

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 's',
        overrides_with("ignore_case"),
        help = "Case-sensitive search (default: smart case)",
        long_help = "Perform a case-sensitive search. By default, fd uses case-insensitive \
                     searches, unless the pattern contains an uppercase character (smart \
                     case)."
    )]
    case_sensitive: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'i',
        overrides_with("case_sensitive"),
        help = "Case-insensitive search (default: smart case)",
        long_help = "Perform a case-insensitive search. By default, fd uses case-insensitive \
                     searches, unless the pattern contains an uppercase character (smart \
                     case)."
    )]
    ignore_case: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'g',
        conflicts_with("fixed_strings"),
        help = "Glob-based search (default: regular expression)",
        long_help = "Perform a glob-based search instead of a regular expression search."
    )]
    glob: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        overrides_with("glob"),
        hide_short_help = true,
        help = "Regular-expression based search (default)",
        long_help = "Perform a regular-expression based search (default). This can be used to \
                     override --glob."
    )]
    regex: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'F',
        alias = "literal",
        hide_short_help = true,
        help = "Treat pattern as literal string stead of regex",
        long_help = "Treat the pattern as a literal string instead of a regular expression. Note \
                     that this also performs substring comparison. If you want to match on an \
                     exact filename, consider using '--glob'."
    )]
    fixed_strings: (),

    #[arg(
        long = "and",
        value_name = "pattern",
        help = "Additional search patterns that need to be matched",
        long_help = "Add additional required search patterns, all of which must be matched. \
                     Multiple additional patterns can be specified. The patterns are regular \
                     expressions, unless '--glob' or '--fixed-strings' is used.",
        hide_short_help = true,
        allow_hyphen_values = true
    )]
    pub(super) exprs: Option<Vec<String>>,

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'a',
        help = "Show absolute instead of relative paths",
        long_help = "Shows the full path starting from the root as opposed to relative paths. \
                     The flag can be overridden with --relative-path."
    )]
    absolute_path: (),

    #[arg(long, overrides_with = "absolute_path", hide = true, action = ArgAction::SetTrue, help = "Overrides --absolute-path")]
    relative_path: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'l',
        conflicts_with("absolute_path"),
        help = "Use a long listing format with file metadata",
        long_help = "Use a detailed listing format like 'ls -l'. This is basically an alias for \
                     '--exec-batch ls -l' with some additional 'ls' options. This can be used to \
                     see more metadata, to show symlink targets and to achieve a deterministic \
                     sort order."
    )]
    list_details: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'L',
        alias = "dereference",
        help = "Follow symbolic links",
        long_help = "By default, fd does not descend into symlinked directories. Using this \
                     flag, symbolic links are also traversed. \
                     Flag can be overridden with --no-follow."
    )]
    follow: (),

    #[arg(long, overrides_with = "follow", hide = true, action = ArgAction::SetTrue, help = "Overrides --follow")]
    no_follow: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'p',
        help = "Search full abs. path (default: filename only)",
        long_help = "By default, the search pattern is only matched against the filename (or \
                     directory name). Using this flag, the pattern is matched against the full \
                     (absolute) path. Example:\n  fd --glob -p '**/.git/config'"
    )]
    full_path: (),

    #[arg(
        action = ArgAction::SetTrue,
        long = "print0",
        short = '0',
        conflicts_with("list_details"),
        hide_short_help = true,
        help = "Separate search results by the null character",
        long_help = "Separate search results by the null character (instead of newlines). \
                     Useful for piping results to 'xargs'."
    )]
    null_separator: (),

    #[arg(
        long,
        short = 'd',
        value_name = "depth",
        alias("maxdepth"),
        help = "Set maximum search depth (default: none)",
        long_help = "Limit the directory traversal to a given depth. By default, there is no \
                     limit on the search depth."
    )]
    max_depth: Option<usize>,

    #[arg(
        long,
        value_name = "depth",
        hide_short_help = true,
        alias("mindepth"),
        help = "Only show search results starting at the given depth.",
        long_help = "Only show search results starting at the given depth. See also: \
                     '--max-depth' and '--exact-depth'"
    )]
    min_depth: Option<usize>,

    #[arg(
        long,
        value_name = "depth",
        hide_short_help = true,
        conflicts_with_all(["max_depth", "min_depth"]),
        help = "Only show search results at the exact given depth",
        long_help = "Only show search results at the exact given depth. This is an alias for \
                     '--min-depth <depth> --max-depth <depth>'."
    )]
    exact_depth: Option<usize>,

    #[arg(
        long,
        short = 'E',
        value_name = "pattern",
        help = "Exclude entries that match the given glob pattern",
        long_help = "Exclude files/directories that match the given glob pattern. This \
                     overrides any other ignore logic. Multiple exclude patterns can be \
                     specified.\n\nExamples: {n}  --exclude '*.pyc' {n}  --exclude node_modules"
    )]
    pub(super) exclude: Vec<String>,

    #[arg(
        action = ArgAction::SetTrue,
        long,
        hide_short_help = true,
        conflicts_with_all(["size", "exact_depth"]),
        help = "Do not traverse into directories that match the search criteria. If you want to \
                exclude specific directories, use the '--exclude=\u{2026}' option",
        long_help = "Do not traverse into directories that match the search criteria. If you \
                     want to exclude specific directories, use the '--exclude=\u{2026}' option."
    )]
    prune: (),

    #[arg(
        long = "type",
        short = 't',
        value_name = "filetype",
        hide_possible_values = true,
        value_enum,
        help = "Filter by type: file (f), directory (d/dir), symlink (l), \
                executable (x), empty (e), socket (s), pipe (p), \
                char-device (c), block-device (b)",
        long_help = "Filter the search by type: {n}  'f' or 'file':         regular files {n}  \
                     'd' or 'dir' or 'directory':    directories {n}  'l' or 'symlink':      \
                     symbolic links {n}  's' or 'socket':       socket {n}  'p' or 'pipe':         \
                     named pipe (FIFO) {n}  'b' or 'block-device': block device {n}  'c' or \
                     'char-device':  character device {n}{n}  'x' or 'executable':   \
                     executables {n}  'e' or 'empty':        empty files or directories\n\nThis \
                     option can be specified more than once to include multiple file types. \
                     Searching for '--type file --type symlink' will show both regular files as \
                     well as symlinks. Note that the 'executable' and 'empty' filters work \
                     differently: '--type executable' implies '--type file' by default. And \
                     '--type empty' searches for empty files and directories, unless either \
                     '--type file' or '--type directory' is specified in addition.\n\nExamples: \
                     {n}  - Only search for files: {n}      fd --type file \u{2026} {n}      fd \
                     -tf \u{2026} {n}  - Find both files and symlinks {n}      fd --type file \
                     --type symlink \u{2026} {n}      fd -tf -tl \u{2026} {n}  - Find executable \
                     files: {n}      fd --type executable {n}      fd -tx {n}  - Find empty \
                     files: {n}      fd --type empty --type file {n}      fd -te -tf {n}  - \
                     Find empty directories: {n}      fd --type empty --type directory {n}      \
                     fd -te -td"
    )]
    pub(super) filetype: Option<Vec<FileType>>,

    #[arg(
        long = "extension",
        short = 'e',
        value_name = "ext",
        help = "Filter by file extension",
        long_help = "(Additionally) filter search results by their file extension. Multiple \
                     allowable file extensions can be specified.\n\nIf you want to search for \
                     files without extension, you can use the regex '^[^.]+$' as a normal search \
                     pattern."
    )]
    pub(super) extensions: Option<Vec<String>>,

    #[arg(
        long,
        short = 'S',
        value_parser = SizeFilter::from_string,
        allow_hyphen_values = true,
        value_name = "size",
        help = "Limit results based on the size of files",
        long_help = "Limit results based on the size of files using the format \
                     <+-><NUM><UNIT>.\n   '+': file size must be greater than or equal to \
                     this\n   '-': file size must be less than or equal to this\n\nIf neither '+' \
                     nor '-' is specified, file size must be exactly equal to this.\n   'NUM':  \
                     The numeric size (e.g. 500)\n   'UNIT': The units for NUM. They are not \
                     case-sensitive.\nAllowed unit values:\n    'b':  bytes\n    'k':  \
                     kilobytes (base ten, 10^3 = 1000 bytes)\n    'm':  megabytes\n    'g':  \
                     gigabytes\n    't':  terabytes\n    'ki': kibibytes (base two, 2^10 = 1024 \
                     bytes)\n    'mi': mebibytes\n    'gi': gibibytes\n    'ti': tebibytes"
    )]
    pub(super) size: Vec<SizeFilter>,

    #[arg(
        long,
        alias("change-newer-than"),
        alias("newer"),
        alias("changed-after"),
        value_name = "date|dur",
        help = "Filter by file modification time (newer than)",
        long_help = "Filter results based on the file modification time. Files with modification \
                     times greater than the argument are returned. The argument can be provided \
                     as a specific point in time (YYYY-MM-DD HH:MM:SS or @timestamp) or as a \
                     duration (10h, 1d, 35min). If the time is not specified, it defaults to \
                     00:00:00. '--change-newer-than', '--newer', or '--changed-after' can be \
                     used as aliases.\n\nExamples: {n}    --changed-within 2weeks {n}    \
                     --change-newer-than '2018-10-27 10:00:00' {n}    --newer 2018-10-27 {n}    \
                     --changed-after 1day"
    )]
    pub(super) changed_within: Option<String>,

    #[arg(
        long,
        alias("change-older-than"),
        alias("older"),
        value_name = "date|dur",
        help = "Filter by file modification time (older than)",
        long_help = "Filter results based on the file modification time. Files with modification \
                     times less than the argument are returned. The argument can be provided as \
                     a specific point in time (YYYY-MM-DD HH:MM:SS or @timestamp) or as a \
                     duration (10h, 1d, 35min). '--change-older-than' or '--older' can be used \
                     as aliases.\n\nExamples: {n}    --changed-before '2018-10-27 10:00:00' {n}    \
                     --change-older-than 2weeks {n}    --older 2018-10-27"
    )]
    pub(super) changed_before: Option<String>,

    #[cfg(unix)]
    #[arg(
        long,
        short = 'o',
        value_parser = OwnerFilter::from_string,
        value_name = "user:group",
        help = "Filter by owning user and/or group",
        long_help = "Filter files by their user and/or group. Format: \
                     [(user|uid)][:(group|gid)]. Either side is optional. Precede either side \
                     with a '!' to exclude files instead.\n\nExamples: {n}    --owner john {n}    \
                     --owner :students {n}    --owner '!john:students'"
    )]
    pub(super) owner: Option<OwnerFilter>,

    #[arg(
        long,
        value_name = "fmt",
        help = "Print results according to template",
        conflicts_with = "list_details"
    )]
    pub(super) format: Option<String>,

    #[command(flatten)]
    pub(super) exec: Exec,

    #[arg(
        long,
        value_name = "size",
        hide_short_help = true,
        requires("exec_batch"),
        value_parser = value_parser!(usize),
        default_value_t,
        help = "Max number of arguments to run as a batch size with -X",
        long_help = "Maximum number of arguments to pass to the command given with -X. If the \
                     number of results is greater than the given size, the command given with -X \
                     is run again with remaining arguments. A batch size of zero means there is \
                     no limit (default), but note that batching might still happen due to OS \
                     restrictions on the maximum length of command lines."
    )]
    pub(super) batch_size: usize,

    #[arg(
        long,
        value_name = "path",
        hide_short_help = true,
        help = "Add a custom ignore-file in '.gitignore' format",
        long_help = "Add a custom ignore-file in '.gitignore' format. These files have a low \
                     precedence."
    )]
    pub(super) ignore_file: Vec<PathBuf>,

    #[arg(
        long,
        short = 'c',
        value_enum,
        default_value_t = ColorWhen::Auto,
        value_name = "when",
        help = "When to use colors",
        long_help = "Declare when to use color for the pattern match output"
    )]
    pub(super) color: ColorWhen,

    #[arg(
        long,
        alias = "hyper",
        value_name = "when",
        require_equals = true,
        value_enum,
        default_value_t = HyperlinkWhen::Never,
        default_missing_value = "auto",
        num_args = 0..=1,
        help = "Add hyperlinks to output paths",
        long_help = "Add a terminal hyperlink to a file:// url for each path in the \
                     output.\n\nAuto mode  is used if no argument is given to this \
                     option.\n\nThis doesn't do anything for --exec and --exec-batch."
    )]
    pub(super) hyperlink: HyperlinkWhen,

    #[arg(
        long,
        value_name = "name",
        help = "Ignore directories containing the named entry"
    )]
    pub(super) ignore_contain: Vec<String>,

    #[arg(
        long,
        short = 'j',
        value_name = "num",
        hide_short_help = true,
        value_parser = str::parse::<NonZeroUsize>,
        help = "Set number of threads to use for searching & executing (default: number of \
                available CPU cores)"
    )]
    pub(super) threads: Option<NonZeroUsize>,

    #[arg(
        long,
        hide = true,
        value_parser = parse_millis,
        help = "Milliseconds to buffer before streaming search results to console",
        long_help = "Milliseconds to buffer before streaming search results to console\n\nAmount \
                     of time in milliseconds to buffer, before streaming the search results to \
                     the console."
    )]
    pub(super) max_buffer_time: Option<Duration>,

    #[arg(
        long,
        value_name = "count",
        hide_short_help = true,
        overrides_with("max_one_result"),
        help = "Limit the number of search results",
        long_help = "Limit the number of search results to 'count' and quit immediately."
    )]
    max_results: Option<usize>,

    #[arg(
        action = ArgAction::SetTrue,
        short = '1',
        hide_short_help = true,
        overrides_with("max_results"),
        help = "Limit search to a single result",
        long_help = "Limit the search to a single result and quit immediately. This is an alias \
                     for '--max-results=1'."
    )]
    max_one_result: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        short = 'q',
        alias = "has-results",
        hide_short_help = true,
        conflicts_with("max_results"),
        help = "Print nothing, exit code 0 if match found, 1 otherwise",
        long_help = "When the flag is present, the program does not print anything and will \
                     return with an exit code of 0 if there is at least one match. Otherwise, \
                     the exit code will be 1. '--has-results' can be used as an alias."
    )]
    quiet: (),

    #[arg(
        action = ArgAction::SetTrue,
        long,
        hide_short_help = true,
        help = "Show filesystem errors",
        long_help = "Enable the display of filesystem errors for situations such as insufficient \
                     permissions or dead symlinks."
    )]
    show_errors: (),

    #[arg(
        long,
        short = 'C',
        value_name = "path",
        hide_short_help = true,
        help = "Change current working directory",
        long_help = "Change the current working directory of fd to the provided path. This means \
                     that search results will be shown with respect to the given base path. Note \
                     that relative paths which are passed to fd via the positional <path> \
                     argument or the '--search-path' option will also be resolved relative to \
                     this directory."
    )]
    pub(super) base_directory: Option<PathBuf>,

    #[arg(
        default_value = "",
        hide_default_value = true,
        value_name = "pattern",
        help = "the search pattern (a regular expression, unless '--glob' is used; optional)",
        long_help = "the search pattern which is either a regular expression (default) or a glob \
                     pattern (if --glob is used). If no pattern has been specified, every entry \
                     is considered a match. If your pattern starts with a dash (-), make sure to \
                     pass '--' first, or it will be considered as a flag (fd -- '-foo')."
    )]
    pub(super) pattern: String,

    #[arg(
        long,
        value_name = "separator",
        hide_short_help = true,
        help = "Set path separator when printing file paths",
        long_help = "Set the path separator to use when printing file paths. The default is the \
                     OS-specific separator ('/' on Unix, '\\' on Windows)."
    )]
    pub(super) path_separator: Option<String>,

    #[arg(
        action = ArgAction::Append,
        value_name = "path",
        help = "the root directories for the filesystem search (optional)",
        long_help = "The directory where the filesystem search is rooted (optional). If omitted, \
                     search the current working directory."
    )]
    path: Vec<PathBuf>,

    #[arg(
        long,
        conflicts_with("path"),
        value_name = "search-path",
        hide_short_help = true,
        help = "Provides paths to search as an alternative to the positional <path> argument",
        long_help = "Provide paths to search as an alternative to the positional <path> \
                     argument. Changes the usage to `fd [OPTIONS] --search-path <path> \
                     --search-path <path2> [<pattern>]`"
    )]
    search_path: Vec<PathBuf>,

    #[arg(
        long,
        conflicts_with_all(["path", "search_path"]),
        value_name = "when",
        hide_short_help = true,
        require_equals = true,
        help = "By default, relative paths are prefixed with './' when -x/--exec, \
                -X/--exec-batch, or -0/--print0 are given, to reduce the risk of a path starting \
                with '-' being treated as a command line option. Use this flag to change this \
                behavior. If this flag is used without a value, it is equivalent to passing \
                \"always\"",
        long_help = "By default, relative paths are prefixed with './' when -x/--exec, \
                     -X/--exec-batch, or -0/--print0 are given, to reduce the risk of a path \
                     starting with '-' being treated as a command line option. Use this flag to \
                     change this behavior. If this flag is used without a value, it is \
                     equivalent to passing \"always\".",
        num_args = 0..=1,
        default_missing_value = "always"
    )]
    strip_cwd_prefix: Option<StripCwdWhen>,

    #[cfg(any(unix, windows))]
    #[arg(
        action = ArgAction::SetTrue,
        long,
        aliases(["mount", "xdev"]),
        hide_short_help = true,
        help = "By default, fd will traverse the file system tree as far as other options \
                dictate. With this flag, fd ensures that it does not descend into a different \
                file system than the one it started in. Comparable to the -mount or -xdev \
                filters of find(1)",
        long_help = "By default, fd will traverse the file system tree as far as other options \
                     dictate. With this flag, fd ensures that it does not descend into a \
                     different file system than the one it started in. Comparable to the -mount \
                     or -xdev filters of find(1)."
    )]
    one_file_system: (),

    #[arg(skip)]
    pub(super) flags: Flags,
}

#[derive(Clone, Copy)]
pub(super) enum Flag {
    Hidden,
    NoIgnore,
    NoIgnoreVcs,
    NoRequireGit,
    NoIgnoreParent,
    NoGlobalIgnoreFile,
    CaseSensitive,
    IgnoreCase,
    Glob,
    FixedStrings,
    AbsolutePath,
    ListDetails,
    Follow,
    FullPath,
    NullSeparator,
    Prune,
    MaxOneResult,
    Quiet,
    ShowErrors,
    OneFileSystem,
}

const FLAG_IDS: [&str; 20] = [
    "hidden",
    "no_ignore",
    "no_ignore_vcs",
    "no_require_git",
    "no_ignore_parent",
    "no_global_ignore_file",
    "case_sensitive",
    "ignore_case",
    "glob",
    "fixed_strings",
    "absolute_path",
    "list_details",
    "follow",
    "full_path",
    "null_separator",
    "prune",
    "max_one_result",
    "quiet",
    "show_errors",
    "one_file_system",
];

#[derive(Clone, Copy, Default)]
pub(super) struct Flags(u32);

impl Flags {
    pub(super) fn from_matches(matches: &ArgMatches) -> Flags {
        Flags(
            FLAG_IDS
                .iter()
                .enumerate()
                .filter(|(_, id)| {
                    matches
                        .try_get_one::<bool>(id)
                        .ok()
                        .flatten()
                        .is_some_and(|&on| on)
                })
                .fold(0, |acc, (i, _)| acc | (1 << i)),
        )
    }

    pub(super) fn has(self, flag: Flag) -> bool {
        self.0 & (1 << flag as u32) != 0
    }
}

impl Opts {
    pub(super) fn search_paths(&self) -> anyhow::Result<Vec<PathBuf>> {
        let paths = if !self.path.is_empty() {
            &self.path
        } else if !self.search_path.is_empty() {
            &self.search_path
        } else {
            let current_directory = Path::new("./");
            ensure_current_directory_exists(current_directory)?;
            return Ok(vec![self.normalize_path(current_directory)]);
        };
        Ok(paths
            .iter()
            .filter_map(|path| {
                if fsx::is_existing_directory(path) {
                    Some(self.normalize_path(path))
                } else {
                    print_error(&format!(
                        "Search path '{}' is not a directory.",
                        path.to_string_lossy()
                    ));
                    None
                }
            })
            .collect())
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        if self.flags.has(Flag::AbsolutePath) {
            fsx::absolute_path(&path.canonicalize().unwrap()).unwrap()
        } else if path == Path::new(".") {
            PathBuf::from("./")
        } else {
            path.to_path_buf()
        }
    }

    pub(super) fn no_search_paths(&self) -> bool {
        self.path.is_empty() && self.search_path.is_empty()
    }

    pub(super) fn rg_alias_ignore(&self) -> bool {
        self.rg_alias_hidden_ignore > 0
    }

    pub(super) fn max_depth(&self) -> Option<usize> {
        self.max_depth.or(self.exact_depth)
    }

    pub(super) fn min_depth(&self) -> Option<usize> {
        self.min_depth.or(self.exact_depth)
    }

    pub(super) fn threads(&self) -> NonZeroUsize {
        self.threads.unwrap_or_else(default_num_threads)
    }

    pub(super) fn max_results(&self) -> Option<usize> {
        self.max_results
            .filter(|&m| m > 0)
            .or_else(|| self.flags.has(Flag::MaxOneResult).then_some(1))
    }

    pub(super) fn strip_cwd_prefix<P: FnOnce() -> bool>(&self, auto_pred: P) -> bool {
        self.no_search_paths()
            && match self.strip_cwd_prefix.unwrap_or(StripCwdWhen::Auto) {
                StripCwdWhen::Auto => auto_pred(),
                StripCwdWhen::Always => true,
                StripCwdWhen::Never => false,
            }
    }
}

fn default_num_threads() -> NonZeroUsize {
    std::thread::available_parallelism()
        .ok()
        .and_then(|n| NonZeroUsize::new(tree_threads(n.get().min(64))))
        .unwrap_or(NonZeroUsize::MIN)
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub(super) enum FileType {
    #[value(alias = "f")]
    File,
    #[value(alias = "d", alias = "dir")]
    Directory,
    #[value(alias = "l")]
    Symlink,
    #[value(alias = "b")]
    BlockDevice,
    #[value(alias = "c")]
    CharDevice,
    #[value(
        alias = "x",
        help = "A file which is executable by the current effective user"
    )]
    Executable,
    #[value(alias = "e")]
    Empty,
    #[value(alias = "s")]
    Socket,
    #[value(alias = "p")]
    Pipe,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
pub(super) enum ColorWhen {
    #[value(help = "show colors if the output goes to an interactive console (default)")]
    Auto,
    #[value(help = "always use colorized output")]
    Always,
    #[value(help = "do not use colorized output")]
    Never,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
pub(super) enum StripCwdWhen {
    #[value(help = "Use the default behavior")]
    Auto,
    #[value(help = "Always strip the ./ at the beginning of paths")]
    Always,
    #[value(help = "Never strip the ./")]
    Never,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, ValueEnum)]
pub(super) enum HyperlinkWhen {
    #[value(help = "Use hyperlinks only if color is enabled")]
    Auto,
    #[value(help = "Always use hyperlinks when printing file paths")]
    Always,
    #[value(help = "Never use hyperlinks")]
    Never,
}

pub(super) struct Exec {
    pub(super) command: Option<CommandSet>,
}

impl clap::FromArgMatches for Exec {
    fn from_arg_matches(matches: &ArgMatches) -> clap::error::Result<Self> {
        let command = matches
            .get_occurrences::<String>("exec")
            .map(CommandSet::new)
            .or_else(|| {
                matches
                    .get_occurrences::<String>("exec_batch")
                    .map(CommandSet::new_batch)
            })
            .transpose()
            .map_err(|e| clap::Error::raw(ErrorKind::InvalidValue, e))?;
        Ok(Exec { command })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> clap::error::Result<()> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl clap::Args for Exec {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(Arg::new("exec")
            .action(ArgAction::Append)
            .long("exec")
            .short('x')
            .num_args(1..)
                .allow_hyphen_values(true)
                .value_terminator(";")
                .value_name("cmd")
                .conflicts_with("list_details")
                .help("Execute a command for each search result")
                .long_help(
                    "Execute a command for each search result in parallel (use --threads=1 for sequential command execution). \
                     There is no guarantee of the order commands are executed in, and the order should not be depended upon. \
                     All positional arguments following --exec are considered to be arguments to the command - not to fd. \
                     It is therefore recommended to place the '-x'/'--exec' option last.\n\
                     The following placeholders are substituted before the command is executed:\n  \
                       '{}':   path (of the current search result)\n  \
                       '{/}':  basename\n  \
                       '{//}': parent directory\n  \
                       '{.}':  path without file extension\n  \
                       '{/.}': basename without file extension\n  \
                       '{{':   literal '{' (for escaping)\n  \
                       '}}':   literal '}' (for escaping)\n\n\
                     If no placeholder is present, an implicit \"{}\" at the end is assumed.\n\n\
                     Examples:\n\n  \
                       - find all *.zip files and unzip them:\n\n      \
                           fd -e zip -x unzip\n\n  \
                       - find *.h and *.cpp files and run \"clang-format -i ..\" for each of them:\n\n      \
                           fd -e h -e cpp -x clang-format -i\n\n  \
                       - Convert all *.jpg files to *.png files:\n\n      \
                           fd -e jpg -x convert {} {.}.png\
                    ",
                ),
        )
        .arg(
            Arg::new("exec_batch")
                .action(ArgAction::Append)
                .long("exec-batch")
                .short('X')
                .num_args(1..)
                .allow_hyphen_values(true)
                .value_terminator(";")
                .value_name("cmd")
                .conflicts_with_all(["exec", "list_details"])
                .help("Execute a command with all search results at once")
                .long_help(
                    "Execute the given command once, with all search results as arguments.\n\
                     The order of the arguments is non-deterministic, and should not be relied upon.\n\
                     One of the following placeholders is substituted before the command is executed:\n  \
                       '{}':   path (of all search results)\n  \
                       '{/}':  basename\n  \
                       '{//}': parent directory\n  \
                       '{.}':  path without file extension\n  \
                       '{/.}': basename without file extension\n  \
                       '{{':   literal '{' (for escaping)\n  \
                       '}}':   literal '}' (for escaping)\n\n\
                     If no placeholder is present, an implicit \"{}\" at the end is assumed.\n\n\
                     Examples:\n\n  \
                       - Find all test_*.py files and open them in your favorite editor:\n\n      \
                           fd -g 'test_*.py' -X vim\n\n  \
                       - Find all *.rs files and count the lines with \"wc -l ...\":\n\n      \
                           fd -e rs -X wc -l\
                     "
                ),
        )
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

fn parse_millis(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    Ok(Duration::from_millis(arg.parse()?))
}

fn ensure_current_directory_exists(current_directory: &Path) -> anyhow::Result<()> {
    if fsx::is_existing_directory(current_directory) {
        Ok(())
    } else {
        Err(anyhow!(
            "Could not retrieve current directory (has it been deleted?)."
        ))
    }
}

pub(super) fn rename(text: &str, name: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while let Some(pos) = text[i..].find("fd") {
        let at = i + pos;
        let before = at.checked_sub(1).map(|j| bytes[j]);
        let after = bytes.get(at + 2).copied();
        let lone_before =
            before.is_none_or(|b| !(b.is_ascii_alphanumeric() || b"._|/-".contains(&b)));
        let lone_after = after.is_none_or(|b| !(b.is_ascii_alphanumeric() || b"_/)-".contains(&b)));
        if lone_before && lone_after {
            out.push_str(&text[last..at]);
            out.push_str(name);
            last = at + 2;
        }
        i = at + 2;
    }
    out.push_str(&text[last..]);
    out
}

pub(super) fn command(name: &'static str) -> Command {
    let renamed = |s: Option<&clap::builder::StyledStr>| s.map(|s| rename(&s.to_string(), name));
    Opts::command()
        .name(name)
        .bin_name(name)
        .mut_args(|arg| {
            let help = renamed(arg.get_help());
            let long = renamed(arg.get_long_help());
            let arg = match help {
                Some(h) => arg.help(h),
                None => arg,
            };
            match long {
                Some(h) => arg.long_help(h),
                None => arg,
            }
        })
        .arg(
            Arg::new("gen_completions")
                .long("gen-completions")
                .hide(true)
                .exclusive(true)
                .value_name("GEN_COMPLETIONS")
                .num_args(0..=1)
                .action(ArgAction::Set)
                .value_parser(value_parser!(Shell)),
        )
}
