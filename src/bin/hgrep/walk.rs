use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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

    pub fn precomposed(raw: &[u8]) -> Option<Vec<u8>> {
        if !raw.iter().any(|&b| b >= 0x80) {
            return None;
        }
        CONV.with(|conv| {
            let mut cd = conv.0.get();
            if cd.is_null() {
                cd = unsafe { iconv_open(c"UTF-8".as_ptr(), c"UTF-8-MAC".as_ptr()) };
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

struct Dir {
    path: PathBuf,
    rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
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

fn root_ignore(path: &Path, errors: &AtomicBool, quiet: bool) -> Option<Dir> {
    let loose = |path: &Path| Dir {
        path: path.to_path_buf(),
        rel: Vec::new(),
        ignore: None,
        opts: RepoOpts::default(),
        in_repo: false,
    };
    let Ok(abs) = path.canonicalize() else {
        return Some(loose(path));
    };
    if abs.join(".git").exists() {
        let (seed, opts) = repo_sources(&abs, errors, quiet);
        return Some(Dir {
            path: path.to_path_buf(),
            rel: Vec::new(),
            ignore: seed,
            opts,
            in_repo: true,
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
    let (seed, opts) = repo_sources(root, errors, quiet);
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
            quiet,
        );
    }
    let rel = child_rel(&rel, name, opts);
    if ignore.as_ref().is_some_and(|ig| ig.ignored(&rel, true)) {
        return None;
    }
    Some(Dir {
        path: path.to_path_buf(),
        rel,
        ignore,
        opts,
        in_repo: true,
    })
}

fn scan_plain(dir: &Dir, errors: &AtomicBool, quiet: bool) -> (Vec<Dir>, Vec<PathBuf>) {
    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    match fs::read_dir(&dir.path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let Ok(t) = entry.file_type() else { continue };
                if t.is_dir() {
                    sub.push(Dir {
                        path: entry.path(),
                        rel: Vec::new(),
                        ignore: None,
                        opts: RepoOpts::default(),
                        in_repo: false,
                    });
                } else if t.is_file() {
                    leaves.push(entry.path());
                }
            }
        }
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !quiet {
                eprintln!("hgrep: {}: {e}", dir.path.display());
            }
        }
    }
    (sub, leaves)
}

fn scan_ignored(dir: &Dir, errors: &AtomicBool, quiet: bool) -> (Vec<Dir>, Vec<PathBuf>) {
    let entries: Vec<fs::DirEntry> = match fs::read_dir(&dir.path) {
        Ok(entries) => entries.flatten().collect(),
        Err(e) => {
            errors.store(true, Ordering::Relaxed);
            if !quiet {
                eprintln!("hgrep: {}: {e}", dir.path.display());
            }
            Vec::new()
        }
    };
    let boundary = entries
        .iter()
        .any(|entry| entry.file_name().as_bytes() == b".git")
        .then(|| repo_sources(&dir.path, errors, quiet));
    let (parent_rel, inherited, opts, in_repo) = match &boundary {
        Some((seed, opts)) => (&[][..], seed.clone(), *opts, true),
        None => (&dir.rel[..], dir.ignore.clone(), dir.opts, dir.in_repo),
    };
    let ignore = if in_repo {
        let base = if parent_rel.is_empty() {
            0
        } else {
            parent_rel.len() + 1
        };
        load_ignore(
            &dir.path.join(".gitignore"),
            base,
            inherited,
            opts.fold,
            errors,
            quiet,
        )
    } else {
        None
    };

    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    for entry in &entries {
        let Ok(t) = entry.file_type() else { continue };
        let is_dir = t.is_dir();
        if !is_dir && !t.is_file() {
            continue;
        }
        let name = entry.file_name();
        if is_dot_git(name.as_bytes(), opts.fold) {
            continue;
        }
        let rel = child_rel(parent_rel, &name, opts);
        if let Some(ig) = &ignore
            && ig.ignored(&rel, is_dir)
        {
            continue;
        }
        if is_dir {
            sub.push(Dir {
                path: entry.path(),
                rel,
                ignore: ignore.clone(),
                opts,
                in_repo,
            });
        } else {
            leaves.push(entry.path());
        }
    }
    (sub, leaves)
}

pub(crate) fn collect_paths(
    roots: &[OsString],
    recursive: bool,
    gitignore: bool,
    errors: &AtomicBool,
    quiet_errors: bool,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut level = Vec::new();

    for root in roots {
        let path = PathBuf::from(root);
        match fs::metadata(&path) {
            Ok(md) if md.is_dir() => {
                if !recursive {
                    errors.store(true, Ordering::Relaxed);
                    if !quiet_errors {
                        eprintln!("hgrep: {}: Is a directory", path.display());
                    }
                } else if gitignore {
                    level.extend(root_ignore(&path, errors, quiet_errors));
                } else {
                    level.push(Dir {
                        path,
                        rel: Vec::new(),
                        ignore: None,
                        opts: RepoOpts::default(),
                        in_repo: false,
                    });
                }
            }
            Ok(_) => files.push(path),
            Err(e) => {
                errors.store(true, Ordering::Relaxed);
                if !quiet_errors {
                    eprintln!("hgrep: {}: {e}", path.display());
                }
            }
        }
    }

    while !level.is_empty() {
        let (dirs, found): (Vec<Vec<Dir>>, Vec<Vec<PathBuf>>) = level
            .par_iter()
            .map(|dir| {
                if gitignore {
                    scan_ignored(dir, errors, quiet_errors)
                } else {
                    scan_plain(dir, errors, quiet_errors)
                }
            })
            .unzip();
        level = dirs.into_iter().flatten().collect();
        files.extend(found.into_iter().flatten());
    }
    files
}
