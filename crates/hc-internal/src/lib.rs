//! Shared filesystem, gitignore, and I/O helpers used by `hgrep` and `hfind`.

pub mod encoding;
pub mod gitconfig;
pub mod gnu;
pub mod grep;
pub mod ignore;
pub mod nfc;
pub mod pcre;

#[cfg(unix)]
pub mod unixhome;
pub mod walker;
