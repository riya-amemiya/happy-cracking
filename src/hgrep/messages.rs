use std::io::{self, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static MESSAGES: AtomicBool = AtomicBool::new(true);
static IGNORE_MESSAGES: AtomicBool = AtomicBool::new(true);
static ERRORED: AtomicBool = AtomicBool::new(false);
static PROG: OnceLock<String> = OnceLock::new();

pub(crate) fn set_prog(name: &str) {
    let _ = PROG.set(name.to_string());
}

pub(crate) fn prog() -> &'static str {
    PROG.get().map_or("hgrep", String::as_str)
}

pub(crate) fn set_messages(yes: bool) {
    MESSAGES.store(yes, Ordering::Relaxed);
}

pub(crate) fn set_ignore_messages(yes: bool) {
    IGNORE_MESSAGES.store(yes, Ordering::Relaxed);
}

pub(crate) fn messages() -> bool {
    MESSAGES.load(Ordering::Relaxed)
}

pub(crate) fn errored() -> bool {
    ERRORED.load(Ordering::Relaxed)
}

pub(crate) fn set_errored() {
    ERRORED.store(true, Ordering::Relaxed);
}

pub(crate) fn eprint_locked(text: &str) {
    let stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let line = format!("{}: {text}\n", prog());
    if let Err(err) = stderr.write_all(line.as_bytes()) {
        std::process::exit(if err.kind() == io::ErrorKind::BrokenPipe {
            0
        } else {
            2
        });
    }
    drop(stdout);
}

pub(crate) fn message(text: &str) {
    if messages() {
        eprint_locked(text);
    }
}

pub(crate) fn err_message(text: &str) {
    set_errored();
    message(text);
}

pub(crate) fn ignore_message(text: &str) {
    if messages() && IGNORE_MESSAGES.load(Ordering::Relaxed) {
        eprint_locked(text);
    }
}

#[must_use]
pub(crate) fn strerror(err: &io::Error) -> String {
    let text = err.to_string();
    match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => text[..at].to_string(),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strerror_drops_os_error_suffix() {
        let err = io::Error::from_raw_os_error(2);
        assert!(!strerror(&err).contains("os error"));
        assert!(strerror(&err).starts_with("No such file"));
        assert_eq!(strerror(&io::Error::other("plain")), "plain");
    }
}
