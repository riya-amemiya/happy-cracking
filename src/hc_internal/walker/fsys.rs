use std::ffi::OsStr;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Kind {
    File,
    Dir,
    Symlink,
    Fifo,
    Socket,
    BlockDevice,
    CharDevice,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FileType(Kind);

impl FileType {
    pub fn is_dir(self) -> bool {
        self.0 == Kind::Dir
    }

    pub fn is_file(self) -> bool {
        self.0 == Kind::File
    }

    pub fn is_symlink(self) -> bool {
        self.0 == Kind::Symlink
    }

    pub fn is_fifo(self) -> bool {
        self.0 == Kind::Fifo
    }

    pub fn is_socket(self) -> bool {
        self.0 == Kind::Socket
    }

    pub fn is_block_device(self) -> bool {
        self.0 == Kind::BlockDevice
    }

    pub fn is_char_device(self) -> bool {
        self.0 == Kind::CharDevice
    }
}

impl From<std::fs::FileType> for FileType {
    fn from(ft: std::fs::FileType) -> FileType {
        #[cfg(unix)]
        use std::os::unix::fs::FileTypeExt;
        let kind = if ft.is_dir() {
            Kind::Dir
        } else if ft.is_file() {
            Kind::File
        } else if ft.is_symlink() {
            Kind::Symlink
        } else {
            #[cfg(unix)]
            {
                if ft.is_fifo() {
                    Kind::Fifo
                } else if ft.is_socket() {
                    Kind::Socket
                } else if ft.is_block_device() {
                    Kind::BlockDevice
                } else if ft.is_char_device() {
                    Kind::CharDevice
                } else {
                    Kind::Other
                }
            }
            #[cfg(not(unix))]
            {
                Kind::Other
            }
        };
        FileType(kind)
    }
}

pub(crate) struct Ent {
    #[cfg(unix)]
    start: u32,
    #[cfg(unix)]
    len: u32,
    #[cfg(not(unix))]
    name: std::ffi::OsString,
    pub(crate) ty: Option<FileType>,
    pub(crate) ino: u64,
    pub(crate) attr_hidden: bool,
}

#[derive(Default)]
pub(crate) struct Listing {
    #[cfg(unix)]
    names: Vec<u8>,
    pub(crate) ents: Vec<Ent>,
    pub(crate) err: Option<io::Error>,
}

impl Listing {
    #[cfg(unix)]
    pub(crate) fn name(&self, ent: &Ent) -> &OsStr {
        use std::os::unix::ffi::OsStrExt;
        let start = ent.start as usize;
        OsStr::from_bytes(&self.names[start..start + ent.len as usize])
    }

    #[cfg(not(unix))]
    pub(crate) fn name<'a>(&self, ent: &'a Ent) -> &'a OsStr {
        &ent.name
    }

    pub(crate) fn lookup_all(&self, names: &[&OsStr]) -> Vec<Lookup> {
        let mut out: Vec<Lookup> = names
            .iter()
            .map(|n| {
                if n.as_encoded_bytes().is_ascii() {
                    Lookup::Absent
                } else {
                    Lookup::Unknown
                }
            })
            .collect();
        let mut exact = vec![false; names.len()];
        for ent in &self.ents {
            let got = self.name(ent).as_encoded_bytes();
            for (k, name) in names.iter().enumerate() {
                let name = name.as_encoded_bytes();
                if exact[k] || got.len() != name.len() {
                    continue;
                }
                if got == name {
                    exact[k] = true;
                    out[k] = match ent.ty {
                        Some(ty) if !ty.is_symlink() => Lookup::Present(ty),
                        _ => Lookup::Unknown,
                    };
                } else if got.eq_ignore_ascii_case(name) {
                    out[k] = Lookup::Unknown;
                }
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Lookup {
    Absent,
    Present(FileType),
    Unknown,
}

#[cfg(unix)]
pub(crate) fn check_dir(path: &Path) -> io::Result<()> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn check_dir(path: &Path) -> io::Result<()> {
    std::fs::read_dir(path).map(drop)
}

pub(crate) fn read_dir(path: &Path) -> io::Result<Listing> {
    let mut listing = Listing::default();
    for next in std::fs::read_dir(path)? {
        let ent = match next {
            Ok(ent) => ent,
            Err(err) => {
                listing.err = Some(err);
                break;
            }
        };
        listing.push(&ent);
    }
    Ok(listing)
}

impl Listing {
    #[cfg(unix)]
    fn push(&mut self, ent: &std::fs::DirEntry) {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::DirEntryExt;
        let name = ent.file_name();
        let name = name.as_bytes();
        let start = self.names.len() as u32;
        self.names.extend_from_slice(name);
        self.ents.push(Ent {
            start,
            len: name.len() as u32,
            ty: ent.file_type().ok().map(FileType::from),
            ino: ent.ino(),
            attr_hidden: false,
        });
    }

    #[cfg(not(unix))]
    fn push(&mut self, ent: &std::fs::DirEntry) {
        self.ents.push(Ent {
            name: ent.file_name(),
            ty: ent.file_type().ok().map(FileType::from),
            ino: 0,
            attr_hidden: ent.metadata().is_ok_and(|md| attr_hidden(&md)),
        });
    }
}

#[cfg(windows)]
pub(crate) fn attr_hidden(md: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes() & 0x2 != 0
}

#[cfg(not(windows))]
pub(crate) fn attr_hidden(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn dev_ino(md: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (md.dev(), md.ino())
}

#[cfg(unix)]
pub(crate) fn device_num(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    path.metadata().map(|md| md.dev())
}

#[cfg(not(unix))]
pub(crate) fn device_num(_: &Path) -> io::Result<u64> {
    Err(io::Error::other(
        "walkdir: same_file_system option not supported on this platform",
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Handle {
    #[cfg(unix)]
    key: (u64, u64),
    #[cfg(not(unix))]
    key: std::path::PathBuf,
}

impl Handle {
    #[cfg(unix)]
    pub(crate) fn from_path(path: &Path) -> io::Result<Handle> {
        let file = std::fs::File::open(path)?;
        Ok(Handle {
            key: dev_ino(&file.metadata()?),
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn from_path(path: &Path) -> io::Result<Handle> {
        Ok(Handle {
            key: std::fs::canonicalize(path)?,
        })
    }

    #[cfg(unix)]
    pub(crate) fn stdout() -> Option<Handle> {
        let st = rustix::fs::fstat(std::io::stdout()).ok()?;
        if rustix::fs::FileType::from_raw_mode(st.st_mode) != rustix::fs::FileType::RegularFile {
            return None;
        }
        Some(Handle {
            key: (st.st_dev as u64, st.st_ino as u64),
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn stdout() -> Option<Handle> {
        None
    }

    #[cfg(unix)]
    pub(crate) fn ino(&self) -> u64 {
        self.key.1
    }
}
