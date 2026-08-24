//! POSIX directory listing with `d_type`, used by hfind and hgrep on Unix.
//!
//! Opens the directory once, reads entries via `getdents`, and keeps the
//! directory fd so callers can `openat` / `fstatat` without another path walk.
//! `d_type` is filled on Linux, macOS, and BSD; `DT_UNKNOWN` falls back to
//! `fstatat`. Implemented with rustix so this crate does not need `unsafe`.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, open, openat, statat};

/// Linux/BSD `dirent.d_type` values, used as a compact tag for callers.
const DT_UNKNOWN: u8 = 0;
const DT_FIFO: u8 = 1;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;
const DT_LNK: u8 = 10;

pub struct RawEnt {
    pub name: OsString,
    pub d_type: u8,
}

/// Open directory whose fd stays alive for `open_at` / `dtype_at`.
pub struct DirFd {
    dir: Dir,
}

#[inline]
pub fn is_dir(d_type: u8) -> Option<bool> {
    match d_type {
        DT_DIR => Some(true),
        DT_UNKNOWN => None,
        _ => Some(false),
    }
}

#[inline]
pub fn is_file(d_type: u8) -> Option<bool> {
    match d_type {
        DT_REG => Some(true),
        DT_UNKNOWN => None,
        _ => Some(false),
    }
}

#[inline]
pub fn is_lnk(d_type: u8) -> bool {
    d_type == DT_LNK
}

fn dtype_from(ft: FileType) -> u8 {
    if ft.is_dir() {
        DT_DIR
    } else if ft.is_file() {
        DT_REG
    } else if ft.is_symlink() {
        DT_LNK
    } else if matches!(ft, FileType::Unknown) {
        DT_UNKNOWN
    } else {
        DT_FIFO
    }
}

/// Open a directory and return a live dir fd plus entries. Caller keeps the
/// handle for `open_at` / `dtype_at` on the same listing.
pub fn list(path: &Path) -> io::Result<(DirFd, Vec<RawEnt>)> {
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut dir = Dir::new(fd)?;
    let ents = read_dents(&mut dir)?;
    Ok((DirFd { dir }, ents))
}

/// Open `name` relative to `dir` (does not follow symlinks).
pub fn open_at(dir: &DirFd, name: &OsStr) -> io::Result<File> {
    let fd = openat(
        dir.dir.fd()?,
        name,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    Ok(File::from(fd))
}

/// `fstatat` when `d_type` is unknown. Returns a concrete `DT_*` value.
pub fn dtype_at(dir: &DirFd, name: &OsStr) -> Option<u8> {
    let st = statat(dir.dir.fd().ok()?, name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
    Some(dtype_from(FileType::from_raw_mode(st.st_mode)))
}

fn read_dents(dir: &mut Dir) -> io::Result<Vec<RawEnt>> {
    let mut out = Vec::with_capacity(32);
    while let Some(entry) = dir.read() {
        let entry = entry?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        out.push(RawEnt {
            name: OsStr::from_bytes(bytes).to_os_string(),
            d_type: dtype_from(entry.file_type()),
        });
    }
    Ok(out)
}
