use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
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
        let dir =
            std::env::temp_dir().join(format!("hfind_sandbox_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    })
}

fn command() -> Command {
    let home = sandbox();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hfind"));
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("absent-config"))
        .env("GIT_CONFIG_SYSTEM", home.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "hfind_{tag}_{}_{nanos}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(dir: &Path, rel: &str, body: &[u8]) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
    raw: Vec<u8>,
}

fn run_env(env: &[(&str, &OsStr)], args: &[&str]) -> Run {
    let mut cmd = command();
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.args(args).output().unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
        raw: out.stdout,
    }
}

fn run(args: &[&str]) -> Run {
    run_env(&[], args)
}

fn run_at(dir: &Path, args: &[&str]) -> Run {
    let mut cmd = command();
    cmd.current_dir(dir);
    let out = cmd.args(args).output().unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
        raw: out.stdout,
    }
}

fn listed(text: &str, root: &Path) -> Vec<String> {
    let root_s = root.to_string_lossy();
    let prefix = format!("{root_s}/");
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l == root_s.as_ref() {
                ".".to_string()
            } else if let Some(rest) = l.strip_prefix(&prefix) {
                rest.to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    out.sort();
    out
}

fn names(root: &Path, args: &[&str]) -> Vec<String> {
    let mut full = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-H" | "-L" | "-P" | "--gitignore" => {
                full.push(args[i]);
                i += 1;
            }
            _ => break,
        }
    }
    full.push(root.to_str().unwrap());
    full.extend_from_slice(&args[i..]);
    let out = run(&full);
    listed(&out.stdout, root)
}

fn set_mtime(path: &Path, secs: i64) {
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let tv = libc::timeval {
        tv_sec: secs,
        tv_usec: 0,
    };
    let times = [tv, tv];
    let rc = unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) };
    assert_eq!(rc, 0, "utimes {}", path.display());
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn hfd_alias_behaves_like_hfind() {
    let hfd = Path::new(env!("CARGO_BIN_EXE_hfd"));
    assert!(hfd.exists(), "missing {hfd:?}");
    let dir = scratch("hfd");
    put(&dir, "a.txt", b"x");
    let home = sandbox();
    let mut cmd = Command::new(hfd);
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("absent-config"))
        .env("GIT_CONFIG_SYSTEM", home.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    let out = cmd
        .args([dir.to_str().unwrap(), "-name", "a.txt"])
        .output()
        .unwrap();
    let via_hfind = run(&[dir.to_str().unwrap(), "-name", "a.txt"]);
    assert_eq!(out.status.code(), Some(via_hfind.code));
    let mut got: Vec<&str> = std::str::from_utf8(&out.stdout).unwrap().lines().collect();
    got.sort();
    let mut exp: Vec<&str> = via_hfind.stdout.lines().collect();
    exp.sort();
    assert_eq!(got, exp);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn help_uses_binary_name() {
    let hfind = run(&["--help"]);
    assert_eq!(hfind.code, 0);
    assert!(hfind.stdout.contains("hfind"), "{}", hfind.stdout);
    assert!(hfind.stdout.contains("--gitignore"), "{}", hfind.stdout);
    let home = sandbox();
    let out = Command::new(env!("CARGO_BIN_EXE_hfd"))
        .env("HOME", home)
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("hfd"), "{text}");
}

#[test]
fn help_is_a_global_only() {
    let dir = scratch("help_global");
    let out = run(&[dir.to_str().unwrap(), "--help"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("unknown predicate"), "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn default_path_is_dot() {
    let dir = scratch("default_dot");
    put(&dir, "only.txt", b"");
    let out = run_at(&dir, &["-name", "only.txt"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .lines()
            .any(|l| l == "./only.txt" || l == "only.txt"),
        "{}",
        out.stdout
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_tree_prints_the_root() {
    let dir = scratch("empty");
    let out = run(&[dir.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(listed(&out.stdout, &dir), ["."]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn lists_hidden_names() {
    let dir = scratch("hidden");
    put(&dir, ".secret", b"");
    put(&dir, "vis.txt", b"");
    let got = names(&dir, &[]);
    assert!(got.contains(&".secret".to_string()), "{got:?}");
    assert!(got.contains(&"vis.txt".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn missing_root_exits_one() {
    let dir = scratch("missing");
    let gone = dir.join("nope");
    let out = run(&[gone.to_str().unwrap()]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("hfind:"), "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_root_string_does_not_panic() {
    let out = run(&[""]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.starts_with("hfind:"), "{}", out.stderr);
}

#[test]
fn unreadable_directory_exits_one() {
    let dir = scratch("unreadable");
    let locked = dir.join("locked");
    fs::create_dir(&locked).unwrap();
    put(&locked, "x.txt", b"");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let out = run(&[dir.to_str().unwrap()]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("locked"), "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parse_errors_exit_two() {
    let cases: &[&[&str]] = &[
        &["-name"],
        &["-iname"],
        &["-path"],
        &["-ipath"],
        &["-regex"],
        &["-iregex"],
        &["-type"],
        &["-type", "x"],
        &["-size"],
        &["-size", "+"],
        &["-size", "3x"],
        &["-size", "k"],
        &["-mtime"],
        &["-mtime", "+"],
        &["-mmin", "nope"],
        &["-newer"],
        &["-maxdepth"],
        &["-maxdepth", "-1"],
        &["-mindepth", "x"],
        &["-unknown"],
        &[","],
        &["."],
        &["-o", "-true"],
        &["-true", "-o"],
        &["("],
        &["(", ")"],
        &["-true", ")"],
        &["-regex", "("],
        &["-a"],
        &["!"],
    ];
    let dir = scratch("parse_err");
    for args in cases {
        let mut full = vec![dir.to_str().unwrap()];
        if args == &["."] {
            full = vec![dir.to_str().unwrap(), ","];
        } else {
            full.extend(*args);
        }
        let out = run(&full);
        assert_eq!(out.code, 2, "args {args:?} stderr {}", out.stderr);
        assert!(out.stderr.starts_with("hfind:"), "{}", out.stderr);
    }
    let missing = run(&[
        dir.to_str().unwrap(),
        "-newer",
        dir.join("gone").to_str().unwrap(),
    ]);
    assert_eq!(missing.code, 2, "{}", missing.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn name_and_iname() {
    let dir = scratch("name");
    put(&dir, "Foo.txt", b"");
    put(&dir, "bar.TXT", b"");
    put(&dir, "skip.log", b"");
    assert_eq!(names(&dir, &["-name", "Foo.txt"]), ["Foo.txt"]);
    let mut got = names(&dir, &["-iname", "*.txt"]);
    got.retain(|n| n != ".");
    assert_eq!(got, ["Foo.txt", "bar.TXT"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn path_and_ipath() {
    let dir = scratch("path");
    put(&dir, "sub/Deep/a.txt", b"");
    let root = dir.to_str().unwrap();
    let out = run(&[root, "-path", &format!("{root}/sub/*/a.txt")]);
    assert!(out.stdout.contains("a.txt"), "{}", out.stdout);
    let out = run(&[root, "-ipath", &format!("{root}/SUB/*/A.TXT")]);
    assert!(out.stdout.contains("a.txt"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn glob_classes_escapes_and_wildcards() {
    let dir = scratch("glob");
    put(&dir, "a", b"");
    put(&dir, "b", b"");
    put(&dir, "c", b"");
    put(&dir, "z", b"");
    put(&dir, "[", b"");
    put(&dir, "star*x", b"");
    put(&dir, "q1z", b"");
    assert_eq!(names(&dir, &["-name", "[ab]"]), ["a", "b"]);
    assert_eq!(names(&dir, &["-name", "[!a]"]), ["[", "b", "c", "z"]);
    assert_eq!(names(&dir, &["-name", "[^a]"]), ["[", "b", "c", "z"]);
    assert_eq!(names(&dir, &["-name", "[a-c]"]), ["a", "b", "c"]);
    assert_eq!(names(&dir, &["-name", "[z-a]"]), [] as [&str; 0]);
    assert_eq!(names(&dir, &["-name", "["]), ["["]);
    assert_eq!(names(&dir, &["-name", "star\\*x"]), ["star*x"]);
    assert_eq!(names(&dir, &["-name", "q?z"]), ["q1z"]);
    assert_eq!(names(&dir, &["-name", "q*"]), ["q1z"]);
    assert_eq!(names(&dir, &["-iname", "[A]"]), ["a"]);
    assert_eq!(names(&dir, &["-iname", "[Z-A]"]), [] as [&str; 0]);
    assert!(names(&dir, &["-name", "[\\[]"]).contains(&"[".to_string()));
    assert_eq!(names(&dir, &["-name", "[\\]"]), [] as [&str; 0]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn regex_and_iregex() {
    let dir = scratch("regex");
    put(&dir, "keep.txt", b"");
    put(&dir, "drop.log", b"");
    let root = dir.to_str().unwrap();
    let out = run(&[root, "-regex", &format!("{root}/.*\\.txt")]);
    assert!(out.stdout.contains("keep.txt"), "{}", out.stdout);
    assert!(!out.stdout.contains("drop.log"), "{}", out.stdout);
    let out = run(&[root, "-iregex", &format!("{root}/.*KEEP.TXT")]);
    assert!(out.stdout.contains("keep.txt"), "{}", out.stdout);
    let out = run(&[root, "-regex", "keep.txt"]);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn type_file_dir_link() {
    let dir = scratch("type");
    put(&dir, "file.txt", b"");
    fs::create_dir(dir.join("sub")).unwrap();
    std::os::unix::fs::symlink("file.txt", dir.join("link")).unwrap();
    assert_eq!(names(&dir, &["-type", "f"]), ["file.txt"]);
    assert_eq!(names(&dir, &["-type", "d"]), [".", "sub"]);
    assert_eq!(names(&dir, &["-type", "l"]), ["link"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn size_units_and_signs() {
    let dir = scratch("size");
    put(&dir, "empty", b"");
    put(&dir, "tiny", b"x");
    put(&dir, "block", &vec![0u8; 512]);
    put(&dir, "over", &vec![0u8; 513]);
    put(&dir, "kilo", &vec![0u8; 1024]);
    assert_eq!(names(&dir, &["-type", "f", "-size", "0c"]), ["empty"]);
    assert_eq!(names(&dir, &["-type", "f", "-size", "1c"]), ["tiny"]);
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "1"]),
        ["block", "tiny"]
    );
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "+1"]),
        ["kilo", "over"]
    );
    assert_eq!(names(&dir, &["-type", "f", "-size", "-1"]), ["empty"]);
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "1k"]),
        ["block", "kilo", "over", "tiny"]
    );
    assert_eq!(names(&dir, &["-type", "f", "-size", "256w"]), ["block"]);
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "1b"]),
        ["block", "tiny"]
    );
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "-2M"]),
        ["block", "empty", "kilo", "over", "tiny"]
    );
    assert_eq!(
        names(&dir, &["-type", "f", "-size", "-2G"]),
        ["block", "empty", "kilo", "over", "tiny"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_file_and_directory() {
    let dir = scratch("empty_pred");
    put(&dir, "zero", b"");
    put(&dir, "one", b"x");
    fs::create_dir(dir.join("vacant")).unwrap();
    put(&dir, "full/a", b"");
    std::os::unix::fs::symlink("zero", dir.join("sl")).unwrap();
    let got = names(&dir, &["-empty"]);
    assert!(got.contains(&"zero".to_string()), "{got:?}");
    assert!(got.contains(&"vacant".to_string()), "{got:?}");
    assert!(!got.contains(&"one".to_string()), "{got:?}");
    assert!(!got.contains(&"full".to_string()), "{got:?}");
    assert!(!got.contains(&"sl".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_on_unreadable_dir_is_false() {
    let dir = scratch("empty_bad");
    let locked = dir.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let out = run(&[dir.to_str().unwrap(), "-empty"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(out.code, 1);
    assert!(!listed(&out.stdout, &dir).contains(&"locked".to_string()));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn mtime_mmin_and_newer() {
    let dir = scratch("time");
    put(&dir, "old", b"");
    put(&dir, "new", b"");
    let now = now_secs();
    set_mtime(&dir.join("old"), now - 10 * 86400);
    set_mtime(&dir.join("new"), now);
    assert!(names(&dir, &["-mtime", "+5"]).contains(&"old".to_string()));
    assert!(names(&dir, &["-mtime", "-5"]).contains(&"new".to_string()));
    assert!(names(&dir, &["-mmin", "+1"]).contains(&"old".to_string()));
    assert!(names(&dir, &["-mmin", "-1"]).contains(&"new".to_string()));
    assert!(
        names(&dir, &["-newer", dir.join("old").to_str().unwrap()]).contains(&"new".to_string())
    );
    assert!(
        !names(&dir, &["-newer", dir.join("new").to_str().unwrap()]).contains(&"old".to_string())
    );
    let future = run(&[dir.to_str().unwrap(), "-mtime", "-1", "-name", "new"]);
    assert!(future.stdout.contains("new"), "{}", future.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn newer_follows_reference_with_h_and_l() {
    let dir = scratch("newer_follow");
    put(&dir, "old", b"");
    put(&dir, "new", b"");
    let now = now_secs();
    set_mtime(&dir.join("old"), now - 100);
    set_mtime(&dir.join("new"), now);
    std::os::unix::fs::symlink("old", dir.join("ref")).unwrap();
    let via_p = names(&dir, &["-newer", dir.join("old").to_str().unwrap()]);
    let via_l = run(&[
        "-L",
        dir.to_str().unwrap(),
        "-type",
        "f",
        "-newer",
        dir.join("ref").to_str().unwrap(),
    ]);
    assert!(via_p.contains(&"new".to_string()), "{via_p:?}");
    assert!(via_l.stdout.contains("new"), "{}", via_l.stdout);
    let via_h = run(&[
        "-H",
        dir.to_str().unwrap(),
        "-name",
        "new",
        "-newer",
        dir.join("ref").to_str().unwrap(),
    ]);
    assert!(via_h.stdout.contains("new"), "{}", via_h.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn true_false_and_implied_print() {
    let dir = scratch("bool");
    put(&dir, "a", b"");
    assert_eq!(names(&dir, &["-false"]), [] as [&str; 0]);
    assert_eq!(names(&dir, &["-true", "-name", "a"]), ["a"]);
    let out = run(&[dir.to_str().unwrap(), "-name", "a"]);
    assert!(out.stdout.contains("a"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn print_and_print0() {
    let dir = scratch("print");
    put(&dir, "a", b"");
    let nl = run(&[dir.to_str().unwrap(), "-name", "a", "-print"]);
    assert!(nl.stdout.ends_with('\n'), "{}", nl.stdout);
    let z = run(&[dir.to_str().unwrap(), "-name", "a", "-print0"]);
    assert!(z.raw.contains(&0), "{:?}", z.raw);
    assert!(!z.stdout.contains('\n'), "{}", z.stdout);
    let twice = run(&[dir.to_str().unwrap(), "-name", "a", "-print", "-print"]);
    assert_eq!(
        twice.stdout.lines().filter(|l| l.ends_with("/a")).count(),
        2
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn operators_and_precedence() {
    let dir = scratch("ops");
    put(&dir, "a.txt", b"");
    put(&dir, "b.log", b"");
    put(&dir, "c.txt", b"");
    let or_and = names(
        &dir,
        &["-name", "a.txt", "-o", "-name", "b.log", "-name", "missing"],
    );
    assert_eq!(or_and, ["a.txt"]);
    let grouped = names(
        &dir,
        &[
            "(", "-name", "a.txt", "-o", "-name", "b.log", ")", "-o", "-name", "c.txt",
        ],
    );
    assert_eq!(grouped, ["a.txt", "b.log", "c.txt"]);
    let not_bang = names(&dir, &["!", "-name", "b.log", "-type", "f"]);
    assert_eq!(not_bang, ["a.txt", "c.txt"]);
    let not_word = names(&dir, &["-not", "-name", "*.txt", "-type", "f"]);
    assert_eq!(not_word, ["b.log"]);
    let and_kw = names(&dir, &["-name", "*.txt", "-a", "-name", "a.txt"]);
    assert_eq!(and_kw, ["a.txt"]);
    let and_word = names(&dir, &["-name", "*.txt", "-and", "-name", "c.txt"]);
    assert_eq!(and_word, ["c.txt"]);
    let or_word = names(&dir, &["-name", "a.txt", "-or", "-name", "c.txt"]);
    assert_eq!(or_word, ["a.txt", "c.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn depths() {
    let dir = scratch("depth");
    put(&dir, "a.txt", b"");
    put(&dir, "sub/b.txt", b"");
    put(&dir, "sub/deep/c.txt", b"");
    assert_eq!(names(&dir, &["-maxdepth", "0"]), ["."]);
    let one = names(&dir, &["-maxdepth", "1"]);
    assert!(one.contains(&"a.txt".to_string()), "{one:?}");
    assert!(one.contains(&"sub".to_string()), "{one:?}");
    assert!(!one.contains(&"sub/b.txt".to_string()), "{one:?}");
    let min = names(&dir, &["-mindepth", "2", "-type", "f"]);
    assert_eq!(min, ["sub/b.txt", "sub/deep/c.txt"]);
    let last = names(&dir, &["-maxdepth", "9", "-maxdepth", "0"]);
    assert_eq!(last, ["."]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn maxdepth_is_true_in_the_expression() {
    let dir = scratch("maxdepth_true");
    put(&dir, "a", b"");
    let out = names(&dir, &["-maxdepth", "0", "-o", "-false"]);
    assert_eq!(out, ["."]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn multiple_roots() {
    let a = scratch("root_a");
    let b = scratch("root_b");
    put(&a, "a.txt", b"");
    put(&b, "b.txt", b"");
    let out = run(&[a.to_str().unwrap(), b.to_str().unwrap(), "-type", "f"]);
    assert!(out.stdout.contains("a.txt"), "{}", out.stdout);
    assert!(out.stdout.contains("b.txt"), "{}", out.stdout);
    fs::remove_dir_all(&a).unwrap();
    fs::remove_dir_all(&b).unwrap();
}

#[test]
fn trailing_slash_root_is_preserved() {
    let dir = scratch("slash");
    put(&dir, "a.txt", b"");
    let mut root = dir.to_string_lossy().into_owned();
    root.push('/');
    let out = run(&[&root, "-name", "a.txt"]);
    assert!(
        out.stdout.contains(&format!("{root}a.txt")),
        "{}",
        out.stdout
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn symlink_p_does_not_follow() {
    let dir = scratch("sym_p");
    put(&dir, "real/a.txt", b"");
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    std::os::unix::fs::symlink("missing", dir.join("broken")).unwrap();
    let p = names(&dir, &["-P", "-type", "l"]);
    assert!(p.contains(&"link".to_string()), "{p:?}");
    assert!(p.contains(&"broken".to_string()), "{p:?}");
    assert!(!names(&dir, &["-P"]).iter().any(|n| n.contains("link/a")));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn symlink_h_follows_only_cli() {
    let dir = scratch("sym_h");
    put(&dir, "real/a.txt", b"");
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    std::os::unix::fs::symlink("a.txt", dir.join("real/inner")).unwrap();
    let out = run(&["-H", dir.join("link").to_str().unwrap()]);
    let got = listed(&out.stdout, &dir.join("link"));
    assert!(got.contains(&".".to_string()), "{got:?}");
    assert!(got.contains(&"a.txt".to_string()), "{got:?}");
    assert!(got.contains(&"inner".to_string()), "{got:?}");
    assert!(!got.iter().any(|n| n.contains("inner/")), "{got:?}");
    let out = run(&["-H", dir.to_str().unwrap(), "-type", "l"]);
    assert!(out.stdout.contains("link"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn symlink_l_follows_and_detects_cycles() {
    let dir = scratch("sym_l");
    put(&dir, "real/a.txt", b"");
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    std::os::unix::fs::symlink("..", dir.join("real/up")).unwrap();
    let out = run(&["-L", dir.to_str().unwrap(), "-name", "a.txt"]);
    assert!(out.stdout.contains("a.txt"), "{}", out.stdout);
    let looped = run(&["-L", dir.to_str().unwrap()]);
    assert_eq!(looped.code, 1, "{}", looped.stderr);
    assert!(
        looped.stderr.contains("File system loop detected"),
        "{}",
        looped.stderr
    );
    let self_link = scratch("self_loop");
    std::os::unix::fs::symlink("self", self_link.join("self")).unwrap();
    let again = run(&["-L", self_link.to_str().unwrap()]);
    assert_eq!(again.code, 1, "{}", again.stderr);
    fs::remove_dir_all(&dir).unwrap();
    fs::remove_dir_all(&self_link).unwrap();
}

#[test]
fn symlink_l_broken_is_a_link() {
    let dir = scratch("broken_l");
    std::os::unix::fs::symlink("missing", dir.join("broken")).unwrap();
    let out = run(&["-L", dir.to_str().unwrap(), "-type", "l"]);
    assert!(out.stdout.contains("broken"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn symlink_l_file_link_has_target_type() {
    let dir = scratch("file_l");
    put(&dir, "real.txt", b"hi");
    std::os::unix::fs::symlink("real.txt", dir.join("alias")).unwrap();
    let out = run(&["-L", dir.to_str().unwrap(), "-type", "f", "-name", "alias"]);
    assert!(out.stdout.contains("alias"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fifo_is_other() {
    let dir = scratch("fifo");
    let fifo = dir.join("pipe");
    let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o644) };
    assert_eq!(rc, 0);
    let out = run(&[dir.to_str().unwrap(), "-name", "pipe"]);
    assert!(out.stdout.contains("pipe"), "{}", out.stdout);
    let typed = run(&[dir.to_str().unwrap(), "-type", "f", "-name", "pipe"]);
    assert!(typed.stdout.is_empty(), "{}", typed.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn globals_last_follow_wins() {
    let dir = scratch("follow_last");
    put(&dir, "real/a.txt", b"");
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    let p = run(&["-L", "-P", dir.join("link").to_str().unwrap()]);
    assert!(
        !listed(&p.stdout, &dir.join("link")).contains(&"a.txt".to_string()),
        "{}",
        p.stdout
    );
    let l = run(&["-P", "-L", dir.join("link").to_str().unwrap()]);
    assert!(
        listed(&l.stdout, &dir.join("link")).contains(&"a.txt".to_string()),
        "{}",
        l.stdout
    );
    fs::remove_dir_all(&dir).unwrap();
}

fn git_repo(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    fs::create_dir_all(dir.join(".git")).unwrap();
    dir
}

#[test]
fn gitignore_off_lists_git() {
    let dir = git_repo("git_off");
    put(&dir, ".git/config", b"");
    put(&dir, "a.txt", b"");
    let got = names(&dir, &[]);
    assert!(got.contains(&".git".to_string()), "{got:?}");
    assert!(got.contains(&".git/config".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_skips_git_and_rules() {
    let dir = git_repo("git_on");
    put(&dir, ".gitignore", b"*.log\n/target\n");
    put(&dir, ".git/config", b"");
    put(&dir, "a.txt", b"");
    put(&dir, "a.log", b"");
    put(&dir, "target/t.txt", b"");
    put(&dir, "keep.log", b"");
    put(&dir, ".gitignore", b"*.log\n!keep.log\n/target\n");
    let got = names(&dir, &["--gitignore"]);
    assert!(
        !got.iter().any(|n| n == ".git" || n.starts_with(".git/")),
        "{got:?}"
    );
    assert!(got.contains(&"a.txt".to_string()), "{got:?}");
    assert!(got.contains(&"keep.log".to_string()), "{got:?}");
    assert!(!got.contains(&"a.log".to_string()), "{got:?}");
    assert!(!got.iter().any(|n| n.starts_with("target")), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_nested_and_dir_only() {
    let dir = git_repo("git_nested");
    put(&dir, ".gitignore", b"logs/\n");
    put(&dir, "sub/.gitignore", b"*.tmp\n");
    put(&dir, "logs/a.txt", b"");
    put(&dir, "logs_file", b"");
    put(&dir, "sub/a.tmp", b"");
    put(&dir, "sub/a.txt", b"");
    let got = names(&dir, &["--gitignore"]);
    assert!(got.contains(&"logs_file".to_string()), "{got:?}");
    assert!(got.contains(&"sub/a.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"sub/a.tmp".to_string()), "{got:?}");
    assert!(!got.iter().any(|n| n.starts_with("logs/")), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_above_the_search_root() {
    let dir = git_repo("git_above");
    put(&dir, ".gitignore", b"*.log\n");
    put(&dir, "src/.gitignore", b"!kept.log\n");
    put(&dir, "src/a.log", b"");
    put(&dir, "src/b.txt", b"");
    put(&dir, "src/kept.log", b"");
    let root = dir.join("src");
    let got = names(&root, &["--gitignore"]);
    assert!(got.contains(&"b.txt".to_string()), "{got:?}");
    assert!(got.contains(&"kept.log".to_string()), "{got:?}");
    assert!(!got.contains(&"a.log".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_skips_an_ignored_root() {
    let dir = git_repo("git_root_ign");
    put(&dir, ".gitignore", b"build/\n");
    put(&dir, "build/a.txt", b"");
    let out = run(&["--gitignore", dir.join("build").to_str().unwrap()]);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert_eq!(out.code, 0, "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_applies_no_rule_outside_a_repository() {
    let dir = scratch("outside_repo");
    let ambient = dir
        .parent()
        .unwrap()
        .ancestors()
        .find(|d| d.join(".git").exists());
    assert!(
        ambient.is_none(),
        "this test needs a temp directory outside every repository, but {ambient:?} holds one"
    );
    put(&dir, ".gitignore", b"*.log\n");
    put(&dir, "inner/a.log", b"");
    put(&dir, "inner/b.txt", b"");
    let got = names(&dir.join("inner"), &["--gitignore"]);
    assert!(got.contains(&"a.log".to_string()), "{got:?}");
    assert!(got.contains(&"b.txt".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_globs_classes_and_bom() {
    let dir = git_repo("git_globs");
    put(
        &dir,
        ".gitignore",
        b"\xef\xbb\xbfa/**/b\n/rooted.txt\nf[a-z]9\nstar\\*x\nq?z\n# comment\n",
    );
    put(&dir, "a/x/b", b"");
    put(&dir, "a/keep", b"");
    put(&dir, "rooted.txt", b"");
    put(&dir, "sub/rooted.txt", b"");
    put(&dir, "fa9", b"");
    put(&dir, "f_9", b"");
    put(&dir, "star*x", b"");
    put(&dir, "qZz", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"a/keep".to_string()), "{got:?}");
    assert!(got.contains(&"sub/rooted.txt".to_string()), "{got:?}");
    assert!(got.contains(&"f_9".to_string()), "{got:?}");
    assert!(!got.contains(&"a/x/b".to_string()), "{got:?}");
    assert!(!got.contains(&"rooted.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"fa9".to_string()), "{got:?}");
    assert!(!got.contains(&"star*x".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_invalid_utf8_and_crlf() {
    let dir = git_repo("git_bytes");
    put(&dir, ".gitignore", b"# caf\xe9\r\n*.log\r\n");
    put(&dir, "a.log", b"");
    put(&dir, "b.txt", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert_eq!(
        got.iter()
            .filter(|n| n.ends_with(".txt") || n.ends_with(".log"))
            .cloned()
            .collect::<Vec<_>>(),
        ["b.txt"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_unreadable_rule_file() {
    let dir = git_repo("git_unreadable");
    fs::create_dir_all(dir.join(".gitignore")).unwrap();
    put(&dir, "y.txt", b"");
    let out = run(&["--gitignore", dir.to_str().unwrap(), "-name", "y.txt"]);
    assert!(out.stdout.contains("y.txt"), "{}", out.stdout);
    assert!(out.stderr.contains(".gitignore"), "{}", out.stderr);
    assert_eq!(out.code, 1, "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_ignorecase_and_dotgit_case() {
    let on = git_repo("igcase_on");
    put(&on, ".git/config", b"[core]\n\tignorecase = true\n");
    put(&on, ".gitignore", b"FOO.txt\n");
    put(&on, "foo.txt", b"");
    put(&on, "keep.txt", b"");
    put(&on, "sub/.GIT/hidden.txt", b"");
    let got = names(&on, &["--gitignore", "-type", "f"]);
    assert_eq!(
        got.iter()
            .filter(|n| n.ends_with(".txt"))
            .cloned()
            .collect::<Vec<_>>(),
        ["keep.txt"]
    );
    fs::remove_dir_all(&on).unwrap();

    let off = git_repo("igcase_off");
    put(&off, ".git/config", b"[core]\n\tignorecase = false\n");
    put(&off, ".gitignore", b"FOO.txt\n");
    put(&off, "foo.txt", b"");
    put(&off, "sub/.GIT/hidden.txt", b"");
    let got = names(&off, &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"foo.txt".to_string()), "{got:?}");
    assert!(got.contains(&"sub/.GIT/hidden.txt".to_string()), "{got:?}");
    fs::remove_dir_all(&off).unwrap();
}

#[test]
fn gitignore_info_exclude_and_global() {
    let dir = git_repo("info_ex");
    put(&dir, ".git/info/exclude", b"*.log\n");
    put(&dir, "a.log", b"");
    put(&dir, "b.txt", b"");
    let got = names(
        &dir,
        &["--gitignore", "-name", "*.log", "-o", "-name", "b.txt"],
    );
    assert_eq!(got, ["b.txt"]);
    fs::remove_dir_all(&dir).unwrap();

    let dir = git_repo("global_ex");
    let home = dir.join("home");
    put(&home, "git/ignore", b"*.log\n");
    put(&dir, "a.log", b"");
    put(&dir, "b.txt", b"");
    let out = run_env(
        &[
            ("XDG_CONFIG_HOME", home.as_os_str()),
            ("HOME", home.as_os_str()),
        ],
        &[
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "b.txt",
        ],
    );
    assert_eq!(listed(&out.stdout, &dir), ["b.txt"], "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_excludes_file_and_empty_value() {
    let dir = git_repo("excludes");
    let home = dir.join("home");
    put(&home, "git/ignore", b"*.log\n");
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let env = [
        ("HOME", home.as_os_str()),
        ("XDG_CONFIG_HOME", home.as_os_str()),
    ];
    let args = [
        "--gitignore",
        dir.to_str().unwrap(),
        "-name",
        "a.log",
        "-o",
        "-name",
        "keep.md",
    ];
    let control = run_env(&env, &args);
    assert_eq!(
        listed(&control.stdout, &dir),
        ["keep.md"],
        "{}",
        control.stderr
    );
    put(&dir, ".git/config", b"[core]\n\texcludesFile =\n");
    let disabled = run_env(&env, &args);
    assert_eq!(
        listed(&disabled.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        disabled.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_tilde_home_and_missing_user() {
    let dir = git_repo("tilde");
    let home = dir.join("home");
    put(&home, "myignore", b"*.log\n");
    put(
        &dir,
        ".git/config",
        b"[core]\n\texcludesFile = ~/myignore\n",
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let out = run_env(
        &[("HOME", home.as_os_str())],
        &[
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "keep.md",
        ],
    );
    assert_eq!(listed(&out.stdout, &dir), ["keep.md"], "{}", out.stderr);

    put(
        &dir,
        ".git/config",
        b"[core]\n\texcludesFile = ~hfind-no-such-user/ig.list\n",
    );
    put(&dir, "~hfind-no-such-user/ig.list", b"*.log\n");
    let out = run(&[
        "--gitignore",
        dir.to_str().unwrap(),
        "-name",
        "a.log",
        "-o",
        "-name",
        "keep.md",
    ]);
    assert_eq!(
        listed(&out.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_tilde_current_user() {
    let dir = git_repo("tilde_user");
    let name = Command::new("id")
        .arg("-un")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        fs::remove_dir_all(&dir).unwrap();
        return;
    }
    put(
        &dir,
        ".git/config",
        format!("[core]\n\texcludesFile = ~{name}/hfind-no-such-ignore\n").as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let got = names(
        &dir,
        &["--gitignore", "-name", "a.log", "-o", "-name", "keep.md"],
    );
    assert_eq!(got, ["a.log", "keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_system_config_and_nosystem() {
    let dir = git_repo("system_cfg");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "system",
        format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let args = [
        "--gitignore",
        dir.to_str().unwrap(),
        "-name",
        "a.log",
        "-o",
        "-name",
        "keep.md",
    ];
    let read = run_env(
        &[
            ("GIT_CONFIG_SYSTEM", home.join("system").as_os_str()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("0")),
        ],
        &args,
    );
    assert_eq!(listed(&read.stdout, &dir), ["keep.md"], "{}", read.stderr);
    let suppressed = run_env(
        &[
            ("GIT_CONFIG_SYSTEM", home.join("system").as_os_str()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("2")),
        ],
        &args,
    );
    assert_eq!(
        listed(&suppressed.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        suppressed.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_gitconfig_when_global_unset() {
    let dir = git_repo("global_unset");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    put(
        &home,
        ".gitconfig",
        format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let mut cmd = command();
    cmd.env_remove("GIT_CONFIG_GLOBAL");
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", home.join("xdg"));
    let out = cmd
        .args([
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "keep.md",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(listed(&stdout, &dir), ["keep.md"], "{stdout}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_git_pointer_and_commondir() {
    let dir = git_repo("gitfile");
    let store = dir.join("store");
    put(&store, "info/exclude", b"*.log\n");
    put(&dir, "ignore.list", b"*.tmp\n");
    put(
        &store,
        "config",
        format!(
            "[core]\n\texcludesFile = {}\n",
            dir.join("ignore.list").display()
        )
        .as_bytes(),
    );
    put(
        &dir,
        "work/.git",
        format!("gitdir: {}\n", store.display()).as_bytes(),
    );
    put(&dir, "work/keep.md", b"");
    put(&dir, "work/drop.log", b"");
    put(&dir, "work/drop.tmp", b"");
    let got = names(&dir.join("work"), &["--gitignore", "-type", "f"]);
    assert_eq!(got, ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();

    let dir = git_repo("commondir");
    put(&dir, "main/.git/info/exclude", b"*.log\n");
    put(&dir, "main/.git/worktrees/wt/commondir", b"../..\n");
    put(&dir, "main/.git/worktrees/wt/info/exclude", b"keep.md\n");
    put(
        &dir,
        "work/.git",
        format!("gitdir: {}\n", dir.join("main/.git/worktrees/wt").display()).as_bytes(),
    );
    put(&dir, "work/keep.md", b"");
    put(&dir, "work/drop.log", b"");
    let got = names(&dir.join("work"), &["--gitignore", "-type", "f"]);
    assert_eq!(got, ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_conditional_include_and_onbranch() {
    let dir = git_repo("include_if");
    let home = dir.join("home");
    let marker = dir.file_name().unwrap().to_str().unwrap().to_string();
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "inc",
        format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    put(&dir, ".git/HEAD", b"ref: refs/heads/main\n");
    let args = [
        "--gitignore",
        dir.to_str().unwrap(),
        "-name",
        "a.log",
        "-o",
        "-name",
        "keep.md",
    ];
    put(
        &home,
        "hit",
        format!(
            "[includeIf \"gitdir:{marker}/\"]\n\tpath = {}\n",
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let hit = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("hit").as_os_str())],
        &args,
    );
    assert_eq!(listed(&hit.stdout, &dir), ["keep.md"], "{}", hit.stderr);

    put(
        &home,
        "fold",
        format!(
            "[includeIf \"gitdir/i:{}/\"]\n\tpath = {}\n",
            marker.to_ascii_uppercase(),
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let fold = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("fold").as_os_str())],
        &args,
    );
    assert_eq!(listed(&fold.stdout, &dir), ["keep.md"], "{}", fold.stderr);

    put(
        &home,
        "rel",
        format!(
            "[includeIf \"gitdir:./{}\"]\n\tpath = {}\n",
            marker,
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let rel = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("rel").as_os_str())],
        &args,
    );
    assert_eq!(
        listed(&rel.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        rel.stderr
    );

    put(
        &home,
        "abs",
        format!(
            "[includeIf \"gitdir:{}/\"]\n\tpath = {}\n",
            dir.display(),
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let abs = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("abs").as_os_str())],
        &args,
    );
    assert_eq!(listed(&abs.stdout, &dir), ["keep.md"], "{}", abs.stderr);

    put(
        &home,
        "branch",
        format!(
            "[includeIf \"onbranch:ma*\"]\n\tpath = {}\n",
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let branch = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("branch").as_os_str())],
        &args,
    );
    assert_eq!(
        listed(&branch.stdout, &dir),
        ["keep.md"],
        "{}",
        branch.stderr
    );

    put(
        &home,
        "branch_slash",
        format!(
            "[includeIf \"onbranch:ma/\"]\n\tpath = {}\n",
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let branch_slash = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("branch_slash").as_os_str())],
        &args,
    );
    assert_eq!(
        listed(&branch_slash.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        branch_slash.stderr
    );

    put(
        &home,
        "miss",
        format!(
            "[includeIf \"gitdir:hfind-no-such-dir/\"]\n\tpath = {}\n",
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let miss = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("miss").as_os_str())],
        &args,
    );
    assert_eq!(
        listed(&miss.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        miss.stderr
    );

    put(
        &home,
        "plain",
        format!("[include]\n\tpath = {}\n", home.join("inc").display()).as_bytes(),
    );
    let plain = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("plain").as_os_str())],
        &args,
    );
    assert_eq!(listed(&plain.stdout, &dir), ["keep.md"], "{}", plain.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_include_depth_and_malformed_config() {
    for (depth, expect_drop) in [(10usize, true), (11, false)] {
        let dir = git_repo(&format!("inc_depth_{depth}"));
        let home = dir.join("home");
        put(&home, "list", b"*.log\n");
        for step in 1..depth {
            put(
                &home,
                &format!("i{step}"),
                format!(
                    "[include]\n\tpath = {}\n",
                    home.join(format!("i{}", step + 1)).display()
                )
                .as_bytes(),
            );
        }
        put(
            &home,
            &format!("i{depth}"),
            format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
        );
        put(
            &home,
            "gitconfig",
            format!("[include]\n\tpath = {}\n", home.join("i1").display()).as_bytes(),
        );
        put(&dir, "a.log", b"");
        put(&dir, "keep.md", b"");
        let out = run_env(
            &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
            &[
                "--gitignore",
                dir.to_str().unwrap(),
                "-name",
                "a.log",
                "-o",
                "-name",
                "keep.md",
            ],
        );
        let got = listed(&out.stdout, &dir);
        if expect_drop {
            assert_eq!(got, ["keep.md"], "depth {depth} {}", out.stderr);
        } else {
            assert_eq!(got, ["a.log", "keep.md"], "depth {depth} {}", out.stderr);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    let broken = [
        "[core ]\n\texcludesFile = LIST\n",
        "[ core]\n\texcludesFile = LIST\n",
        "[core]\n\texcludesFile = \"LIST\n\tignorecase = true\n",
        "[core]\n\texcludesFile = LIST\\.\n",
        "[core]\n\texcludesFile\n= LIST\n",
        "[core]\n\texcludesFile LIST\n",
        "[core]\n1 = x\n\texcludesFile = LIST\n",
        "[core]\n\texcludesFile\n",
        "[core \"x\n",
        "[core \"a\\nb\"]\n",
        "[core extra]\n",
        "[core \"ab\n",
        "!\n",
        "[core]\n\texcludesFile = \"LIST\"\n",
    ];
    for (index, body) in broken.iter().enumerate() {
        let dir = git_repo(&format!("bad_cfg_{index}"));
        let home = dir.join("home");
        put(&home, "list", b"*.log\n");
        put(
            &home,
            "gitconfig",
            body.replace("LIST", &home.join("list").display().to_string())
                .as_bytes(),
        );
        put(&dir, "a.log", b"");
        put(&dir, "keep.md", b"");
        let out = run_env(
            &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
            &[
                "--gitignore",
                dir.to_str().unwrap(),
                "-name",
                "a.log",
                "-o",
                "-name",
                "keep.md",
            ],
        );
        if *body == "[core]\n\texcludesFile = \"LIST\"\n" {
            assert_eq!(
                listed(&out.stdout, &dir),
                ["keep.md"],
                "case {index} {}",
                out.stderr
            );
        } else {
            assert_eq!(
                listed(&out.stdout, &dir),
                ["a.log", "keep.md"],
                "case {index} {body:?} {}",
                out.stderr
            );
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn gitignore_config_escapes_and_bools() {
    let dir = git_repo("cfg_esc");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "gitconfig",
        format!(
            "[core]\n\texcludesFile = \"{}\"\n\tignorecase = yes\n\tprecomposeunicode = on\n",
            home.join("list").display()
        )
        .as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "FOO.txt", b"");
    put(&dir, ".gitignore", b"foo.txt\n");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &[
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "FOO.txt",
        ],
    );
    assert!(listed(&out.stdout, &dir).is_empty(), "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();

    let numbered = git_repo("cfg_int");
    put(&numbered, ".git/config", b"[core]\n\tignorecase = 2\n");
    put(&numbered, ".gitignore", b"FOO.txt\n");
    put(&numbered, "foo.txt", b"");
    put(&numbered, "keep.md", b"");
    let got = names(
        &numbered,
        &["--gitignore", "-name", "foo.txt", "-o", "-name", "keep.md"],
    );
    assert_eq!(got, ["keep.md"]);
    fs::remove_dir_all(&numbered).unwrap();
}

#[test]
fn gitignore_config_value_escapes() {
    let dir = git_repo("cfg_val");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    let path = home.join("list");
    put(
        &home,
        "gitconfig",
        format!(
            "[core]\n\texcludesFile = {p}\\n\n\tignorecase\n",
            p = path.display()
        )
        .replace("{p}", &path.display().to_string())
        .as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &[
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "keep.md",
        ],
    );
    assert_eq!(
        listed(&out.stdout, &dir),
        ["a.log", "keep.md"],
        "{}",
        out.stderr
    );

    put(
        &home,
        "gitconfig",
        b"[core]\n\tignorecase = true\\\n\n\tprecomposeunicode = false\n",
    );
    put(&dir, ".gitignore", b"FOO.txt\n");
    put(&dir, "foo.txt", b"");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-name", "foo.txt"],
    );
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_relative_excludes_and_empty_xdg() {
    let dir = git_repo("rel_ex");
    put(&dir, "my.ignore", b"*.log\n");
    put(&dir, ".git/config", b"[core]\n\texcludesFile = my.ignore\n");
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let got = names(
        &dir,
        &["--gitignore", "-name", "a.log", "-o", "-name", "keep.md"],
    );
    assert_eq!(got, ["keep.md"]);

    put(
        &dir,
        ".git/config",
        b"[core]\n\tignorecase = false\n[include]\n\tpath = extra.cfg\n",
    );
    put(
        &dir,
        ".git/extra.cfg",
        b"[core]\n\tprecomposeunicode = true\n",
    );
    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    let mut cmd = command();
    cmd.env("HOME", &home);
    cmd.env("XDG_CONFIG_HOME", "");
    cmd.env_remove("GIT_CONFIG_GLOBAL");
    put(&home, ".config/git/ignore", b"keep.md\n");
    let out = cmd
        .args(["--gitignore", dir.to_str().unwrap(), "-name", "keep.md"])
        .output()
        .unwrap();
    assert!(out.stdout.is_empty(), "{:?}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_nested_repo_and_repo_below() {
    let dir = git_repo("nested_repo");
    put(&dir, ".git/info/exclude", b"*.txt\n");
    put(&dir, "inner/.git/info/exclude", b"*.log\n");
    put(&dir, "outer.txt", b"");
    put(&dir, "inner/keep.txt", b"");
    put(&dir, "inner/drop.log", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"inner/keep.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"outer.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"inner/drop.log".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();

    let dir = git_repo("repo_below");
    fs::remove_dir_all(dir.join(".git")).unwrap();
    put(&dir, "outer/repo/.git/info/exclude", b"*.log\n");
    put(&dir, "outer/repo/.gitignore", b"*.tmp\n");
    put(&dir, "outer/repo/a.log", b"");
    put(&dir, "outer/repo/b.tmp", b"");
    put(&dir, "outer/repo/c.txt", b"");
    let got = names(&dir.join("outer"), &["--gitignore", "-type", "f"]);
    assert_eq!(got, ["repo/.gitignore", "repo/c.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_broken_symlink_root_is_loose() {
    let dir = scratch("broken_root");
    std::os::unix::fs::symlink("missing", dir.join("gone")).unwrap();
    let out = run(&["--gitignore", dir.join("gone").to_str().unwrap()]);
    assert!(out.stdout.contains("gone"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_slash_root_does_not_panic() {
    let out = run(&["--gitignore", "/", "-maxdepth", "0"]);
    assert!(out.code == 0 || out.code == 1, "{}", out.stderr);
}

#[test]
fn invalid_utf8_name_and_regex_bytes() {
    let dir = scratch("utf8");
    let raw: &[u8] = b"bad\xffname.txt";
    let name = OsStr::from_bytes(raw);
    if fs::write(dir.join(name), b"").is_err() {
        fs::remove_dir_all(&dir).unwrap();
        return;
    }
    let mut cmd = command();
    let out = cmd.arg(&dir).arg("-name").arg(name).output().unwrap();
    assert!(
        out.stdout.windows(raw.len()).any(|w| w == raw),
        "{:?}",
        out.stdout
    );
    let bad_re = command()
        .arg(&dir)
        .arg("-regex")
        .arg(OsStr::from_bytes(b"a\xffb"))
        .output()
        .unwrap();
    assert_eq!(bad_re.status.code(), Some(2));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn maxdepth_overflow_is_invalid() {
    let out = run(&["-maxdepth", "999999999999999999999"]);
    assert_eq!(out.code, 2);
}

#[cfg(target_os = "macos")]
#[test]
fn gitignore_precomposes_decomposed_names() {
    let dir = git_repo("precompose");
    let decomposed: &[u8] = b"e\xcc\x81drop.txt";
    put(&dir, ".git/config", b"[core]\n\tprecomposeunicode = true\n");
    put(&dir, ".gitignore", b"\xc3\xa9drop.txt\n\xc3\xa9dir/\n");
    fs::write(dir.join(OsStr::from_bytes(decomposed)), b"").unwrap();
    fs::create_dir_all(dir.join(OsStr::from_bytes(b"e\xcc\x81dir"))).unwrap();
    fs::write(
        dir.join(OsStr::from_bytes(b"e\xcc\x81dir")).join("a.txt"),
        b"",
    )
    .unwrap();
    put(&dir, "keep.md", b"");
    let on = command()
        .args(["--gitignore", dir.to_str().unwrap(), "-type", "f"])
        .output()
        .unwrap()
        .stdout;
    assert!(
        !on.windows(decomposed.len()).any(|w| w == decomposed),
        "{on:?}"
    );
    assert!(
        on.windows(b"keep.md".len()).any(|w| w == b"keep.md"),
        "{on:?}"
    );
    put(
        &dir,
        ".git/config",
        b"[core]\n\tprecomposeunicode = false\n",
    );
    let off = command()
        .args(["--gitignore", dir.to_str().unwrap(), "-type", "f"])
        .output()
        .unwrap()
        .stdout;
    assert!(
        off.windows(decomposed.len()).any(|w| w == decomposed),
        "{off:?}"
    );
    let _ = nfc_probe();
    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(target_os = "macos")]
fn nfc_probe() {
    let dir = git_repo("nfc_probe");
    put(&dir, ".git/config", b"[core]\n\tprecomposeunicode = true\n");
    put(&dir, ".gitignore", b"*\n");
    fs::write(dir.join(OsStr::from_bytes(b"\xff\xff\xff")), b"").ok();
    let _ = command()
        .args(["--gitignore", dir.to_str().unwrap()])
        .output();
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn follow_cli_symlink_to_file() {
    let dir = scratch("h_file");
    put(&dir, "real.txt", b"abc");
    std::os::unix::fs::symlink("real.txt", dir.join("alias")).unwrap();
    let out = run(&[
        "-H",
        dir.join("alias").to_str().unwrap(),
        "-type",
        "f",
        "-size",
        "3c",
    ]);
    assert!(out.stdout.contains("alias"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn and_short_circuit_skips_print() {
    let dir = scratch("short");
    put(&dir, "a", b"");
    let out = run(&[dir.to_str().unwrap(), "-false", "-a", "-print"]);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unexpected_close_paren_in_or() {
    let dir = scratch("paren");
    let out = run(&[dir.to_str().unwrap(), "(", "-true", "-o", "-true"]);
    assert_eq!(out.code, 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn file_root_is_printed() {
    let dir = scratch("file_root");
    put(&dir, "only", b"z");
    let path = dir.join("only");
    let out = run(&[path.to_str().unwrap()]);
    assert_eq!(out.stdout.trim(), path.to_string_lossy());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn exact_mtime_and_future_file() {
    let dir = scratch("mtime_exact");
    put(&dir, "aged", b"");
    put(&dir, "soon", b"");
    let now = now_secs();
    set_mtime(&dir.join("aged"), now - 2 * 86400 - 60);
    set_mtime(&dir.join("soon"), now + 3600);
    assert!(names(&dir, &["-mtime", "2"]).contains(&"aged".to_string()));
    assert!(names(&dir, &["-mtime", "-1"]).contains(&"soon".to_string()));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn follow_missing_and_loop_roots() {
    let missing = scratch("l_missing").join("gone");
    let out = run(&["-L", missing.to_str().unwrap()]);
    assert_eq!(out.code, 1);
    let dir = scratch("l_root_loop");
    std::os::unix::fs::symlink("loop", dir.join("loop")).unwrap();
    let again = run(&["-L", dir.join("loop").to_str().unwrap()]);
    assert_eq!(again.code, 1, "{}", again.stderr);
    fs::remove_dir_all(missing.parent().unwrap()).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_two_levels_down() {
    let dir = git_repo("two_levels");
    put(&dir, ".gitignore", b"*.tmp\n");
    put(&dir, "a/.gitignore", b"");
    put(&dir, "a/b/keep.txt", b"");
    put(&dir, "a/b/drop.tmp", b"");
    let got = names(&dir.join("a/b"), &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"keep.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"drop.tmp".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_classes_posix_and_spaces() {
    let dir = git_repo("ig_class");
    put(
        &dir,
        ".gitignore",
        b"[[:digit:]].log\n[:alpha:].txt\n[z-a]\nkeep\\ \ntrail \n[a[b].x\n[[]\n",
    );
    put(&dir, "1.log", b"");
    put(&dir, "a.log", b"");
    put(&dir, ":.txt", b"");
    put(&dir, "z", b"");
    put(&dir, "keep ", b"");
    put(&dir, "trail", b"");
    put(&dir, "a.x", b"");
    put(&dir, "[", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(!got.contains(&"1.log".to_string()), "{got:?}");
    assert!(got.contains(&"a.log".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_unclosed_class_and_escaped_range() {
    let dir = git_repo("ig_open");
    put(&dir, ".gitignore", b"[abc.txt\na\\-z\n");
    put(&dir, "[abc.txt", b"");
    put(&dir, "a-z", b"");
    put(&dir, "keep.txt", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"keep.txt".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitconfig_comments_numbers_and_gitdir_edges() {
    let dir = git_repo("cfg_more");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "gitconfig",
        format!(
            "[core]\n\texcludesFile = {} ; tail\n\tignorecase = 0x1\n#eof",
            home.join("list").display()
        )
        .as_bytes(),
    );
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &[
            "--gitignore",
            dir.to_str().unwrap(),
            "-name",
            "a.log",
            "-o",
            "-name",
            "keep.md",
        ],
    );
    assert_eq!(listed(&out.stdout, &dir), ["keep.md"], "{}", out.stderr);

    put(
        &home,
        "gitconfig",
        b"[core]\n\tignorecase = +1k\n\tprecomposeunicode = 01\n",
    );
    put(&dir, ".gitignore", b"FOO.txt\n");
    put(&dir, "foo.txt", b"");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-name", "foo.txt"],
    );
    assert!(out.stdout.is_empty(), "{}", out.stdout);

    put(
        &home,
        "gitconfig",
        b"[core]\n\tignorecase = 0X0\n\tprecomposeunicode = 1m\n",
    );
    put(&home, "gitconfig2", b"[core]\n\tignorecase = 1g\n");
    fs::remove_dir_all(&dir).unwrap();

    let dir = git_repo("gitdir_edges");
    put(&dir, "work/.git", b"gitdir:\n");
    put(&dir, "work/a.txt", b"");
    let _ = names(&dir.join("work"), &["--gitignore"]);
    put(&dir, "rel/.git", b"gitdir: ../store\n");
    put(&dir, "store/info/exclude", b"*.log\n");
    put(&dir, "rel/a.log", b"");
    put(&dir, "rel/b.txt", b"");
    let got = names(&dir.join("rel"), &["--gitignore", "-type", "f"]);
    assert_eq!(got, ["b.txt"]);
    put(&dir, "abs/.git", b"gitdir: ../store2\n");
    put(&dir, "store2/commondir", b"");
    put(&dir, "store2/info/exclude", b"*.tmp\n");
    put(&dir, "abs/a.tmp", b"");
    put(&dir, "abs/b.txt", b"");
    let got = names(&dir.join("abs"), &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"b.txt".to_string()), "{got:?}");
    put(
        &dir,
        "abs2/.git",
        format!("gitdir: {}\n", dir.join("store3").display()).as_bytes(),
    );
    put(
        &dir,
        "store3/commondir",
        format!("{}\n", dir.join("common").display()).as_bytes(),
    );
    put(&dir, "common/info/exclude", b"*.bak\n");
    put(&dir, "abs2/a.bak", b"");
    put(&dir, "abs2/b.txt", b"");
    let got = names(&dir.join("abs2"), &["--gitignore", "-type", "f"]);
    assert_eq!(got, ["b.txt"]);
    put(&dir, "flag/.git/config", b"[core]\n\tignorecase\n");
    put(&dir, "flag/.gitignore", b"FOO.txt\n");
    put(&dir, "flag/foo.txt", b"");
    let got = names(&dir.join("flag"), &["--gitignore", "-name", "foo.txt"]);
    assert!(got.is_empty(), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitconfig_malformed_sections_and_values() {
    let dir = git_repo("cfg_mal2");
    let home = dir.join("home");
    put(&dir, "a.log", b"");
    put(&dir, "keep.md", b"");
    let bodies = [
        "[core!]\n\texcludesFile = LIST\n",
        "[includeIf \"x\\n\"]\n\tpath = LIST\n",
        "[includeIf \"x\"] \n",
        "[core]\n\texcludesFile = LIST # c\n",
        "[core]\n\texcludesFile = LIST\r\n",
        "[core]\n\tignorecase\r\n",
        "[core]\nignorecase",
        "[core]\n\texcludesFile = ~\n",
        "[includeIf \"gitdir:./\"]\n\tpath = LIST\n",
        "[includeIf \"onbranch:\"]\n\tpath = LIST\n",
        "[includeIf \"unknown:x\"]\n\tpath = LIST\n",
        "[core]\n\texcludesFile = LIST\\\r\nmore\n",
        "[core]\n\texcludesFile = \"LIST\\t\"\n",
        "[core]\n\texcludesFile = \"LIST\\b\"\n",
        "[core]\n\texcludesFile = \"LIST\\\\\"\n",
        "[core]\n\texcludesFile = \"LIST\\\"\"\n",
    ];
    for (i, body) in bodies.iter().enumerate() {
        put(
            &home,
            &format!("c{i}"),
            body.replace("LIST", &home.join("list").display().to_string())
                .as_bytes(),
        );
        put(&home, "list", b"*.log\n");
        let _ = run_env(
            &[("GIT_CONFIG_GLOBAL", home.join(format!("c{i}")).as_os_str())],
            &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
        );
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn glob_name_edges_via_cli() {
    let dir = scratch("glob_cli");
    put(&dir, "a", b"");
    put(&dir, "ab", b"");
    assert_eq!(names(&dir, &["-name", "\\"]), [] as [&str; 0]);
    assert_eq!(names(&dir, &["-name", "a?"]), ["ab"]);
    assert_eq!(names(&dir, &["-name", "a*b"]), ["ab"]);
    assert_eq!(names(&dir, &["-name", "["]), [] as [&str; 0]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn name_star_against_slash_root() {
    let out = run(&["/", "-maxdepth", "0", "-name", "*"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.lines().any(|l| l == "/"), "{}", out.stdout);
    let miss = run(&["/", "-maxdepth", "0", "-name", "*z"]);
    assert!(miss.code == 0 || miss.code == 1, "{}", miss.stderr);
    assert!(!miss.stdout.lines().any(|l| l == "/"), "{}", miss.stdout);
}

#[test]
fn name_star_matches_dot_slash_and_trailing_slash() {
    let dir = scratch("name_slash");
    put(&dir, "keep.txt", b"");
    let out = run_at(&dir, &["./", "-maxdepth", "0", "-name", "*"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.lines().any(|l| l == "./" || l == "."),
        "{}",
        out.stdout
    );
    let mut root = dir.to_string_lossy().into_owned();
    root.push('/');
    let named = run(&[
        &root,
        "-maxdepth",
        "0",
        "-name",
        dir.file_name().unwrap().to_str().unwrap(),
    ]);
    assert!(
        named.stdout.contains(&root) || named.stdout.contains(dir.to_str().unwrap()),
        "{}",
        named.stdout
    );
    let starred = run(&[&root, "-maxdepth", "0", "-name", "*"]);
    assert!(
        starred
            .stdout
            .lines()
            .any(|l| l == root || l == dir.to_str().unwrap()),
        "{}",
        starred.stdout
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn regex_alternation_matches_longer_alternative() {
    let dir = scratch("re_alt");
    put(&dir, "ab", b"");
    let path = dir.join("ab");
    let p = path.to_str().unwrap();
    let prefix = p.strip_suffix("ab").unwrap();
    let out = run(&[p, "-regex", &format!("{prefix}a|{prefix}ab")]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("ab"), "{}", out.stdout);
    let foo = run(&[p, "-regex", &format!("{p}|{p}x")]);
    assert!(foo.stdout.contains("ab"), "{}", foo.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn name_posix_digit_class() {
    let dir = scratch("posix");
    put(&dir, "1", b"");
    put(&dir, "a", b"");
    put(&dir, "7", b"");
    assert_eq!(names(&dir, &["-name", "[[:digit:]]"]), ["1", "7"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_file_root_ignores_dir_only_rule() {
    let dir = git_repo("file_root_dironly");
    put(&dir, ".gitignore", b"build/\n");
    put(&dir, "build", b"file");
    let out = run(&["--gitignore", dir.join("build").to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("build"), "{}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_p_uses_link_name_not_target() {
    let dir = git_repo("link_root_p");
    put(&dir, ".gitignore", b"hidden/\n");
    put(&dir, "hidden/a.txt", b"");
    std::os::unix::fs::symlink("hidden", dir.join("visible")).unwrap();
    let out = run(&["--gitignore", "-P", dir.join("visible").to_str().unwrap()]);
    assert!(out.stdout.contains("visible"), "{}", out.stdout);
    assert!(!out.stdout.contains("a.txt"), "{}", out.stdout);
    put(&dir, ".gitignore", b"visible\n");
    put(&dir, "keep/a.txt", b"");
    std::os::unix::fs::symlink("keep", dir.join("named")).unwrap();
    put(&dir, ".gitignore", b"named\nkeep/\n");
    let skipped = run(&["--gitignore", "-P", dir.join("named").to_str().unwrap()]);
    assert!(skipped.stdout.is_empty(), "{}", skipped.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_l_skips_ignored_target_and_git_link() {
    let dir = git_repo("l_ign_target");
    put(&dir, ".gitignore", b"hidden/\n");
    put(&dir, "hidden/secret.txt", b"");
    std::os::unix::fs::symlink("hidden", dir.join("visible")).unwrap();
    let out = run(&["-L", "--gitignore", dir.to_str().unwrap()]);
    assert!(!out.stdout.contains("secret.txt"), "{}", out.stdout);
    assert!(
        !listed(&out.stdout, &dir)
            .iter()
            .any(|n| n == "visible" || n.starts_with("visible/")),
        "{}",
        out.stdout
    );
    fs::remove_dir_all(&dir).unwrap();

    let dir = git_repo("l_git_link");
    put(&dir, ".git/objects/pack/x", b"");
    std::os::unix::fs::symlink(".git", dir.join("notgit")).unwrap();
    let got = names(&dir, &["-L", "--gitignore"]);
    assert!(
        !got.iter()
            .any(|n| n.contains("objects") || n == "notgit" || n.starts_with("notgit/")),
        "{got:?}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_fold_ranges_negation_and_escapes() {
    let dir = git_repo("ig_fold");
    put(&dir, ".git/config", b"[core]\n\tignorecase = true\n");
    put(
        &dir,
        ".gitignore",
        b"[A-C].txt\n[!x].log\n[a\\-c]\n[d-\\f]\n[[:upper:]].u\n[:].t\n[[:].log\n",
    );
    put(&dir, "b.txt", b"");
    put(&dir, "keep.txt", b"");
    put(&dir, "y.log", b"");
    put(&dir, "x.log", b"");
    put(&dir, "a-c", b"");
    put(&dir, "e", b"");
    put(&dir, "Z.u", b"");
    put(&dir, ":.t", b"");
    put(&dir, "[.log", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(!got.contains(&"b.txt".to_string()), "{got:?}");
    assert!(got.contains(&"keep.txt".to_string()), "{got:?}");
    assert!(!got.contains(&"y.log".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitconfig_include_relative_and_missing_value() {
    let dir = git_repo("inc_rel");
    let home = dir.join("home");
    put(&home, "inc", b"[core]\n\tignorecase = -2\n");
    put(&home, "gitconfig", b"[include]\n\tpath = inc\n");
    put(&dir, ".gitignore", b"FOO.txt\n");
    put(&dir, "foo.txt", b"");
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-name", "foo.txt"],
    );
    assert!(out.stdout.is_empty(), "{}", out.stdout);

    put(&home, "gitconfig", b"[include]\n\tpath\n");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(
        &home,
        "gitconfig",
        b"[include]\n\tpath = ~hfind-no-such-user/x\n",
    );
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(&home, "gitconfig", b"[core]\n\tignorecase = 2M\n");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-name", "foo.txt"],
    );
    put(&home, "gitconfig", b"[core]\n\tignorecase = 1G\n");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-name", "foo.txt"],
    );
    put(&home, "gitconfig", b"[core]\n\tignorecase = k\n");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(&home, "qend", b"[core]\n\texcludesFile = \"LIST");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("qend").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(&home, "eof", b"[core]\n\tprecomposeunicode = true");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("eof").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(&home, "badsec", b"[includeIf \"x\" extra]\n\tpath = inc\n");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("badsec").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitconfig_gitdir_none_and_detached() {
    let dir = git_repo("detached");
    put(&dir, ".git/HEAD", b"abc123\n");
    let home = dir.join("home");
    put(
        &home,
        "gitconfig",
        b"[includeIf \"onbranch:main\"]\n\tpath = inc\n[includeIf \"gitdir:[\"]\n\tpath = inc\n",
    );
    put(&home, "inc", b"[core]\n\tignorecase = true\n");
    put(&dir, "a.txt", b"");
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    put(&dir, "empty/.git", b"gitdir:\n");
    put(
        &home,
        "gitconfig",
        b"[includeIf \"gitdir:foo/\"]\n\tpath = inc\n[includeIf \"onbranch:x\"]\n\tpath = inc\n",
    );
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &[
            "--gitignore",
            dir.join("empty").to_str().unwrap(),
            "-maxdepth",
            "0",
        ],
    );
    put(
        &home,
        "nlsec",
        b"[includeIf \"x\\\n\"]\n\tpath = inc\n[includeIf \"x\" y]\n\tpath = inc\n",
    );
    let _ = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("nlsec").as_os_str())],
        &["--gitignore", dir.to_str().unwrap(), "-maxdepth", "0"],
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn name_posix_classes() {
    let dir = scratch("posix_all");
    put(&dir, "q", b"");
    put(&dir, "1", b"");
    put(&dir, "Z", b"");
    put(&dir, "!", b"");
    put(&dir, " ", b"");
    put(&dir, "\t", b"");
    put(&dir, "\u{0001}", b"");
    put(&dir, "f", b"");
    let files = |pat: &str| {
        names(&dir, &["-name", pat])
            .into_iter()
            .filter(|n| n != ".")
            .collect::<Vec<_>>()
    };
    let ifiles = |pat: &str| {
        names(&dir, &["-iname", pat])
            .into_iter()
            .filter(|n| n != ".")
            .collect::<Vec<_>>()
    };
    assert_eq!(files("[[:alnum:]]"), ["1", "Z", "f", "q"]);
    assert_eq!(files("[[:alpha:]]"), ["Z", "f", "q"]);
    assert_eq!(files("[[:blank:]]"), ["\t", " "]);
    assert_eq!(files("[[:cntrl:]]"), ["\u{0001}", "\t"]);
    assert_eq!(files("[[:digit:]]"), ["1"]);
    assert_eq!(files("[[:graph:]]"), ["!", "1", "Z", "f", "q"]);
    assert_eq!(files("[[:lower:]]"), ["f", "q"]);
    assert_eq!(files("[[:print:]]"), [" ", "!", "1", "Z", "f", "q"]);
    assert_eq!(files("[[:punct:]]"), ["!"]);
    assert_eq!(files("[[:space:]]"), ["\t", " "]);
    assert_eq!(files("[[:upper:]]"), ["Z"]);
    assert_eq!(ifiles("[[:upper:]]"), ["Z", "f", "q"]);
    assert_eq!(files("[[:xdigit:]]"), ["1", "f"]);
    assert_eq!(files("[[:nope:]]"), [] as [&str; 0]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_l_broken_root_is_loose() {
    let dir = scratch("broken_l_root");
    std::os::unix::fs::symlink("missing", dir.join("gone")).unwrap();
    let out = run(&["-L", "--gitignore", dir.join("gone").to_str().unwrap()]);
    assert!(out.stdout.contains("gone"), "{}", out.stdout);
    assert_eq!(out.code, 1, "{}", out.stderr);
    let h = run(&["-H", "--gitignore", dir.join("gone").to_str().unwrap()]);
    assert!(h.stdout.contains("gone"), "{}", h.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_l_dir_link_outside_a_repository() {
    let dir = scratch("out_link");
    let ambient = dir
        .parent()
        .unwrap()
        .ancestors()
        .find(|d| d.join(".git").exists());
    assert!(
        ambient.is_none(),
        "this test needs a temp directory outside every repository, but {ambient:?} holds one"
    );
    put(&dir, "real/a.txt", b"");
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    let got = names(&dir, &["-L", "--gitignore"]);
    assert!(got.contains(&"link/a.txt".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_l_dir_link_points_outside_repo() {
    let dir = git_repo("out_repo_link");
    let other = scratch("out_repo_tgt");
    put(&other, "x.txt", b"");
    std::os::unix::fs::symlink(&other, dir.join("out")).unwrap();
    put(&dir, "keep.md", b"");
    let got = names(&dir, &["-L", "--gitignore", "-type", "f"]);
    assert!(got.contains(&"keep.md".to_string()), "{got:?}");
    assert!(
        got.iter()
            .any(|n| n == "out/x.txt" || n.ends_with("/x.txt")),
        "{got:?}"
    );
    fs::remove_dir_all(&dir).unwrap();
    fs::remove_dir_all(&other).unwrap();
}

#[test]
fn double_not_and_implied_and() {
    let dir = scratch("double_not");
    put(&dir, "a.txt", b"");
    put(&dir, "b.log", b"");
    assert_eq!(names(&dir, &["!", "!", "-name", "a.txt"]), ["a.txt"]);
    assert_eq!(names(&dir, &["-not", "-not", "-name", "b.log"]), ["b.log"]);
    assert_eq!(names(&dir, &["-false", "-o", "-name", "a.txt"]), ["a.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn size_overflow_and_empty_maxdepth_arg() {
    let dir = scratch("size_ov");
    put(&dir, "a", b"");
    let out = run(&[dir.to_str().unwrap(), "-size", "18446744073709551616c"]);
    assert_eq!(out.code, 2, "{}", out.stderr);
    let out = run(&[dir.to_str().unwrap(), "-maxdepth", ""]);
    assert_eq!(out.code, 2, "{}", out.stderr);
    let out = run(&[dir.to_str().unwrap(), "-mtime", ""]);
    assert_eq!(out.code, 2, "{}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_double_star_and_question() {
    let dir = git_repo("ig_dstar");
    put(&dir, ".gitignore", b"a/**\n**/end\n??\n*\n!keep.md\n");
    put(&dir, "a/x/y", b"");
    put(&dir, "z/end", b"");
    put(&dir, "ab", b"");
    put(&dir, "keep.md", b"");
    let got = names(&dir, &["--gitignore", "-type", "f"]);
    assert!(got.contains(&"keep.md".to_string()), "{got:?}");
    assert!(!got.contains(&"a/x/y".to_string()), "{got:?}");
    assert!(!got.contains(&"z/end".to_string()), "{got:?}");
    fs::remove_dir_all(&dir).unwrap();
}
