use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use memchr::memchr;
use regex::bytes::{Regex, RegexBuilder};

use crate::ignore::{Ignore, glob_to_regex, load_ignore};

#[derive(Clone, Default)]
struct GitConfig {
    excludes_file: Option<Vec<u8>>,
    ignorecase: bool,
    precompose: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct RepoOpts {
    pub(crate) fold: bool,
    pub(crate) precompose: bool,
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

fn trim_end(text: &[u8]) -> &[u8] {
    let keep = text.iter().rposition(|b| !b.is_ascii_whitespace());
    keep.map_or(&[][..], |i| &text[..=i])
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
    let name = std::ffi::CString::new(user).ok()?;
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut scratch = vec![0 as libc::c_char; 4096];
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            &mut record,
            scratch.as_mut_ptr(),
            scratch.len(),
            &mut found,
        )
    };
    if rc != 0 || found.is_null() {
        return None;
    }
    let dir = unsafe { (*found).pw_dir };
    if dir.is_null() {
        return None;
    }
    let bytes = unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes().to_vec();
    Some(PathBuf::from(OsString::from_vec(bytes)))
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
    let line = trim_end(head.split(|&b| b == b'\n').next().unwrap_or(&[]));
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
        let Some(raw) = expand_tilde(&v) else {
            return false;
        };
        if scope.depth >= CONFIG_INCLUDE_DEPTH {
            return false;
        }
        let path = if raw.is_absolute() {
            raw
        } else {
            scope.dir.join(raw)
        };
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
        Some(v) => {
            let raw = expand_tilde(v)?;
            Some(if raw.is_absolute() {
                raw
            } else {
                repo.join(raw)
            })
        }
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
    let raw = trim_end(line.strip_prefix(b"gitdir:")?);
    let target = trim_end(raw.strip_prefix(b" ").unwrap_or(raw));
    if target.is_empty() {
        return None;
    }
    let named = PathBuf::from(OsStr::from_bytes(target));
    let gitdir = if named.is_absolute() {
        named
    } else {
        repo.join(named)
    };
    let Ok(shared) = fs::read(gitdir.join("commondir")) else {
        return Some(gitdir);
    };
    let common = trim_end(shared.split(|&b| b == b'\n').next().unwrap_or(&[]));
    if common.is_empty() {
        return Some(gitdir);
    }
    let named = PathBuf::from(OsStr::from_bytes(common));
    Some(if named.is_absolute() {
        named
    } else {
        gitdir.join(named)
    })
}

pub(crate) fn repo_sources(
    repo: &Path,
    errors: &AtomicBool,
    quiet: bool,
) -> (Option<Arc<Ignore>>, RepoOpts) {
    let gitdir = resolve_gitdir(repo);
    let cfg = repo_config(gitdir.as_deref());
    let opts = RepoOpts {
        fold: cfg.ignorecase,
        precompose: cfg.precompose,
    };
    let global = excludes_path(&cfg, repo)
        .and_then(|path| load_ignore(&path, 0, None, opts.fold, errors, quiet));
    let stacked = match &gitdir {
        Some(dir) => load_ignore(
            &dir.join("info").join("exclude"),
            0,
            global,
            opts.fold,
            errors,
            quiet,
        ),
        None => global,
    };
    (stacked, opts)
}
