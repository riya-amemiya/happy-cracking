//! Linux getdents64 directory listing with d_type, used by hfind and hgrep.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

use libc::{O_CLOEXEC, O_DIRECTORY, O_RDONLY};

const GETDENTS_BUF: usize = 64 * 1024;

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

pub fn read(path: &Path) -> io::Result<Vec<RawEnt>> {
    let cpath = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let fd = unsafe { libc::open(cpath.as_ptr(), O_RDONLY | O_DIRECTORY | O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut buf = vec![0u8; GETDENTS_BUF];
    let mut out = Vec::with_capacity(32);
    loop {
        let n = unsafe {
            libc::syscall(
                libc::SYS_getdents64,
                fd.as_raw_fd(),
                buf.as_mut_ptr(),
                buf.len(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            break;
        }
        let n = n as usize;
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
    Ok(out)
}
