use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static FORCE_DIR_ENTRY_ERR: Cell<bool> = const { Cell::new(false) };
    static FORCE_ENTRY_TYPE_ERR: Cell<bool> = const { Cell::new(false) };
}

use crate::hc_internal::gitconfig::{RepoOpts, repo_sources};
use crate::hc_internal::gnu::output::strerror;
use crate::hc_internal::ignore::{Ignore, load_ignore};
use crate::hc_internal::nfc;

const AHEAD_LIMIT: usize = 4096;
const INODE_SORT_THRESHOLD: usize = 10_000;
pub(crate) const GNU_FIND: bool = !cfg!(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
));

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Follow {
    Never,
    Cli,
    Always,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Dir,
    Link,
    Other,
}

pub(crate) struct Item<'a> {
    pub(crate) path: &'a Path,
    pub(crate) kind: Kind,
    pub(crate) meta: Option<&'a fs::Metadata>,
}

#[derive(Clone, Copy)]
pub(crate) struct WalkCfg {
    pub(crate) follow: Follow,
    pub(crate) gitignore: bool,
    pub(crate) mindepth: usize,
    pub(crate) maxdepth: Option<usize>,
    pub(crate) need_meta: bool,
}

struct Anc {
    prev: Option<Arc<Anc>>,
    id: (u64, u64),
    path: PathBuf,
}

impl Anc {
    fn contains(&self, id: (u64, u64)) -> Option<&Path> {
        let mut cur = Some(self);
        while let Some(a) = cur {
            if a.id == id {
                return Some(&a.path);
            }
            cur = a.prev.as_deref();
        }
        None
    }
}

struct Node {
    path: PathBuf,
    depth: usize,
    ignore_rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
    ancestors: Option<Arc<Anc>>,
    repo: Option<PathBuf>,
}

pub(crate) struct Sink<'a> {
    errors: &'a AtomicBool,
    pub(crate) out: Vec<u8>,
    events: Vec<Event>,
}

struct Event {
    at: usize,
    msg: Option<String>,
    next: Option<Arc<Slot>>,
}

impl Sink<'_> {
    pub(crate) fn new(errors: &AtomicBool) -> Sink<'_> {
        Sink {
            errors,
            out: Vec::new(),
            events: Vec::new(),
        }
    }

    fn message(&mut self, msg: String) {
        self.errors.store(true, Ordering::Relaxed);
        self.events.push(Event {
            at: self.out.len(),
            msg: Some(msg),
            next: None,
        });
    }

    pub(crate) fn report(&mut self, path: &Path, e: &io::Error) {
        self.message(format!(
            "{}: {}: {}",
            super::prog(),
            path.display(),
            strerror(e)
        ));
    }

    pub(crate) fn emit(&mut self, bytes: &[u8], term: u8) {
        self.out.extend_from_slice(bytes);
        self.out.push(term);
    }
}

struct Listing {
    out: Vec<u8>,
    events: Vec<Event>,
    queued: bool,
}

impl From<Sink<'_>> for Listing {
    fn from(sink: Sink<'_>) -> Listing {
        Listing {
            out: sink.out,
            events: sink.events,
            queued: false,
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Slot {
    claimed: AtomicBool,
    node: Mutex<Option<Node>>,
    value: Mutex<Option<Listing>>,
    ready: Condvar,
}

impl Slot {
    fn new(node: Node) -> Arc<Slot> {
        Arc::new(Slot {
            claimed: AtomicBool::new(false),
            node: Mutex::new(Some(node)),
            value: Mutex::new(None),
            ready: Condvar::new(),
        })
    }

    fn claim(&self) -> Option<Node> {
        if self.claimed.swap(true, Ordering::AcqRel) {
            None
        } else {
            lock(&self.node).take()
        }
    }

    fn put(&self, listing: Listing) {
        *lock(&self.value) = Some(listing);
        self.ready.notify_all();
    }

    fn wait(&self) -> Listing {
        let mut value = lock(&self.value);
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

fn kind_from_meta(m: &fs::Metadata) -> Kind {
    kind_from_ft(m.file_type())
}

fn kind_from_ft(t: fs::FileType) -> Kind {
    if t.is_dir() {
        Kind::Dir
    } else if t.is_file() {
        Kind::File
    } else if t.is_symlink() {
        Kind::Link
    } else {
        Kind::Other
    }
}

#[cfg(unix)]
fn file_id(m: &fs::Metadata, _: &Path) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

#[cfg(not(unix))]
fn file_id(_: &fs::Metadata, path: &Path) -> (u64, u64) {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    (
        0,
        BuildHasherDefault::<DefaultHasher>::default().hash_one(real),
    )
}

#[cfg(unix)]
fn entry_ino(entry: &fs::DirEntry) -> u64 {
    std::os::unix::fs::DirEntryExt::ino(entry)
}

#[cfg(not(unix))]
fn entry_ino(_: &fs::DirEntry) -> u64 {
    0
}

fn report(path: &Path, e: &io::Error, errors: &AtomicBool) {
    errors.store(true, Ordering::Relaxed);
    eprintln!("{}: {}: {}", super::prog(), path.display(), strerror(e));
}

fn loop_message(here: &Path, there: &Path) -> String {
    format!(
        "{}: File system loop detected; '{}' is part of the same file system loop as '{}'.",
        super::prog(),
        here.display(),
        there.display()
    )
}

fn should_descend(maxdepth: Option<usize>, depth: usize) -> bool {
    maxdepth.is_none_or(|m| depth < m)
}

fn push_rel_component(rel: &mut Vec<u8>, name: &OsStr, opts: RepoOpts) {
    let raw = name.as_encoded_bytes();
    if !rel.is_empty() {
        rel.push(b'/');
    }
    let head = rel.len();
    match opts.precompose.then(|| nfc::precomposed(raw)).flatten() {
        Some(text) => rel.extend_from_slice(&text),
        None => rel.extend_from_slice(raw),
    }
    if opts.fold {
        rel[head..].make_ascii_lowercase();
    }
}

fn child_rel(parent: &[u8], name: &OsStr, opts: RepoOpts) -> Vec<u8> {
    let raw = name.as_encoded_bytes();
    let mut rel = Vec::with_capacity(parent.len() + raw.len() + 1);
    rel.extend_from_slice(parent);
    push_rel_component(&mut rel, name, opts);
    rel
}

fn is_dot_git(name: &[u8], fold: bool) -> bool {
    if fold {
        name.eq_ignore_ascii_case(b".git")
    } else {
        name == b".git"
    }
}

fn start_abs(path: &Path, follow: bool) -> Option<PathBuf> {
    if follow {
        return path.canonicalize().ok();
    }
    match path.file_name() {
        None => path.canonicalize().ok(),
        Some(name) => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Some(parent.canonicalize().ok()?.join(name))
        }
    }
}

fn plain_node(path: PathBuf) -> Node {
    Node {
        path,
        depth: 0,
        ignore_rel: Vec::new(),
        ignore: None,
        opts: RepoOpts::default(),
        in_repo: false,
        ancestors: None,
        repo: None,
    }
}

fn root_ignore(path: &Path, is_dir: bool, follow: bool, errors: &AtomicBool) -> Option<Node> {
    let Some(abs) = start_abs(path, follow) else {
        return Some(plain_node(path.to_path_buf()));
    };
    if abs.join(".git").exists() {
        let (seed, opts) = repo_sources(&abs, errors, false, super::prog());
        return Some(Node {
            path: path.to_path_buf(),
            depth: 0,
            ignore_rel: Vec::new(),
            ignore: seed,
            opts,
            in_repo: true,
            ancestors: None,
            repo: Some(abs),
        });
    }
    let mut chain = Vec::new();
    let mut repo = None;
    for dir in abs.ancestors().skip(1) {
        chain.push(dir);
        if dir.join(".git").exists() {
            repo = Some(dir);
            break;
        }
    }
    let (Some(root), Some(name)) = (repo, abs.file_name()) else {
        return Some(plain_node(path.to_path_buf()));
    };
    let (seed, opts) = repo_sources(root, errors, false, super::prog());
    let mut rel = Vec::new();
    let mut ignore = seed;
    for (idx, dir) in chain.iter().rev().enumerate() {
        if idx > 0 {
            rel = child_rel(&rel, dir.file_name().unwrap_or(OsStr::new("")), opts);
        }
        let base = if rel.is_empty() { 0 } else { rel.len() + 1 };
        ignore = load_ignore(
            &dir.join(".gitignore"),
            base,
            ignore,
            opts.fold,
            errors,
            false,
            super::prog(),
        );
    }
    let rel = child_rel(&rel, name, opts);
    if ignore.as_ref().is_some_and(|ig| ig.ignored(&rel, is_dir)) {
        return None;
    }
    Some(Node {
        path: path.to_path_buf(),
        depth: 0,
        ignore_rel: rel,
        ignore,
        opts,
        in_repo: true,
        ancestors: None,
        repo: Some(root.to_path_buf()),
    })
}

fn resolve(path: &Path, follow: bool, errors: &AtomicBool) -> Option<(fs::Metadata, Kind)> {
    if !follow {
        return match fs::symlink_metadata(path) {
            Ok(m) => {
                let kind = kind_from_meta(&m);
                Some((m, kind))
            }
            Err(e) => {
                report(path, &e, errors);
                None
            }
        };
    }
    match fs::metadata(path) {
        Ok(m) => {
            let kind = kind_from_meta(&m);
            Some((m, kind))
        }
        Err(e) => match fs::symlink_metadata(path) {
            Ok(m) => {
                let kind = kind_from_meta(&m);
                if kind == Kind::Link {
                    report(path, &e, errors);
                }
                Some((m, kind))
            }
            Err(e2) => {
                report(path, &e2, errors);
                None
            }
        },
    }
}

fn classify(
    path: &Path,
    ft: fs::FileType,
    follow: bool,
    need_meta: bool,
    sink: &mut Sink<'_>,
) -> (Option<fs::Metadata>, Kind) {
    if follow && ft.is_symlink() {
        match fs::metadata(path) {
            Ok(m) => {
                let kind = kind_from_meta(&m);
                (Some(m), kind)
            }
            Err(e) => {
                sink.report(path, &e);
                match fs::symlink_metadata(path) {
                    Ok(m) => (Some(m), Kind::Link),
                    Err(e2) => {
                        sink.report(path, &e2);
                        (None, Kind::Link)
                    }
                }
            }
        }
    } else {
        let kind = kind_from_ft(ft);
        if !need_meta && (kind != Kind::Dir || !follow) {
            return (None, kind);
        }
        match fs::symlink_metadata(path) {
            Ok(m) => (Some(m), kind),
            Err(e) => {
                sink.report(path, &e);
                (None, kind)
            }
        }
    }
}

fn push_anc(parent: Option<&Arc<Anc>>, id: (u64, u64), path: PathBuf) -> Arc<Anc> {
    Arc::new(Anc {
        prev: parent.cloned(),
        id,
        path,
    })
}

struct Ent {
    name: OsString,
    ft: fs::FileType,
    ino: u64,
}

fn read_entries(path: &Path, sink: &mut Sink<'_>) -> Vec<Ent> {
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => {
            sink.report(path, &e);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for ent in rd {
        #[cfg(test)]
        let ent = if FORCE_DIR_ENTRY_ERR.with(|f| f.replace(false)) {
            Err(io::Error::other("forced"))
        } else {
            ent
        };
        match ent {
            Ok(e) => {
                if let Some(ft) = entry_type(&e, sink) {
                    out.push(Ent {
                        ino: entry_ino(&e),
                        name: e.file_name(),
                        ft,
                    });
                }
            }
            Err(e) => sink.report(path, &e),
        }
    }
    if GNU_FIND && out.len() > INODE_SORT_THRESHOLD {
        out.sort_by_key(|e| e.ino);
    }
    out
}

fn entry_type(entry: &fs::DirEntry, sink: &mut Sink<'_>) -> Option<fs::FileType> {
    #[cfg(test)]
    let typed = if FORCE_ENTRY_TYPE_ERR.with(|f| f.replace(false)) {
        Err(io::Error::other("forced"))
    } else {
        entry.file_type()
    };
    #[cfg(not(test))]
    let typed = entry.file_type();
    match typed {
        Ok(ft) => Some(ft),
        Err(e) => {
            sink.report(&entry.path(), &e);
            None
        }
    }
}

fn rel_from_path(path: &Path, opts: RepoOpts) -> Vec<u8> {
    let mut rel = Vec::new();
    for comp in path.components() {
        if let std::path::Component::Normal(name) = comp {
            push_rel_component(&mut rel, name, opts);
        }
    }
    rel
}

fn skip_followed_dir(
    child: &Path,
    kind: Kind,
    was_link: bool,
    ignore: Option<&Ignore>,
    opts: RepoOpts,
    repo: Option<&Path>,
) -> bool {
    if !was_link || kind != Kind::Dir {
        return false;
    }
    let Ok(real) = child.canonicalize() else {
        return false;
    };
    if real
        .file_name()
        .is_some_and(|n| is_dot_git(n.as_encoded_bytes(), opts.fold))
    {
        return true;
    }
    let Some(repo) = repo else {
        return false;
    };
    let Ok(suffix) = real.strip_prefix(repo) else {
        return false;
    };
    ignore.is_some_and(|ig| ig.ignored(&rel_from_path(suffix, opts), true))
}

struct Scope<'n> {
    parent_rel: &'n [u8],
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
    repo: Option<PathBuf>,
}

struct Walk<'a, V> {
    cfg: &'a WalkCfg,
    errors: &'a AtomicBool,
    visit: &'a V,
    queue: Mutex<(Vec<Arc<Slot>>, bool)>,
    wake: Condvar,
    ahead: AtomicUsize,
}

impl<V: Fn(&Item<'_>, &mut Sink<'_>) + Sync> Walk<'_, V> {
    fn scope<'n>(&self, node: &'n Node, entries: &[Ent]) -> Scope<'n> {
        if !self.cfg.gitignore {
            return Scope {
                parent_rel: &[],
                ignore: None,
                opts: RepoOpts::default(),
                in_repo: false,
                repo: None,
            };
        }
        let always = self.cfg.follow == Follow::Always;
        let boundary = entries
            .iter()
            .any(|entry| entry.name.as_encoded_bytes() == b".git")
            .then(|| repo_sources(&node.path, self.errors, false, super::prog()));
        let (parent_rel, inherited, opts, in_repo, repo) = match boundary {
            Some((seed, opts)) => (
                &[][..],
                seed,
                opts,
                true,
                always.then(|| {
                    node.path
                        .canonicalize()
                        .unwrap_or_else(|_| node.path.clone())
                }),
            ),
            None => (
                &node.ignore_rel[..],
                node.ignore.clone(),
                node.opts,
                node.in_repo,
                if always { node.repo.clone() } else { None },
            ),
        };
        let ignore = if !in_repo {
            None
        } else if entries.iter().any(|entry| {
            let name = entry.name.as_encoded_bytes();
            if opts.fold {
                name.eq_ignore_ascii_case(b".gitignore")
            } else {
                name == b".gitignore"
            }
        }) {
            let base = if parent_rel.is_empty() {
                0
            } else {
                parent_rel.len() + 1
            };
            load_ignore(
                &node.path.join(".gitignore"),
                base,
                inherited,
                opts.fold,
                self.errors,
                false,
                super::prog(),
            )
        } else {
            inherited
        };
        Scope {
            parent_rel,
            ignore,
            opts,
            in_repo,
            repo,
        }
    }

    fn list(&self, node: &Node) -> Listing {
        let cfg = self.cfg;
        let mut sink = Sink::new(self.errors);
        let entries = read_entries(&node.path, &mut sink);
        let scope = self.scope(node, &entries);
        let follow_child = cfg.follow == Follow::Always;
        let depth = node.depth + 1;
        let mut child = node.path.clone();
        for entry in &entries {
            if cfg.gitignore && is_dot_git(entry.name.as_encoded_bytes(), scope.opts.fold) {
                continue;
            }
            child.push(&entry.name);
            let (meta, kind) = classify(&child, entry.ft, follow_child, cfg.need_meta, &mut sink);
            let rel = if cfg.gitignore {
                if skip_followed_dir(
                    &child,
                    kind,
                    entry.ft.is_symlink(),
                    scope.ignore.as_deref(),
                    scope.opts,
                    scope.repo.as_deref(),
                ) {
                    child.pop();
                    continue;
                }
                let rel = child_rel(scope.parent_rel, &entry.name, scope.opts);
                if let Some(ig) = &scope.ignore
                    && ig.ignored(&rel, kind == Kind::Dir)
                {
                    child.pop();
                    continue;
                }
                rel
            } else {
                Vec::new()
            };
            if depth >= cfg.mindepth {
                (self.visit)(
                    &Item {
                        path: &child,
                        kind,
                        meta: meta.as_ref(),
                    },
                    &mut sink,
                );
            }
            if kind == Kind::Dir && should_descend(cfg.maxdepth, depth) {
                let id = meta.as_ref().map(|m| file_id(m, &child));
                if let Some(hit) = id.and_then(|id| node.ancestors.as_ref()?.contains(id)) {
                    sink.message(loop_message(&child, hit));
                } else {
                    let next = Node {
                        path: child.clone(),
                        depth,
                        ignore_rel: rel,
                        ignore: scope.ignore.clone(),
                        opts: scope.opts,
                        in_repo: scope.in_repo,
                        ancestors: id
                            .map(|id| push_anc(node.ancestors.as_ref(), id, child.clone())),
                        repo: scope.repo.clone(),
                    };
                    sink.events.push(Event {
                        at: sink.out.len(),
                        msg: None,
                        next: Some(Slot::new(next)),
                    });
                }
            }
            child.pop();
        }
        Listing::from(sink)
    }

    fn queue_children(&self, listing: &mut Listing) {
        if listing.queued {
            return;
        }
        listing.queued = true;
        let mut queue = lock(&self.queue);
        if queue.1 {
            return;
        }
        let before = queue.0.len();
        queue.0.extend(
            listing
                .events
                .iter()
                .rev()
                .filter_map(|ev| ev.next.as_ref().map(Arc::clone)),
        );
        let added = queue.0.len() - before;
        drop(queue);
        match added {
            0 => {}
            1 => self.wake.notify_one(),
            _ => self.wake.notify_all(),
        }
    }

    fn pop(&self) -> Option<Arc<Slot>> {
        let mut queue = lock(&self.queue);
        loop {
            if queue.1 {
                return None;
            }
            if let Some(slot) = queue.0.pop() {
                return Some(slot);
            }
            queue = self
                .wake
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn close(&self) {
        let mut queue = lock(&self.queue);
        queue.1 = true;
        queue.0.clear();
        drop(queue);
        self.wake.notify_all();
    }

    fn helper(&self) {
        while let Some(slot) = self.pop() {
            if let Some(node) = slot.claim() {
                let mut listing = self.list(&node);
                if self.ahead.load(Ordering::Relaxed) < AHEAD_LIMIT {
                    self.queue_children(&mut listing);
                }
                self.ahead.fetch_add(1, Ordering::Relaxed);
                slot.put(listing);
            }
        }
    }

    fn take(&self, slot: &Slot) -> Listing {
        let mut listing = if let Some(node) = slot.claim() {
            self.list(&node)
        } else {
            let listing = slot.wait();
            self.ahead.fetch_sub(1, Ordering::Relaxed);
            listing
        };
        self.queue_children(&mut listing);
        listing
    }

    fn drain(&self, first: Listing, out: &mut dyn Write) -> io::Result<()> {
        let mut stack = vec![(first, 0usize, 0usize)];
        while let Some((listing, ev, pos)) = stack.last_mut() {
            let Some(event) = listing.events.get_mut(*ev) else {
                out.write_all(&listing.out[*pos..])?;
                stack.pop();
                continue;
            };
            *ev += 1;
            out.write_all(&listing.out[*pos..event.at])?;
            *pos = event.at;
            if let Some(msg) = event.msg.take() {
                out.flush()?;
                eprintln!("{msg}");
            }
            if let Some(next) = event.next.take() {
                let child = self.take(&next);
                stack.push((child, 0, 0));
            }
        }
        Ok(())
    }

    fn roots(&self, roots: &[OsString], out: &mut dyn Write) -> io::Result<()> {
        let cfg = self.cfg;
        let follow_root = matches!(cfg.follow, Follow::Cli | Follow::Always);
        for root in roots {
            let path = PathBuf::from(root);
            let Some((meta, kind)) = resolve(&path, follow_root, self.errors) else {
                continue;
            };
            let mut node = if cfg.gitignore {
                match root_ignore(&path, kind == Kind::Dir, follow_root, self.errors) {
                    None => continue,
                    Some(n) => n,
                }
            } else {
                plain_node(path.clone())
            };
            node.ancestors = Some(push_anc(None, file_id(&meta, &path), path.clone()));
            let mut sink = Sink::new(self.errors);
            if cfg.mindepth == 0 {
                (self.visit)(
                    &Item {
                        path: &path,
                        kind,
                        meta: Some(&meta),
                    },
                    &mut sink,
                );
            }
            if kind == Kind::Dir && should_descend(cfg.maxdepth, 0) {
                node.path = path;
                sink.events.push(Event {
                    at: sink.out.len(),
                    msg: None,
                    next: Some(Slot::new(node)),
                });
            }
            self.drain(Listing::from(sink), out)?;
        }
        Ok(())
    }
}

pub(crate) fn for_each<V>(
    roots: &[OsString],
    cfg: &WalkCfg,
    errors: &AtomicBool,
    out: &mut dyn Write,
    visit: &V,
) -> io::Result<()>
where
    V: Fn(&Item<'_>, &mut Sink<'_>) + Sync,
{
    let walk = Walk {
        cfg,
        errors,
        visit,
        queue: Mutex::new((Vec::new(), false)),
        wake: Condvar::new(),
        ahead: AtomicUsize::new(0),
    };
    let helpers = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
    std::thread::scope(|scope| {
        let walk = &walk;
        for _ in 0..helpers {
            scope.spawn(move || walk.helper());
        }
        let result = walk.roots(roots, out);
        walk.close();
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_walk(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hfind_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cfg(follow: Follow, gitignore: bool) -> WalkCfg {
        WalkCfg {
            follow,
            gitignore,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
        }
    }

    fn print(item: &Item<'_>, sink: &mut Sink<'_>) {
        sink.emit(item.path.as_os_str().as_encoded_bytes(), b'\n');
    }

    fn run(roots: &[PathBuf], cfg: &WalkCfg, errors: &AtomicBool) -> String {
        let mut out = Vec::new();
        let roots: Vec<OsString> = roots.iter().map(|p| p.clone().into_os_string()).collect();
        for_each(&roots, cfg, errors, &mut out, &print).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn preorder(dir: &Path, out: &mut String) {
        out.push_str(&dir.display().to_string());
        out.push('\n');
        if fs::symlink_metadata(dir).unwrap().is_dir() {
            for entry in fs::read_dir(dir).unwrap() {
                preorder(&entry.unwrap().path(), out);
            }
        }
    }

    #[test]
    fn nfc_ascii_and_invalid() {
        assert!(nfc::precomposed(b"ascii").is_none());
        let _ = nfc::precomposed(&[0xff, 0xff, 0xff]);
        let _ = nfc::precomposed("e\u{0301}".as_bytes());
    }

    #[test]
    fn output_is_readdir_preorder() {
        let dir = scratch_walk("preorder");
        for rel in ["b/x", "a/y/z", "c", "a/w", "d/e/f/g", "b/q"] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"").unwrap();
        }
        let mut want = String::new();
        preorder(&dir, &mut want);
        let errors = AtomicBool::new(false);
        for _ in 0..20 {
            assert_eq!(
                run(
                    std::slice::from_ref(&dir),
                    &cfg(Follow::Never, false),
                    &errors
                ),
                want
            );
        }
        let twice = run(
            &[dir.join("b"), dir.join("a")],
            &cfg(Follow::Never, false),
            &errors,
        );
        let mut want_b = String::new();
        preorder(&dir.join("b"), &mut want_b);
        preorder(&dir.join("a"), &mut want_b);
        assert_eq!(twice, want_b);
        assert!(!errors.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classify_stats_directories_without_need_meta() {
        let dir = scratch_walk("classify");
        let ft = fs::symlink_metadata(&dir).unwrap().file_type();
        let errors = AtomicBool::new(false);
        let mut sink = Sink::new(&errors);
        let (meta, kind) = classify(&dir, ft, false, false, &mut sink);
        assert!(meta.is_none());
        assert!(kind == Kind::Dir);
        assert!(!errors.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn follow_reports_directory_cycle_once() {
        let dir = scratch_walk("cycle");
        fs::create_dir(dir.join("sub")).unwrap();
        std::os::unix::fs::symlink("..", dir.join("sub/up")).unwrap();
        let errors = AtomicBool::new(false);
        let out = run(
            std::slice::from_ref(&dir),
            &cfg(Follow::Always, false),
            &errors,
        );
        assert!(errors.load(Ordering::Relaxed));
        assert_eq!(out.lines().count(), 3, "{out}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classify_reports_vanished_paths() {
        let dir = scratch_walk("vanish");
        let link = dir.join("sl");
        std::os::unix::fs::symlink("missing", &link).unwrap();
        let ft = fs::symlink_metadata(&link).unwrap().file_type();
        fs::remove_file(&link).unwrap();
        let errors = AtomicBool::new(false);
        let mut sink = Sink::new(&errors);
        let (meta, kind) = classify(&link, ft, true, false, &mut sink);
        assert!(meta.is_none());
        assert!(kind == Kind::Link);
        assert!(errors.load(Ordering::Relaxed));
        assert_eq!(sink.events.len(), 2);

        let file = dir.join("gone");
        fs::write(&file, b"x").unwrap();
        let ft = fs::symlink_metadata(&file).unwrap().file_type();
        fs::remove_file(&file).unwrap();
        let errors = AtomicBool::new(false);
        let mut sink = Sink::new(&errors);
        let (meta, kind) = classify(&file, ft, false, true, &mut sink);
        assert!(meta.is_none());
        assert!(kind == Kind::File);
        assert!(errors.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn skip_followed_dir_edges() {
        let dir = scratch_walk("skip_follow");
        assert!(!skip_followed_dir(
            Path::new("/hfind-no-such-dir"),
            Kind::Dir,
            true,
            None,
            RepoOpts::default(),
            Some(Path::new("/tmp")),
        ));

        let real = dir.join("real");
        fs::create_dir(&real).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink("real", &link).unwrap();
        assert!(!skip_followed_dir(
            &link,
            Kind::Dir,
            true,
            None,
            RepoOpts::default(),
            None,
        ));

        let repo = dir.join("repo");
        fs::create_dir(&repo).unwrap();
        assert!(!skip_followed_dir(
            &link,
            Kind::Dir,
            true,
            None,
            RepoOpts::default(),
            Some(&repo),
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn root_ignore_falls_back_when_abs_fails() {
        let errors = AtomicBool::new(false);
        let missing_parent = root_ignore(
            Path::new("/hfind-no-such-parent/child"),
            false,
            false,
            &errors,
        )
        .unwrap();
        assert!(!missing_parent.in_repo);
        let missing_follow =
            root_ignore(Path::new("/hfind-no-such-root"), false, true, &errors).unwrap();
        assert!(!missing_follow.in_repo);
    }

    #[test]
    fn read_entries_reports_forced_errors() {
        let dir = scratch_walk("scan_err");
        fs::write(dir.join("a"), b"").unwrap();

        let errors = AtomicBool::new(false);
        let mut sink = Sink::new(&errors);
        assert_eq!(read_entries(&dir, &mut sink).len(), 1);
        assert!(!errors.load(Ordering::Relaxed));

        FORCE_DIR_ENTRY_ERR.with(|f| f.set(true));
        let mut sink = Sink::new(&errors);
        let _ = read_entries(&dir, &mut sink);
        assert!(errors.load(Ordering::Relaxed));

        let errors = AtomicBool::new(false);
        FORCE_ENTRY_TYPE_ERR.with(|f| f.set(true));
        let mut sink = Sink::new(&errors);
        assert!(read_entries(&dir, &mut sink).is_empty());
        assert!(errors.load(Ordering::Relaxed));

        let errors = AtomicBool::new(false);
        let mut sink = Sink::new(&errors);
        assert!(read_entries(&dir.join("missing"), &mut sink).is_empty());
        assert!(errors.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn walk_helpers_and_for_each() {
        assert!(kind_from_ft(fs::symlink_metadata(".").unwrap().file_type()) == Kind::Dir);
        let dir = scratch_walk("foreach");
        fs::write(dir.join("a.txt"), b"").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/b.txt"), b"").unwrap();
        std::os::unix::fs::symlink("a.txt", dir.join("l")).unwrap();
        let fifo = dir.join("pipe");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("spawn mkfifo");
        assert!(status.success(), "mkfifo {}", fifo.display());
        let errors = AtomicBool::new(false);
        let ft = fs::symlink_metadata(&fifo).unwrap().file_type();
        assert!(kind_from_ft(ft) == Kind::Other);
        assert!(kind_from_meta(&fs::symlink_metadata(&fifo).unwrap()) == Kind::Other);
        assert!(is_dot_git(b".git", false));
        assert!(is_dot_git(b".GIT", true));
        assert!(!is_dot_git(b".GIT", false));
        let folded = child_rel(
            b"a",
            OsStr::new("B"),
            RepoOpts {
                fold: true,
                precompose: false,
            },
        );
        assert_eq!(folded, b"a/b");
        let first = child_rel(b"", OsStr::new("x"), RepoOpts::default());
        assert_eq!(first, b"x");
        assert!(start_abs(Path::new("/"), false).is_some());
        assert!(start_abs(Path::new("."), false).is_some());
        let a1 = push_anc(None, (1, 1), dir.clone());
        let a2 = push_anc(Some(&a1), (2, 2), dir.join("sub"));
        assert!(a2.contains((1, 1)).is_some());
        assert!(a2.contains((9, 9)).is_none());
        assert_eq!(
            rel_from_path(Path::new("/a/b"), RepoOpts::default()),
            b"a/b"
        );

        fs::create_dir_all(dir.join("repo/.git")).unwrap();
        fs::write(dir.join("repo/.gitignore"), b"*.log\nbuild/\n").unwrap();
        fs::create_dir_all(dir.join("repo/a/b")).unwrap();
        fs::write(dir.join("repo/a/.gitignore"), b"*.tmp\n").unwrap();
        fs::create_dir(dir.join("repo/build")).unwrap();
        fs::write(dir.join("repo/a.log"), b"").unwrap();
        let at_root = root_ignore(&dir.join("repo"), true, false, &errors).unwrap();
        assert!(at_root.in_repo);
        let _ = root_ignore(&dir.join("repo"), true, true, &errors);
        let deep = root_ignore(&dir.join("repo/a/b"), true, false, &errors).unwrap();
        assert!(deep.in_repo);
        assert_ne!(deep.ignore_rel.len(), 0);
        let _ = root_ignore(&dir.join("repo/build"), true, false, &errors);

        let link = dir.join("todir");
        std::os::unix::fs::symlink("sub", &link).unwrap();
        let ft = fs::symlink_metadata(&link).unwrap().file_type();
        let mut sink = Sink::new(&errors);
        let (meta, kind) = classify(&link, ft, true, false, &mut sink);
        assert!(kind == Kind::Dir);
        assert!(meta.is_some());
        let ft = fs::symlink_metadata(dir.join("a.txt")).unwrap().file_type();
        let (meta, kind) = classify(&dir.join("a.txt"), ft, false, false, &mut sink);
        assert!(kind == Kind::File);
        assert!(meta.is_none());

        let git = dir.join("githole");
        fs::create_dir_all(git.join(".git")).unwrap();
        let gitlink = git.join("g");
        std::os::unix::fs::symlink(".git", &gitlink).unwrap();
        assert!(skip_followed_dir(
            &gitlink,
            Kind::Dir,
            true,
            None,
            RepoOpts {
                fold: true,
                precompose: false
            },
            Some(&git),
        ));
        let _ = skip_followed_dir(
            &dir.join("repo/build"),
            Kind::Dir,
            true,
            None,
            RepoOpts::default(),
            Some(&dir.join("repo")),
        );

        let mut all = cfg(Follow::Never, false);
        all.need_meta = true;
        assert!(
            run(std::slice::from_ref(&dir), &all, &errors)
                .lines()
                .count()
                >= 2
        );
        let repo = run(
            &[dir.join("repo")],
            &WalkCfg {
                follow: Follow::Always,
                gitignore: true,
                mindepth: 1,
                maxdepth: Some(2),
                need_meta: false,
            },
            &errors,
        );
        assert!(repo.contains("repo/a/b"), "{repo}");
        let shallow = WalkCfg {
            follow: Follow::Cli,
            gitignore: false,
            mindepth: 0,
            maxdepth: Some(0),
            need_meta: false,
        };
        assert_eq!(
            run(&[dir.join("a.txt")], &shallow, &errors),
            format!("{}\n", dir.join("a.txt").display())
        );
        let missing = AtomicBool::new(false);
        assert_eq!(
            run(
                &[PathBuf::from("/hfind-no-such-walk-root")],
                &shallow,
                &missing
            ),
            ""
        );
        assert!(missing.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }
}
