use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use crate::hc_internal::gnu::exclude::Excludes;
use crate::hc_internal::walker::DirEntry;

use super::flags::os_bytes;
use super::queue::WorkQueue;

const INODE_SORT_THRESHOLD: usize = 10_000;

pub(crate) enum Step {
    Entry(DirEntry),
    Error(String),
    Warning(String),
    Boundary,
}

struct Child {
    name: OsString,
    ty: fs::FileType,
    ino: u64,
}

type Listing = io::Result<Vec<Child>>;

#[cfg(unix)]
fn child_ino(entry: &fs::DirEntry) -> u64 {
    std::os::unix::fs::DirEntryExt::ino(entry)
}

#[cfg(not(unix))]
fn child_ino(_: &fs::DirEntry) -> u64 {
    0
}

#[cfg(unix)]
fn dir_id(md: &fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (md.dev(), md.ino())
}

#[cfg(not(unix))]
fn dir_id(_: &fs::Metadata) -> (u64, u64) {
    (0, 0)
}

fn read_listing(path: &Path) -> Listing {
    let mut children = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        children.push(Child {
            ty: entry.file_type()?,
            ino: child_ino(&entry),
            name: entry.file_name(),
        });
    }
    if children.len() > INODE_SORT_THRESHOLD {
        children.sort_by_key(|child| child.ino);
    }
    Ok(children)
}

struct Slot {
    claimed: AtomicBool,
    value: Mutex<Option<Listing>>,
    ready: Condvar,
}

impl Slot {
    fn claim(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }

    fn fill(&self, path: &Path) {
        let listing = read_listing(path);
        *self.value.lock().unwrap_or_else(PoisonError::into_inner) = Some(listing);
        self.ready.notify_all();
    }

    fn take(&self, path: &Path) -> Listing {
        if self.claim() {
            return read_listing(path);
        }
        let mut value = self.value.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(listing) = value.take() {
                return listing;
            }
            value = self
                .ready
                .wait(value)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

type Request = (PathBuf, Arc<Slot>);

struct Frame {
    path: PathBuf,
    depth: usize,
    id: (u64, u64),
    children: Vec<Child>,
    pending: Vec<Option<Arc<Slot>>>,
    next: usize,
}

pub(crate) struct Walker<'a> {
    pub(crate) threads: usize,
    pub(crate) deref: bool,
    pub(crate) excludes: Option<&'a Excludes>,
    pub(crate) strip_dot: bool,
}

impl Walker<'_> {
    fn shown(&self, path: &Path) -> String {
        let text = path.display().to_string();
        match text.strip_prefix("./") {
            Some(rest) if self.strip_dot && !rest.is_empty() => rest.to_string(),
            _ => text,
        }
    }

    fn failure(&self, path: &Path, err: &io::Error) -> Step {
        Step::Error(format!(
            "{}: {}",
            self.shown(path),
            super::messages::strerror(err)
        ))
    }

    fn frame(
        &self,
        queue: &WorkQueue<Request>,
        path: PathBuf,
        depth: usize,
        id: (u64, u64),
        children: Vec<Child>,
    ) -> Frame {
        let pending: Vec<Option<Arc<Slot>>> = children
            .iter()
            .map(|child| {
                let wanted = child.ty.is_dir()
                    && self
                        .excludes
                        .is_none_or(|ex| !ex.skip_dir(os_bytes(&child.name), false));
                wanted.then(|| {
                    Arc::new(Slot {
                        claimed: AtomicBool::new(false),
                        value: Mutex::new(None),
                        ready: Condvar::new(),
                    })
                })
            })
            .collect();
        for (child, slot) in children.iter().zip(&pending).rev() {
            if let Some(slot) = slot {
                queue.push((path.join(&child.name), Arc::clone(slot)));
            }
        }
        Frame {
            path,
            depth,
            id,
            children,
            pending,
            next: 0,
        }
    }

    pub(crate) fn walk(&self, root: &Path, emit: &mut dyn FnMut(Step) -> bool) {
        let queue: WorkQueue<Request> = WorkQueue::new();
        std::thread::scope(|scope| {
            for _ in 0..(self.threads / 4).clamp(1, 4) {
                scope.spawn(|| {
                    while let Some((path, slot)) = queue.pop_newest() {
                        if slot.claim() {
                            slot.fill(&path);
                        }
                    }
                });
            }
            self.walk_with(&queue, root, emit);
            queue.close();
        });
    }

    fn walk_with(
        &self,
        queue: &WorkQueue<Request>,
        root: &Path,
        emit: &mut dyn FnMut(Step) -> bool,
    ) {
        let id = fs::metadata(root).map(|md| dir_id(&md)).unwrap_or_default();
        let children = match read_listing(root) {
            Ok(children) => children,
            Err(err) => {
                emit(self.failure(root, &err));
                return;
            }
        };
        let mut stack = vec![self.frame(queue, root.to_path_buf(), 0, id, children)];
        while let Some(frame) = stack.last_mut() {
            let index = frame.next;
            let Some(child) = frame.children.get(index) else {
                stack.pop();
                continue;
            };
            frame.next += 1;
            let path = frame.path.join(&child.name);
            let depth = frame.depth + 1;
            let name = child.name.clone();
            let mut ty = child.ty;
            let mut ino = child.ino;
            let pending = frame.pending.get_mut(index).and_then(Option::take);
            let mut follow = false;
            let mut id = (0, 0);
            if ty.is_symlink() {
                if !self.deref {
                    continue;
                }
                match fs::metadata(&path) {
                    Ok(md) => {
                        ty = md.file_type();
                        id = dir_id(&md);
                        ino = id.1;
                        follow = true;
                    }
                    Err(err) => {
                        if !emit(self.failure(&path, &err)) {
                            return;
                        }
                        continue;
                    }
                }
            }
            if !ty.is_dir() {
                let skipped = self
                    .excludes
                    .is_some_and(|ex| ex.skip_file(os_bytes(&name), false));
                if !skipped
                    && !emit(Step::Entry(DirEntry::from_parts(
                        path, ty, depth, follow, ino,
                    )))
                {
                    return;
                }
                continue;
            }
            if self
                .excludes
                .is_some_and(|ex| ex.skip_dir(os_bytes(&name), false))
            {
                continue;
            }
            if self.deref {
                if !follow {
                    id = fs::metadata(&path)
                        .map(|md| dir_id(&md))
                        .unwrap_or_default();
                }
                if stack.iter().any(|f| f.id == id) {
                    let warning =
                        format!("{}: warning: recursive directory loop", self.shown(&path));
                    if !emit(Step::Warning(warning)) {
                        return;
                    }
                    continue;
                }
            }
            let listing = match pending {
                Some(slot) => slot.take(&path),
                None => read_listing(&path),
            };
            match listing {
                Ok(children) => {
                    if !emit(Step::Boundary) {
                        return;
                    }
                    stack.push(self.frame(queue, path, depth, id, children));
                }
                Err(err) => {
                    if !emit(self.failure(&path, &err)) {
                        return;
                    }
                }
            }
        }
    }
}
