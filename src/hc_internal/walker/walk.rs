use std::cmp::Ordering;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use super::dir::{Ignore, IgnoreBuilder, Probe};
use super::fsys::{self, FileType, Handle, Listing};
use super::gitignore::GitignoreBuilder;
use super::overrides::Override;
use super::types::Types;
use super::{Error, PartialErrorBuilder};

#[derive(Clone, Debug)]
pub struct DirEntry {
    path: PathBuf,
    ty: Option<FileType>,
    follow_link: bool,
    depth: usize,
    ino: u64,
    attr_hidden: bool,
    walkdir: bool,
    err: Option<Error>,
}

impl DirEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_path(self) -> PathBuf {
        self.path
    }

    pub fn path_is_symlink(&self) -> bool {
        self.ty.is_some_and(FileType::is_symlink) || self.follow_link
    }

    pub fn is_stdin(&self) -> bool {
        self.ty.is_none()
    }

    pub fn metadata(&self) -> Result<Metadata, Error> {
        if self.is_stdin() {
            let err = Error::Io(io::Error::other("<stdin> has no metadata"));
            return Err(err.with_path("<stdin>"));
        }
        let md = if self.follow_link {
            fs::metadata(&self.path)
        } else {
            fs::symlink_metadata(&self.path)
        };
        md.map_err(|err| {
            let err = if self.walkdir {
                WdError::from_path(self.depth, self.path.clone(), err).into_io()
            } else {
                err
            };
            Error::Io(err).with_path(&self.path)
        })
    }

    pub fn file_type(&self) -> Option<FileType> {
        self.ty
    }

    pub fn file_name(&self) -> &OsStr {
        if self.is_stdin() {
            return OsStr::new("<stdin>");
        }
        self.path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    #[cfg(unix)]
    pub fn ino(&self) -> Option<u64> {
        if self.is_stdin() {
            None
        } else {
            Some(self.ino)
        }
    }

    pub fn error(&self) -> Option<&Error> {
        self.err.as_ref()
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.ty.is_some_and(FileType::is_dir)
    }

    pub(crate) fn from_parts(
        path: PathBuf,
        ty: std::fs::FileType,
        depth: usize,
        follow_link: bool,
        ino: u64,
    ) -> DirEntry {
        DirEntry {
            path,
            ty: Some(ty.into()),
            follow_link,
            depth,
            ino,
            attr_hidden: false,
            walkdir: false,
            err: None,
        }
    }

    fn new_stdin() -> DirEntry {
        DirEntry {
            path: PathBuf::from("<stdin>"),
            ty: None,
            follow_link: false,
            depth: 0,
            ino: 0,
            attr_hidden: false,
            walkdir: false,
            err: None,
        }
    }

    fn from_path(depth: usize, pb: PathBuf, link: bool) -> Result<DirEntry, Error> {
        match fs::metadata(&pb) {
            Ok(md) => Ok(DirEntry {
                ty: Some(md.file_type().into()),
                ino: ino_of(&md),
                attr_hidden: fsys::attr_hidden(&md),
                path: pb,
                follow_link: link,
                depth,
                walkdir: false,
                err: None,
            }),
            Err(err) => Err(Error::Io(err).with_path(&pb)),
        }
    }

    fn from_wd(ent: WdEnt) -> DirEntry {
        DirEntry {
            path: ent.path,
            ty: Some(ent.ty),
            follow_link: ent.follow_link,
            depth: ent.depth,
            ino: ent.ino,
            attr_hidden: ent.attr_hidden,
            walkdir: true,
            err: None,
        }
    }
}

#[cfg(unix)]
fn ino_of(md: &Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(md)
}

#[cfg(not(unix))]
fn ino_of(_: &Metadata) -> u64 {
    0
}

type NameCmp = dyn Fn(&OsStr, &OsStr) -> Ordering + Send + Sync + 'static;

#[derive(Clone)]
enum Sorter {
    ByName(Arc<NameCmp>),
}

#[derive(Clone)]
struct Filter(Arc<dyn Fn(&DirEntry) -> bool + Send + Sync + 'static>);

#[derive(Clone)]
pub struct WalkBuilder {
    paths: Vec<PathBuf>,
    ig_builder: IgnoreBuilder,
    max_depth: Option<usize>,
    min_depth: Option<usize>,
    max_filesize: Option<u64>,
    follow_links: bool,
    same_file_system: bool,
    sorter: Option<Sorter>,
    threads: usize,
    skip: Option<Arc<Handle>>,
    filter: Option<Filter>,
    global_gitignores_relative_to: OnceLock<Result<PathBuf, Arc<io::Error>>>,
}

impl fmt::Debug for WalkBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WalkBuilder")
            .field("paths", &self.paths)
            .field("ig_builder", &self.ig_builder)
            .field("max_depth", &self.max_depth)
            .field("min_depth", &self.min_depth)
            .field("max_filesize", &self.max_filesize)
            .field("follow_links", &self.follow_links)
            .field("same_file_system", &self.same_file_system)
            .field("threads", &self.threads)
            .finish_non_exhaustive()
    }
}

impl WalkBuilder {
    pub fn new<P: AsRef<Path>>(path: P) -> WalkBuilder {
        WalkBuilder {
            paths: vec![path.as_ref().to_path_buf()],
            ig_builder: IgnoreBuilder::new(),
            max_depth: None,
            min_depth: None,
            max_filesize: None,
            follow_links: false,
            same_file_system: false,
            sorter: None,
            threads: 0,
            skip: None,
            filter: None,
            global_gitignores_relative_to: OnceLock::new(),
        }
    }

    fn ig_root(&self) -> Ignore {
        self.ig_builder
            .build_with_cwd(self.get_or_set_current_dir().map(Path::to_path_buf))
    }

    pub fn build(&self) -> Walk {
        let its = self
            .paths
            .iter()
            .map(|p| {
                if p == Path::new("-") {
                    return (p.clone(), None);
                }
                let wd = Wd {
                    follow_links: self.follow_links || p.is_file(),
                    max_depth: self.max_depth.unwrap_or(usize::MAX),
                    min_depth: self.min_depth.unwrap_or(0),
                    same_file_system: self.same_file_system,
                    sorter: self.sorter.clone(),
                    start: Some(p.clone()),
                    stack: vec![],
                    ancestors: vec![],
                    depth: 0,
                    root_device: None,
                };
                (
                    p.clone(),
                    Some(EventIter {
                        depth: 0,
                        it: wd,
                        next: None,
                    }),
                )
            })
            .collect::<Vec<_>>()
            .into_iter();
        let ig_root = self.ig_root();
        Walk {
            its,
            it: None,
            igs: vec![ig_root.clone()],
            ig_root,
            max_filesize: self.max_filesize,
            skip: self.skip.clone(),
            filter: self.filter.clone(),
        }
    }

    pub fn build_parallel(&self) -> WalkParallel {
        WalkParallel {
            paths: self.paths.clone(),
            ig_root: self.ig_root(),
            max_depth: self.max_depth,
            min_depth: self.min_depth,
            max_filesize: self.max_filesize,
            follow_links: self.follow_links,
            same_file_system: self.same_file_system,
            threads: self.threads,
            skip: self.skip.clone(),
            filter: self.filter.clone(),
        }
    }

    pub fn add<P: AsRef<Path>>(&mut self, path: P) -> &mut WalkBuilder {
        self.paths.push(path.as_ref().to_path_buf());
        self
    }

    pub fn max_depth(&mut self, depth: Option<usize>) -> &mut WalkBuilder {
        self.max_depth = depth;
        if let (Some(min), Some(max)) = (self.min_depth, self.max_depth)
            && max < min
        {
            self.max_depth = Some(min);
        }
        self
    }

    #[cfg(test)]
    pub fn min_depth(&mut self, depth: Option<usize>) -> &mut WalkBuilder {
        self.min_depth = depth;
        if let (Some(min), Some(max)) = (self.min_depth, self.max_depth)
            && min > max
        {
            self.min_depth = Some(max);
        }
        self
    }

    pub fn follow_links(&mut self, yes: bool) -> &mut WalkBuilder {
        self.follow_links = yes;
        self
    }

    pub fn max_filesize(&mut self, filesize: Option<u64>) -> &mut WalkBuilder {
        self.max_filesize = filesize;
        self
    }

    pub fn threads(&mut self, n: usize) -> &mut WalkBuilder {
        self.threads = n;
        self
    }

    pub fn add_ignore<P: AsRef<Path>>(&mut self, path: P) -> Option<Error> {
        let path = path.as_ref();
        let Some(cwd) = self.get_or_set_current_dir() else {
            let err = io::Error::other(format!(
                "CWD is not known, ignoring global gitignore {}",
                path.display()
            ));
            return Some(err.into());
        };
        let mut builder = GitignoreBuilder::new(cwd);
        let mut errs = PartialErrorBuilder::default();
        errs.maybe_push(builder.add(path));
        match builder.build() {
            Ok(gi) => {
                self.ig_builder.add_ignore(gi);
            }
            Err(err) => errs.push(err),
        }
        errs.into_error_option()
    }

    pub fn add_custom_ignore_filename<S: AsRef<OsStr>>(
        &mut self,
        file_name: S,
    ) -> &mut WalkBuilder {
        self.ig_builder.add_custom_ignore_filename(file_name);
        self
    }

    pub fn overrides(&mut self, overrides: Override) -> &mut WalkBuilder {
        self.ig_builder.overrides(overrides);
        self
    }

    pub fn types(&mut self, types: Types) -> &mut WalkBuilder {
        self.ig_builder.types(types);
        self
    }

    pub fn standard_filters(&mut self, yes: bool) -> &mut WalkBuilder {
        self.hidden(yes)
            .parents(yes)
            .ignore(yes)
            .git_ignore(yes)
            .git_global(yes)
            .git_exclude(yes)
    }

    pub fn hidden(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.hidden = yes;
        self
    }

    pub fn parents(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.parents = yes;
        self
    }

    pub fn ignore(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.ignore = yes;
        self
    }

    pub fn git_global(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.git_global = yes;
        self
    }

    pub fn git_ignore(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.git_ignore = yes;
        self
    }

    pub fn git_exclude(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.git_exclude = yes;
        self
    }

    pub fn require_git(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.require_git = yes;
        self
    }

    pub fn ignore_case_insensitive(&mut self, yes: bool) -> &mut WalkBuilder {
        self.ig_builder.opts.ignore_case_insensitive = yes;
        self
    }

    pub fn sort_by_file_name<F>(&mut self, cmp: F) -> &mut WalkBuilder
    where
        F: Fn(&OsStr, &OsStr) -> Ordering + Send + Sync + 'static,
    {
        self.sorter = Some(Sorter::ByName(Arc::new(cmp)));
        self
    }

    pub fn same_file_system(&mut self, yes: bool) -> &mut WalkBuilder {
        self.same_file_system = yes;
        self
    }

    pub fn skip_stdout(&mut self, yes: bool) -> &mut WalkBuilder {
        self.skip = if yes {
            Handle::stdout().map(Arc::new)
        } else {
            None
        };
        self
    }

    pub fn filter_entry<P>(&mut self, filter: P) -> &mut WalkBuilder
    where
        P: Fn(&DirEntry) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Filter(Arc::new(filter)));
        self
    }

    pub fn current_dir(&mut self, cwd: impl Into<PathBuf>) -> &mut WalkBuilder {
        let cwd = cwd.into();
        self.ig_builder.current_dir(cwd.clone());
        self.global_gitignores_relative_to = OnceLock::from(Ok(cwd));
        self
    }

    fn get_or_set_current_dir(&self) -> Option<&Path> {
        self.global_gitignores_relative_to
            .get_or_init(|| std::env::current_dir().map_err(Arc::new))
            .as_ref()
            .ok()
            .map(PathBuf::as_path)
    }
}

fn skip_filesize(max_filesize: u64, md: Option<&Metadata>) -> bool {
    md.is_some_and(|md| md.len() > max_filesize)
}

fn path_equals(dent: &DirEntry, handle: &Handle) -> Result<bool, Error> {
    #[cfg(unix)]
    if dent.ino() != Some(handle.ino()) {
        return Ok(false);
    }
    if dent.is_stdin() {
        return Ok(false);
    }
    Handle::from_path(dent.path())
        .map(|h| &h == handle)
        .map_err(|err| Error::Io(err).with_path(dent.path()))
}

#[derive(Debug)]
struct WdError {
    depth: usize,
    inner: WdInner,
}

#[derive(Debug)]
enum WdInner {
    Io {
        path: Option<PathBuf>,
        err: io::Error,
    },
    Loop {
        ancestor: PathBuf,
        child: PathBuf,
    },
}

impl WdError {
    fn from_path(depth: usize, path: PathBuf, err: io::Error) -> WdError {
        WdError {
            depth,
            inner: WdInner::Io {
                path: Some(path),
                err,
            },
        }
    }

    fn from_io(depth: usize, err: io::Error) -> WdError {
        WdError {
            depth,
            inner: WdInner::Io { path: None, err },
        }
    }

    fn into_io(self) -> io::Error {
        let kind = match &self.inner {
            WdInner::Io { err, .. } => err.kind(),
            WdInner::Loop { .. } => io::ErrorKind::Other,
        };
        io::Error::new(kind, self)
    }

    fn into_ignore_error(self) -> Error {
        let depth = self.depth;
        match self.inner {
            WdInner::Loop { ancestor, child } => Error::Loop { ancestor, child }.with_depth(depth),
            WdInner::Io { ref path, .. } => {
                let path = path.clone();
                let err = Error::Io(self.into_io());
                match path {
                    Some(path) => err.with_path(path),
                    None => err,
                }
            }
        }
    }
}

impl fmt::Display for WdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            WdInner::Io { path: None, err } => err.fmt(f),
            WdInner::Io {
                path: Some(path),
                err,
            } => write!(f, "IO error for operation on {}: {err}", path.display()),
            WdInner::Loop { ancestor, child } => write!(
                f,
                "File system loop found: {} points to an ancestor {}",
                child.display(),
                ancestor.display()
            ),
        }
    }
}

impl std::error::Error for WdError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.inner {
            WdInner::Io { err, .. } => Some(err),
            WdInner::Loop { .. } => None,
        }
    }
}

struct WdEnt {
    path: PathBuf,
    ty: FileType,
    follow_link: bool,
    depth: usize,
    ino: u64,
    attr_hidden: bool,
    pushed: bool,
}

impl WdEnt {
    fn from_path(depth: usize, pb: PathBuf, follow: bool) -> Result<WdEnt, WdError> {
        let md = if follow {
            fs::metadata(&pb)
        } else {
            fs::symlink_metadata(&pb)
        };
        match md {
            Ok(md) => Ok(WdEnt {
                ty: md.file_type().into(),
                ino: ino_of(&md),
                attr_hidden: fsys::attr_hidden(&md),
                path: pb,
                follow_link: follow,
                depth,
                pushed: false,
            }),
            Err(err) => Err(WdError::from_path(depth, pb, err)),
        }
    }

    fn file_name(&self) -> &OsStr {
        self.path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
    }
}

enum ListState {
    Lazy,
    Open {
        listing: Listing,
        items: std::vec::IntoIter<Result<WdEnt, WdError>>,
    },
    Failed(Option<WdError>),
}

struct WdList {
    dir: PathBuf,
    depth: usize,
    state: ListState,
}

struct Wd {
    follow_links: bool,
    max_depth: usize,
    min_depth: usize,
    same_file_system: bool,
    sorter: Option<Sorter>,
    start: Option<PathBuf>,
    stack: Vec<WdList>,
    ancestors: Vec<PathBuf>,
    depth: usize,
    root_device: Option<u64>,
}

impl WdList {
    fn open(&mut self, sorter: Option<&Sorter>) {
        if !matches!(self.state, ListState::Lazy) {
            return;
        }
        let mut listing = match fsys::read_dir(&self.dir) {
            Ok(listing) => listing,
            Err(err) => {
                let err = WdError::from_path(self.depth, self.dir.clone(), err);
                self.state = ListState::Failed(Some(err));
                return;
            }
        };
        let depth = self.depth + 1;
        let mut items: Vec<Result<WdEnt, WdError>> = Vec::with_capacity(listing.ents.len() + 1);
        for ent in &listing.ents {
            let path = self.dir.join(listing.name(ent));
            let ty = match ent.ty {
                Some(ty) => ty,
                None => match fs::symlink_metadata(&path) {
                    Ok(md) => md.file_type().into(),
                    Err(err) => {
                        items.push(Err(WdError::from_path(depth, path, err)));
                        continue;
                    }
                },
            };
            items.push(Ok(WdEnt {
                path,
                ty,
                follow_link: false,
                depth,
                ino: ent.ino,
                attr_hidden: ent.attr_hidden,
                pushed: false,
            }));
        }
        if let Some(err) = listing.err.take() {
            items.push(Err(WdError::from_io(depth, err)));
        }
        if let Some(sorter) = sorter {
            items.sort_by(|a, b| match (a, b) {
                (Ok(a), Ok(b)) => match sorter {
                    Sorter::ByName(cmp) => cmp(a.file_name(), b.file_name()),
                },
                (Err(_), Err(_)) => Ordering::Equal,
                (Ok(_), Err(_)) => Ordering::Greater,
                (Err(_), Ok(_)) => Ordering::Less,
            });
        }
        self.state = ListState::Open {
            listing,
            items: items.into_iter(),
        };
    }

    fn next(&mut self, sorter: Option<&Sorter>) -> Option<Result<WdEnt, WdError>> {
        self.open(sorter);
        match &mut self.state {
            ListState::Lazy => None,
            ListState::Open { items, .. } => items.next(),
            ListState::Failed(err) => err.take().map(Err),
        }
    }
}

impl Wd {
    fn next(&mut self) -> Option<Result<WdEnt, WdError>> {
        if let Some(start) = self.start.take() {
            if self.same_file_system {
                match fsys::device_num(&start) {
                    Ok(dev) => self.root_device = Some(dev),
                    Err(err) => return Some(Err(WdError::from_path(0, start, err))),
                }
            }
            let dent = match WdEnt::from_path(0, start, false) {
                Ok(dent) => dent,
                Err(err) => return Some(Err(err)),
            };
            if let Some(result) = self.handle_entry(dent) {
                return Some(result);
            }
        }
        while !self.stack.is_empty() {
            self.depth = self.stack.len();
            if self.depth > self.max_depth {
                self.pop();
                continue;
            }
            let sorter = self.sorter.as_ref();
            let next = self.stack.last_mut().and_then(|list| list.next(sorter));
            match next {
                None => self.pop(),
                Some(Err(err)) => return Some(Err(err)),
                Some(Ok(dent)) => {
                    if let Some(result) = self.handle_entry(dent) {
                        return Some(result);
                    }
                }
            }
        }
        None
    }

    fn skip_current_dir(&mut self) {
        if !self.stack.is_empty() {
            self.pop();
        }
    }

    fn top_listing(&mut self) -> Option<&Listing> {
        let sorter = self.sorter.as_ref();
        let list = self.stack.last_mut()?;
        list.open(sorter);
        match &list.state {
            ListState::Open { listing, .. } => Some(listing),
            _ => None,
        }
    }

    fn handle_entry(&mut self, mut dent: WdEnt) -> Option<Result<WdEnt, WdError>> {
        if self.follow_links && dent.ty.is_symlink() {
            dent = match self.follow(dent) {
                Ok(dent) => dent,
                Err(err) => return Some(Err(err)),
            };
        }
        let is_normal_dir = !dent.ty.is_symlink() && dent.ty.is_dir();
        if is_normal_dir {
            if self.same_file_system && dent.depth > 0 {
                match fsys::device_num(&dent.path) {
                    Ok(dev) => {
                        if Some(dev) == self.root_device {
                            self.push(&mut dent);
                        }
                    }
                    Err(err) => {
                        return Some(Err(WdError::from_path(dent.depth, dent.path, err)));
                    }
                }
            } else {
                self.push(&mut dent);
            }
        } else if dent.depth == 0 && dent.ty.is_symlink() {
            match fs::metadata(&dent.path) {
                Ok(md) => {
                    if md.file_type().is_dir() {
                        self.push(&mut dent);
                    }
                }
                Err(err) => {
                    return Some(Err(WdError::from_path(dent.depth, dent.path, err)));
                }
            }
        }
        if self.depth < self.min_depth || self.depth > self.max_depth {
            None
        } else {
            Some(Ok(dent))
        }
    }

    fn push(&mut self, dent: &mut WdEnt) {
        self.stack.push(WdList {
            dir: dent.path.clone(),
            depth: self.depth,
            state: ListState::Lazy,
        });
        if self.follow_links {
            self.ancestors.push(dent.path.clone());
        }
        dent.pushed = true;
    }

    fn pop(&mut self) {
        self.stack.pop();
        if self.follow_links {
            self.ancestors.pop();
        }
    }

    fn follow(&self, dent: WdEnt) -> Result<WdEnt, WdError> {
        let dent = WdEnt::from_path(self.depth, dent.path, true)?;
        if dent.ty.is_dir() {
            self.check_loop(&dent.path)?;
        }
        Ok(dent)
    }

    fn check_loop(&self, child: &Path) -> Result<(), WdError> {
        let hchild = Handle::from_path(child).map_err(|err| WdError::from_io(self.depth, err))?;
        for ancestor in self.ancestors.iter().rev() {
            let h = Handle::from_path(ancestor).map_err(|err| WdError::from_io(self.depth, err))?;
            if h == hchild {
                return Err(WdError {
                    depth: self.depth,
                    inner: WdInner::Loop {
                        ancestor: ancestor.clone(),
                        child: child.to_path_buf(),
                    },
                });
            }
        }
        Ok(())
    }
}

enum WalkEvent {
    Dir(WdEnt),
    File(WdEnt),
    Exit,
}

struct EventIter {
    depth: usize,
    it: Wd,
    next: Option<Result<WdEnt, WdError>>,
}

fn walkdir_is_dir(dent: &WdEnt) -> bool {
    if dent.ty.is_dir() {
        return true;
    }
    if !dent.ty.is_symlink() || dent.depth > 0 {
        return false;
    }
    dent.path.metadata().is_ok_and(|md| md.file_type().is_dir())
}

impl EventIter {
    fn next(&mut self) -> Option<Result<WalkEvent, WdError>> {
        let dent = self.next.take().or_else(|| self.it.next());
        let depth = match &dent {
            None => 0,
            Some(Ok(dent)) => dent.depth,
            Some(Err(err)) => err.depth,
        };
        if depth < self.depth {
            self.depth -= 1;
            self.next = dent;
            return Some(Ok(WalkEvent::Exit));
        }
        self.depth = depth;
        match dent {
            None => None,
            Some(Err(err)) => Some(Err(err)),
            Some(Ok(dent)) => {
                if walkdir_is_dir(&dent) {
                    self.depth += 1;
                    Some(Ok(WalkEvent::Dir(dent)))
                } else {
                    Some(Ok(WalkEvent::File(dent)))
                }
            }
        }
    }
}

pub struct Walk {
    its: std::vec::IntoIter<(PathBuf, Option<EventIter>)>,
    it: Option<EventIter>,
    ig_root: Ignore,
    igs: Vec<Ignore>,
    max_filesize: Option<u64>,
    skip: Option<Arc<Handle>>,
    filter: Option<Filter>,
}

impl Walk {
    fn ig(&self) -> &Ignore {
        self.igs.last().unwrap_or(&self.ig_root)
    }

    fn skip_entry(&self, ent: &DirEntry) -> Result<bool, Error> {
        if ent.depth() == 0 {
            return Ok(false);
        }
        if self
            .ig()
            .should_skip(ent.path(), ent.is_dir(), ent.attr_hidden)
        {
            return Ok(true);
        }
        if let Some(stdout) = &self.skip
            && path_equals(ent, stdout)?
        {
            return Ok(true);
        }
        if let Some(max) = self.max_filesize
            && !ent.is_dir()
        {
            return Ok(skip_filesize(max, ent.metadata().ok().as_ref()));
        }
        if let Some(Filter(filter)) = &self.filter
            && !filter(ent)
        {
            return Ok(true);
        }
        Ok(false)
    }

    fn enter_dir(&mut self, pushed: bool, path: &Path, depth: usize) -> Option<Error> {
        let ig = self.ig().clone();
        let it = self.it.as_mut().map(|it| &mut it.it);
        let listing = match it {
            Some(wd) if pushed && depth < wd.max_depth => wd.top_listing(),
            _ => None,
        };
        let probe = listing.map_or(Probe::Fs, Probe::Listing);
        let (child, err) = ig.add_child(path, &probe, false);
        self.igs.push(child);
        err
    }
}

impl Iterator for Walk {
    type Item = Result<DirEntry, Error>;

    fn next(&mut self) -> Option<Result<DirEntry, Error>> {
        loop {
            let Some(ev) = self.it.as_mut().and_then(EventIter::next) else {
                match self.its.next() {
                    None => return None,
                    Some((_, None)) => return Some(Ok(DirEntry::new_stdin())),
                    Some((path, Some(it))) => {
                        self.it = Some(it);
                        if path.is_dir() {
                            let (ig, err) = self.ig_root.add_parents(path);
                            self.igs = vec![ig];
                            if let Some(err) = err {
                                return Some(Err(err));
                            }
                        } else {
                            self.igs = vec![self.ig_root.clone()];
                        }
                    }
                }
                continue;
            };
            match ev {
                Err(err) => return Some(Err(err.into_ignore_error())),
                Ok(WalkEvent::Exit) => {
                    if self.igs.len() > 1 {
                        self.igs.pop();
                    }
                }
                Ok(WalkEvent::Dir(ent)) => {
                    let pushed = ent.pushed;
                    let mut ent = DirEntry::from_wd(ent);
                    match self.skip_entry(&ent) {
                        Err(err) => return Some(Err(err)),
                        Ok(true) => {
                            if let Some(it) = self.it.as_mut() {
                                it.it.skip_current_dir();
                            }
                            let ig = self.ig().clone();
                            self.igs.push(ig);
                            continue;
                        }
                        Ok(false) => {}
                    }
                    ent.err = self.enter_dir(pushed, &ent.path, ent.depth);
                    return Some(Ok(ent));
                }
                Ok(WalkEvent::File(ent)) => {
                    let ent = DirEntry::from_wd(ent);
                    match self.skip_entry(&ent) {
                        Err(err) => return Some(Err(err)),
                        Ok(true) => {}
                        Ok(false) => return Some(Ok(ent)),
                    }
                }
            }
        }
    }
}

impl std::iter::FusedIterator for Walk {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalkState {
    Continue,
    Skip,
    Quit,
}

impl WalkState {
    fn is_continue(self) -> bool {
        self == WalkState::Continue
    }

    fn is_quit(self) -> bool {
        self == WalkState::Quit
    }
}

pub trait ParallelVisitorBuilder<'s> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 's>;
}

impl<'s, P: ParallelVisitorBuilder<'s>> ParallelVisitorBuilder<'s> for &mut P {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 's> {
        (**self).build()
    }
}

pub trait ParallelVisitor: Send {
    fn visit(&mut self, entry: Result<DirEntry, Error>) -> WalkState;
}

type FnVisitor<'s> = Box<dyn FnMut(Result<DirEntry, Error>) -> WalkState + Send + 's>;

struct FnBuilder<F> {
    builder: F,
}

impl<'s, F: FnMut() -> FnVisitor<'s>> ParallelVisitorBuilder<'s> for FnBuilder<F> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 's> {
        Box::new(FnVisitorImp {
            visitor: (self.builder)(),
        })
    }
}

struct FnVisitorImp<'s> {
    visitor: FnVisitor<'s>,
}

impl ParallelVisitor for FnVisitorImp<'_> {
    fn visit(&mut self, entry: Result<DirEntry, Error>) -> WalkState {
        (self.visitor)(entry)
    }
}

pub struct WalkParallel {
    paths: Vec<PathBuf>,
    ig_root: Ignore,
    max_filesize: Option<u64>,
    max_depth: Option<usize>,
    min_depth: Option<usize>,
    follow_links: bool,
    same_file_system: bool,
    threads: usize,
    skip: Option<Arc<Handle>>,
    filter: Option<Filter>,
}

struct Work {
    dent: DirEntry,
    ignore: Ignore,
    root_device: Option<u64>,
}

enum Job {
    Entry(Work),
    Files(Vec<DirEntry>),
}

struct Pool {
    queues: Vec<Mutex<VecDeque<Job>>>,
    readers: Option<AtomicUsize>,
    pending: AtomicUsize,
    idle: AtomicUsize,
    quit: AtomicBool,
    done: AtomicBool,
    lock: Mutex<()>,
    cv: Condvar,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Pool {
    fn new(threads: usize) -> Pool {
        let limit = dir_read_limit();
        Pool {
            queues: (0..threads).map(|_| Mutex::new(VecDeque::new())).collect(),
            readers: (threads > limit).then(|| AtomicUsize::new(limit)),
            pending: AtomicUsize::new(0),
            idle: AtomicUsize::new(0),
            quit: AtomicBool::new(false),
            done: AtomicBool::new(false),
            lock: Mutex::new(()),
            cv: Condvar::new(),
        }
    }

    fn push(&self, me: usize, job: Job) {
        lock(&self.queues[me]).push_back(job);
        self.pending.fetch_add(1, AtomicOrdering::SeqCst);
        if self.idle.load(AtomicOrdering::SeqCst) > 0 {
            let _guard = lock(&self.lock);
            self.cv.notify_one();
        }
    }

    fn acquire_reader(&self) {
        let Some(readers) = &self.readers else {
            return;
        };
        let mut spins = 0u32;
        loop {
            let cur = readers.load(AtomicOrdering::Relaxed);
            if cur > 0
                && readers
                    .compare_exchange_weak(
                        cur,
                        cur - 1,
                        AtomicOrdering::Acquire,
                        AtomicOrdering::Relaxed,
                    )
                    .is_ok()
            {
                return;
            }
            spins = spins.saturating_add(1);
            if spins < 64 {
                std::thread::yield_now();
            } else {
                std::thread::sleep(Duration::from_micros(50));
            }
        }
    }

    fn release_reader(&self) {
        if let Some(readers) = &self.readers {
            readers.fetch_add(1, AtomicOrdering::Release);
        }
    }

    fn has_idle(&self) -> bool {
        self.idle.load(AtomicOrdering::Relaxed) > 0
    }

    fn quit_now(&self) {
        self.quit.store(true, AtomicOrdering::SeqCst);
        let _guard = lock(&self.lock);
        self.cv.notify_all();
    }

    fn is_quit(&self) -> bool {
        self.quit.load(AtomicOrdering::Relaxed)
    }

    fn pop(&self, me: usize) -> Option<Job> {
        let n = self.queues.len();
        let own = lock(&self.queues[me]).pop_back();
        let job = match own {
            Some(job) => job,
            None => (1..n).find_map(|k| lock(&self.queues[(me + k) % n]).pop_front())?,
        };
        self.pending.fetch_sub(1, AtomicOrdering::SeqCst);
        Some(job)
    }

    fn next_job(&self, me: usize) -> Option<Job> {
        let n = self.queues.len();
        loop {
            if self.is_quit() {
                return None;
            }
            if let Some(job) = self.pop(me) {
                return Some(job);
            }
            let mut guard = lock(&self.lock);
            self.idle.fetch_add(1, AtomicOrdering::SeqCst);
            loop {
                if self.is_quit() || self.done.load(AtomicOrdering::SeqCst) {
                    self.idle.fetch_sub(1, AtomicOrdering::SeqCst);
                    return None;
                }
                if self.pending.load(AtomicOrdering::SeqCst) > 0 {
                    self.idle.fetch_sub(1, AtomicOrdering::SeqCst);
                    break;
                }
                if self.idle.load(AtomicOrdering::SeqCst) == n {
                    self.done.store(true, AtomicOrdering::SeqCst);
                    self.cv.notify_all();
                    self.idle.fetch_sub(1, AtomicOrdering::SeqCst);
                    return None;
                }
                guard = self.cv.wait(guard).unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
}

struct Opts {
    max_depth: Option<usize>,
    min_depth: Option<usize>,
    max_filesize: Option<u64>,
    follow_links: bool,
    skip: Option<Arc<Handle>>,
    filter: Option<Filter>,
}

struct Worker<'s, 'p> {
    visitor: Box<dyn ParallelVisitor + 's>,
    pool: &'p Pool,
    opts: &'p Opts,
    me: usize,
    buf: PathBuf,
}

impl Worker<'_, '_> {
    fn run(&mut self) {
        while let Some(job) = self.pool.next_job(self.me) {
            let state = match job {
                Job::Entry(work) => self.run_one(work),
                Job::Files(files) => self.visit_files(files),
            };
            if state.is_quit() {
                self.pool.quit_now();
            }
        }
    }

    fn should_visit(&self, depth: usize) -> bool {
        self.opts.min_depth.is_none_or(|min| depth >= min)
    }

    fn visit(&mut self, entry: Result<DirEntry, Error>) -> WalkState {
        self.visitor.visit(entry)
    }

    fn run_one(&mut self, mut work: Work) -> WalkState {
        let depth = work.dent.depth();
        let should_visit = self.should_visit(depth);
        if work
            .dent
            .ty
            .is_none_or(|ty| ty.is_symlink() || !ty.is_dir())
        {
            return if should_visit {
                self.visit(Ok(work.dent))
            } else {
                WalkState::Continue
            };
        }
        if depth == 0 {
            let (ig, err) = work.ignore.add_parents(work.dent.path());
            work.ignore = ig;
            if let Some(err) = err
                && self.visit(Err(err)).is_quit()
            {
                return WalkState::Quit;
            }
        }
        let descend = match work.root_device {
            None => true,
            Some(root) => match fsys::device_num(work.dent.path()) {
                Ok(dev) => dev == root,
                Err(err) => {
                    let err = Error::Io(err).with_path(work.dent.path());
                    if self.visit(Err(err)).is_quit() {
                        return WalkState::Quit;
                    }
                    false
                }
            },
        };
        let at_max = self.opts.max_depth.is_some_and(|max| depth >= max);
        let listing = if descend && !at_max {
            self.pool.acquire_reader();
            let listing = fsys::read_dir(work.dent.path()).map(Some);
            self.pool.release_reader();
            listing
        } else {
            fsys::check_dir(work.dent.path()).map(|()| None)
        };
        let listing = match listing {
            Ok(listing) => {
                let probe = listing.as_ref().map_or(Probe::Fs, Probe::Listing);
                let (ig, err) =
                    work.ignore
                        .add_child(work.dent.path(), &probe, self.opts.follow_links);
                work.ignore = ig;
                work.dent.err = err;
                Ok(listing)
            }
            Err(err) => Err(Error::from(err)
                .with_path(work.dent.path())
                .with_depth(depth)),
        };
        self.buf.clear();
        self.buf.push(work.dent.path());
        if should_visit {
            let state = self.visit(Ok(work.dent));
            if !state.is_continue() {
                return state;
            }
        }
        if !descend {
            return WalkState::Skip;
        }
        let listing = match listing {
            Ok(listing) => listing,
            Err(err) => return self.visit(Err(err)),
        };
        let Some(listing) = listing else {
            return WalkState::Skip;
        };
        self.descend(&work.ignore, depth + 1, work.root_device, &listing)
    }

    fn descend(
        &mut self,
        ig: &Ignore,
        depth: usize,
        root_device: Option<u64>,
        listing: &Listing,
    ) -> WalkState {
        let mut files = Vec::new();
        let single = self.pool.queues.len() == 1;
        let base = std::mem::take(&mut self.buf);
        let mut path = base.clone();
        for ent in &listing.ents {
            if self.pool.is_quit() {
                return WalkState::Quit;
            }
            path.clone_from(&base);
            path.push(listing.name(ent));
            match self.generate(ig, depth, root_device, &path, ent) {
                Ok(Some(work)) => {
                    if single || work.dent.is_dir() {
                        self.pool.push(self.me, Job::Entry(work));
                    } else {
                        files.push(work.dent);
                    }
                }
                Ok(None) => {}
                Err(state) => {
                    if state.is_quit() {
                        return state;
                    }
                }
            }
        }
        self.buf = base;
        if let Some(err) = &listing.err {
            let err = Error::Io(clone_io(err)).with_depth(depth);
            if self.visit(Err(err)).is_quit() {
                return WalkState::Quit;
            }
        }
        self.visit_files(files)
    }

    fn visit_files(&mut self, mut files: Vec<DirEntry>) -> WalkState {
        let start = Instant::now();
        let mut visited: u32 = 0;
        while let Some(dent) = files.pop() {
            if self.pool.is_quit() {
                return WalkState::Quit;
            }
            if visited > 0
                && (visited < 8 || visited.is_multiple_of(8))
                && files.len() > 1
                && self.pool.has_idle()
                && (start.elapsed() / visited)
                    .checked_mul(u32::try_from(files.len()).unwrap_or(u32::MAX))
                    .is_none_or(|cost| cost > SPLIT_COST)
            {
                let half = files.split_off(files.len() / 2);
                self.pool.push(self.me, Job::Files(half));
            }
            visited += 1;
            if self.should_visit(dent.depth()) && self.visit(Ok(dent)).is_quit() {
                return WalkState::Quit;
            }
        }
        WalkState::Continue
    }

    fn generate(
        &mut self,
        ig: &Ignore,
        depth: usize,
        root_device: Option<u64>,
        path: &Path,
        ent: &fsys::Ent,
    ) -> Result<Option<Work>, WalkState> {
        let mut ty = match ent.ty {
            Some(ty) => ty,
            None => match fs::symlink_metadata(path) {
                Ok(md) => md.file_type().into(),
                Err(err) => {
                    let err = Error::Io(err).with_path(path).with_depth(depth);
                    return Err(self.visit(Err(err)));
                }
            },
        };
        let mut ino = ent.ino;
        let mut attr_hidden = ent.attr_hidden;
        let mut follow_link = false;
        if self.opts.follow_links && ty.is_symlink() {
            match fs::metadata(path) {
                Ok(md) => {
                    ty = md.file_type().into();
                    ino = ino_of(&md);
                    attr_hidden = fsys::attr_hidden(&md);
                    follow_link = true;
                }
                Err(err) => return Err(self.visit(Err(Error::Io(err).with_path(path)))),
            }
            if ty.is_dir()
                && let Err(err) = check_symlink_loop(ig, path, depth)
            {
                return Err(self.visit(Err(err)));
            }
        }
        let is_dir = ty.is_dir();
        if ig.should_skip(path, is_dir, attr_hidden) {
            return Ok(None);
        }
        let dent = DirEntry {
            path: path.to_path_buf(),
            ty: Some(ty),
            follow_link,
            depth,
            ino,
            attr_hidden,
            walkdir: false,
            err: None,
        };
        if let Some(stdout) = &self.opts.skip {
            match path_equals(&dent, stdout) {
                Ok(true) => return Ok(None),
                Ok(false) => {}
                Err(err) => return Err(self.visit(Err(err))),
            }
        }
        if let Some(max) = self.opts.max_filesize
            && !is_dir
        {
            let md = if follow_link {
                fs::metadata(path)
            } else {
                fs::symlink_metadata(path)
            };
            if skip_filesize(max, md.ok().as_ref()) {
                return Ok(None);
            }
        }
        if let Some(Filter(filter)) = &self.opts.filter
            && !filter(&dent)
        {
            return Ok(None);
        }
        Ok(Some(Work {
            dent,
            ignore: ig.clone(),
            root_device,
        }))
    }
}

fn clone_io(err: &io::Error) -> io::Error {
    match err.raw_os_error() {
        Some(code) => io::Error::from_raw_os_error(code),
        None => io::Error::new(err.kind(), err.to_string()),
    }
}

fn check_symlink_loop(
    ig_parent: &Ignore,
    child_path: &Path,
    child_depth: usize,
) -> Result<(), Error> {
    let tag = |err: io::Error| {
        Error::from(err)
            .with_path(child_path)
            .with_depth(child_depth)
    };
    let hchild = Handle::from_path(child_path).map_err(tag)?;
    for ig in ig_parent
        .parents()
        .take_while(|ig| !ig.is_absolute_parent())
    {
        let h = Handle::from_path(ig.path()).map_err(tag)?;
        if hchild == h {
            return Err(Error::Loop {
                ancestor: ig.path().to_path_buf(),
                child: child_path.to_path_buf(),
            }
            .with_depth(child_depth));
        }
    }
    Ok(())
}

impl WalkParallel {
    pub fn run<'s, F>(self, mkf: F)
    where
        F: FnMut() -> FnVisitor<'s>,
    {
        self.visit(&mut FnBuilder { builder: mkf });
    }

    pub fn visit(self, builder: &mut dyn ParallelVisitorBuilder<'_>) {
        let threads = self.threads();
        let mut roots = vec![];
        {
            let mut visitor = builder.build();
            for path in self.paths {
                let (dent, root_device) = if path == Path::new("-") {
                    (DirEntry::new_stdin(), None)
                } else {
                    let root_device = if self.same_file_system {
                        match fsys::device_num(&path) {
                            Ok(dev) => Some(dev),
                            Err(err) => {
                                if visitor.visit(Err(Error::Io(err).with_path(path))).is_quit() {
                                    return;
                                }
                                continue;
                            }
                        }
                    } else {
                        None
                    };
                    match DirEntry::from_path(0, path, false) {
                        Ok(dent) => (dent, root_device),
                        Err(err) => {
                            if visitor.visit(Err(err)).is_quit() {
                                return;
                            }
                            continue;
                        }
                    }
                };
                roots.push(Work {
                    dent,
                    ignore: self.ig_root.clone(),
                    root_device,
                });
            }
            if roots.is_empty() {
                return;
            }
        }
        let pool = Pool::new(threads);
        for (i, work) in roots.into_iter().enumerate() {
            pool.push(i % threads, Job::Entry(work));
        }
        let opts = Opts {
            max_depth: self.max_depth,
            min_depth: self.min_depth,
            max_filesize: self.max_filesize,
            follow_links: self.follow_links,
            skip: self.skip,
            filter: self.filter,
        };
        let mut workers: Vec<Worker<'_, '_>> = (0..threads)
            .map(|me| Worker {
                visitor: builder.build(),
                pool: &pool,
                opts: &opts,
                me,
                buf: PathBuf::new(),
            })
            .collect();
        if threads == 1 {
            workers[0].run();
            return;
        }
        std::thread::scope(|s| {
            for mut worker in workers.drain(..) {
                s.spawn(move || worker.run());
            }
        });
    }

    fn threads(&self) -> usize {
        if self.threads == 0 {
            std::thread::available_parallelism().map_or(1, |n| tree_threads(n.get().min(12)))
        } else {
            self.threads
        }
    }
}

const SPLIT_COST: Duration = Duration::from_micros(200);

#[cfg(target_os = "macos")]
#[must_use]
pub fn tree_threads(n: usize) -> usize {
    n.min(6)
}

#[cfg(not(target_os = "macos"))]
#[must_use]
pub fn tree_threads(n: usize) -> usize {
    n
}

#[cfg(target_os = "macos")]
fn dir_read_limit() -> usize {
    8
}

#[cfg(not(target_os = "macos"))]
fn dir_read_limit() -> usize {
    usize::MAX
}
