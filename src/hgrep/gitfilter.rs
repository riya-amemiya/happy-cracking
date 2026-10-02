use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::hc_internal::gitconfig::{RepoOpts, repo_sources};
use crate::hc_internal::ignore::{Ignore, load_ignore};
use crate::hc_internal::nfc;
use crate::hc_internal::walker::DirEntry;

use super::messages;

#[derive(Clone)]
struct Outer {
    rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
}

#[derive(Clone)]
struct Inner {
    rel: Vec<u8>,
    ignore: Option<Arc<Ignore>>,
    opts: RepoOpts,
    in_repo: bool,
}

#[derive(Clone, Default)]
pub(crate) struct GitFilter {
    outer: Arc<RwLock<HashMap<PathBuf, Option<Outer>>>>,
    inner: Arc<RwLock<HashMap<PathBuf, Inner>>>,
}

fn child_rel(parent: &[u8], name: &OsStr, opts: RepoOpts) -> Vec<u8> {
    let raw = name.as_encoded_bytes();
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

fn track(errors: &AtomicBool) {
    if errors.load(Ordering::Relaxed) {
        messages::set_errored();
    }
}

fn quiet() -> bool {
    !messages::messages()
}

fn root_outer(path: &Path) -> Option<Outer> {
    let errors = AtomicBool::new(false);
    let prog = messages::prog();
    let loose = Outer {
        rel: Vec::new(),
        ignore: None,
        opts: RepoOpts::default(),
        in_repo: false,
    };
    let Ok(abs) = path.canonicalize() else {
        return Some(loose);
    };
    if abs.join(".git").exists() {
        let (seed, opts) = repo_sources(&abs, &errors, quiet(), prog);
        track(&errors);
        return Some(Outer {
            rel: Vec::new(),
            ignore: seed,
            opts,
            in_repo: true,
        });
    }
    let chain: Vec<&Path> = abs.ancestors().skip(1).collect();
    let Some(repo_at) = chain.iter().position(|dir| dir.join(".git").exists()) else {
        return Some(loose);
    };
    let Some(name) = abs.file_name() else {
        return Some(loose);
    };
    let (seed, opts) = repo_sources(chain[repo_at], &errors, quiet(), prog);
    let mut rel = Vec::new();
    let mut ignore = seed;
    for (idx, dir) in chain[..=repo_at].iter().rev().enumerate() {
        if idx > 0 {
            rel = child_rel(&rel, dir.file_name().unwrap_or(OsStr::new("")), opts);
        }
        let base = if rel.is_empty() { 0 } else { rel.len() + 1 };
        ignore = load_ignore(
            &dir.join(".gitignore"),
            base,
            ignore,
            opts.fold,
            &errors,
            quiet(),
            prog,
        );
    }
    track(&errors);
    let rel = child_rel(&rel, name, opts);
    if ignore.as_ref().is_some_and(|ig| ig.ignored(&rel, true)) {
        return None;
    }
    Some(Outer {
        rel,
        ignore,
        opts,
        in_repo: true,
    })
}

fn inner_of(dir: &Path, outer: &Outer) -> Inner {
    let errors = AtomicBool::new(false);
    let prog = messages::prog();
    let boundary = dir.join(".git").symlink_metadata().is_ok()
        && std::fs::read_dir(dir).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().as_encoded_bytes() == b".git")
        });
    let (rel, inherited, opts, in_repo) = if boundary {
        let (seed, opts) = repo_sources(dir, &errors, quiet(), prog);
        (Vec::new(), seed, opts, true)
    } else {
        (
            outer.rel.clone(),
            outer.ignore.clone(),
            outer.opts,
            outer.in_repo,
        )
    };
    let ignore = if in_repo {
        let base = if rel.is_empty() { 0 } else { rel.len() + 1 };
        load_ignore(
            &dir.join(".gitignore"),
            base,
            inherited,
            opts.fold,
            &errors,
            quiet(),
            prog,
        )
    } else {
        None
    };
    track(&errors);
    Inner {
        rel,
        ignore,
        opts,
        in_repo,
    }
}

impl GitFilter {
    pub(crate) fn new() -> GitFilter {
        GitFilter::default()
    }

    fn inner(&self, dir: &Path) -> Option<Inner> {
        if let Some(found) = self.inner.read().ok()?.get(dir) {
            return Some(found.clone());
        }
        let outer = self.outer.read().ok()?.get(dir).cloned().flatten()?;
        let computed = inner_of(dir, &outer);
        self.inner
            .write()
            .ok()?
            .entry(dir.to_path_buf())
            .or_insert(computed)
            .clone()
            .into()
    }

    pub(crate) fn keep(&self, dent: &DirEntry) -> bool {
        let path = dent.path();
        let is_dir = dent
            .file_type()
            .is_some_and(crate::hc_internal::walker::FileType::is_dir)
            || (dent.path_is_symlink() && path.is_dir());
        if dent.depth() == 0 {
            if !is_dir {
                return true;
            }
            let outer = root_outer(path);
            let keep = outer.is_some();
            if let Ok(mut map) = self.outer.write() {
                map.insert(path.to_path_buf(), outer);
            }
            return keep;
        }
        let Some(parent) = path.parent() else {
            return true;
        };
        if dent.depth() == 1 {
            let known = self
                .outer
                .read()
                .ok()
                .and_then(|map| map.get(parent).map(Option::is_some));
            match known {
                Some(false) => return false,
                Some(true) => {}
                None => {
                    let outer = root_outer(parent);
                    let kept = outer.is_some();
                    if let Ok(mut map) = self.outer.write() {
                        map.entry(parent.to_path_buf()).or_insert(outer);
                    }
                    if !kept {
                        return false;
                    }
                }
            }
        }
        let Some(state) = self.inner(parent) else {
            return true;
        };
        let name = dent.file_name();
        if is_dot_git(name.as_encoded_bytes(), state.opts.fold) {
            return false;
        }
        if !is_dir
            && !dent
                .file_type()
                .is_some_and(crate::hc_internal::walker::FileType::is_file)
        {
            return true;
        }
        let rel = child_rel(&state.rel, name, state.opts);
        if state
            .ignore
            .as_ref()
            .is_some_and(|ig| ig.ignored(&rel, is_dir))
        {
            return false;
        }
        if is_dir && let Ok(mut map) = self.outer.write() {
            map.insert(
                path.to_path_buf(),
                Some(Outer {
                    rel,
                    ignore: state.ignore.clone(),
                    opts: state.opts,
                    in_repo: state.in_repo,
                }),
            );
        }
        true
    }
}
