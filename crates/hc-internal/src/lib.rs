//! Shared filesystem, gitignore, and I/O helpers used by `hgrep` and `hfind`.

#[cfg(unix)]
pub mod gitconfig;
pub mod ignore;
pub mod nfc;
pub mod outbuf;

#[cfg(unix)]
pub mod unixdir;
#[cfg(unix)]
pub mod unixhome;
