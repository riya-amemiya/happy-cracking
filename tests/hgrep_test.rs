use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
            std::env::temp_dir().join(format!("hgrep_sandbox_{}_{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    })
}

fn command() -> Command {
    let home = sandbox();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hgrep"));
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_GLOBAL", home.join("absent-config"))
        .env("GIT_CONFIG_SYSTEM", home.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

fn hgrep(args: &[&str], stdin: &str) -> (String, i32) {
    let mut child = command()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn hgrep");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

const SAMPLE: &str = "alpha\nBeta\ngamma beta\ndelta\n";

#[test]
fn plain_literal_match() {
    assert_eq!(hgrep(&["beta"], SAMPLE), ("gamma beta\n".to_string(), 0));
}

#[test]
fn no_match_exits_one() {
    assert_eq!(hgrep(&["zzz"], SAMPLE), (String::new(), 1));
}

#[test]
fn ignore_case() {
    assert_eq!(
        hgrep(&["-i", "beta"], SAMPLE),
        ("Beta\ngamma beta\n".to_string(), 0)
    );
}

#[test]
fn invert_match() {
    assert_eq!(
        hgrep(&["-v", "beta"], SAMPLE),
        ("alpha\nBeta\ndelta\n".to_string(), 0)
    );
    assert_eq!(hgrep(&["-v", "a"], SAMPLE), (String::new(), 1));
}

#[test]
fn line_numbers_and_count() {
    assert_eq!(hgrep(&["-n", "beta"], SAMPLE).0, "3:gamma beta\n");
    assert_eq!(hgrep(&["-c", "a"], SAMPLE).0, "4\n");
    assert_eq!(hgrep(&["-c", "beta"], SAMPLE).0, "1\n");
}

#[test]
fn word_and_line_anchors() {
    assert_eq!(hgrep(&["-w", "beta"], SAMPLE).0, "gamma beta\n");
    assert_eq!(hgrep(&["-w", "bet"], SAMPLE).1, 1);
    assert_eq!(hgrep(&["-x", "delta"], SAMPLE).0, "delta\n");
    assert_eq!(hgrep(&["-x", "delt"], SAMPLE).1, 1);
}

#[test]
fn regex_and_fixed_strings() {
    assert_eq!(hgrep(&["^d.*a$"], SAMPLE).0, "delta\n");
    assert_eq!(hgrep(&["-F", "^d.*a$"], SAMPLE).1, 1);
}

#[test]
fn only_matching_and_max_count() {
    assert_eq!(hgrep(&["-o", "a"], "aXaXa\nbab\n").0, "a\na\na\na\n");
    assert_eq!(hgrep(&["-m", "2", "a"], SAMPLE).0, "alpha\nBeta\n");
}

#[test]
fn multiple_patterns() {
    assert_eq!(
        hgrep(&["-e", "alpha", "-e", "delta"], SAMPLE).0,
        "alpha\ndelta\n"
    );
}

#[test]
fn missing_final_newline_still_matches() {
    assert_eq!(hgrep(&["tail"], "head\ntail").0, "tail\n");
}

fn scratch(tag: &str) -> PathBuf {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "hgrep_{tag}_{}_{nanos}_{}",
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

fn seed(dir: &Path, rels: &[&str]) {
    for rel in rels {
        put(dir, rel, b"needle\n");
    }
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
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
    }
}

fn run(args: &[&str]) -> Run {
    run_env(&[], args)
}

fn run_bytes(args: &[&str]) -> Vec<u8> {
    command().args(args).output().unwrap().stdout
}

fn holds(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
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

fn found_in(root: &Path, args: &[&str]) -> Vec<String> {
    let out = run(&[args, &["needle", root.to_str().unwrap()]].concat());
    rels(&out.stdout, root)
}

fn ignored_view(dir: &Path) -> Vec<String> {
    found_in(dir, &["-rl", "--gitignore"])
}

#[test]
fn recursive_over_directory() {
    let dir = scratch("tree");
    seed(&dir, &["a.txt"]);
    put(&dir, "sub/b.txt", b"nothing\n");

    let out = run(&["-r", "needle", dir.to_str().unwrap()]);
    assert!(out.stdout.contains("a.txt:needle"), "got {:?}", out.stdout);
    assert!(!out.stdout.contains("b.txt"), "got {:?}", out.stdout);

    let listed = run(&["-rl", "needle", dir.to_str().unwrap()]);
    assert_eq!(
        listed.stdout.trim(),
        dir.join("a.txt").to_string_lossy(),
        "got {:?}",
        listed.stdout
    );

    fs::remove_dir_all(&dir).unwrap();
}

fn build_ignore_tree(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    for sub in ["target", "logs", "sub/deep", "excluded/nested", "keepdir"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
    }
    put(
        &dir,
        ".gitignore",
        b"# comment\n*.log\n!keep.log\n/target\nlogs/\nexcluded/\n!excluded/keep.txt\n",
    );
    put(&dir, "sub/.gitignore", b"!*.log\n");
    put(&dir, "sub/deep/.gitignore", b"*.log\n");
    put(&dir, "logs/.gitignore", b"!l.txt\n");
    seed(
        &dir,
        &[
            "a.txt",
            "a.log",
            "keep.log",
            "target/t.txt",
            "logs/l.txt",
            "sub/s.log",
            "sub/s.txt",
            "sub/deep/d.log",
            "sub/deep/d.txt",
            "excluded/keep.txt",
            "excluded/nested/n.txt",
            "keepdir/k.txt",
            ".git/config",
        ],
    );
    dir
}

#[test]
fn gitignore_is_off_by_default() {
    let dir = build_ignore_tree("gitignore_off");
    assert_eq!(
        found_in(&dir, &["-rl"]),
        [
            ".git/config",
            "a.log",
            "a.txt",
            "excluded/keep.txt",
            "excluded/nested/n.txt",
            "keep.log",
            "keepdir/k.txt",
            "logs/l.txt",
            "sub/deep/d.log",
            "sub/deep/d.txt",
            "sub/s.log",
            "sub/s.txt",
            "target/t.txt",
        ]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_flag_applies_nesting_negation_and_pruning() {
    let dir = build_ignore_tree("gitignore_on");
    assert_eq!(
        ignored_view(&dir),
        [
            "a.txt",
            "keep.log",
            "keepdir/k.txt",
            "sub/deep/d.txt",
            "sub/s.log",
            "sub/s.txt",
        ]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_matches_anchors_classes_and_double_stars() {
    let dir = scratch("gitignore_globs");
    put(
        &dir,
        ".gitignore",
        b"a/**/b\n/rooted.txt\ndoc/frotz\nf[a-z]9\nstar\\*x\nq?z\ntrail \n",
    );
    seed(
        &dir,
        &[
            "a/b",
            "a/x/b",
            "a/x/y/b",
            "a/keep",
            "rooted.txt",
            "sub/rooted.txt",
            "doc/frotz",
            "sub/doc/frotz",
            "fa9",
            "f_9",
            "star*x",
            "starAx",
            "qZz",
            "trail",
        ],
    );
    assert_eq!(
        ignored_view(&dir),
        ["a/keep", "f_9", "starAx", "sub/doc/frotz", "sub/rooted.txt"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_rules_above_the_search_root() {
    let dir = scratch("gitignore_ancestor");
    put(&dir, ".gitignore", b"*.log\n");
    put(&dir, "src/.gitignore", b"!kept.log\n");
    seed(&dir, &["src/a.log", "src/b.txt", "src/kept.log"]);
    let root = dir.join("src");
    assert_eq!(ignored_view(&root), ["b.txt", "kept.log"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_keeps_rules_of_file_holding_invalid_utf8() {
    let dir = scratch("gitignore_invalid_utf8");
    put(&dir, ".gitignore", b"# caf\xe9\n*.log\n");
    seed(&dir, &["a.log", "b.txt"]);
    assert_eq!(ignored_view(&dir), ["b.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_strips_leading_byte_order_mark() {
    let dir = scratch("gitignore_bom");
    put(&dir, ".gitignore", b"\xef\xbb\xbf*.log\n");
    seed(&dir, &["a.log", "b.txt"]);
    assert_eq!(ignored_view(&dir), ["b.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_wildcards_consume_one_byte_not_one_codepoint() {
    let one = scratch("gitignore_wild_one");
    put(&one, ".gitignore", b"?.log\n");
    seed(&one, &["a.log", "\u{e9}.log"]);
    assert_eq!(ignored_view(&one), ["\u{e9}.log"]);
    fs::remove_dir_all(&one).unwrap();

    let two = scratch("gitignore_wild_two");
    put(&two, ".gitignore", b"??.log\n");
    seed(&two, &["ab.log", "\u{e9}.log", "abc.log"]);
    assert_eq!(ignored_view(&two), ["abc.log"]);
    fs::remove_dir_all(&two).unwrap();

    let cls = scratch("gitignore_wild_class");
    put(&cls, ".gitignore", b"[!a].log\n");
    seed(&cls, &["a.log", "\u{e9}.log", "b.log"]);
    assert_eq!(ignored_view(&cls), ["a.log", "\u{e9}.log"]);
    fs::remove_dir_all(&cls).unwrap();
}

#[test]
fn gitignore_class_takes_inner_bracket_as_member() {
    let dir = scratch("gitignore_class_bracket");
    put(&dir, ".gitignore", b"[a[b].log\n");
    seed(&dir, &["a.log", "b.log", "c.log", "[.log"]);
    assert_eq!(ignored_view(&dir), ["c.log"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_class_of_colon_body_is_literal_set() {
    let dir = scratch("gitignore_class_colon");
    put(&dir, ".gitignore", b"[:alpha:].log\n");
    seed(&dir, &["a.log", "h.log", ":.log", "b.log", "z.log"]);
    assert_eq!(ignored_view(&dir), ["b.log", "z.log"]);
    fs::remove_dir_all(&dir).unwrap();

    let posix = scratch("gitignore_class_posix");
    put(&posix, ".gitignore", b"[[:digit:]].log\n");
    seed(&posix, &["1.log", "a.log"]);
    assert_eq!(ignored_view(&posix), ["a.log"]);
    fs::remove_dir_all(&posix).unwrap();
}

#[test]
fn gitignore_class_dash_run_is_not_set_difference() {
    let dir = scratch("gitignore_class_dash");
    put(&dir, ".gitignore", b"[a--b].log\n");
    seed(&dir, &["a.log", "b.log", "c.log"]);
    assert_eq!(ignored_view(&dir), ["c.log"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_unterminated_class_matches_nothing() {
    let dir = scratch("gitignore_class_open");
    put(&dir, ".gitignore", b"[abc.txt\na[b\n");
    seed(&dir, &["[abc.txt", "a[b", "ab", "keep.txt"]);
    assert_eq!(ignored_view(&dir), ["[abc.txt", "a[b", "ab", "keep.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_class_keeps_bracket_member_and_reverse_range() {
    let dir = scratch("gitignore_class_odd");
    put(&dir, ".gitignore", b"[[]\n[z-a]\n");
    seed(&dir, &["[", "z", "a", "keep"]);
    assert_eq!(ignored_view(&dir), ["a", "keep"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_applies_every_rule_of_a_large_file() {
    let dir = scratch("gitignore_large");
    let rules: String = (0..8000).map(|i| format!("build{i}/\n")).collect();
    put(&dir, ".gitignore", format!("{rules}*.log\n").as_bytes());
    seed(&dir, &["x.log", "y.txt"]);
    assert_eq!(ignored_view(&dir), ["y.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reports_an_unreadable_rule_file() {
    let dir = scratch("gitignore_unreadable");
    fs::create_dir_all(dir.join(".gitignore")).unwrap();
    seed(&dir, &["x.log", "y.txt"]);
    let out = run(&["-rl", "--gitignore", "needle", dir.to_str().unwrap()]);
    assert_eq!(rels(&out.stdout, &dir), ["x.log", "y.txt"]);
    assert!(out.stderr.contains(".gitignore"), "got {:?}", out.stderr);
    assert_eq!(out.code, 2, "stderr {:?}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_matches_across_a_newline_in_a_path() {
    let dir = scratch("gitignore_newline");
    put(&dir, ".gitignore", b"*.log\n**/z.txt\n");
    seed(
        &dir,
        &["di\nr/x.log", "di\nr/z.txt", "di\nr/keep.txt", "top.log"],
    );
    let out = run(&["-rl", "--gitignore", "needle", dir.to_str().unwrap()]);
    assert_eq!(
        out.stdout,
        format!("{}/di\nr/keep.txt\n", dir.display()),
        "stderr {:?}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_dir_only_rule_keeps_a_plain_file_of_that_name() {
    let dir = scratch("gitignore_dir_only");
    put(&dir, ".gitignore", b"logs/\nbuild/\n");
    seed(&dir, &["logs", "keep.txt", "logs_dir/inside.txt"]);
    fs::create_dir_all(dir.join("build")).unwrap();
    seed(&dir, &["build/hidden.txt"]);
    assert_eq!(
        ignored_view(&dir),
        ["keep.txt", "logs", "logs_dir/inside.txt"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_anchors_a_nested_rule_to_its_own_directory() {
    let dir = scratch("gitignore_nested_anchor");
    put(&dir, "sub/.gitignore", b"/only-here.txt\n");
    seed(
        &dir,
        &[
            "sub/only-here.txt",
            "sub/deep/only-here.txt",
            "sub/other.txt",
        ],
    );
    assert_eq!(
        ignored_view(&dir),
        ["sub/deep/only-here.txt", "sub/other.txt"]
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_skips_a_git_pointer_file() {
    let dir = scratch("gitignore_git_file");
    put(&dir, "wt/.git", b"gitdir: /elsewhere/needle\n");
    seed(&dir, &["wt/a.txt"]);
    let root = dir.join("wt");
    assert_eq!(ignored_view(&root), ["a.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_the_repository_exclude_file() {
    let dir = scratch("info_exclude");
    put(&dir, ".git/info/exclude", b"*.log\n");
    seed(&dir, &["a.log", "b.txt"]);
    assert_eq!(ignored_view(&dir), ["b.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_the_global_excludes_file() {
    let dir = scratch("global_excludes");
    let home = dir.join("home");
    put(&home, "git/ignore", b"*.log\n");
    seed(&dir, &["a.log", "b.txt"]);
    let out = run_env(
        &[
            ("XDG_CONFIG_HOME", home.as_os_str()),
            ("HOME", home.as_os_str()),
        ],
        &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
    );
    assert_eq!(
        rels(&out.stdout, &dir),
        ["b.txt"],
        "stderr {:?}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_ranks_per_directory_above_repository_above_global() {
    let dir = scratch("exclude_precedence");
    let home = dir.join("home");
    let global = home.join("globalignore");
    put(&home, "globalignore", b"a.txt\n!g.txt\n");
    put(
        &home,
        "gitconfig",
        format!("[core]\n\texcludesFile = {}\n", global.display()).as_bytes(),
    );
    put(&dir, ".git/info/exclude", b"!a.txt\nb.txt\ng.txt\n");
    put(&dir, ".gitignore", b"!b.txt\nc.txt\n");
    seed(&dir, &["a.txt", "b.txt", "c.txt", "g.txt", "keep.txt"]);
    let out = run_env(
        &[
            ("HOME", home.as_os_str()),
            ("XDG_CONFIG_HOME", home.as_os_str()),
            ("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str()),
        ],
        &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
    );
    assert_eq!(
        rels(&out.stdout, &dir),
        ["a.txt", "b.txt", "keep.txt"],
        "stderr {:?}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_honors_core_ignorecase() {
    let on = scratch("ignorecase_on");
    put(&on, ".git/config", b"[core]\n\tignorecase = true\n");
    put(&on, ".gitignore", b"FOO.txt\n");
    seed(&on, &["foo.txt", "keep.txt"]);
    assert_eq!(ignored_view(&on), ["keep.txt"]);
    fs::remove_dir_all(&on).unwrap();

    let off = scratch("ignorecase_off");
    put(&off, ".git/config", b"[core]\n\tignorecase = false\n");
    put(&off, ".gitignore", b"FOO.txt\n");
    seed(&off, &["foo.txt", "keep.txt"]);
    assert_eq!(ignored_view(&off), ["foo.txt", "keep.txt"]);
    fs::remove_dir_all(&off).unwrap();
}

#[test]
fn gitignore_matches_a_name_holding_invalid_utf8() {
    let dir = scratch("invalid_utf8_name");
    let raw: &[u8] = b"bad\xffname.log";
    let name = OsStr::from_bytes(raw);
    if let Err(e) = fs::write(dir.join(name), b"needle\n") {
        assert!(
            !dir.join(name).exists(),
            "creation reported {e} yet the name exists"
        );
        assert!(
            fs::write(dir.join("control.log"), b"needle\n").is_ok(),
            "the directory itself is unwritable, so the refusal was not about the name"
        );
        fs::remove_dir_all(&dir).unwrap();
        return;
    }
    put(&dir, "keep.txt", b"needle\n");

    let root = dir.to_str().unwrap();
    let reachable = run_bytes(&["-rl", "--gitignore", "needle", root]);
    assert!(holds(&reachable, raw), "got {reachable:?}");
    assert!(holds(&reachable, b"keep.txt"), "got {reachable:?}");

    put(&dir, ".gitignore", b"*.log\n");
    let filtered = run_bytes(&["-rl", "--gitignore", "needle", root]);
    assert!(!holds(&filtered, raw), "got {filtered:?}");
    assert!(holds(&filtered, b"keep.txt"), "got {filtered:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_question_mark_consumes_exactly_one_byte() {
    let dir = scratch("qmark_bytes");
    put(&dir, ".gitignore", b"???.log\n");
    let wide: &[u8] = b"\xe2\x82\xac.log";
    fs::write(dir.join(OsStr::from_bytes(wide)), b"needle\n").unwrap();
    seed(&dir, &["ab.log", "keep.txt"]);

    let out = run_bytes(&["-rl", "--gitignore", "needle", dir.to_str().unwrap()]);
    assert!(!holds(&out, wide), "got {out:?}");
    assert!(holds(&out, b"ab.log"), "got {out:?}");
    assert!(holds(&out, b"keep.txt"), "got {out:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_flag_demands_recursive() {
    let dir = scratch("gitignore_needs_r");
    seed(&dir, &["a.txt"]);
    let out = run(&["--gitignore", "needle", dir.join("a.txt").to_str().unwrap()]);
    assert_eq!(out.code, 2, "stdout {:?}", out.stdout);
    assert!(out.stderr.contains("--recursive"), "got {:?}", out.stderr);
    assert!(out.stdout.is_empty(), "got {:?}", out.stdout);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_flag_is_documented_in_help() {
    let out = run(&["--help"]);
    let line = out
        .stdout
        .lines()
        .find(|l| l.trim_start().starts_with("--gitignore"))
        .unwrap_or_else(|| panic!("no --gitignore row in {:?}", out.stdout));
    let text = line.trim_start().trim_start_matches("--gitignore").trim();
    assert!(text.contains(".gitignore"), "got {line:?}");
    assert!(text.contains("-r"), "got {line:?}");
}

#[test]
fn files_without_match_exits_zero_when_it_lists_a_file() {
    let dir = scratch("without_match");
    put(&dir, "a.txt", b"hello\n");
    let path = dir.join("a.txt");
    let name = path.to_str().unwrap();

    let listed = run(&["-L", "zzz", name]);
    assert_eq!(listed.stdout.trim(), name);
    assert_eq!(listed.code, 0);

    let empty = run(&["-L", "hello", name]);
    assert!(empty.stdout.is_empty(), "got {:?}", empty.stdout);
    assert_eq!(empty.code, 1);

    assert_eq!(run(&["-l", "hello", name]).code, 0);
    assert_eq!(run(&["-qL", "zzz", name]).code, 0);
    assert_eq!(hgrep(&["-L", "zzz"], "hi\n").1, 0);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn directory_operand_without_recursive_exits_two() {
    let dir = scratch("dir_operand");
    seed(&dir, &["a.txt"]);
    let out = run(&["needle", dir.to_str().unwrap()]);
    assert!(
        out.stderr.contains("Is a directory"),
        "got {:?}",
        out.stderr
    );
    assert_eq!(out.code, 2);
    assert_eq!(run(&["-s", "needle", dir.to_str().unwrap()]).code, 2);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fixture_roots_never_collide() {
    let first = scratch("collide");
    let second = scratch("collide");
    assert_ne!(first, second);
    seed(&first, &["x.txt"]);
    seed(&second, &["x.txt"]);
    assert_eq!(found_in(&first, &["-rl"]), ["x.txt"]);
    assert_eq!(found_in(&second, &["-rl"]), ["x.txt"]);
    fs::remove_dir_all(&first).unwrap();
    fs::remove_dir_all(&second).unwrap();
}

#[test]
fn binary_file_is_reported_not_dumped() {
    let dir = scratch("binary_case");
    put(&dir, "case.bin", b"secret\x00\x01\x02payload\n");
    let out = run(&["secret", dir.join("case.bin").to_str().unwrap()]);
    assert!(
        out.stdout.starts_with("Binary file "),
        "got {:?}",
        out.stdout
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_config_and_exclude_through_a_git_pointer_file() {
    let dir = scratch("gitfile_sources");
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
    seed(&dir, &["work/keep.md", "work/drop.log", "work/drop.tmp"]);
    assert_eq!(ignored_view(&dir.join("work")), ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_follows_commondir_out_of_a_linked_worktree() {
    let dir = scratch("gitfile_commondir");
    put(&dir, "main/.git/info/exclude", b"*.log\n");
    put(&dir, "main/.git/worktrees/wt/commondir", b"../..\n");
    put(&dir, "main/.git/worktrees/wt/info/exclude", b"keep.md\n");
    put(
        &dir,
        "work/.git",
        format!("gitdir: {}\n", dir.join("main/.git/worktrees/wt").display()).as_bytes(),
    );
    seed(&dir, &["work/keep.md", "work/drop.log"]);
    assert_eq!(ignored_view(&dir.join("work")), ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_ignorecase_leaves_bracket_members_case_sensitive() {
    let dir = scratch("ignorecase_class");
    put(&dir, ".git/config", b"[core]\n\tignorecase = true\n");
    put(
        &dir,
        ".gitignore",
        b"[XY]1.txt\n[A-Z]2.txt\n[[:upper:]]3.txt\n\\Z4.txt\nz5.txt\n",
    );
    seed(
        &dir,
        &["x1.txt", "x2.txt", "x3.txt", "z4.txt", "Z5.txt", "keep.md"],
    );
    assert_eq!(ignored_view(&dir), ["keep.md", "x1.txt", "z4.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_ignorecase_folds_a_negated_bracket_the_same_way() {
    let dir = scratch("ignorecase_negclass");
    put(&dir, ".git/config", b"[core]\n\tignorecase = true\n");
    put(&dir, ".gitignore", b"[^XY]1.txt\n[^xy]2.txt\n");
    seed(&dir, &["x1.txt", "x2.txt", "a2.txt", "keep.md"]);
    assert_eq!(ignored_view(&dir), ["keep.md", "x2.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn gitignore_precomposes_decomposed_directory_entries() {
    let dir = scratch("precompose");
    let decomposed: &[u8] = b"e\xcc\x81drop.txt";
    put(&dir, ".git/config", b"[core]\n\tprecomposeunicode = true\n");
    put(&dir, ".gitignore", b"\xc3\xa9drop.txt\n\xc3\xa9dir/\n");
    fs::write(dir.join(OsStr::from_bytes(decomposed)), b"needle\n").unwrap();
    fs::create_dir_all(dir.join(OsStr::from_bytes(b"e\xcc\x81dir"))).unwrap();
    fs::write(
        dir.join(OsStr::from_bytes(b"e\xcc\x81dir")).join("a.txt"),
        b"needle\n",
    )
    .unwrap();
    put(&dir, "keep.md", b"needle\n");
    let root = dir.to_str().unwrap();

    let on = run_bytes(&["-rl", "--gitignore", "needle", root]);
    assert!(!holds(&on, decomposed), "got {on:?}");
    assert!(holds(&on, b"keep.md"), "got {on:?}");

    put(
        &dir,
        ".git/config",
        b"[core]\n\tprecomposeunicode = false\n",
    );
    let off = run_bytes(&["-rl", "--gitignore", "needle", root]);
    assert!(holds(&off, decomposed), "got {off:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_applies_the_inner_repository_sources_below_a_nested_root() {
    let dir = scratch("nested_repo");
    put(&dir, ".git/info/exclude", b"*.txt\n");
    put(&dir, "inner/.git/info/exclude", b"*.log\n");
    seed(&dir, &["outer.txt", "inner/keep.txt", "inner/drop.log"]);
    assert_eq!(ignored_view(&dir), ["inner/keep.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_the_exclude_file_of_a_repository_below_the_search_root() {
    let dir = scratch("repo_below_root");
    put(&dir, "outer/repo/.git/info/exclude", b"*.log\n");
    put(&dir, "outer/repo/.gitignore", b"*.tmp\n");
    seed(
        &dir,
        &["outer/repo/a.log", "outer/repo/b.tmp", "outer/repo/c.txt"],
    );
    assert_eq!(ignored_view(&dir.join("outer")), ["repo/c.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_skips_a_case_variant_of_the_git_directory() {
    let on = scratch("dotgit_case_on");
    put(&on, ".git/config", b"[core]\n\tignorecase = true\n");
    seed(&on, &["sub/.GIT/hidden.txt", "sub/keep.md"]);
    assert_eq!(ignored_view(&on), ["sub/keep.md"]);
    fs::remove_dir_all(&on).unwrap();

    let off = scratch("dotgit_case_off");
    put(&off, ".git/config", b"[core]\n\tignorecase = false\n");
    seed(&off, &["sub/.GIT/hidden.txt", "sub/keep.md"]);
    assert_eq!(ignored_view(&off), ["sub/.GIT/hidden.txt", "sub/keep.md"]);
    fs::remove_dir_all(&off).unwrap();
}

#[test]
fn gitignore_skips_a_search_root_the_repository_ignores() {
    let dir = scratch("root_ignored");
    put(&dir, ".gitignore", b"build/\n");
    seed(&dir, &["build/a.txt", "build/deep/b.txt", "keep.md"]);
    let out = run(&[
        "-rl",
        "--gitignore",
        "needle",
        dir.join("build").to_str().unwrap(),
    ]);
    assert!(out.stdout.is_empty(), "got {:?}", out.stdout);
    assert_eq!(out.code, 1, "stderr {:?}", out.stderr);
    assert_eq!(ignored_view(&dir), ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_applies_no_rule_outside_a_repository() {
    let dir = scratch("outside_repo");
    fs::remove_dir_all(dir.join(".git")).unwrap();
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
    seed(&dir, &["inner/a.log", "inner/b.txt"]);
    assert_eq!(ignored_view(&dir.join("inner")), ["a.log", "b.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_git_style_integer_booleans() {
    let dir = scratch("int_bool");
    put(&dir, ".git/config", b"[core]\n\tignorecase = 2\n");
    put(&dir, ".gitignore", b"FOO.txt\n");
    seed(&dir, &["foo.txt", "keep.md"]);
    assert_eq!(ignored_view(&dir), ["keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_reads_the_system_config_unless_nosystem_is_true() {
    let dir = scratch("system_config");
    let home = dir.join("home");
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "system",
        format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
    );
    seed(&dir, &["a.log", "keep.md"]);
    let args = ["-rl", "--gitignore", "needle", dir.to_str().unwrap()];

    let read = run_env(
        &[
            ("GIT_CONFIG_SYSTEM", home.join("system").as_os_str()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("0")),
        ],
        &args,
    );
    assert_eq!(rels(&read.stdout, &dir), ["keep.md"], "{:?}", read.stderr);

    let suppressed = run_env(
        &[
            ("GIT_CONFIG_SYSTEM", home.join("system").as_os_str()),
            ("GIT_CONFIG_NOSYSTEM", OsStr::new("2")),
        ],
        &args,
    );
    assert_eq!(
        rels(&suppressed.stdout, &dir),
        ["a.log", "keep.md"],
        "{:?}",
        suppressed.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_drops_an_excludes_file_with_an_unexpandable_tilde() {
    let dir = scratch("tilde_user");
    put(
        &dir,
        ".git/config",
        b"[core]\n\texcludesFile = ~hgrep-no-such-user/ig.list\n",
    );
    put(&dir, "~hgrep-no-such-user/ig.list", b"*.log\n");
    seed(&dir, &["a.log", "keep.md"]);
    assert_eq!(ignored_view(&dir), ["a.log", "keep.md"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_expands_a_home_relative_excludes_file() {
    let dir = scratch("tilde_home");
    let home = dir.join("home");
    put(&home, "myignore", b"*.log\n");
    put(
        &dir,
        ".git/config",
        b"[core]\n\texcludesFile = ~/myignore\n",
    );
    seed(&dir, &["a.log", "keep.md"]);
    let out = run_env(
        &[("HOME", home.as_os_str())],
        &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
    );
    assert_eq!(rels(&out.stdout, &dir), ["keep.md"], "{:?}", out.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_treats_an_empty_excludes_file_value_as_disabled() {
    let dir = scratch("empty_excludes");
    let home = dir.join("home");
    put(&home, "git/ignore", b"*.log\n");
    seed(&dir, &["a.log", "keep.md"]);
    let env = [
        ("HOME", home.as_os_str()),
        ("XDG_CONFIG_HOME", home.as_os_str()),
    ];
    let args = ["-rl", "--gitignore", "needle", dir.to_str().unwrap()];

    let control = run_env(&env, &args);
    assert_eq!(
        rels(&control.stdout, &dir),
        ["keep.md"],
        "{:?}",
        control.stderr
    );

    put(&dir, ".git/config", b"[core]\n\texcludesFile =\n");
    let disabled = run_env(&env, &args);
    assert_eq!(
        rels(&disabled.stdout, &dir),
        ["a.log", "keep.md"],
        "{:?}",
        disabled.stderr
    );
    assert!(disabled.stderr.is_empty(), "got {:?}", disabled.stderr);
    assert_eq!(disabled.code, 0);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_follows_a_conditional_include() {
    let dir = scratch("include_if");
    let home = dir.join("home");
    let marker = dir.file_name().unwrap().to_str().unwrap().to_string();
    put(&home, "list", b"*.log\n");
    put(
        &home,
        "inc",
        format!("[core]\n\texcludesFile = {}\n", home.join("list").display()).as_bytes(),
    );
    seed(&dir, &["a.log", "keep.md"]);
    let args = ["-rl", "--gitignore", "needle", dir.to_str().unwrap()];

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
    assert_eq!(rels(&hit.stdout, &dir), ["keep.md"], "{:?}", hit.stderr);

    put(
        &home,
        "miss",
        format!(
            "[includeIf \"gitdir:hgrep-no-such-dir/\"]\n\tpath = {}\n",
            home.join("inc").display()
        )
        .as_bytes(),
    );
    let miss = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("miss").as_os_str())],
        &args,
    );
    assert_eq!(
        rels(&miss.stdout, &dir),
        ["a.log", "keep.md"],
        "{:?}",
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
    assert_eq!(rels(&plain.stdout, &dir), ["keep.md"], "{:?}", plain.stderr);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_follows_ten_levels_of_config_include() {
    for (depth, expected) in [(10usize, vec!["keep.md"]), (11, vec!["a.log", "keep.md"])] {
        let dir = scratch(&format!("include_depth_{depth}"));
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
        seed(&dir, &["a.log", "keep.md"]);
        let out = run_env(
            &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
            &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
        );
        assert_eq!(
            rels(&out.stdout, &dir),
            expected,
            "depth {depth} stderr {:?}",
            out.stderr
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn gitignore_discards_a_malformed_config_file() {
    let broken = [
        "[core ]\n\texcludesFile = LIST\n",
        "[ core]\n\texcludesFile = LIST\n",
        "[core]\n\texcludesFile = \"LIST\n\tignorecase = true\n",
        "[core]\n\texcludesFile = LIST\\.\n",
        "[core]\n\texcludesFile\n= LIST\n",
        "[core]\n\texcludesFile LIST\n",
        "[core]\n1 = x\n\texcludesFile = LIST\n",
        "[core]\n\texcludesFile\n",
    ];
    for (index, body) in broken.iter().enumerate() {
        let dir = scratch(&format!("bad_config_{index}"));
        let home = dir.join("home");
        put(&home, "list", b"*.log\n");
        put(
            &home,
            "gitconfig",
            body.replace("LIST", &home.join("list").display().to_string())
                .as_bytes(),
        );
        seed(&dir, &["a.log", "keep.md"]);
        let out = run_env(
            &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
            &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
        );
        assert_eq!(
            rels(&out.stdout, &dir),
            ["a.log", "keep.md"],
            "case {index} {body:?} stderr {:?}",
            out.stderr
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn core_ignorecase_false_overrides_an_inherited_true() {
    let dir = scratch("ignorecase_override");
    let home = dir.join("home");
    put(&home, "gitconfig", b"[core]\n\tignorecase = true\n");
    put(&dir, ".git/config", b"[core]\n\tignorecase = false\n");
    put(&dir, ".gitignore", b"FOO.txt\n");
    seed(&dir, &["foo.txt", "keep.md"]);
    let out = run_env(
        &[("GIT_CONFIG_GLOBAL", home.join("gitconfig").as_os_str())],
        &["-rl", "--gitignore", "needle", dir.to_str().unwrap()],
    );
    assert_eq!(
        rels(&out.stdout, &dir),
        ["foo.txt", "keep.md"],
        "{:?}",
        out.stderr
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_strips_a_carriage_return_from_each_rule() {
    let dir = scratch("gitignore_crlf");
    put(&dir, ".gitignore", b"*.log\r\nkept\r\n");
    seed(&dir, &["a.log", "kept", "b.txt"]);
    assert_eq!(ignored_view(&dir), ["b.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn gitignore_resolves_a_search_root_several_levels_down() {
    let dir = scratch("deep_root");
    put(&dir, ".gitignore", b"/a/b/dropped.log\n*.tmp\n");
    put(&dir, "a/.gitignore", b"/b/dropped2.log\n!/b/kept.tmp\n");
    seed(
        &dir,
        &[
            "a/b/dropped.log",
            "a/b/dropped2.log",
            "a/b/kept.tmp",
            "a/b/y.txt",
        ],
    );
    assert_eq!(ignored_view(&dir.join("a/b")), ["kept.tmp", "y.txt"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn zero_width_match_adds_no_line_past_the_final_newline() {
    assert_eq!(hgrep(&["-c", "x*"], "a\nb\n").0, "2\n");
    assert_eq!(hgrep(&["-n", "x*"], "a\nb\n").0, "1:a\n2:b\n");
    assert_eq!(hgrep(&["-o", "x*"], "a\nb\n").0.lines().count(), 4);
    assert_eq!(hgrep(&["-c", "x*"], "a\nb").0, "2\n");
    assert_eq!(hgrep(&["-c", "-e", ""], ""), ("0\n".to_string(), 1));
    assert_eq!(hgrep(&["-c", "-v", "zzz"], ""), ("0\n".to_string(), 1));
}

#[test]
fn large_input_is_searched_through_mmap_and_parallel_chunks() {
    let dir = scratch("large_input");
    let rows = 200_000usize;
    let mut body = String::with_capacity(rows * 12);
    for row in 0..rows {
        if row == 0 || row == rows - 1 {
            body.push_str("needle\n");
        } else {
            body.push_str("filler line\n");
        }
    }
    put(&dir, "big.txt", body.as_bytes());
    let path = dir.join("big.txt");
    let name = path.to_str().unwrap();

    assert_eq!(run(&["-c", "needle", name]).stdout, "2\n");
    assert_eq!(
        run(&["-n", "needle", name]).stdout,
        format!("1:needle\n{rows}:needle\n")
    );
    assert_eq!(
        run(&["-c", "filler", name]).stdout,
        format!("{}\n", rows - 2)
    );
    assert_eq!(run(&["-c", "x*", name]).stdout, format!("{rows}\n"));
    assert_eq!(run(&["-c", "-v", "filler", name]).stdout, "2\n");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn empty_pattern_file_matches_nothing() {
    let dir = scratch("empty_pattern_file");
    put(&dir, "empty.pat", b"");
    put(&dir, "s.txt", b"alpha\nbeta\n");
    let pat = dir.join("empty.pat");
    let src = dir.join("s.txt");

    let out = run(&["-f", pat.to_str().unwrap(), src.to_str().unwrap()]);
    assert!(out.stdout.is_empty(), "got {:?}", out.stdout);
    assert_eq!(out.code, 1);

    let inverted = run(&["-v", "-f", pat.to_str().unwrap(), src.to_str().unwrap()]);
    assert_eq!(inverted.stdout, "alpha\nbeta\n");
    assert_eq!(inverted.code, 0);

    put(&dir, "one.pat", b"beta\n");
    let one = run(&[
        "-f",
        dir.join("one.pat").to_str().unwrap(),
        src.to_str().unwrap(),
    ]);
    assert_eq!(one.stdout, "beta\n");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn symlink_to_a_directory_is_searched() {
    let dir = scratch("symlink_root");
    seed(&dir, &["real/top.txt", "real/inner/a.txt"]);
    std::os::unix::fs::symlink("real", dir.join("link")).unwrap();
    let out = run(&["-rl", "needle", dir.join("link").to_str().unwrap()]);
    let mut got: Vec<String> = out.stdout.lines().map(str::to_string).collect();
    got.sort();
    assert_eq!(
        got,
        [
            dir.join("link/inner/a.txt").to_string_lossy().to_string(),
            dir.join("link/top.txt").to_string_lossy().to_string(),
        ],
        "stderr {:?}",
        out.stderr
    );
    assert_eq!(out.code, 0);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn non_utf8_pattern_and_path_operands_are_accepted() {
    let dir = scratch("non_utf8_argv");
    let pattern: &[u8] = b"caf\xe9";
    let body: &[u8] = b"caf\xe9 au lait\nplain tea\n";
    put(&dir, "menu.txt", body);
    let path = dir.join("menu.txt");

    let out = command()
        .arg(OsStr::from_bytes(pattern))
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"caf\xe9 au lait\n");
    assert_eq!(out.status.code(), Some(0));

    let fixed = command()
        .arg("-F")
        .arg(OsStr::from_bytes(pattern))
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(fixed.stdout, b"caf\xe9 au lait\n");

    let missing = command()
        .arg(OsStr::from_bytes(b"z\xe9z"))
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn max_count_zero_selects_nothing() {
    assert_eq!(hgrep(&["-m", "0", "a"], SAMPLE), (String::new(), 1));
    assert_eq!(
        hgrep(&["-m", "0", "-c", "a"], SAMPLE),
        ("0\n".to_string(), 1)
    );
    assert_eq!(hgrep(&["-m", "0", "-v", "zzz"], SAMPLE), (String::new(), 1));
    assert_eq!(hgrep(&["-m", "1", "a"], SAMPLE), ("alpha\n".to_string(), 0));
}

#[test]
fn word_match_requires_a_non_word_neighbour() {
    assert_eq!(hgrep(&["-w", "--", "-foo"], "x-foo\n"), (String::new(), 1));
    assert_eq!(hgrep(&["-w", "--", "-foo"], "a -foo b\n").0, "a -foo b\n");
    assert_eq!(hgrep(&["-w", "--", "-foo"], "-foo\n").0, "-foo\n");
    assert_eq!(hgrep(&["-w", "-o", "--", "-foo"], "a -foo b\n").0, "-foo\n");
    assert_eq!(hgrep(&["-w", "foo"], "_foo\n"), (String::new(), 1));
    assert_eq!(hgrep(&["-w", "foo"], "foo bar foo\n").0, "foo bar foo\n");
    assert_eq!(hgrep(&["-w", "-o", "foo"], "foo bar foo\n").0, "foo\nfoo\n");
}

#[test]
fn text_flag_prints_binary_content_and_quiet_prints_nothing() {
    let dir = scratch("binary_text");
    put(&dir, "case.bin", b"secret\x00tail\n");
    let path = dir.join("case.bin");
    let name = path.to_str().unwrap();

    let plain = run(&["secret", name]);
    assert!(
        plain.stdout.starts_with("Binary file "),
        "{:?}",
        plain.stdout
    );
    assert_eq!(plain.code, 0);

    let text = run_bytes(&["-a", "secret", name]);
    assert!(holds(&text, b"secret\x00tail"), "got {text:?}");

    let quiet = run(&["-q", "secret", name]);
    assert!(quiet.stdout.is_empty(), "got {:?}", quiet.stdout);
    assert_eq!(quiet.code, 0);

    let quiet_list = run(&["-qL", "zzz", name]);
    assert!(quiet_list.stdout.is_empty(), "got {:?}", quiet_list.stdout);
    assert_eq!(quiet_list.code, 0);
    fs::remove_dir_all(&dir).unwrap();
}
