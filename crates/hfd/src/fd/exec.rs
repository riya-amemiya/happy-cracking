use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf, Prefix};
use std::process::{self, Stdio};

use anyhow::{Result, bail};

use super::{Exit, merge_exits, print_error};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Token {
    Placeholder,
    Basename,
    Parent,
    NoExt,
    BasenameNoExt,
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum FormatTemplate {
    Tokens(Vec<Token>),
    Text(String),
}

const PLACEHOLDERS: [&str; 7] = ["{{", "}}", "{}", "{/}", "{//}", "{.}", "{/.}"];

fn first_placeholder(text: &str) -> Option<(usize, usize, usize)> {
    let bytes = text.as_bytes();
    (1..=bytes.len()).find_map(|end| {
        PLACEHOLDERS.iter().enumerate().find_map(|(id, p)| {
            (end >= p.len() && &bytes[end - p.len()..end] == p.as_bytes())
                .then(|| (end - p.len(), end, id))
        })
    })
}

fn token_from_id(id: usize) -> Token {
    match id {
        2 => Token::Placeholder,
        3 => Token::Basename,
        4 => Token::Parent,
        5 => Token::NoExt,
        _ => Token::BasenameNoExt,
    }
}

impl FormatTemplate {
    pub(super) fn has_tokens(&self) -> bool {
        matches!(self, FormatTemplate::Tokens(_))
    }

    pub(super) fn parse(fmt: &str) -> Self {
        let mut tokens = Vec::new();
        let mut remaining = fmt;
        let mut buf = String::new();
        while let Some((start, end, id)) = first_placeholder(remaining) {
            if id < 2 {
                buf += &remaining[..=start];
                remaining = &remaining[end..];
            } else if !remaining[end..].starts_with('}') {
                buf += &remaining[..start];
                if !buf.is_empty() {
                    tokens.push(Token::Text(std::mem::take(&mut buf)));
                }
                tokens.push(token_from_id(id));
                remaining = &remaining[end..];
            } else {
                buf += &remaining[..end];
                remaining = &remaining[end + 1..];
            }
        }
        buf += remaining;
        if tokens.is_empty() {
            return FormatTemplate::Text(buf);
        }
        if !buf.is_empty() {
            tokens.push(Token::Text(buf));
        }
        FormatTemplate::Tokens(tokens)
    }

    pub(super) fn generate(
        &self,
        path: impl AsRef<Path>,
        path_separator: Option<&str>,
    ) -> OsString {
        let path = path.as_ref();
        match self {
            Self::Tokens(tokens) => {
                let mut s = OsString::new();
                for token in tokens {
                    match token {
                        Token::Basename => {
                            s.push(replace_separator(basename(path), path_separator));
                        }
                        Token::BasenameNoExt => s.push(replace_separator(
                            &remove_extension(Path::new(basename(path))),
                            path_separator,
                        )),
                        Token::NoExt => {
                            s.push(replace_separator(&remove_extension(path), path_separator));
                        }
                        Token::Parent => s.push(replace_separator(&dirname(path), path_separator)),
                        Token::Placeholder => {
                            s.push(replace_separator(path.as_os_str(), path_separator));
                        }
                        Token::Text(string) => s.push(string),
                    }
                }
                s
            }
            Self::Text(text) => OsString::from(text),
        }
    }
}

fn replace_separator<'a>(path: &'a OsStr, path_separator: Option<&str>) -> Cow<'a, OsStr> {
    let Some(sep) = path_separator else {
        return Cow::Borrowed(path);
    };
    let mut out = OsString::with_capacity(path.len());
    let mut components = Path::new(path).components().peekable();
    while let Some(comp) = components.next() {
        match comp {
            Component::Prefix(prefix) => {
                if let Prefix::UNC(server, share) = prefix.kind() {
                    out.push(sep);
                    out.push(sep);
                    out.push(server);
                    out.push(sep);
                    out.push(share);
                } else {
                    out.push(comp.as_os_str());
                }
            }
            Component::RootDir => out.push(sep),
            _ => {
                out.push(comp.as_os_str());
                if components.peek().is_some() {
                    out.push(sep);
                }
            }
        }
    }
    Cow::Owned(out)
}

pub(super) fn basename(path: &Path) -> &OsStr {
    path.file_name().unwrap_or(path.as_os_str())
}

pub(super) fn remove_extension(path: &Path) -> OsString {
    let stem = path.file_stem().unwrap_or(path.as_os_str());
    let joined = PathBuf::from(dirname(path)).join(stem);
    joined
        .strip_prefix(".")
        .unwrap_or(&joined)
        .as_os_str()
        .to_owned()
}

pub(super) fn dirname(path: &Path) -> OsString {
    path.parent().map_or_else(
        || path.as_os_str().to_owned(),
        |p| {
            if p.as_os_str().is_empty() {
                OsString::from(".")
            } else {
                p.as_os_str().to_owned()
            }
        },
    )
}

const REASONABLE_DEFAULT_LENGTH: i64 = 8 * 1024;
const POINTER_SIZE: i64 = 8;
#[cfg(unix)]
const UPPER_BOUND: i64 = 16 * 1024 * 1024;

#[cfg(unix)]
mod sys {
    use std::ffi::{c_int, c_long};

    unsafe extern "C" {
        fn sysconf(name: c_int) -> c_long;
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    const NAMES: Option<(c_int, c_int)> = Some((0, 30));
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const NAMES: Option<(c_int, c_int)> = Some((1, 29));
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    const NAMES: Option<(c_int, c_int)> = Some((1, 47));
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    const NAMES: Option<(c_int, c_int)> = Some((1, 28));
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    const NAMES: Option<(c_int, c_int)> = None;

    fn query(name: c_int) -> Option<i64> {
        let v = unsafe { sysconf(name) };
        (v != -1).then_some(v as i64)
    }

    pub(super) fn arg_max() -> Option<i64> {
        query(NAMES?.0)
    }

    pub(super) fn page_size() -> i64 {
        static PAGE: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
        *PAGE.get_or_init(|| {
            NAMES
                .and_then(|(_, page)| query(page))
                .filter(|&s| s >= 4096)
                .unwrap_or(4096)
        })
    }
}

#[cfg(unix)]
fn max_single_argument_length() -> i64 {
    32 * sys::page_size() - 1
}

#[cfg(not(unix))]
fn max_single_argument_length() -> i64 {
    REASONABLE_DEFAULT_LENGTH
}

fn arg_size(arg: &OsStr) -> i64 {
    POINTER_SIZE + arg.len() as i64 + 1
}

#[cfg(unix)]
fn available_argument_length(program: &OsStr) -> Option<i64> {
    let raw = sys::arg_max()?;
    let mut arg_max = if raw < 0 { UPPER_BOUND } else { raw };
    if cfg!(all(target_os = "illumos", target_pointer_width = "64")) {
        arg_max /= 2;
    }
    arg_max -= std::env::vars_os()
        .map(|(k, v)| POINTER_SIZE + k.len() as i64 + 1 + v.len() as i64 + 1)
        .sum::<i64>();
    arg_max -= POINTER_SIZE;
    arg_max -= arg_size(program);
    arg_max -= POINTER_SIZE;
    arg_max -= sys::page_size();
    arg_max -= 2048;
    Some(arg_max.clamp(0, UPPER_BOUND))
}

#[cfg(not(unix))]
fn available_argument_length(program: &OsStr) -> Option<i64> {
    Some(REASONABLE_DEFAULT_LENGTH - arg_size(program) - 1)
}

#[cfg(unix)]
fn e2big() -> io::Error {
    io::Error::from_raw_os_error(7)
}

#[cfg(not(unix))]
fn e2big() -> io::Error {
    io::ErrorKind::Other.into()
}

pub(super) struct ArgCommand {
    inner: process::Command,
    remaining: i64,
}

impl ArgCommand {
    fn new(program: &OsStr) -> Self {
        Self {
            inner: process::Command::new(program),
            remaining: available_argument_length(program).unwrap_or(REASONABLE_DEFAULT_LENGTH),
        }
    }

    fn check_size<I, S>(&self, args: I) -> io::Result<i64>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let single = max_single_argument_length();
        let mut size = 0;
        for arg in args {
            let arg = arg.as_ref();
            if arg.len() as i64 > single {
                return Err(e2big());
            }
            size += arg_size(arg);
        }
        if size > self.remaining {
            return Err(e2big());
        }
        Ok(size)
    }

    fn args_would_fit<I, S>(&self, args: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.check_size(args).is_ok()
    }

    fn try_args<I, S>(&mut self, args: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S> + Copy,
        S: AsRef<OsStr>,
    {
        let size = self.check_size(args)?;
        self.inner.args(args);
        self.remaining -= size;
        Ok(())
    }

    fn try_arg(&mut self, arg: &OsStr) -> io::Result<()> {
        self.try_args([arg])
    }

    fn program(&self) -> Cow<'_, str> {
        self.inner.get_program().to_string_lossy()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    OneByOne,
    Batch,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct CommandSet {
    mode: ExecutionMode,
    commands: Vec<CommandTemplate>,
}

impl CommandSet {
    pub(super) fn new<I, T, S>(input: I) -> Result<CommandSet>
    where
        I: IntoIterator<Item = T>,
        T: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Ok(CommandSet {
            mode: ExecutionMode::OneByOne,
            commands: input
                .into_iter()
                .map(CommandTemplate::new)
                .collect::<Result<_>>()?,
        })
    }

    pub(super) fn new_batch<I, T, S>(input: I) -> Result<CommandSet>
    where
        I: IntoIterator<Item = T>,
        T: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Ok(CommandSet {
            mode: ExecutionMode::Batch,
            commands: input
                .into_iter()
                .map(|args| {
                    let cmd = CommandTemplate::new(args)?;
                    if cmd.number_of_tokens() > 1 {
                        bail!("Only one placeholder allowed for batch commands");
                    }
                    if cmd.args[0].has_tokens() {
                        bail!("First argument of exec-batch is expected to be a fixed executable");
                    }
                    Ok(cmd)
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    pub(super) fn in_batch_mode(&self) -> bool {
        self.mode == ExecutionMode::Batch
    }

    pub(super) fn execute(
        &self,
        input: &Path,
        path_separator: Option<&str>,
        null_separator: bool,
        buffer_output: bool,
    ) -> Exit {
        let mut output = OutputBuffer {
            null_separator,
            outputs: Vec::new(),
        };
        for template in &self.commands {
            let mut cmd = match template.generate(input, path_separator) {
                Ok(cmd) => cmd,
                Err(e) => return handle_cmd_error(None, &e),
            };
            let result = if buffer_output {
                cmd.inner.output()
            } else {
                cmd.inner.spawn().and_then(process::Child::wait_with_output)
            };
            match result {
                Ok(out) => {
                    if buffer_output {
                        output.outputs.push((out.stdout, out.stderr));
                    }
                    if out.status.code() != Some(0) {
                        output.write();
                        return Exit::GeneralError;
                    }
                }
                Err(why) => {
                    output.write();
                    return handle_cmd_error(Some(&cmd), &why);
                }
            }
        }
        output.write();
        Exit::Success
    }

    pub(super) fn execute_batch<I>(
        &self,
        paths: I,
        limit: usize,
        path_separator: Option<&str>,
    ) -> Exit
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let built: io::Result<Vec<CommandBuilder>> = self
            .commands
            .iter()
            .map(|c| CommandBuilder::new(c, limit))
            .collect();
        let mut builders = match built {
            Ok(b) => b,
            Err(e) => return handle_cmd_error(None, &e),
        };
        for path in paths {
            for builder in &mut builders {
                if let Err(e) = builder.push(&path, path_separator) {
                    return handle_cmd_error(Some(&builder.cmd), &e);
                }
            }
        }
        for builder in &mut builders {
            if let Err(e) = builder.finish() {
                return handle_cmd_error(Some(&builder.cmd), &e);
            }
        }
        merge_exits(builders.iter().map(|b| b.exit))
    }
}

struct OutputBuffer {
    null_separator: bool,
    outputs: Vec<(Vec<u8>, Vec<u8>)>,
}

impl OutputBuffer {
    fn write(self) {
        if self.outputs.is_empty() && !self.null_separator {
            return;
        }
        let mut stdout = io::stdout().lock();
        let mut stderr = io::stderr().lock();
        for (out, err) in &self.outputs {
            let _ = stdout.write_all(out);
            let _ = stderr.write_all(err);
        }
        if self.null_separator {
            let _ = stdout.write_all(b"\0");
        }
    }
}

fn handle_cmd_error(cmd: Option<&ArgCommand>, err: &io::Error) -> Exit {
    match cmd {
        Some(cmd) if err.kind() == io::ErrorKind::NotFound => {
            print_error(&format!("Command not found: {}", cmd.program()));
        }
        _ => print_error(&format!("Problem while executing command: {err}")),
    }
    Exit::GeneralError
}

struct CommandBuilder {
    pre_args: Vec<OsString>,
    path_arg: FormatTemplate,
    post_args: Vec<OsString>,
    cmd: ArgCommand,
    count: usize,
    limit: usize,
    exit: Exit,
}

impl CommandBuilder {
    fn new(template: &CommandTemplate, limit: usize) -> io::Result<Self> {
        let mut pre_args = vec![];
        let mut path_arg = None;
        let mut post_args = vec![];
        for arg in &template.args {
            if arg.has_tokens() {
                path_arg = Some(arg.clone());
            } else if path_arg.is_none() {
                pre_args.push(arg.generate("", None));
            } else {
                post_args.push(arg.generate("", None));
            }
        }
        let cmd = Self::new_command(&pre_args)?;
        Ok(Self {
            pre_args,
            path_arg: path_arg.unwrap(),
            post_args,
            cmd,
            count: 0,
            limit,
            exit: Exit::Success,
        })
    }

    fn new_command(pre_args: &[OsString]) -> io::Result<ArgCommand> {
        let mut cmd = ArgCommand::new(&pre_args[0]);
        cmd.inner
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        cmd.try_args(&pre_args[1..])?;
        Ok(cmd)
    }

    fn push(&mut self, path: &Path, separator: Option<&str>) -> io::Result<()> {
        if self.limit > 0 && self.count >= self.limit {
            self.finish()?;
        }
        let arg = self.path_arg.generate(path, separator);
        if !self
            .cmd
            .args_would_fit(std::iter::once(&arg).chain(&self.post_args))
        {
            self.finish()?;
        }
        self.cmd.try_arg(&arg)?;
        self.count += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.count > 0 {
            self.cmd.try_args(&self.post_args)?;
            if !self.cmd.inner.status()?.success() {
                self.exit = Exit::GeneralError;
            }
            self.cmd = Self::new_command(&self.pre_args)?;
            self.count = 0;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CommandTemplate {
    args: Vec<FormatTemplate>,
}

impl CommandTemplate {
    fn new<I, S>(input: I) -> Result<CommandTemplate>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args: Vec<FormatTemplate> = input
            .into_iter()
            .map(|a| FormatTemplate::parse(a.as_ref()))
            .collect();
        if args.is_empty() {
            bail!("No executable provided for --exec or --exec-batch");
        }
        if !args.iter().any(FormatTemplate::has_tokens) {
            args.push(FormatTemplate::Tokens(vec![Token::Placeholder]));
        }
        Ok(CommandTemplate { args })
    }

    fn number_of_tokens(&self) -> usize {
        self.args.iter().filter(|arg| arg.has_tokens()).count()
    }

    fn generate(&self, input: &Path, path_separator: Option<&str>) -> io::Result<ArgCommand> {
        let mut cmd = ArgCommand::new(&self.args[0].generate(input, path_separator));
        for arg in &self.args[1..] {
            cmd.try_arg(&arg.generate(input, path_separator))?;
        }
        Ok(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_str(template: &CommandTemplate, input: &str) -> Vec<String> {
        template
            .args
            .iter()
            .map(|arg| arg.generate(input, None).into_string().unwrap())
            .collect()
    }

    #[test]
    fn templates_match_fd_tokenizer() {
        assert_eq!(
            CommandSet::new(vec![vec!["echo", "${SHELL}:"]]).unwrap(),
            CommandSet {
                commands: vec![CommandTemplate {
                    args: vec![
                        FormatTemplate::Text("echo".into()),
                        FormatTemplate::Text("${SHELL}:".into()),
                        FormatTemplate::Tokens(vec![Token::Placeholder]),
                    ]
                }],
                mode: ExecutionMode::OneByOne,
            }
        );
        for (text, token) in [
            ("{.}", Token::NoExt),
            ("{/}", Token::Basename),
            ("{//}", Token::Parent),
            ("{/.}", Token::BasenameNoExt),
        ] {
            assert_eq!(
                CommandSet::new(vec![vec!["echo", text]]).unwrap().commands[0].args[1],
                FormatTemplate::Tokens(vec![token])
            );
        }
        let t = CommandTemplate::new(vec!["{{}}", "{{", "{.}}"]).unwrap();
        assert_eq!(gen_str(&t, "foo"), vec!["{}", "{", "{.}", "foo"]);
        let t = CommandTemplate::new(vec!["{{{},end}"]).unwrap();
        assert_eq!(gen_str(&t, "foo"), vec!["{foo,end}"]);
        assert_eq!(
            CommandSet::new(vec![vec!["cp", "{}", "{/.}.ext"]])
                .unwrap()
                .commands[0]
                .args[2],
            FormatTemplate::Tokens(vec![Token::BasenameNoExt, Token::Text(".ext".into())])
        );
        assert!(CommandSet::new_batch(vec![vec!["echo", "{.}"]]).is_ok());
        assert!(CommandSet::new_batch(vec![vec!["echo", "{.}", "{}"]]).is_err());
        assert!(CommandSet::new_batch(vec![vec!["{}"]]).is_err());
        assert!(CommandTemplate::new::<Vec<_>, &'static str>(vec![]).is_err());
        assert!(CommandSet::new(vec![vec!["echo"], vec![]]).is_err());
    }

    #[test]
    fn format_generation_and_separators() {
        let templ = FormatTemplate::parse(
            "{{path={} basename={/} parent={//} noExt={.} basenameNoExt={/.} }}",
        );
        assert_eq!(
            templ.generate("a/folder/file.txt", Some("/")),
            OsString::from(
                "{path=a/folder/file.txt basename=file.txt parent=a/folder noExt=a/folder/file basenameNoExt=file }"
            )
        );
        assert_eq!(
            FormatTemplate::parse("only {{ and }}"),
            FormatTemplate::Text("only { and }".into())
        );
        let arg = FormatTemplate::Tokens(vec![Token::Placeholder]);
        assert_eq!(
            arg.generate("foo/bar", Some("#")),
            OsString::from("foo#bar")
        );
        assert_eq!(
            arg.generate("/foo/bar/baz", Some("#")),
            OsString::from("#foo#bar#baz")
        );
        assert_eq!(
            remove_extension(Path::new("dir/foo.txt")),
            OsString::from("dir/foo")
        );
        assert_eq!(remove_extension(Path::new(".foo")), OsString::from(".foo"));
        assert_eq!(remove_extension(Path::new("")), OsString::from(""));
        assert_eq!(dirname(Path::new("foo.txt")), OsString::from("."));
        assert_eq!(dirname(Path::new("/")), OsString::from("/"));
        assert_eq!(basename(Path::new("")), OsStr::new(""));
    }

    #[test]
    fn argument_limits() {
        let cmd = ArgCommand::new(OsStr::new("echo"));
        assert!(cmd.remaining > 0);
        assert!(cmd.args_would_fit(["a"]));
        let huge =
            OsString::from("x".repeat(usize::try_from(max_single_argument_length()).unwrap() + 1));
        assert!(!cmd.args_would_fit([&huge]));
    }
}
