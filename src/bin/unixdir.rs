//! POSIX directory listing with `d_type`, used by hfind and hgrep on Unix.
//!
//! Opens the directory once, reads entries via `fdopendir`/`readdir`, and keeps
//! the `DIR*` so callers can `openat` / `fstatat` via `dirfd` without another
//! path walk. `d_type` is filled on Linux, macOS, and BSD; `DT_UNKNOWN` falls
//! back to `fstatat`.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use libc::{O_CLOEXEC, O_DIRECTORY, O_RDONLY};

pub struct RawEnt {
    pub name: OsString,
    pub d_type: u8,
}

/// Open directory whose `DIR*` owns the fd. `closedir` runs on drop.
pub struct DirFd {
    dir: *mut libc::DIR,
}

impl Drop for DirFd {
    fn drop(&mut self) {
        if !self.dir.is_null() {
            unsafe { libc::closedir(self.dir) };
            self.dir = std::ptr::null_mut();
        }
    }
}

impl AsRawFd for DirFd {
    fn as_raw_fd(&self) -> RawFd {
        unsafe { libc::dirfd(self.dir) }
    }
}

#[inline]
pub fn is_dir(d_type: u8) -> Option<bool> {
    match d_type {
        libc::DT_DIR => Some(true),
        libc::DT_UNKNOWN => None,
        _ => Some(false),
    }
}

#[inline]
pub fn is_file(d_type: u8) -> Option<bool> {
    match d_type {
        libc::DT_REG => Some(true),
        libc::DT_UNKNOWN => None,
        _ => Some(false),
    }
}

#[inline]
#[allow(dead_code)]
pub fn is_lnk(d_type: u8) -> bool {
    d_type == libc::DT_LNK
}

/// Open a directory and return a live `DIR*` plus entries. Caller keeps the
/// handle for `open_at` / `dtype_at` on the same listing.
pub fn list(path: &Path) -> io::Result<(DirFd, Vec<RawEnt>)> {
    let cpath = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let fd = unsafe { libc::open(cpath.as_ptr(), O_RDONLY | O_DIRECTORY | O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { libc::fdopendir(fd) };
    if stream.is_null() {
        let err = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(err);
    }
    let dir = DirFd { dir: stream };
    let ents = read_dents(dir.dir)?;
    Ok((dir, ents))
}

#[allow(dead_code)]
pub fn read(path: &Path) -> io::Result<Vec<RawEnt>> {
    list(path).map(|(_, ents)| ents)
}

/// Open `name` relative to `dir` (does not follow symlinks).
#[allow(dead_code)]
pub fn open_at(dir: &impl AsRawFd, name: &OsStr) -> io::Result<File> {
    open_at_fd(dir.as_raw_fd(), name)
}

#[allow(dead_code)]
pub fn open_at_fd(dirfd: i32, name: &OsStr) -> io::Result<File> {
    let cname = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains NUL"))?;
    let flags = O_RDONLY | O_CLOEXEC | libc::O_NOFOLLOW;
    let fd = unsafe { libc::openat(dirfd, cname.as_ptr(), flags) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// `fstatat` when `d_type` is unknown. Returns a concrete `DT_*` value.
#[allow(dead_code)]
pub fn dtype_at(dir: &impl AsRawFd, name: &OsStr) -> Option<u8> {
    let cname = CString::new(name.as_bytes()).ok()?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            cname.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return None;
    }
    let mode = st.st_mode & libc::S_IFMT;
    if mode == libc::S_IFDIR {
        Some(libc::DT_DIR)
    } else if mode == libc::S_IFREG {
        Some(libc::DT_REG)
    } else if mode == libc::S_IFLNK {
        Some(libc::DT_LNK)
    } else {
        Some(libc::DT_FIFO)
    }
}

fn read_dents(dir: *mut libc::DIR) -> io::Result<Vec<RawEnt>> {
    let mut out = Vec::with_capacity(32);
    loop {
        let ent = unsafe { libc::readdir(dir) };
        if ent.is_null() {
            break;
        }
        let name_c = unsafe { CStr::from_ptr((*ent).d_name.as_ptr()) };
        let bytes = name_c.to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        out.push(RawEnt {
            name: OsStr::from_bytes(bytes).to_os_string(),
            d_type: unsafe { (*ent).d_type },
        });
    }
    Ok(out)
}
