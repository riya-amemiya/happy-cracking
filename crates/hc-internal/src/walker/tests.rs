use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{DirEntry, Error, WalkBuilder, WalkState};

#[derive(Debug)]
pub(crate) struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl TempDir {
    pub(crate) fn new() -> TempDir {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("hc-walker-{}", std::process::id()))
            .join(count.to_string());
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

pub(crate) fn wfile<P: AsRef<Path>>(path: P, contents: &str) {
    let mut file = File::create(path).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

fn wfile_size<P: AsRef<Path>>(path: P, size: u64) {
    File::create(path).unwrap().set_len(size).unwrap();
}

#[cfg(unix)]
fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dst: Q) {
    std::os::unix::fs::symlink(src, dst).unwrap();
}

pub(crate) fn mkdirp<P: AsRef<Path>>(path: P) {
    fs::create_dir_all(path).unwrap();
}

fn rel(prefix: &Path, path: &Path) -> Option<String> {
    let path = path.strip_prefix(prefix).unwrap();
    if path.as_os_str().is_empty() {
        return None;
    }
    Some(path.to_str().unwrap().replace('\\', "/"))
}

fn walk_collect(prefix: &Path, builder: &WalkBuilder) -> Vec<String> {
    let mut paths: Vec<String> = builder
        .build()
        .filter_map(Result::ok)
        .filter_map(|dent| rel(prefix, dent.path()))
        .collect();
    paths.sort();
    paths
}

fn walk_collect_entries_parallel(builder: &WalkBuilder) -> Vec<DirEntry> {
    let dents = Arc::new(Mutex::new(vec![]));
    builder.build_parallel().run(|| {
        let dents = dents.clone();
        Box::new(move |result| {
            if let Ok(dent) = result {
                dents.lock().unwrap().push(dent);
            }
            WalkState::Continue
        })
    });
    let dents = dents.lock().unwrap();
    dents.clone()
}

fn walk_collect_parallel(prefix: &Path, builder: &WalkBuilder) -> Vec<String> {
    let mut paths: Vec<String> = walk_collect_entries_parallel(builder)
        .iter()
        .filter_map(|dent| rel(prefix, dent.path()))
        .collect();
    paths.sort();
    paths
}

fn mkpaths(paths: &[&str]) -> Vec<String> {
    let mut paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
    paths.sort();
    paths
}

fn assert_paths(prefix: &Path, builder: &WalkBuilder, expected: &[&str]) {
    assert_eq!(
        walk_collect(prefix, builder),
        mkpaths(expected),
        "single threaded"
    );
    assert_eq!(
        walk_collect_parallel(prefix, builder),
        mkpaths(expected),
        "parallel"
    );
}

fn quiet(td: &TempDir) -> WalkBuilder {
    let mut builder = WalkBuilder::new(td.path());
    builder.git_global(false);
    builder
}

#[test]
fn no_ignores() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b/c"));
    mkdirp(td.path().join("x/y"));
    wfile(td.path().join("a/b/foo"), "");
    wfile(td.path().join("x/y/foo"), "");
    assert_paths(
        td.path(),
        &quiet(&td),
        &["x", "x/y", "x/y/foo", "a", "a/b", "a/b/foo", "a/b/c"],
    );
}

#[test]
fn custom_ignore() {
    let td = TempDir::new();
    mkdirp(td.path().join("a"));
    wfile(td.path().join(".customignore"), "foo");
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("bar"), "");
    wfile(td.path().join("a/bar"), "");
    let mut builder = quiet(&td);
    builder.add_custom_ignore_filename(".customignore");
    assert_paths(td.path(), &builder, &["bar", "a", "a/bar"]);
}

#[test]
fn custom_ignore_exclusive_use() {
    let td = TempDir::new();
    mkdirp(td.path().join("a"));
    wfile(td.path().join(".customignore"), "foo");
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("bar"), "");
    wfile(td.path().join("a/bar"), "");
    let mut builder = quiet(&td);
    builder.ignore(false).git_ignore(false).git_exclude(false);
    builder.add_custom_ignore_filename(".customignore");
    assert_paths(td.path(), &builder, &["bar", "a", "a/bar"]);
}

#[test]
fn gitignore() {
    let td = TempDir::new();
    mkdirp(td.path().join(".git"));
    mkdirp(td.path().join("a"));
    wfile(td.path().join(".gitignore"), "foo");
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("bar"), "");
    wfile(td.path().join("a/bar"), "");
    assert_paths(td.path(), &quiet(&td), &["bar", "a", "a/bar"]);
}

#[test]
fn explicit_ignore() {
    let td = TempDir::new();
    let igpath = td.path().join(".not-an-ignore");
    mkdirp(td.path().join("a"));
    wfile(&igpath, "foo");
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("bar"), "");
    wfile(td.path().join("a/bar"), "");
    let mut builder = quiet(&td);
    assert!(builder.add_ignore(&igpath).is_none());
    assert_paths(td.path(), &builder, &["bar", "a", "a/bar"]);
}

#[test]
fn explicit_ignore_exclusive_use() {
    let td = TempDir::new();
    let igpath = td.path().join(".not-an-ignore");
    mkdirp(td.path().join("a"));
    wfile(&igpath, "foo");
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("bar"), "");
    wfile(td.path().join("a/bar"), "");
    let mut builder = WalkBuilder::new(td.path());
    builder.standard_filters(false);
    assert!(builder.add_ignore(&igpath).is_none());
    assert_paths(
        td.path(),
        &builder,
        &[".not-an-ignore", "bar", "a", "a/bar"],
    );
}

#[test]
fn gitignore_parent() {
    let td = TempDir::new();
    mkdirp(td.path().join(".git"));
    mkdirp(td.path().join("a"));
    wfile(td.path().join(".gitignore"), "foo");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("a/bar"), "");
    let root = td.path().join("a");
    let mut builder = WalkBuilder::new(&root);
    builder.git_global(false);
    assert_paths(&root, &builder, &["bar"]);
}

#[test]
fn max_depth() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b/c"));
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("a/b/foo"), "");
    wfile(td.path().join("a/b/c/foo"), "");
    let mut builder = quiet(&td);
    assert_paths(
        td.path(),
        &builder,
        &["a", "a/b", "a/b/c", "foo", "a/foo", "a/b/foo", "a/b/c/foo"],
    );
    assert_paths(td.path(), builder.max_depth(Some(0)), &[]);
    assert_paths(td.path(), builder.max_depth(Some(1)), &["a", "foo"]);
    assert_paths(
        td.path(),
        builder.max_depth(Some(2)),
        &["a", "a/b", "foo", "a/foo"],
    );
}

#[test]
fn min_depth() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b/c"));
    wfile(td.path().join("foo"), "");
    wfile(td.path().join("a/foo"), "");
    wfile(td.path().join("a/b/foo"), "");
    wfile(td.path().join("a/b/c/foo"), "");
    let all = ["a", "a/b", "a/b/c", "foo", "a/foo", "a/b/foo", "a/b/c/foo"];
    assert_paths(td.path(), &quiet(&td), &all);
    let mut builder = quiet(&td);
    assert_paths(td.path(), builder.min_depth(Some(0)), &all);
    assert_paths(td.path(), builder.min_depth(Some(1)), &all);
    assert_paths(
        td.path(),
        builder.min_depth(Some(2)),
        &["a/b", "a/b/c", "a/b/c/foo", "a/b/foo", "a/foo"],
    );
    assert_paths(
        td.path(),
        builder.min_depth(Some(3)),
        &["a/b/c", "a/b/c/foo", "a/b/foo"],
    );
    assert_paths(td.path(), builder.min_depth(Some(10)), &[]);
    assert_paths(
        td.path(),
        builder.min_depth(Some(2)).max_depth(Some(1)),
        &["a/b", "a/foo"],
    );
}

#[test]
fn max_filesize() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b"));
    wfile_size(td.path().join("foo"), 0);
    wfile_size(td.path().join("bar"), 400);
    wfile_size(td.path().join("baz"), 600);
    wfile_size(td.path().join("a/foo"), 600);
    wfile_size(td.path().join("a/bar"), 500);
    wfile_size(td.path().join("a/baz"), 200);
    let mut builder = quiet(&td);
    assert_paths(
        td.path(),
        &builder,
        &["a", "a/b", "foo", "bar", "baz", "a/foo", "a/bar", "a/baz"],
    );
    assert_paths(
        td.path(),
        builder.max_filesize(Some(0)),
        &["a", "a/b", "foo"],
    );
    assert_paths(
        td.path(),
        builder.max_filesize(Some(500)),
        &["a", "a/b", "foo", "bar", "a/bar", "a/baz"],
    );
    assert_paths(
        td.path(),
        builder.max_filesize(Some(50000)),
        &["a", "a/b", "foo", "bar", "baz", "a/foo", "a/bar", "a/baz"],
    );
}

#[cfg(unix)]
#[test]
fn symlinks() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b"));
    symlink(td.path().join("a/b"), td.path().join("z"));
    wfile(td.path().join("a/b/foo"), "");
    let mut builder = quiet(&td);
    assert_paths(td.path(), &builder, &["a", "a/b", "a/b/foo", "z"]);
    assert_paths(
        td.path(),
        builder.follow_links(true),
        &["a", "a/b", "a/b/foo", "z", "z/foo"],
    );
}

#[cfg(unix)]
#[test]
fn first_path_not_symlink() {
    let td = TempDir::new();
    mkdirp(td.path().join("foo"));
    let dents = WalkBuilder::new(td.path().join("foo"))
        .build()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(1, dents.len());
    assert!(!dents[0].path_is_symlink());
    let dents = walk_collect_entries_parallel(&WalkBuilder::new(td.path().join("foo")));
    assert_eq!(1, dents.len());
    assert!(!dents[0].path_is_symlink());
}

#[cfg(unix)]
#[test]
fn symlink_loop() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b"));
    symlink(td.path().join("a"), td.path().join("a/b/c"));
    let mut builder = quiet(&td);
    assert_paths(td.path(), &builder, &["a", "a/b", "a/b/c"]);
    assert_paths(td.path(), builder.follow_links(true), &["a", "a/b"]);
}

#[cfg(unix)]
#[test]
fn symlink_loop_errors_match() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b"));
    symlink(td.path().join("a"), td.path().join("a/b/c"));
    let mut builder = quiet(&td);
    builder.follow_links(true);
    let single: Vec<String> = builder
        .build()
        .filter_map(Result::err)
        .map(|e| e.to_string())
        .collect();
    let errs = Arc::new(Mutex::new(vec![]));
    builder.build_parallel().run(|| {
        let errs = errs.clone();
        Box::new(move |r: Result<DirEntry, Error>| {
            if let Err(e) = r {
                errs.lock().unwrap().push(e.to_string());
            }
            WalkState::Continue
        })
    });
    let expected = format!(
        "File system loop found: {} points to an ancestor {}",
        td.path().join("a/b/c").display(),
        td.path().join("a").display()
    );
    assert_eq!(single, vec![expected.clone()]);
    assert_eq!(*errs.lock().unwrap(), vec![expected]);
}

#[test]
fn filter() {
    let td = TempDir::new();
    mkdirp(td.path().join("a/b/c"));
    mkdirp(td.path().join("x/y"));
    wfile(td.path().join("a/b/foo"), "");
    wfile(td.path().join("x/y/foo"), "");
    assert_paths(
        td.path(),
        &quiet(&td),
        &["x", "x/y", "x/y/foo", "a", "a/b", "a/b/foo", "a/b/c"],
    );
    let mut builder = quiet(&td);
    builder.filter_entry(|entry| entry.file_name() != OsStr::new("a"));
    assert_paths(td.path(), &builder, &["x", "x/y", "x/y/foo"]);
}

#[test]
fn hidden_and_whitelist() {
    let td = TempDir::new();
    mkdirp(td.path().join(".git"));
    mkdirp(td.path().join(".hid/sub"));
    wfile(td.path().join(".hid/sub/f"), "");
    wfile(td.path().join(".shown"), "");
    wfile(td.path().join(".secret"), "");
    wfile(td.path().join(".gitignore"), "!.shown\n");
    assert_paths(td.path(), &quiet(&td), &[".shown"]);
    let mut builder = quiet(&td);
    builder.hidden(false);
    assert_paths(
        td.path(),
        &builder,
        &[
            ".git",
            ".gitignore",
            ".hid",
            ".hid/sub",
            ".hid/sub/f",
            ".secret",
            ".shown",
        ],
    );
}

#[test]
fn sorted_single_threaded_order() {
    let td = TempDir::new();
    mkdirp(td.path().join("b/z"));
    mkdirp(td.path().join("a"));
    wfile(td.path().join("c"), "");
    wfile(td.path().join("b/y"), "");
    wfile(td.path().join("a/x"), "");
    let mut builder = quiet(&td);
    builder.sort_by_file_name(OsStr::cmp);
    let got: Vec<String> = builder
        .build()
        .filter_map(Result::ok)
        .filter_map(|d| rel(td.path(), d.path()))
        .collect();
    assert_eq!(got, ["a", "a/x", "b", "b/y", "b/z", "c"]);
}

#[test]
fn quit_stops_parallel_walk() {
    let td = TempDir::new();
    for i in 0..50 {
        mkdirp(td.path().join(format!("d{i}")));
        wfile(td.path().join(format!("d{i}/f")), "");
    }
    let seen = Arc::new(AtomicUsize::new(0));
    let mut builder = quiet(&td);
    builder.threads(4);
    builder.build_parallel().run(|| {
        let seen = seen.clone();
        Box::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            WalkState::Quit
        })
    });
    assert!(seen.load(Ordering::SeqCst) <= 4);
}

#[test]
fn many_files_are_split_across_threads() {
    let td = TempDir::new();
    for i in 0..2000 {
        wfile(td.path().join(format!("f{i}")), "");
    }
    let mut builder = quiet(&td);
    builder.threads(8);
    let got = walk_collect_parallel(td.path(), &builder);
    assert_eq!(got.len(), 2000);
}

#[test]
fn nonexistent_root_errors() {
    let td = TempDir::new();
    let missing = td.path().join("missing");
    let single: Vec<String> = WalkBuilder::new(&missing)
        .build()
        .filter_map(Result::err)
        .map(|e| e.to_string())
        .collect();
    let shown = missing.display();
    assert_eq!(
        single,
        vec![format!(
            "{shown}: IO error for operation on {shown}: No such file or directory (os error 2)"
        )]
    );
    let errs = Arc::new(Mutex::new(vec![]));
    WalkBuilder::new(&missing).build_parallel().run(|| {
        let errs = errs.clone();
        Box::new(move |r: Result<DirEntry, Error>| {
            if let Err(e) = r {
                errs.lock().unwrap().push(e.to_string());
            }
            WalkState::Continue
        })
    });
    assert_eq!(
        *errs.lock().unwrap(),
        vec![format!("{shown}: No such file or directory (os error 2)")]
    );
}

#[test]
fn ignore_file_errors_are_attached() {
    let td = TempDir::new();
    mkdirp(td.path().join("sub"));
    wfile(td.path().join("sub/.ignore"), "ok\n{bad\n");
    wfile(td.path().join("sub/ok"), "");
    wfile(td.path().join("sub/keep"), "");
    let builder = quiet(&td);
    let expected = format!(
        "{}: line 2: error parsing glob '{{bad': unclosed alternate group; missing '}}' (maybe escape '{{' with '[{{]'?)",
        td.path().join("sub/.ignore").display()
    );
    let single: Vec<String> = builder
        .build()
        .filter_map(Result::ok)
        .filter_map(|d| d.error().map(ToString::to_string))
        .collect();
    assert_eq!(single, vec![expected.clone()]);
    let par: Vec<String> = walk_collect_entries_parallel(&builder)
        .iter()
        .filter_map(|d| d.error().map(ToString::to_string))
        .collect();
    assert_eq!(par, vec![expected]);
    assert_paths(td.path(), &builder, &["sub", "sub/keep"]);
}

#[test]
fn slow_visitors_are_balanced_across_threads() {
    let td = TempDir::new();
    for i in 0..64 {
        wfile(td.path().join(format!("f{i}")), "");
    }
    let mut builder = quiet(&td);
    builder.threads(4);
    let ids = Arc::new(Mutex::new(std::collections::HashSet::new()));
    builder.build_parallel().run(|| {
        let ids = ids.clone();
        Box::new(move |r: Result<DirEntry, Error>| {
            if r.is_ok_and(|d| d.depth() == 1) {
                std::thread::sleep(std::time::Duration::from_millis(1));
                ids.lock().unwrap().insert(std::thread::current().id());
            }
            WalkState::Continue
        })
    });
    assert!(ids.lock().unwrap().len() >= 2);
}
