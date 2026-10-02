use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::thread::JoinHandle;

#[derive(Debug)]
pub(crate) enum CommandError {
    Io(io::Error),
    Stderr(Vec<u8>),
}

impl CommandError {
    fn is_empty(&self) -> bool {
        matches!(self, CommandError::Stderr(bytes) if bytes.is_empty())
    }
}

impl std::error::Error for CommandError {}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Io(err) => err.fmt(f),
            CommandError::Stderr(bytes) => {
                let msg = String::from_utf8_lossy(bytes);
                if msg.trim().is_empty() {
                    write!(f, "<stderr is empty>")
                } else {
                    let div = "-".repeat(79);
                    write!(f, "\n{div}\n{}\n{div}", msg.trim())
                }
            }
        }
    }
}

impl From<io::Error> for CommandError {
    fn from(err: io::Error) -> CommandError {
        CommandError::Io(err)
    }
}

impl From<CommandError> for io::Error {
    fn from(err: CommandError) -> io::Error {
        match err {
            CommandError::Io(err) => err,
            stderr @ CommandError::Stderr(_) => io::Error::other(stderr),
        }
    }
}

fn read_stderr(stderr: &mut ChildStderr) -> CommandError {
    let mut bytes = Vec::new();
    match stderr.read_to_end(&mut bytes) {
        Ok(_) => CommandError::Stderr(bytes),
        Err(err) => CommandError::Io(err),
    }
}

#[derive(Debug)]
pub(crate) struct CommandReader {
    child: Child,
    stderr: Option<JoinHandle<CommandError>>,
    eof: bool,
}

impl CommandReader {
    pub(crate) fn new(command: &mut Command) -> Result<CommandReader, CommandError> {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stderr = child.stderr.take().expect("stderr was piped");
        let handle = std::thread::spawn(move || read_stderr(&mut stderr));
        Ok(CommandReader {
            child,
            stderr: Some(handle),
            eof: false,
        })
    }

    pub(crate) fn close(&mut self) -> io::Result<()> {
        let Some(stdout) = self.child.stdout.take() else {
            return Ok(());
        };
        drop(stdout);
        if self.child.wait()?.success() {
            return Ok(());
        }
        let err = self
            .stderr
            .take()
            .map_or(CommandError::Stderr(Vec::new()), |handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| CommandError::Stderr(Vec::new()))
            });
        if !self.eof && err.is_empty() {
            return Ok(());
        }
        Err(io::Error::from(err))
    }
}

impl Drop for CommandReader {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl Read for CommandReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(stdout) = self.child.stdout.as_mut() else {
            return Ok(0);
        };
        let nread = stdout.read(buf)?;
        if nread == 0 {
            self.eof = true;
            self.close().map(|()| 0)
        } else {
            Ok(nread)
        }
    }
}

fn is_exe(path: &Path) -> bool {
    path.metadata().is_ok_and(|md| md.is_file())
}

pub(crate) fn resolve_binary(prog: &Path) -> Result<PathBuf, CommandError> {
    if !cfg!(windows) || prog.is_absolute() {
        return Ok(prog.to_path_buf());
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return Err(CommandError::Io(io::Error::other(
            "could not resolve binary because PATH is not set",
        )));
    };
    for dir in std::env::split_paths(&paths) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(prog);
        if is_exe(&candidate) {
            return Ok(candidate);
        }
        if candidate.extension().is_none() {
            for ext in ["com", "exe"] {
                let with_ext = candidate.with_extension(ext);
                if is_exe(&with_ext) {
                    return Ok(with_ext);
                }
            }
        }
    }
    Err(CommandError::Io(io::Error::other(format!(
        "{}: could not find executable in PATH",
        prog.display()
    ))))
}

#[derive(Clone, Debug)]
struct DecompressionCommand {
    suffix: &'static str,
    bin: PathBuf,
    args: Vec<OsString>,
}

#[derive(Clone, Debug)]
pub(crate) struct DecompressionMatcher {
    commands: Vec<DecompressionCommand>,
}

const DEFAULT_COMMANDS: &[(&str, &[&str])] = &[
    (".gz", &["gzip", "-d", "-c"]),
    (".tgz", &["gzip", "-d", "-c"]),
    (".bz2", &["bzip2", "-d", "-c"]),
    (".tbz2", &["bzip2", "-d", "-c"]),
    (".xz", &["xz", "-d", "-c"]),
    (".txz", &["xz", "-d", "-c"]),
    (".lz4", &["lz4", "-d", "-c"]),
    (".lzma", &["xz", "--format=lzma", "-d", "-c"]),
    (".br", &["brotli", "-d", "-c"]),
    (".zst", &["zstd", "-q", "-d", "-c"]),
    (".zstd", &["zstd", "-q", "-d", "-c"]),
    (".Z", &["uncompress", "-c"]),
];

impl DecompressionMatcher {
    pub(crate) fn new() -> DecompressionMatcher {
        let commands = DEFAULT_COMMANDS
            .iter()
            .filter_map(|&(suffix, args)| {
                let bin = resolve_binary(Path::new(args[0])).ok()?;
                Some(DecompressionCommand {
                    suffix,
                    bin,
                    args: args[1..].iter().map(OsString::from).collect(),
                })
            })
            .collect();
        DecompressionMatcher { commands }
    }

    fn find(&self, path: &Path) -> Option<&DecompressionCommand> {
        let bytes = path.as_os_str().as_encoded_bytes();
        self.commands
            .iter()
            .rev()
            .find(|cmd| bytes.ends_with(cmd.suffix.as_bytes()))
    }

    pub(crate) fn has_command(&self, path: &Path) -> bool {
        self.find(path).is_some()
    }

    pub(crate) fn command(&self, path: &Path) -> Option<Command> {
        self.find(path).map(|cmd| {
            let mut command = Command::new(&cmd.bin);
            command.args(&cmd.args);
            command
        })
    }
}

#[derive(Debug)]
pub(crate) enum DecompressionReader {
    Command(CommandReader),
    Passthru(File),
}

impl DecompressionReader {
    pub(crate) fn new(
        matcher: &DecompressionMatcher,
        path: &Path,
    ) -> io::Result<DecompressionReader> {
        let Some(mut cmd) = matcher.command(path) else {
            return File::open(path).map(DecompressionReader::Passthru);
        };
        cmd.arg(path);
        match CommandReader::new(&mut cmd) {
            Ok(reader) => Ok(DecompressionReader::Command(reader)),
            Err(_) => File::open(path).map(DecompressionReader::Passthru),
        }
    }

    pub(crate) fn close(&mut self) -> io::Result<()> {
        match self {
            DecompressionReader::Command(reader) => reader.close(),
            DecompressionReader::Passthru(_) => Ok(()),
        }
    }
}

impl Read for DecompressionReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            DecompressionReader::Command(reader) => reader.read(buf),
            DecompressionReader::Passthru(file) => file.read(buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompression_suffixes_match_known_extensions() {
        let matcher = DecompressionMatcher::new();
        assert!(matcher.has_command(Path::new("a/b.gz")));
        assert!(matcher.has_command(Path::new("x.tar.zst")));
        assert!(matcher.has_command(Path::new("x.Z")));
        assert!(!matcher.has_command(Path::new("x.z")));
        assert!(!matcher.has_command(Path::new("x.txt")));
    }

    #[test]
    fn command_error_display_wraps_stderr() {
        let err = CommandError::Stderr(b"boom\n".to_vec());
        let div = "-".repeat(79);
        assert_eq!(err.to_string(), format!("\n{div}\nboom\n{div}"));
        assert_eq!(
            CommandError::Stderr(b"  ".to_vec()).to_string(),
            "<stderr is empty>"
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_reader_reports_failing_exit_with_stderr() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf out; printf err >&2; exit 3"]);
        let mut reader = CommandReader::new(&mut cmd).unwrap();
        let mut out = String::new();
        let err = reader.read_to_string(&mut out).unwrap_err();
        assert_eq!(out, "out");
        assert!(err.to_string().contains("err"));
    }
}
