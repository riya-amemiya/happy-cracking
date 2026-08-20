use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;

const MMAP_MIN: u64 = 64 * 1024;

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

enum Kind<'a> {
    Map(Mapped),
    Mem(&'a [u8]),
}

pub(crate) struct Source<'a>(Kind<'a>);

impl Source<'_> {
    pub(crate) fn bytes(&self) -> &[u8] {
        match &self.0 {
            Kind::Map(m) => unsafe { std::slice::from_raw_parts(m.ptr as *const u8, m.len) },
            Kind::Mem(v) => v,
        }
    }

    pub(crate) fn prefetch_from(&self, offset: usize) {
        if let Kind::Map(m) = &self.0 {
            let page = 4096usize;
            let off = offset & !(page - 1);
            if off < m.len {
                unsafe {
                    libc::madvise(
                        (m.ptr as *mut u8).add(off) as *mut libc::c_void,
                        m.len - off,
                        libc::MADV_WILLNEED,
                    )
                };
            }
        }
    }
}

fn read_into(file: &File, len: u64, buf: &mut Vec<u8>) -> io::Result<()> {
    buf.clear();
    buf.reserve(len as usize);
    file.take(len).read_to_end(buf)?;
    Ok(())
}

pub(crate) fn open_source<'a>(
    path: &Path,
    buf: &'a mut Vec<u8>,
    early: bool,
) -> io::Result<Source<'a>> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > usize::MAX as u64 {
        read_into(&file, len, buf)?;
        return Ok(Source(Kind::Mem(buf)));
    }
    if len >= MMAP_MIN {
        #[cfg(target_os = "linux")]
        let map_flags = libc::MAP_PRIVATE | libc::MAP_POPULATE;
        #[cfg(not(target_os = "linux"))]
        let map_flags = libc::MAP_PRIVATE;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len as usize,
                libc::PROT_READ,
                map_flags,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr != libc::MAP_FAILED {
            if !early {
                unsafe { libc::madvise(ptr, len as usize, libc::MADV_WILLNEED) };
            }
            return Ok(Source(Kind::Map(Mapped {
                ptr,
                len: len as usize,
            })));
        }
    }
    read_into(&file, len, buf)?;
    Ok(Source(Kind::Mem(buf)))
}
