use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use memmap2::{Advice, Mmap, MmapOptions};

const MMAP_MIN: u64 = 64 * 1024;
const CHUNK: usize = 16 * 1024;

fn open_read(path: &Path) -> io::Result<File> {
    File::open(path)
}

fn read_first_chunk(file: &mut File, buf: &mut Vec<u8>) -> io::Result<usize> {
    buf.clear();
    buf.resize(CHUNK, 0);
    let n = file.read(&mut buf[..])?;
    buf.truncate(n);
    Ok(n)
}

pub(crate) fn from_file(mut file: File, buf: &mut Vec<u8>, early: bool) -> io::Result<Source<'_>> {
    let n = read_first_chunk(&mut file, buf)?;
    if n < CHUNK {
        return Ok(Source(Kind::Mem(buf)));
    }
    if let Ok(meta) = file.metadata() {
        let len = meta.len();
        if len >= MMAP_MIN {
            // SAFETY: the mapping is used only as an immutable `&[u8]` for
            // searching. Concurrent truncation/writes can SIGBUS; that is the
            // same mmap contract as GNU grep and the previous libc MAP_PRIVATE
            // implementation.
            if let Ok(mmap) = unsafe { MmapOptions::new().map_copy_read_only(&file) } {
                if !early {
                    let _ = mmap.advise(Advice::WillNeed);
                }
                return Ok(Source(Kind::Map(mmap)));
            }
        }
    }
    file.read_to_end(buf)?;
    Ok(Source(Kind::Mem(buf)))
}

enum Kind<'a> {
    Map(Mmap),
    Mem(&'a [u8]),
}

pub(crate) struct Source<'a>(Kind<'a>);

impl Source<'_> {
    pub(crate) fn bytes(&self) -> &[u8] {
        match &self.0 {
            Kind::Map(m) => m.as_ref(),
            Kind::Mem(v) => v,
        }
    }

    pub(crate) fn prefetch_from(&self, offset: usize) {
        if let Kind::Map(m) = &self.0 {
            let page = 4096usize;
            let off = offset & !(page - 1);
            if off < m.len() {
                let _ = m.advise_range(Advice::WillNeed, off, m.len() - off);
            }
        }
    }
}

pub(crate) fn open_source<'a>(
    path: &Path,
    buf: &'a mut Vec<u8>,
    early: bool,
) -> io::Result<Source<'a>> {
    from_file(open_read(path)?, buf, early)
}
