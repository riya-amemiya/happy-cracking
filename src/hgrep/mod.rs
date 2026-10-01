mod app;
mod doc;
mod escape;
mod flags;
mod gitfilter;
mod gnu_search;
mod gnuwalk;
mod haystack;
mod hiargs;
mod hostname;
mod matchers;
mod messages;
mod ordered;
mod out;
mod patterns;
mod process;
mod queue;
mod worker;

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;
use std::sync::OnceLock;

use flags::Personality;

pub const MAX_PATTERN_FILE_BYTES: usize = 16 * 1024 * 1024;

static PERSONALITY: OnceLock<Personality> = OnceLock::new();

fn set_personality(personality: Personality) {
    let _ = PERSONALITY.set(personality);
}

fn personality() -> Personality {
    PERSONALITY.get().copied().unwrap_or(Personality::Grep)
}

fn strip_os_error(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(" (os error ") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + " (os error ".len()..];
        match tail.find(')') {
            Some(end) if tail[..end].bytes().all(|b| b.is_ascii_digit()) => {
                rest = &tail[end + 1..];
            }
            _ => {
                out.push_str(" (os error ");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_io_prefix(text: &str) -> String {
    let marker = "IO error for operation on ";
    let Some(at) = text.find(marker) else {
        return text.to_string();
    };
    let tail = &text[at + marker.len()..];
    match tail.find(": ") {
        Some(end) => format!("{}{}", &text[..at], &tail[end + 2..]),
        None => text.to_string(),
    }
}

pub(crate) fn format_error(text: &str) -> String {
    match personality() {
        Personality::Rg => text.to_string(),
        Personality::Grep => strip_os_error(&strip_io_prefix(text)),
    }
}

pub(crate) fn format_parallel_error(text: &str) -> String {
    format_error(&strip_io_prefix(text))
}

pub fn read_pattern_file_with_limit(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let max_bytes = max_bytes.min(MAX_PATTERN_FILE_BYTES);
    let file = File::open(path)?;
    let mut buf = Vec::new();
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut buf)?;
    if buf.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "pattern file exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
            ),
        ));
    }
    Ok(buf)
}

#[must_use]
pub fn run() -> ExitCode {
    app::run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_errors_drop_rust_decorations() {
        assert_eq!(
            strip_io_prefix("x.txt: IO error for operation on x.txt: No such file or directory"),
            "x.txt: No such file or directory"
        );
        assert_eq!(
            strip_os_error("x: Permission denied (os error 13)"),
            "x: Permission denied"
        );
        assert_eq!(strip_os_error("a (os error x) b"), "a (os error x) b");
    }
}
