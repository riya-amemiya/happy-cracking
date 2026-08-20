//! Linux getdents64 directory listing with d_type, used by hfind and hgrep.

use std::cell::RefCell;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

use libc::{O_CLOEXEC, O_DIRECTORY, O_RDONLY};

const GETDENTS_BUF: usize = 64 * 1024;

thread_local! {
    static DENT_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[repr(C)]
struct LinuxDirent64 {
    d_ino: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
}

pub struct RawEnt {
    pub name: OsString,
    pub d_type: u8,
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

/// Open a directory and return its fd plus entries. Caller keeps the fd for
/// `open_at` / `dtype_at` on the same listing.
pub fn list(path: &Path) -> io::Result<(OwnedFd, Vec<RawEnt>)> {
    let cpath = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let fd = unsafe { libc::open(cpath.as_ptr(), O_RDONLY | O_DIRECTORY | O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let ents = read_dents(fd.as_raw_fd())?;
    Ok((fd, ents))
}

#[allow(dead_code)]
pub fn read(path: &Path) -> io::Result<Vec<RawEnt>> {
    list(path).map(|(_, ents)| ents)
}

/// Open `name` relative to `dir` (does not follow symlinks).
#[allow(dead_code)]
pub fn open_at(dir: &OwnedFd, name: &OsStr) -> io::Result<File> {
    let cname = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "name contains NUL"))?;
    let flags = O_RDONLY | O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NOATIME;
    let mut fd = unsafe { libc::openat(dir.as_raw_fd(), cname.as_ptr(), flags) };
    if fd < 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EPERM) {
            let retry = O_RDONLY | O_CLOEXEC | libc::O_NOFOLLOW;
            fd = unsafe { libc::openat(dir.as_raw_fd(), cname.as_ptr(), retry) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
        } else {
            return Err(err);
        }
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// `fstatat` when `d_type` is unknown. Returns a concrete `DT_*` value.
#[allow(dead_code)]
pub fn dtype_at(dir: &OwnedFd, name: &OsStr) -> Option<u8> {
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

fn read_dents(fd: i32) -> io::Result<Vec<RawEnt>> {
    DENT_BUF.with(|cell| {
        let mut buf = cell.borrow_mut();
        if buf.len() < GETDENTS_BUF {
            buf.resize(GETDENTS_BUF, 0);
        }
        let mut out = Vec::with_capacity(32);
        loop {
            let n = unsafe { libc::syscall(libc::SYS_getdents64, fd, buf.as_mut_ptr(), buf.len()) };
            if n < 0 {
                return Err(io::Error::last_os_error());
            }
            if n == 0 {
                break;
            }
            parse_dents(&buf[..n as usize], &mut out);
        }
        Ok(out)
    })
}

fn parse_dents(buf: &[u8], out: &mut Vec<RawEnt>) {
    let n = buf.len();
    let mut off = 0usize;
    while off + std::mem::size_of::<LinuxDirent64>() <= n {
        let dent =
            unsafe { std::ptr::read_unaligned(buf.as_ptr().add(off).cast::<LinuxDirent64>()) };
        let reclen = dent.d_reclen as usize;
        if reclen < 20 || off + reclen > n {
            break;
        }
        let name_c = unsafe { CStr::from_ptr(buf.as_ptr().add(off + 19).cast()) };
        let bytes = name_c.to_bytes();
        if bytes != b"." && bytes != b".." {
            out.push(RawEnt {
                name: OsStr::from_bytes(bytes).to_os_string(),
                d_type: dent.d_type,
            });
        }
        off += reclen;
    }
}
