use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn sandbox() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hg_sandbox_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    })
}

fn hg() -> Command {
    let home = sandbox();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hg"));
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("absent-config"))
        .env("GIT_CONFIG_SYSTEM", home.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

fn hgrep() -> Command {
    let home = sandbox();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hgrep"));
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("absent-config"))
        .env("GIT_CONFIG_SYSTEM", home.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run(mut cmd: Command, args: &[&str]) -> Run {
    let out = cmd.args(args).output().unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "hg_{tag}_{}_{nanos}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".git")).unwrap();
    dir
}

fn put(dir: &Path, rel: &str, body: &[u8]) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn rels(text: &str, root: &Path) -> Vec<String> {
    let prefix = format!("{}/", root.display());
    let mut out: Vec<String> = text
        .lines()
        .map(|l| l.strip_prefix(prefix.as_str()).unwrap_or(l).to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn hg_searches_a_directory_without_recursive_flag() {
    let dir = scratch("dir_operand");
    put(&dir, "keep/a.txt", b"needle\n");
    put(&dir, "keep/b.txt", b"nothing\n");

    let out = run(hg(), &["needle", dir.to_str().unwrap()]);
    assert!(
        out.stdout.contains("keep/a.txt:needle"),
        "stdout {:?}\nstderr {:?}",
        out.stdout,
        out.stderr
    );
    assert!(!out.stdout.contains("b.txt"), "got {:?}", out.stdout);
    assert_eq!(out.code, 0, "stderr {:?}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hgrep_still_rejects_a_directory_without_recursive() {
    let dir = scratch("hgrep_dir");
    put(&dir, "a.txt", b"needle\n");
    let out = run(hgrep(), &["needle", dir.to_str().unwrap()]);
    assert!(
        out.stderr.contains("Is a directory"),
        "got {:?}",
        out.stderr
    );
    assert_eq!(out.code, 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hg_directory_search_honors_gitignore_by_default() {
    let dir = scratch("gitignore_default");
    put(&dir, ".gitignore", b"drop.log\n");
    put(&dir, "keep.txt", b"needle\n");
    put(&dir, "drop.log", b"needle\n");

    let out = run(hg(), &["-l", "needle", dir.to_str().unwrap()]);
    assert_eq!(
        rels(&out.stdout, &dir),
        ["keep.txt"],
        "stderr {:?}",
        out.stderr
    );
    assert_eq!(out.code, 0);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hg_no_ignore_includes_gitignored_files() {
    let dir = scratch("no_ignore");
    put(&dir, ".gitignore", b"drop.log\n");
    put(&dir, "keep.txt", b"needle\n");
    put(&dir, "drop.log", b"needle\n");

    let out = run(
        hg(),
        &["--no-ignore", "-l", "needle", dir.to_str().unwrap()],
    );
    assert_eq!(
        rels(&out.stdout, &dir),
        ["drop.log", "keep.txt"],
        "stderr {:?}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hg_help_documents_no_ignore() {
    let out = run(hg(), &["--help"]);
    let line = out
        .stdout
        .lines()
        .find(|l| l.contains("--no-ignore"))
        .unwrap_or_else(|| panic!("no --no-ignore row in {:?}", out.stdout));
    assert!(
        line.to_ascii_lowercase().contains("gitignore"),
        "got {line:?}"
    );
}

#[test]
fn hg_searches_hidden_paths_that_are_not_gitignored() {
    let dir = scratch("hidden");
    put(&dir, ".secret/flag.txt", b"needle\n");
    put(&dir, "visible.txt", b"needle\n");
    let out = run(hg(), &["-l", "needle", dir.to_str().unwrap()]);
    let found = rels(&out.stdout, &dir);
    assert!(
        found.iter().any(|p| p.ends_with(".secret/flag.txt")),
        "got {found:?} stderr {:?}",
        out.stderr
    );
    assert!(found.iter().any(|p| p == "visible.txt"), "got {found:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn hg_file_operand_does_not_require_a_tree_walk() {
    let dir = scratch("file_only");
    put(&dir, "a.txt", b"needle\n");
    put(&dir, ".gitignore", b"a.txt\n");
    let path = dir.join("a.txt");
    let out = run(hg(), &["needle", path.to_str().unwrap()]);
    assert_eq!(out.stdout, "needle\n", "stderr {:?}", out.stderr);
    assert_eq!(out.code, 0);
    fs::remove_dir_all(&dir).unwrap();
}
