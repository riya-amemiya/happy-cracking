use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use memchr::memchr;
use regex::bytes::{Regex, RegexBuilder};

use super::ignore::{Ignore, glob_to_regex, load_ignore};
use super::unixhome;

#[derive(Clone, Default)]
struct GitConfig {
    excludes_file: Option<Vec<u8>>,
    ignorecase: bool,
    precompose: bool,
}

#[derive(Clone, Copy, Default)]
pub struct RepoOpts {
    pub fold: bool,
    pub precompose: bool,
}

struct Scope<'a> {
    dir: &'a Path,
    gitdir: Option<&'a Path>,
    depth: u32,
}

const CONFIG_INCLUDE_DEPTH: u32 = 10;

fn skip_line(data: &[u8], from: usize) -> usize {
    memchr(b'\n', &data[from..]).map_or(data.len(), |i| from + i + 1)
}

fn join_or_abs(base: &Path, raw: PathBuf) -> PathBuf {
    if raw.is_absolute() {
        raw
    } else {
        base.join(raw)
    }
}

fn config_path(base: &Path, value: &[u8]) -> Option<PathBuf> {
    Some(join_or_abs(base, expand_tilde(value)?))
}

fn read_section(data: &[u8], start: usize) -> Option<(Vec<u8>, Vec<u8>, usize)> {
    let mut name = Vec::new();
    let mut i = start + 1;
    loop {
        match *data.get(i)? {
            b']' => return Some((name, Vec::new(), i + 1)),
            b' ' | b'\t' | b'\r' => break,
            c if c.is_ascii_alphanumeric() || c == b'-' || c == b'.' => {
                name.push(c.to_ascii_lowercase());
                i += 1;
            }
            _ => return None,
        }
    }
    while matches!(data.get(i), Some(b' ' | b'\t' | b'\r')) {
        i += 1;
    }
    if data.get(i) != Some(&b'"') {
        return None;
    }
    i += 1;
    let mut sub = Vec::new();
    loop {
        match *data.get(i)? {
            b'"' => {
                i += 1;
                break;
            }
            b'\n' => return None,
            b'\\' => {
                let esc = *data.get(i + 1)?;
                if esc == b'\n' {
                    return None;
                }
                sub.push(esc);
                i += 2;
            }
            c => {
                sub.push(c);
                i += 1;
            }
        }
    }
    if data.get(i) != Some(&b']') {
        return None;
    }
    Some((name, sub, i + 1))
}

fn read_value(data: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    let mut value = Vec::new();
    let mut keep = 0usize;
    let mut quoted = false;
    let mut comment = false;
    let mut i = start;
    while let Some(&c) = data.get(i) {
        let crlf = c == b'\r' && data.get(i + 1) == Some(&b'\n');
        if c == b'\n' || crlf {
            if quoted {
                return None;
            }
            value.truncate(keep);
            return Some((value, i + if crlf { 2 } else { 1 }));
        }
        if comment {
            i += 1;
            continue;
        }
        match c {
            b'#' | b';' if !quoted => {
                comment = true;
                i += 1;
            }
            b' ' | b'\t' | b'\r' if !quoted => {
                if !value.is_empty() {
                    value.push(c);
                }
                i += 1;
            }
            b'"' => {
                quoted = !quoted;
                i += 1;
            }
            b'\\' => {
                let esc = *data.get(i + 1)?;
                let crlf = esc == b'\r' && data.get(i + 2) == Some(&b'\n');
                if esc == b'\n' || crlf {
                    i += if crlf { 3 } else { 2 };
                    continue;
                }
                value.push(match esc {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'\\' | b'"' => esc,
                    _ => return None,
                });
                keep = value.len();
                i += 2;
            }
            _ => {
                value.push(c);
                keep = value.len();
                i += 1;
            }
        }
    }
    if quoted {
        return None;
    }
    value.truncate(keep);
    Some((value, i))
}

fn read_entry(data: &[u8], start: usize) -> Option<(Vec<u8>, Option<Vec<u8>>, usize)> {
    let mut key = Vec::new();
    let mut i = start;
    while let Some(&c) = data.get(i) {
        if c.is_ascii_alphanumeric() || c == b'-' {
            key.push(c.to_ascii_lowercase());
            i += 1;
        } else {
            break;
        }
    }
    while matches!(data.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    match data.get(i) {
        Some(b'=') => {
            let (value, next) = read_value(data, i + 1)?;
            Some((key, Some(value), next))
        }
        None => Some((key, None, i)),
        Some(b'\n') => Some((key, None, i + 1)),
        Some(b'\r') if data.get(i + 1) == Some(&b'\n') => Some((key, None, i + 2)),
        _ => None,
    }
}

fn config_int(text: &[u8]) -> Option<i64> {
    let (negative, rest) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let (radix, digits) = match rest
        .strip_prefix(b"0x")
        .or_else(|| rest.strip_prefix(b"0X"))
    {
        Some(hex) => (16u32, hex),
        None if rest.len() > 1 && rest[0] == b'0' => (8, &rest[1..]),
        None => (10, rest),
    };
    let (body, scale) = match digits.last() {
        Some(b'k' | b'K') => (&digits[..digits.len() - 1], 1024i64),
        Some(b'm' | b'M') => (&digits[..digits.len() - 1], 1024 * 1024),
        Some(b'g' | b'G') => (&digits[..digits.len() - 1], 1024 * 1024 * 1024),
        _ => (digits, 1),
    };
    if body.is_empty() {
        return None;
    }
    let magnitude = body.iter().try_fold(0i64, |acc, &b| {
        let digit = (b as char).to_digit(radix)?;
        acc.checked_mul(radix as i64)?.checked_add(digit as i64)
    })?;
    let scaled = magnitude.checked_mul(scale)?;
    Some(if negative { -scaled } else { scaled })
}

fn config_bool(value: &[u8]) -> bool {
    match value.to_ascii_lowercase().as_slice() {
        b"" => false,
        b"true" | b"yes" | b"on" => true,
        b"false" | b"no" | b"off" => false,
        _ => config_int(value).is_some_and(|v| v != 0),
    }
}

fn home_of(user: &[u8]) -> Option<PathBuf> {
    unixhome::home_of(user)
}

fn expand_tilde(value: &[u8]) -> Option<PathBuf> {
    let Some(rest) = value.strip_prefix(b"~") else {
        return Some(PathBuf::from(OsStr::from_bytes(value)));
    };
    let cut = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
    let home = if cut == 0 {
        PathBuf::from(std::env::var_os("HOME")?)
    } else {
        home_of(&rest[..cut])?
    };
    Some(match rest.get(cut + 1..) {
        Some(tail) => home.join(OsStr::from_bytes(tail)),
        None => home,
    })
}

fn path_glob(pattern: &[u8], fold: bool) -> Option<Regex> {
    let body = glob_to_regex(pattern, fold)?;
    RegexBuilder::new(&format!("^{body}$"))
        .unicode(false)
        .dot_matches_new_line(true)
        .build()
        .ok()
}

fn by_gitdir(cond: &[u8], scope: &Scope, fold: bool) -> bool {
    let Some(gitdir) = scope.gitdir else {
        return false;
    };
    let expanded =
        expand_tilde(cond).map_or_else(|| cond.to_vec(), |p| p.into_os_string().into_vec());
    let mut pattern = if expanded.starts_with(b"./") || expanded.starts_with(b"../") {
        let base = scope
            .dir
            .canonicalize()
            .unwrap_or_else(|_| scope.dir.to_path_buf());
        let mut joined = base.into_os_string().into_vec();
        joined.push(b'/');
        joined.extend_from_slice(&expanded);
        joined
    } else if expanded.starts_with(b"/") {
        expanded
    } else {
        let mut prefixed = b"**/".to_vec();
        prefixed.extend_from_slice(&expanded);
        prefixed
    };
    if pattern.last() == Some(&b'/') {
        pattern.extend_from_slice(b"**");
    }
    let Some(glob) = path_glob(&pattern, fold) else {
        return false;
    };
    let real = gitdir.canonicalize().ok();
    [real.as_deref(), Some(gitdir)]
        .into_iter()
        .flatten()
        .any(|candidate| {
            let mut text = candidate.as_os_str().as_bytes().to_vec();
            if fold {
                text.make_ascii_lowercase();
            }
            glob.is_match(&text)
        })
}

fn by_branch(cond: &[u8], scope: &Scope) -> bool {
    let Some(gitdir) = scope.gitdir else {
        return false;
    };
    let Ok(head) = fs::read(gitdir.join("HEAD")) else {
        return false;
    };
    let line = head
        .split(|&b| b == b'\n')
        .next()
        .unwrap_or(&[])
        .trim_ascii_end();
    let Some(branch) = line.strip_prefix(b"ref: refs/heads/") else {
        return false;
    };
    let mut pattern = cond.to_vec();
    if pattern.last() == Some(&b'/') {
        pattern.extend_from_slice(b"**");
    }
    path_glob(&pattern, false).is_some_and(|glob| glob.is_match(branch))
}

fn condition_holds(cond: &[u8], scope: &Scope) -> bool {
    if let Some(rest) = cond.strip_prefix(b"gitdir:") {
        return by_gitdir(rest, scope, false);
    }
    if let Some(rest) = cond.strip_prefix(b"gitdir/i:") {
        return by_gitdir(rest, scope, true);
    }
    match cond.strip_prefix(b"onbranch:") {
        Some(rest) => by_branch(rest, scope),
        None => false,
    }
}

fn apply_entry(
    section: &[u8],
    sub: &[u8],
    key: &[u8],
    value: Option<Vec<u8>>,
    scope: &Scope,
    cfg: &mut GitConfig,
) -> bool {
    if section == b"core" && sub.is_empty() {
        if key == b"excludesfile" {
            let Some(v) = value else {
                return false;
            };
            cfg.excludes_file = Some(v);
        } else if key == b"ignorecase" {
            cfg.ignorecase = value.is_none_or(|v| config_bool(&v));
        } else if key == b"precomposeunicode" {
            cfg.precompose = value.is_none_or(|v| config_bool(&v));
        }
        return true;
    }
    let conditional = section == b"includeif";
    if (section == b"include" || conditional) && key == b"path" {
        if conditional && !condition_holds(sub, scope) {
            return true;
        }
        let Some(v) = value else {
            return false;
        };
        let Some(path) = config_path(scope.dir, &v) else {
            return false;
        };
        if scope.depth >= CONFIG_INCLUDE_DEPTH {
            return false;
        }
        read_config_file(&path, scope.gitdir, scope.depth + 1, cfg);
    }
    true
}

fn read_config_bytes(data: &[u8], scope: &Scope, cfg: &mut GitConfig) -> bool {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let mut section = Vec::new();
    let mut sub = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        match data[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'#' | b';' => i = skip_line(data, i),
            b'[' => match read_section(data, i) {
                Some((name, quoted, next)) => {
                    section = name;
                    sub = quoted;
                    i = next;
                }
                None => return false,
            },
            c if c.is_ascii_alphabetic() => {
                let Some((key, value, next)) = read_entry(data, i) else {
                    return false;
                };
                i = next;
                if !apply_entry(&section, &sub, &key, value, scope, cfg) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn apply_config(data: &[u8], scope: &Scope, cfg: &mut GitConfig) {
    let mut scratch = cfg.clone();
    if read_config_bytes(data, scope, &mut scratch) {
        *cfg = scratch;
    }
}

fn read_config_file(path: &Path, gitdir: Option<&Path>, depth: u32, cfg: &mut GitConfig) {
    let Ok(data) = fs::read(path) else {
        return;
    };
    let scope = Scope {
        dir: path.parent().unwrap_or(Path::new(".")),
        gitdir,
        depth,
    };
    apply_config(&data, &scope, cfg);
}

fn xdg_config_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => Some(PathBuf::from(x)),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")),
    }
}

struct RawConfig {
    dir: PathBuf,
    data: Vec<u8>,
}

fn push_source(out: &mut Vec<RawConfig>, path: PathBuf) {
    if let Ok(data) = fs::read(&path) {
        out.push(RawConfig {
            dir: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            data,
        });
    }
}

fn base_sources() -> &'static [RawConfig] {
    static CACHE: OnceLock<Vec<RawConfig>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out = Vec::new();
        let nosystem = std::env::var_os("GIT_CONFIG_NOSYSTEM")
            .is_some_and(|v| config_bool(v.as_os_str().as_bytes()));
        if !nosystem {
            let system = std::env::var_os("GIT_CONFIG_SYSTEM")
                .map_or_else(|| PathBuf::from("/etc/gitconfig"), PathBuf::from);
            push_source(&mut out, system);
        }
        match std::env::var_os("GIT_CONFIG_GLOBAL") {
            Some(g) => push_source(&mut out, PathBuf::from(g)),
            None => {
                if let Some(dir) = xdg_config_home() {
                    push_source(&mut out, dir.join("git").join("config"));
                }
                if let Some(home) = std::env::var_os("HOME") {
                    push_source(&mut out, PathBuf::from(home).join(".gitconfig"));
                }
            }
        }
        out
    })
}

fn repo_config(gitdir: Option<&Path>) -> GitConfig {
    let mut cfg = GitConfig::default();
    for src in base_sources() {
        let scope = Scope {
            dir: &src.dir,
            gitdir,
            depth: 0,
        };
        apply_config(&src.data, &scope, &mut cfg);
    }
    if let Some(dir) = gitdir {
        read_config_file(&dir.join("config"), gitdir, 0, &mut cfg);
    }
    cfg
}

fn excludes_path(cfg: &GitConfig, repo: &Path) -> Option<PathBuf> {
    match &cfg.excludes_file {
        Some(v) if v.is_empty() => None,
        Some(v) => config_path(repo, v),
        None => xdg_config_home().map(|d| d.join("git").join("ignore")),
    }
}

fn resolve_gitdir(repo: &Path) -> Option<PathBuf> {
    let dot = repo.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let data = fs::read(&dot).ok()?;
    let line = data.split(|&b| b == b'\n').next()?;
    let raw = line.strip_prefix(b"gitdir:")?.trim_ascii_end();
    let target = raw.strip_prefix(b" ").unwrap_or(raw).trim_ascii_end();
    if target.is_empty() {
        return None;
    }
    let named = PathBuf::from(OsStr::from_bytes(target));
    let gitdir = join_or_abs(repo, named);
    let Ok(shared) = fs::read(gitdir.join("commondir")) else {
        return Some(gitdir);
    };
    let common = shared
        .split(|&b| b == b'\n')
        .next()
        .unwrap_or(&[])
        .trim_ascii_end();
    if common.is_empty() {
        return Some(gitdir);
    }
    let named = PathBuf::from(OsStr::from_bytes(common));
    Some(join_or_abs(&gitdir, named))
}

pub fn repo_sources(
    repo: &Path,
    errors: &AtomicBool,
    quiet: bool,
    prog: &str,
) -> (Option<Arc<Ignore>>, RepoOpts) {
    let gitdir = resolve_gitdir(repo);
    let cfg = repo_config(gitdir.as_deref());
    let opts = RepoOpts {
        fold: cfg.ignorecase,
        precompose: cfg.precompose,
    };
    let global = excludes_path(&cfg, repo)
        .and_then(|path| load_ignore(&path, 0, None, opts.fold, errors, quiet, prog));
    let stacked = match &gitdir {
        Some(dir) => load_ignore(
            &dir.join("info").join("exclude"),
            0,
            global,
            opts.fold,
            errors,
            quiet,
            prog,
        ),
        None => global,
    };
    (stacked, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repo_sources_precompose_and_relative_include() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hfind_gitcfg_{}_{nanos}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(
            dir.join(".git/config"),
            b"[core]\n\tprecomposeunicode = true\n[include]\n\tpath = extra\n",
        )
        .unwrap();
        fs::write(dir.join(".git/extra"), b"[core]\n\tignorecase = true\n").unwrap();
        let errors = AtomicBool::new(false);
        let (_, opts) = repo_sources(&dir, &errors, false, "hc-internal");
        assert!(opts.precompose);
        assert!(opts.fold);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn home_of_rejects_invalid_names() {
        assert!(home_of(b"").is_none());
        assert!(home_of(b"a\0b").is_none());
        assert!(home_of(b"a/b").is_none());
        assert!(home_of(b"a:b").is_none());
        assert_eq!(
            unixhome::home_from_passwd_bytes(
                b"alice:x:1000:1000:Alice:/home/alice:/bin/bash\n",
                b"alice"
            )
            .as_deref(),
            Some(Path::new("/home/alice"))
        );
        assert!(
            unixhome::home_from_passwd_bytes(
                b"alice:x:1000:1000:Alice:/home/alice:/bin/bash\n",
                b"bob"
            )
            .is_none()
        );
        assert!(
            unixhome::home_from_passwd_bytes(b"alice:x:1000:1000:Alice::/bin/bash\n", b"alice")
                .is_none()
        );
    }

    #[test]
    fn config_parser_numbers_and_paths() {
        assert_eq!(skip_line(b"abc", 0), 3);
        assert_eq!(skip_line(b"a\nb", 0), 2);
        assert_eq!(config_int(b"-10"), Some(-10));
        assert_eq!(config_int(b"+2"), Some(2));
        assert_eq!(config_int(b"0x10"), Some(16));
        assert_eq!(config_int(b"0X10"), Some(16));
        assert_eq!(config_int(b"010"), Some(8));
        assert_eq!(config_int(b"1k"), Some(1024));
        assert_eq!(config_int(b"1K"), Some(1024));
        assert_eq!(config_int(b"1m"), Some(1024 * 1024));
        assert_eq!(config_int(b"1M"), Some(1024 * 1024));
        assert_eq!(config_int(b"1g"), Some(1024 * 1024 * 1024));
        assert_eq!(config_int(b"1G"), Some(1024 * 1024 * 1024));
        assert!(config_int(b"").is_none());
        assert!(config_int(b"k").is_none());
        assert!(config_int(b"999999999999999999999").is_none());
        assert!(config_bool(b"true"));
        assert!(config_bool(b"YES"));
        assert!(config_bool(b"on"));
        assert!(!config_bool(b"false"));
        assert!(!config_bool(b"no"));
        assert!(!config_bool(b"off"));
        assert!(config_bool(b"2"));
        assert!(!config_bool(b"0"));
        assert!(!config_bool(b"xyz"));
        assert_eq!(
            expand_tilde(b"/abs/x").as_deref(),
            Some(Path::new("/abs/x"))
        );
        assert!(expand_tilde(b"~hfind-no-such-user/x").is_none());
        let _ = expand_tilde(b"~");
        let _ = expand_tilde(b"~/tmp-hfind");
        assert!(home_of(b"hfind-no-such-user").is_none());
        assert!(home_of(b"a\0b").is_none());
        if let Ok(name) = std::env::var("USER") {
            let _ = home_of(name.as_bytes());
        }
        assert!(path_glob(b"[", false).is_none());
        assert!(path_glob(b"foo*", false).is_some());
        let _ = xdg_config_home();
        let cfg = GitConfig {
            excludes_file: Some(Vec::new()),
            ignorecase: false,
            precompose: false,
        };
        assert!(excludes_path(&cfg, Path::new("/tmp")).is_none());
        let cfg = GitConfig {
            excludes_file: Some(b"rel".to_vec()),
            ..GitConfig::default()
        };
        assert_eq!(
            excludes_path(&cfg, Path::new("/repo")).as_deref(),
            Some(Path::new("/repo/rel"))
        );
        let cfg = GitConfig {
            excludes_file: Some(b"/abs".to_vec()),
            ..GitConfig::default()
        };
        assert_eq!(
            excludes_path(&cfg, Path::new("/repo")).as_deref(),
            Some(Path::new("/abs"))
        );
        let cfg = GitConfig::default();
        let _ = excludes_path(&cfg, Path::new("/repo"));
        assert!(!config_bool(b""));
    }

    #[test]
    fn config_bytes_sections_and_include() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_gitcfg_u_{}_{nanos}", std::process::id()));
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
        let extra = dir.join("extra.cfg");
        fs::write(&extra, b"[core]\n\tignorecase = true\n").unwrap();
        let bom = "\u{feff}";
        let data = format!(
            "{bom}[core]\n\texcludesFile = {p}\n\tignorecase = yes\n\tprecomposeunicode = on\n[include]\n\tpath = extra.cfg\n[includeIf \"gitdir:{d}/\"]\n\tpath = extra.cfg\n[includeIf \"gitdir/i:{D}/\"]\n\tpath = extra.cfg\n[includeIf \"onbranch:ma*\"]\n\tpath = extra.cfg\n[includeIf \"unknown:x\"]\n\tpath = extra.cfg\n#c\n;c\n",
            p = extra.display(),
            d = dir.display(),
            D = dir.display().to_string().to_ascii_uppercase(),
        );
        let gitdir = dir.join(".git");
        let scope = Scope {
            dir: &dir,
            gitdir: Some(gitdir.as_path()),
            depth: 0,
        };
        let mut cfg = GitConfig::default();
        assert!(read_config_bytes(data.as_bytes(), &scope, &mut cfg));
        apply_config(data.as_bytes(), &scope, &mut cfg);
        apply_config(b"!", &scope, &mut cfg);
        assert!(read_section(b"[core]", 0).is_some());
        assert!(read_section(b"[core \"sub\"]", 0).is_some());
        assert!(read_section(b"[core \"a\\\"b\"]", 0).is_some());
        assert!(read_section(b"[core \"a\\\nb\"]", 0).is_none());
        assert!(read_section(b"[core \"ab\n\"]", 0).is_none());
        assert!(read_section(b"[core extra]", 0).is_none());
        assert!(read_section(b"[core!]", 0).is_none());
        assert!(read_section(b"[core \"x\"", 0).is_none());
        assert!(read_section(b"[core \"x\" y]", 0).is_none());
        assert!(read_value(b"abc\n", 0).is_some());
        assert!(read_value(b"\"ab\"\n", 0).is_some());
        assert!(read_value(b"ab #c\n", 0).is_some());
        assert!(read_value(b"ab ;c\n", 0).is_some());
        assert!(read_value(b"ab\r\n", 0).is_some());
        assert!(read_value(b"a\\\nb\n", 0).is_some());
        assert!(read_value(b"a\\\r\nb\n", 0).is_some());
        assert!(read_value(b"\\n\\t\\b\\\\\\\"\n", 0).is_some());
        assert!(read_value(b"\\.\n", 0).is_none());
        assert!(read_value(b"\"ab", 0).is_none());
        assert!(read_value(b"\"ab\n", 0).is_none());
        assert!(read_entry(b"key=val\n", 0).is_some());
        assert!(read_entry(b"key", 0).is_some());
        assert!(read_entry(b"key\n", 0).is_some());
        assert!(read_entry(b"key\r\n", 0).is_some());
        assert!(read_entry(b"key =\n", 0).is_some());
        assert!(read_entry(b"key x", 0).is_none());
        let no_git = Scope {
            dir: &dir,
            gitdir: None,
            depth: 0,
        };
        assert!(!by_gitdir(b"foo", &no_git, false));
        assert!(!by_branch(b"main", &no_git));
        assert!(!condition_holds(b"nope", &scope));
        assert!(condition_holds(b"onbranch:main", &scope));
        assert!(condition_holds(b"gitdir:**/.git", &scope));
        let _ = by_gitdir(b"./", &scope, false);
        assert!(!by_gitdir(b"[", &scope, false));
        let _ = by_gitdir(b"./nope/", &scope, false);
        let _ = by_gitdir(b"../.git/", &scope, true);
        fs::write(dir.join(".git/HEAD"), b"abc\n").unwrap();
        assert!(!by_branch(b"main", &scope));
        fs::write(dir.join(".git/HEAD"), b"ref: refs/heads/feat/\n").unwrap();
        let _ = by_branch(b"feat/", &scope);
        let missing_head = Scope {
            dir: &dir,
            gitdir: Some(Path::new("/hfind-no-gitdir")),
            depth: 0,
        };
        assert!(!by_branch(b"main", &missing_head));
        let mut cfg = GitConfig::default();
        assert!(!apply_entry(
            b"core",
            b"",
            b"excludesfile",
            None,
            &scope,
            &mut cfg
        ));
        apply_entry(b"core", b"", b"ignorecase", None, &scope, &mut cfg);
        apply_entry(b"core", b"", b"precomposeunicode", None, &scope, &mut cfg);
        apply_entry(
            b"core",
            b"",
            b"other",
            Some(b"x".to_vec()),
            &scope,
            &mut cfg,
        );
        assert!(apply_entry(
            b"includeIf",
            b"gitdir:hfind-no-such/",
            b"path",
            Some(b"x".to_vec()),
            &scope,
            &mut cfg
        ));
        assert!(!apply_entry(
            b"include", b"", b"path", None, &scope, &mut cfg
        ));
        assert!(!apply_entry(
            b"include",
            b"",
            b"path",
            Some(b"~hfind-no-such-user/x".to_vec()),
            &scope,
            &mut cfg
        ));
        let deep = Scope {
            dir: &dir,
            gitdir: Some(gitdir.as_path()),
            depth: CONFIG_INCLUDE_DEPTH,
        };
        assert!(!apply_entry(
            b"include",
            b"",
            b"path",
            Some(b"extra.cfg".to_vec()),
            &deep,
            &mut cfg
        ));
        apply_entry(
            b"include",
            b"",
            b"path",
            Some(extra.as_os_str().as_bytes().to_vec()),
            &scope,
            &mut cfg,
        );
        apply_entry(
            b"include",
            b"",
            b"path",
            Some(b"extra.cfg".to_vec()),
            &scope,
            &mut cfg,
        );
        assert!(!read_config_bytes(b"[core!]\n", &scope, &mut cfg));
        assert!(!read_config_bytes(b"1key\n", &scope, &mut cfg));
        assert!(!read_config_bytes(b"!\n", &scope, &mut cfg));
        read_config_file(Path::new("/hfind-no-config"), None, 0, &mut cfg);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_gitdir_shapes() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("hfind_gitdir_u_{}_{nanos}", std::process::id()));
        fs::create_dir_all(dir.join("repo/.git")).unwrap();
        assert!(resolve_gitdir(&dir.join("repo")).unwrap().ends_with(".git"));
        fs::create_dir_all(dir.join("store")).unwrap();
        fs::create_dir_all(dir.join("ptr")).unwrap();
        fs::write(
            dir.join("ptr/.git"),
            format!("gitdir: {}\n", dir.join("store").display()).as_bytes(),
        )
        .unwrap();
        assert_eq!(
            resolve_gitdir(&dir.join("ptr")).as_deref(),
            Some(dir.join("store").as_path())
        );
        fs::create_dir_all(dir.join("rel")).unwrap();
        fs::write(dir.join("rel/.git"), b"gitdir: ../store2\n").unwrap();
        fs::create_dir_all(dir.join("store2")).unwrap();
        assert!(
            resolve_gitdir(&dir.join("rel"))
                .unwrap()
                .ends_with("store2")
        );
        fs::create_dir_all(dir.join("empty")).unwrap();
        fs::write(dir.join("empty/.git"), b"gitdir:\n").unwrap();
        assert!(resolve_gitdir(&dir.join("empty")).is_none());
        fs::create_dir_all(dir.join("wt")).unwrap();
        fs::write(dir.join("wt/.git"), b"gitdir: ../wtstore\n").unwrap();
        fs::create_dir_all(dir.join("wtstore")).unwrap();
        fs::write(dir.join("wtstore/commondir"), b"").unwrap();
        assert!(
            resolve_gitdir(&dir.join("wt"))
                .unwrap()
                .ends_with("wtstore")
        );
        fs::write(dir.join("wtstore/commondir"), b"../common\n").unwrap();
        fs::create_dir_all(dir.join("common")).unwrap();
        assert!(resolve_gitdir(&dir.join("wt")).unwrap().ends_with("common"));
        fs::write(
            dir.join("wtstore/commondir"),
            format!("{}\n", dir.join("abscommon").display()).as_bytes(),
        )
        .unwrap();
        fs::create_dir_all(dir.join("abscommon")).unwrap();
        assert_eq!(
            resolve_gitdir(&dir.join("wt")).as_deref(),
            Some(dir.join("abscommon").as_path())
        );
        fs::create_dir_all(dir.join("nocommon")).unwrap();
        fs::write(dir.join("nocommon/.git"), b"gitdir: ../missing-store\n").unwrap();
        assert!(resolve_gitdir(&dir.join("nocommon")).is_some());
        let errors = AtomicBool::new(false);
        let _ = repo_sources(&dir.join("repo"), &errors, true, "hc-internal");
        fs::remove_dir_all(&dir).unwrap();
    }
}
