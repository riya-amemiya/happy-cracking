use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

pub use fsys::FileType;
pub use walk::{
    DirEntry, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState, tree_threads,
};

mod default_types;
mod dir;
mod fsys;
pub mod gitignore;
pub mod glob;
pub mod overrides;
pub mod types;
mod walk;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum Error {
    Partial(Vec<Error>),
    WithLineNumber { line: u64, err: Box<Error> },
    WithPath { path: PathBuf, err: Box<Error> },
    WithDepth { depth: usize, err: Box<Error> },
    Loop { ancestor: PathBuf, child: PathBuf },
    Io(io::Error),
    Glob { glob: Option<String>, err: String },
    UnrecognizedFileType(String),
    InvalidDefinition,
}

impl Clone for Error {
    fn clone(&self) -> Error {
        match self {
            Error::Partial(errs) => Error::Partial(errs.clone()),
            Error::WithLineNumber { line, err } => Error::WithLineNumber {
                line: *line,
                err: err.clone(),
            },
            Error::WithPath { path, err } => Error::WithPath {
                path: path.clone(),
                err: err.clone(),
            },
            Error::WithDepth { depth, err } => Error::WithDepth {
                depth: *depth,
                err: err.clone(),
            },
            Error::Loop { ancestor, child } => Error::Loop {
                ancestor: ancestor.clone(),
                child: child.clone(),
            },
            Error::Io(err) => match err.raw_os_error() {
                Some(e) => Error::Io(io::Error::from_raw_os_error(e)),
                None => Error::Io(io::Error::new(err.kind(), err.to_string())),
            },
            Error::Glob { glob, err } => Error::Glob {
                glob: glob.clone(),
                err: err.clone(),
            },
            Error::UnrecognizedFileType(err) => Error::UnrecognizedFileType(err.clone()),
            Error::InvalidDefinition => Error::InvalidDefinition,
        }
    }
}

impl Error {
    pub fn is_io(&self) -> bool {
        match self {
            Error::Partial(errs) => errs.len() == 1 && errs[0].is_io(),
            Error::WithLineNumber { err, .. }
            | Error::WithPath { err, .. }
            | Error::WithDepth { err, .. } => err.is_io(),
            Error::Io(_) => true,
            Error::Loop { .. }
            | Error::Glob { .. }
            | Error::UnrecognizedFileType(_)
            | Error::InvalidDefinition => false,
        }
    }

    pub(crate) fn with_path<P: AsRef<Path>>(self, path: P) -> Error {
        Error::WithPath {
            path: path.as_ref().to_path_buf(),
            err: Box::new(self),
        }
    }

    pub(crate) fn with_depth(self, depth: usize) -> Error {
        Error::WithDepth {
            depth,
            err: Box::new(self),
        }
    }

    pub(crate) fn tagged<P: AsRef<Path>>(self, path: P, lineno: u64) -> Error {
        let errline = Error::WithLineNumber {
            line: lineno,
            err: Box::new(self),
        };
        if path.as_ref().as_os_str().is_empty() {
            return errline;
        }
        errline.with_path(path)
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Partial(errs) => {
                let msgs: Vec<String> = errs.iter().map(ToString::to_string).collect();
                f.write_str(&msgs.join("\n"))
            }
            Error::WithLineNumber { line, err } => write!(f, "line {line}: {err}"),
            Error::WithPath { path, err } => write!(f, "{}: {err}", path.display()),
            Error::WithDepth { err, .. } => err.fmt(f),
            Error::Loop { ancestor, child } => write!(
                f,
                "File system loop found: {} points to an ancestor {}",
                child.display(),
                ancestor.display()
            ),
            Error::Io(err) => err.fmt(f),
            Error::Glob { glob: None, err } => f.write_str(err),
            Error::Glob {
                glob: Some(glob),
                err,
            } => write!(f, "error parsing glob '{glob}': {err}"),
            Error::UnrecognizedFileType(ty) => write!(f, "unrecognized file type: {ty}"),
            Error::InvalidDefinition => {
                f.write_str("invalid definition (format is type:glob, e.g., html:*.html)")
            }
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PartialErrorBuilder(Vec<Error>);

impl PartialErrorBuilder {
    pub(crate) fn push(&mut self, err: Error) {
        self.0.push(err);
    }

    pub(crate) fn push_ignore_io(&mut self, err: Error) {
        if !err.is_io() {
            self.push(err);
        }
    }

    pub(crate) fn maybe_push(&mut self, err: Option<Error>) {
        if let Some(err) = err {
            self.push(err);
        }
    }

    pub(crate) fn maybe_push_ignore_io(&mut self, err: Option<Error>) {
        if let Some(err) = err {
            self.push_ignore_io(err);
        }
    }

    pub(crate) fn into_error_option(mut self) -> Option<Error> {
        match self.0.len() {
            0 => None,
            1 => self.0.pop(),
            _ => Some(Error::Partial(self.0)),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Match<T> {
    None,
    Ignore(T),
    Whitelist(T),
}

impl<T> Match<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, Match::None)
    }

    pub fn is_ignore(&self) -> bool {
        matches!(self, Match::Ignore(_))
    }

    pub fn is_whitelist(&self) -> bool {
        matches!(self, Match::Whitelist(_))
    }

    pub fn invert(self) -> Match<T> {
        match self {
            Match::None => Match::None,
            Match::Ignore(t) => Match::Whitelist(t),
            Match::Whitelist(t) => Match::Ignore(t),
        }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Match<U> {
        match self {
            Match::None => Match::None,
            Match::Ignore(t) => Match::Ignore(f(t)),
            Match::Whitelist(t) => Match::Whitelist(f(t)),
        }
    }

    pub fn or(self, other: Self) -> Self {
        if self.is_none() { other } else { self }
    }
}

#[cfg(unix)]
pub(crate) fn strip_prefix<'a, P: AsRef<Path> + ?Sized>(
    prefix: &P,
    path: &'a Path,
) -> Option<&'a Path> {
    use std::os::unix::ffi::OsStrExt;
    let prefix = prefix.as_ref().as_os_str().as_bytes();
    let path = path.as_os_str().as_bytes();
    path.strip_prefix(prefix)
        .map(|rest| Path::new(OsStr::from_bytes(rest)))
}

#[cfg(not(unix))]
pub(crate) fn strip_prefix<'a, P: AsRef<Path> + ?Sized>(
    prefix: &P,
    path: &'a Path,
) -> Option<&'a Path> {
    path.strip_prefix(prefix).ok()
}

#[cfg(unix)]
pub(crate) fn is_file_name(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    memchr::memchr(b'/', path.as_os_str().as_bytes()).is_none()
}

#[cfg(not(unix))]
pub(crate) fn is_file_name(path: &Path) -> bool {
    path.parent().is_some_and(|p| p.as_os_str().is_empty())
}

#[cfg(unix)]
pub(crate) fn file_name<P: AsRef<Path> + ?Sized>(path: &P) -> Option<&OsStr> {
    use std::os::unix::ffi::OsStrExt;
    let path = path.as_ref().as_os_str().as_bytes();
    if path.is_empty() || path.last() == Some(&b'.') {
        return None;
    }
    let last_slash = memchr::memrchr(b'/', path).map_or(0, |i| i + 1);
    Some(OsStr::from_bytes(&path[last_slash..]))
}

#[cfg(not(unix))]
pub(crate) fn file_name<P: AsRef<Path> + ?Sized>(path: &P) -> Option<&OsStr> {
    path.as_ref().file_name()
}

pub(crate) fn is_hidden(path: &Path) -> bool {
    file_name(path).is_some_and(|name| name.as_encoded_bytes().first() == Some(&b'.'))
}
