use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;

const MMAP_THRESHOLD: u64 = 64 * 1024;

struct Mapped {
    ptr: *mut libc::c_void,
    len: usize,
}

unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

impl Drop for Mapped {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr, self.len) };
    }
}

enum Kind {
    Map(Mapped),
    Owned(Vec<u8>),
}

pub(crate) struct Source(Kind);

impl Source {
    pub(crate) fn bytes(&self) -> &[u8] {
        match &self.0 {
            Kind::Map(m) => unsafe { std::slice::from_raw_parts(m.ptr as *const u8, m.len) },
            Kind::Owned(v) => v,
        }
    }
}

pub(crate) fn open_source(path: &Path) -> io::Result<Source> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < MMAP_THRESHOLD || len > usize::MAX as u64 {
        let mut buf = Vec::with_capacity(len as usize);
        (&file).read_to_end(&mut buf)?;
        return Ok(Source(Kind::Owned(buf)));
    }
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len as usize,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            file.as_raw_fd(),
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        let mut buf = Vec::with_capacity(len as usize);
        (&file).read_to_end(&mut buf)?;
        return Ok(Source(Kind::Owned(buf)));
    }
    unsafe { libc::madvise(ptr, len as usize, libc::MADV_WILLNEED) };
    Ok(Source(Kind::Map(Mapped {
        ptr,
        len: len as usize,
    })))
}
