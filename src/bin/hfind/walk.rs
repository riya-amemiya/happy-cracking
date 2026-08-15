use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static FORCE_DIR_ENTRY_ERR: Cell<bool> = const { Cell::new(false) };
    static FORCE_ENTRY_TYPE_ERR: Cell<bool> = const { Cell::new(false) };
}

use rayon::prelude::*;

use crate::gitconfig::{RepoOpts, repo_sources};
use crate::ignore::{Ignore, load_ignore};

#[cfg(target_os = "macos")]
mod nfc {
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::os::raw::{c_char, c_int};

    #[link(name = "iconv")]
    unsafe extern "C" {
        fn iconv_open(to: *const c_char, from: *const c_char) -> *mut c_void;
        fn iconv(
            cd: *mut c_void,
            input: *mut *const c_char,
            inleft: *mut usize,
            output: *mut *mut c_char,
            outleft: *mut usize,
        ) -> usize;
        fn iconv_close(cd: *mut c_void) -> c_int;
    }

    struct Conv(Cell<*mut c_void>);

    impl Drop for Conv {
        fn drop(&mut self) {
            let cd = self.0.get();
            if !cd.is_null() && cd as usize != usize::MAX {
                unsafe { iconv_close(cd) };
            }
        }
    }

    thread_local! {
        static CONV: Conv = const { Conv(Cell::new(std::ptr::null_mut())) };
    }

    #[cfg(test)]
    pub(super) static ICONV_FAIL: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    fn open_iconv() -> *mut c_void {
        #[cfg(test)]
        if ICONV_FAIL.load(std::sync::atomic::Ordering::Relaxed) {
            return usize::MAX as *mut c_void;
        }
        unsafe { iconv_open(c"UTF-8".as_ptr(), c"UTF-8-MAC".as_ptr()) }
    }

    pub fn precomposed(raw: &[u8]) -> Option<Vec<u8>> {
        if !raw.iter().any(|&b| b >= 0x80) {
            return None;
        }
        CONV.with(|conv| {
            let mut cd = conv.0.get();
            if cd.is_null() {
                cd = open_iconv();
                conv.0.set(cd);
            }
            if cd as usize == usize::MAX {
                return None;
            }
            let mut out = vec![0u8; raw.len() * 2 + 4];
            let mut input = raw.as_ptr() as *const c_char;
            let mut inleft = raw.len();
            let mut output = out.as_mut_ptr() as *mut c_char;
            let mut outleft = out.len();
            let rc = unsafe { iconv(cd, &mut input, &mut inleft, &mut output, &mut outleft) };
            if rc == usize::MAX || inleft != 0 {
                unsafe {
                    iconv(
                        cd,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                return None;
            }
            let used = out.len() - outleft;
            out.truncate(used);
            Some(out)
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod nfc {
    pub fn precomposed(_raw: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

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

pub(crate) struct Item {
    pub(crate) path: PathBuf,
    pub(crate) kind: Kind,
    pub(crate) meta: Option<fs::Metadata>,
}

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

struct Ctx<'a, F> {
    follow: Follow,
    gitignore: bool,
    mindepth: usize,
    maxdepth: Option<usize>,
    need_meta: bool,
    errors: &'a AtomicBool,
    visit: F,
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

fn file_id(m: &fs::Metadata) -> (u64, u64) {
    (m.dev(), m.ino())
}

fn report(path: &Path, e: impl std::fmt::Display, errors: &AtomicBool) {
    errors.store(true, Ordering::Relaxed);
    eprintln!("hfind: {}: {e}", path.display());
}

fn report_loop(here: &Path, there: &Path, errors: &AtomicBool) {
    errors.store(true, Ordering::Relaxed);
    eprintln!(
        "hfind: File system loop detected; '{}' is part of the same file system loop as '{}'.",
        here.display(),
        there.display()
    );
}

fn should_descend(maxdepth: Option<usize>, depth: usize) -> bool {
    maxdepth.is_none_or(|m| depth < m)
}

fn child_rel(parent: &[u8], name: &OsStr, opts: RepoOpts) -> Vec<u8> {
    let raw = name.as_bytes();
    let mut rel = Vec::with_capacity(parent.len() + raw.len() + 1);
    rel.extend_from_slice(parent);
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

fn root_ignore(path: &Path, is_dir: bool, follow: bool, errors: &AtomicBool) -> Option<Node> {
    let loose = |path: &Path| Node {
        path: path.to_path_buf(),
        depth: 0,
        ignore_rel: Vec::new(),
        ignore: None,
        opts: RepoOpts::default(),
        in_repo: false,
        ancestors: None,
        repo: None,
    };
    let Some(abs) = start_abs(path, follow) else {
        return Some(loose(path));
    };
    if abs.join(".git").exists() {
        let (seed, opts) = repo_sources(&abs, errors, false);
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
        return Some(loose(path));
    };
    let (seed, opts) = repo_sources(root, errors, false);
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

fn resolve(path: &Path, follow: bool, errors: &AtomicBool) -> Option<(Option<fs::Metadata>, Kind)> {
    if !follow {
        return match fs::symlink_metadata(path) {
            Ok(m) => Some((Some(m.clone()), kind_from_meta(&m))),
            Err(e) => {
                report(path, e, errors);
                None
            }
        };
    }
    match fs::metadata(path) {
        Ok(m) => Some((Some(m.clone()), kind_from_meta(&m))),
        Err(e) => match fs::symlink_metadata(path) {
            Ok(m) => {
                let kind = kind_from_meta(&m);
                if kind == Kind::Link {
                    report(path, e, errors);
                }
                Some((Some(m), kind))
            }
            Err(e2) => {
                report(path, e2, errors);
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
    errors: &AtomicBool,
) -> (Option<fs::Metadata>, Kind) {
    if follow && ft.is_symlink() {
        match fs::metadata(path) {
            Ok(m) => {
                let kind = kind_from_meta(&m);
                (Some(m), kind)
            }
            Err(e) => {
                report(path, e, errors);
                match fs::symlink_metadata(path) {
                    Ok(m) => (Some(m), Kind::Link),
                    Err(e2) => {
                        report(path, e2, errors);
                        (None, Kind::Link)
                    }
                }
            }
        }
    } else {
        let kind = kind_from_ft(ft);
        if !need_meta && kind != Kind::Dir {
            return (None, kind);
        }
        match fs::symlink_metadata(path) {
            Ok(m) => (Some(m), kind),
            Err(e) => {
                report(path, e, errors);
                (None, kind)
            }
        }
    }
}

fn consider<F: Fn(&Item)>(
    ctx: &Ctx<'_, F>,
    path: &Path,
    kind: Kind,
    meta: Option<&fs::Metadata>,
    depth: usize,
) {
    if depth < ctx.mindepth {
        return;
    }
    (ctx.visit)(&Item {
        path: path.to_path_buf(),
        kind,
        meta: meta.cloned(),
    });
}

fn push_anc(parent: Option<&Arc<Anc>>, id: (u64, u64), path: PathBuf) -> Arc<Anc> {
    Arc::new(Anc {
        prev: parent.cloned(),
        id,
        path,
    })
}

struct Child {
    path: PathBuf,
    depth: usize,
    kind: Kind,
    ignore_rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
    repo: Option<PathBuf>,
}

fn maybe_enqueue(
    ctx: &Ctx<'_, impl Fn(&Item)>,
    child: Child,
    meta: Option<&fs::Metadata>,
    parent: &Node,
    next: &mut Vec<Node>,
) {
    if child.kind != Kind::Dir || !should_descend(ctx.maxdepth, child.depth) {
        return;
    }
    if let Some(m) = meta
        && let Some(hit) = parent
            .ancestors
            .as_ref()
            .and_then(|a| a.contains(file_id(m)))
    {
        report_loop(&child.path, hit, ctx.errors);
        return;
    }
    let ancestors =
        meta.map(|m| push_anc(parent.ancestors.as_ref(), file_id(m), child.path.clone()));
    next.push(Node {
        path: child.path,
        depth: child.depth,
        ignore_rel: child.ignore_rel,
        ignore: child.ignore,
        opts: child.opts,
        in_repo: child.in_repo,
        ancestors,
        repo: child.repo,
    });
}

fn read_entries(path: &Path, errors: &AtomicBool) -> Vec<fs::DirEntry> {
    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(e) => {
            report(path, e, errors);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for ent in rd {
        #[cfg(test)]
        let ent = if FORCE_DIR_ENTRY_ERR.with(|f| f.replace(false)) {
            Err(std::io::Error::other("forced"))
        } else {
            ent
        };
        match ent {
            Ok(e) => out.push(e),
            Err(e) => report(path, e, errors),
        }
    }
    out
}

fn entry_type(entry: &fs::DirEntry, errors: &AtomicBool) -> Option<fs::FileType> {
    #[cfg(test)]
    let typed = if FORCE_ENTRY_TYPE_ERR.with(|f| f.replace(false)) {
        Err(std::io::Error::other("forced"))
    } else {
        entry.file_type()
    };
    #[cfg(not(test))]
    let typed = entry.file_type();
    match typed {
        Ok(ft) => Some(ft),
        Err(e) => {
            report(&entry.path(), e, errors);
            None
        }
    }
}

fn rel_from_path(path: &Path, opts: RepoOpts) -> Vec<u8> {
    let mut rel = Vec::new();
    for comp in path.components() {
        if let std::path::Component::Normal(name) = comp {
            rel = child_rel(&rel, name, opts);
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
        .is_some_and(|n| is_dot_git(n.as_bytes(), opts.fold))
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

fn scan<F: Fn(&Item)>(node: &Node, ctx: &Ctx<'_, F>) -> Vec<Node> {
    let entries = read_entries(&node.path, ctx.errors);
    let follow_child = ctx.follow == Follow::Always;
    let mut next = Vec::new();
    if !ctx.gitignore {
        for entry in entries {
            let Some(ft) = entry_type(&entry, ctx.errors) else {
                continue;
            };
            let child = entry.path();
            let (meta, kind) = classify(&child, ft, follow_child, ctx.need_meta, ctx.errors);
            consider(ctx, &child, kind, meta.as_ref(), node.depth + 1);
            maybe_enqueue(
                ctx,
                Child {
                    path: child,
                    depth: node.depth + 1,
                    kind,
                    ignore_rel: Vec::new(),
                    ignore: None,
                    opts: RepoOpts::default(),
                    in_repo: false,
                    repo: None,
                },
                meta.as_ref(),
                node,
                &mut next,
            );
        }
        return next;
    }
    let boundary = entries
        .iter()
        .any(|entry| entry.file_name().as_bytes() == b".git")
        .then(|| repo_sources(&node.path, ctx.errors, false));
    let (parent_rel, inherited, opts, in_repo, repo) = match &boundary {
        Some((seed, opts)) => (
            &[][..],
            seed.clone(),
            *opts,
            true,
            Some(
                node.path
                    .canonicalize()
                    .unwrap_or_else(|_| node.path.clone()),
            ),
        ),
        None => (
            &node.ignore_rel[..],
            node.ignore.clone(),
            node.opts,
            node.in_repo,
            node.repo.clone(),
        ),
    };
    let ignore = if in_repo {
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
            ctx.errors,
            false,
        )
    } else {
        None
    };
    for entry in entries {
        let Some(ft) = entry_type(&entry, ctx.errors) else {
            continue;
        };
        let name = entry.file_name();
        if is_dot_git(name.as_bytes(), opts.fold) {
            continue;
        }
        let child = entry.path();
        let (meta, kind) = classify(&child, ft, follow_child, ctx.need_meta, ctx.errors);
        if skip_followed_dir(
            &child,
            kind,
            ft.is_symlink(),
            ignore.as_deref(),
            opts,
            repo.as_deref(),
        ) {
            continue;
        }
        let rel = child_rel(parent_rel, &name, opts);
        if let Some(ig) = &ignore
            && ig.ignored(&rel, kind == Kind::Dir)
        {
            continue;
        }
        consider(ctx, &child, kind, meta.as_ref(), node.depth + 1);
        maybe_enqueue(
            ctx,
            Child {
                path: child,
                depth: node.depth + 1,
                kind,
                ignore_rel: rel,
                ignore: ignore.clone(),
                opts,
                in_repo,
                repo: repo.clone(),
            },
            meta.as_ref(),
            node,
            &mut next,
        );
    }
    next
}

pub(crate) fn for_each<F>(roots: &[OsString], cfg: &WalkCfg, errors: &AtomicBool, visit: F)
where
    F: Fn(&Item) + Sync,
{
    let ctx = Ctx {
        follow: cfg.follow,
        gitignore: cfg.gitignore,
        mindepth: cfg.mindepth,
        maxdepth: cfg.maxdepth,
        need_meta: cfg.need_meta,
        errors,
        visit,
    };
    let mut level = Vec::new();
    for root in roots {
        let path = PathBuf::from(root);
        let follow_root = matches!(cfg.follow, Follow::Cli | Follow::Always);
        let Some((meta, kind)) = resolve(&path, follow_root, errors) else {
            continue;
        };
        let mut node = if cfg.gitignore {
            match root_ignore(&path, kind == Kind::Dir, follow_root, errors) {
                None => continue,
                Some(n) => n,
            }
        } else {
            Node {
                path: path.clone(),
                depth: 0,
                ignore_rel: Vec::new(),
                ignore: None,
                opts: RepoOpts::default(),
                in_repo: false,
                ancestors: None,
                repo: None,
            }
        };
        if let Some(m) = &meta {
            node.ancestors = Some(push_anc(None, file_id(m), path.clone()));
        }
        consider(&ctx, &path, kind, meta.as_ref(), 0);
        if kind == Kind::Dir && should_descend(cfg.maxdepth, 0) {
            node.path = path;
            level.push(node);
        }
    }
    while !level.is_empty() {
        let next: Vec<Vec<Node>> = level.par_iter().map(|n| scan(n, &ctx)).collect();
        level = next.into_iter().flatten().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn nfc_ascii_and_invalid() {
        assert!(nfc::precomposed(b"ascii").is_none());
        let _ = nfc::precomposed(&[0xff, 0xff, 0xff]);
        let _ = nfc::precomposed("e\u{0301}".as_bytes());
        #[cfg(target_os = "macos")]
        {
            nfc::ICONV_FAIL.store(true, std::sync::atomic::Ordering::Relaxed);
            let failed = std::thread::spawn(|| nfc::precomposed(&[0xc3, 0xa9]).is_none())
                .join()
                .unwrap();
            nfc::ICONV_FAIL.store(false, std::sync::atomic::Ordering::Relaxed);
            assert!(failed);
        }
    }

    #[test]
    fn classify_stats_directories_without_need_meta() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_classify_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ft = fs::symlink_metadata(&dir).unwrap().file_type();
        let errors = AtomicBool::new(false);
        let (meta, kind) = classify(&dir, ft, false, false, &errors);
        assert!(meta.is_some());
        assert!(kind == Kind::Dir);
        assert!(!errors.load(Ordering::Relaxed));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn enqueue_detects_directory_cycle() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hfind_cycle_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let meta = fs::symlink_metadata(&dir).unwrap();
        let id = file_id(&meta);
        let errors = AtomicBool::new(false);
        let parent = Node {
            path: dir.clone(),
            depth: 0,
            ignore_rel: Vec::new(),
            ignore: None,
            opts: RepoOpts::default(),
            in_repo: false,
            ancestors: Some(push_anc(None, id, dir.clone())),
            repo: None,
        };
        let ctx = Ctx {
            follow: Follow::Never,
            gitignore: false,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
            errors: &errors,
            visit: |_: &Item| {},
        };
        (ctx.visit)(&Item {
            path: dir.clone(),
            kind: Kind::Dir,
            meta: None,
        });
        let mut next = Vec::new();
        maybe_enqueue(
            &ctx,
            Child {
                path: dir.join("again"),
                depth: 1,
                kind: Kind::Dir,
                ignore_rel: Vec::new(),
                ignore: None,
                opts: RepoOpts::default(),
                in_repo: false,
                repo: None,
            },
            Some(&meta),
            &parent,
            &mut next,
        );
        assert!(errors.load(Ordering::Relaxed));
        assert!(next.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    fn scratch_walk(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hfind_{tag}_{}_{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn empty_node(path: PathBuf) -> Node {
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

    #[test]
    fn classify_reports_vanished_paths() {
        let dir = scratch_walk("vanish");
        let link = dir.join("sl");
        std::os::unix::fs::symlink("missing", &link).unwrap();
        let ft = fs::symlink_metadata(&link).unwrap().file_type();
        fs::remove_file(&link).unwrap();
        let errors = AtomicBool::new(false);
        let (meta, kind) = classify(&link, ft, true, false, &errors);
        assert!(meta.is_none());
        assert!(kind == Kind::Link);
        assert!(errors.load(Ordering::Relaxed));

        let file = dir.join("gone");
        fs::write(&file, b"x").unwrap();
        let ft = fs::symlink_metadata(&file).unwrap().file_type();
        fs::remove_file(&file).unwrap();
        let errors = AtomicBool::new(false);
        let (meta, kind) = classify(&file, ft, false, true, &errors);
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

    fn dummy_item(path: &Path) -> Item {
        Item {
            path: path.to_path_buf(),
            kind: Kind::File,
            meta: None,
        }
    }

    #[test]
    fn scan_reports_forced_entry_errors() {
        let dir = scratch_walk("scan_err");
        fs::write(dir.join("a"), b"").unwrap();
        let node = empty_node(dir.clone());

        let errors = AtomicBool::new(false);
        let ctx = Ctx {
            follow: Follow::Never,
            gitignore: false,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
            errors: &errors,
            visit: |_: &Item| {},
        };
        let _ = scan(&node, &ctx);
        assert!(!errors.load(Ordering::Relaxed));

        let errors = AtomicBool::new(false);
        FORCE_DIR_ENTRY_ERR.with(|f| f.set(true));
        let ctx = Ctx {
            follow: Follow::Never,
            gitignore: false,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
            errors: &errors,
            visit: |_: &Item| {},
        };
        (ctx.visit)(&dummy_item(&dir));
        let _ = scan(&node, &ctx);
        assert!(errors.load(Ordering::Relaxed));

        let errors = AtomicBool::new(false);
        FORCE_ENTRY_TYPE_ERR.with(|f| f.set(true));
        let ctx = Ctx {
            follow: Follow::Never,
            gitignore: false,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
            errors: &errors,
            visit: |_: &Item| {},
        };
        (ctx.visit)(&dummy_item(&dir));
        let _ = scan(&node, &ctx);
        assert!(errors.load(Ordering::Relaxed));

        let errors = AtomicBool::new(false);
        FORCE_ENTRY_TYPE_ERR.with(|f| f.set(true));
        let ctx = Ctx {
            follow: Follow::Never,
            gitignore: true,
            mindepth: 0,
            maxdepth: None,
            need_meta: false,
            errors: &errors,
            visit: |_: &Item| {},
        };
        (ctx.visit)(&dummy_item(&dir));
        let _ = scan(&node, &ctx);
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
        let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o644) }, 0);
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
        assert!(!deep.ignore_rel.is_empty());
        let _ = root_ignore(&dir.join("repo/build"), true, false, &errors);

        let link = dir.join("todir");
        std::os::unix::fs::symlink("sub", &link).unwrap();
        let ft = fs::symlink_metadata(&link).unwrap().file_type();
        let (meta, kind) = classify(&link, ft, true, false, &errors);
        assert!(kind == Kind::Dir);
        assert!(meta.is_some());
        let ft = fs::symlink_metadata(dir.join("a.txt")).unwrap().file_type();
        let (meta, kind) = classify(&dir.join("a.txt"), ft, false, false, &errors);
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

        let seen = std::sync::atomic::AtomicUsize::new(0);
        let cfg = WalkCfg {
            follow: Follow::Never,
            gitignore: false,
            mindepth: 0,
            maxdepth: None,
            need_meta: true,
        };
        for_each(&[dir.clone().into_os_string()], &cfg, &errors, |_| {
            seen.fetch_add(1, Ordering::Relaxed);
        });
        assert!(seen.load(Ordering::Relaxed) >= 2);
        let cfg = WalkCfg {
            follow: Follow::Always,
            gitignore: true,
            mindepth: 1,
            maxdepth: Some(2),
            need_meta: false,
        };
        fn touch(_: &Item) {}
        for_each(&[dir.join("repo").into_os_string()], &cfg, &errors, touch);
        let cfg = WalkCfg {
            follow: Follow::Cli,
            gitignore: false,
            mindepth: 0,
            maxdepth: Some(0),
            need_meta: false,
        };
        for_each(&[dir.join("a.txt").into_os_string()], &cfg, &errors, touch);
        for_each(
            &[OsString::from("/hfind-no-such-walk-root")],
            &cfg,
            &errors,
            touch,
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
