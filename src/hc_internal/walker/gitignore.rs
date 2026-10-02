use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::bytes::Regex;

use super::glob::{Candidate, GlobBuilder, GlobSet, GlobSetBuilder};
use super::{Error, Match, PartialErrorBuilder, is_file_name, strip_prefix};

#[derive(Clone, Debug)]
pub struct Glob {
    original: String,
    actual: String,
    is_whitelist: bool,
    is_only_dir: bool,
}

impl Glob {
    pub fn is_whitelist(&self) -> bool {
        self.is_whitelist
    }

    fn has_doublestar_prefix(&self) -> bool {
        self.actual.starts_with("**/") || self.actual == "**"
    }
}

#[derive(Clone, Debug)]
pub struct Gitignore {
    set: GlobSet,
    root: PathBuf,
    root_is_dot: bool,
    globs: Vec<Glob>,
    num_ignores: u64,
}

impl Gitignore {
    #[cfg(test)]
    pub fn new<P: AsRef<Path>>(gitignore_path: P) -> (Gitignore, Option<Error>) {
        let path = gitignore_path.as_ref();
        let parent = path.parent().unwrap_or(Path::new("/"));
        let mut builder = GitignoreBuilder::new(parent);
        let mut errs = PartialErrorBuilder::default();
        errs.maybe_push_ignore_io(builder.add(path));
        match builder.build() {
            Ok(gi) => (gi, errs.into_error_option()),
            Err(err) => {
                errs.push(err);
                (Gitignore::empty(), errs.into_error_option())
            }
        }
    }

    pub fn empty() -> Gitignore {
        Gitignore {
            set: GlobSet::empty(),
            root: PathBuf::new(),
            root_is_dot: false,
            globs: vec![],
            num_ignores: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn num_ignores(&self) -> u64 {
        self.num_ignores
    }

    pub fn matched<P: AsRef<Path>>(&self, path: P, is_dir: bool) -> Match<&Glob> {
        if self.is_empty() {
            return Match::None;
        }
        self.matched_stripped(self.strip(path.as_ref()), is_dir)
    }

    #[cfg(test)]
    pub fn matched_path_or_any_parents<P: AsRef<Path>>(
        &self,
        path: P,
        is_dir: bool,
    ) -> Match<&Glob> {
        if self.is_empty() {
            return Match::None;
        }
        let mut path = self.strip(path.as_ref());
        assert!(!path.has_root(), "path is expected to be under the root");
        match self.matched_stripped(path, is_dir) {
            Match::None => {}
            a_match => return a_match,
        }
        while let Some(parent) = path.parent() {
            match self.matched_stripped(parent, true) {
                Match::None => path = parent,
                a_match => return a_match,
            }
        }
        Match::None
    }

    fn matched_stripped(&self, path: &Path, is_dir: bool) -> Match<&Glob> {
        let candidate = Candidate::new(path);
        let globs = &self.globs;
        match self
            .set
            .last_match(&candidate, &|i| is_dir || !globs[i].is_only_dir)
        {
            None => Match::None,
            Some(i) if globs[i].is_whitelist => Match::Whitelist(&globs[i]),
            Some(i) => Match::Ignore(&globs[i]),
        }
    }

    fn strip<'a>(&self, path: &'a Path) -> &'a Path {
        let mut path = path;
        if let Some(p) = strip_prefix("./", path) {
            path = p;
        }
        if !self.root_is_dot
            && !is_file_name(path)
            && let Some(p) = strip_prefix(&self.root, path)
        {
            path = p;
            if let Some(p) = strip_prefix("/", path) {
                path = p;
            }
        }
        path
    }
}

#[derive(Clone, Debug)]
pub struct GitignoreBuilder {
    builder: GlobSetBuilder,
    root: PathBuf,
    globs: Vec<Glob>,
    case_insensitive: bool,
    allow_unclosed_class: bool,
}

impl GitignoreBuilder {
    pub fn new<P: AsRef<Path>>(root: P) -> GitignoreBuilder {
        let root = root.as_ref();
        GitignoreBuilder {
            builder: GlobSetBuilder::new(),
            root: strip_prefix("./", root).unwrap_or(root).to_path_buf(),
            globs: vec![],
            case_insensitive: false,
            allow_unclosed_class: true,
        }
    }

    pub fn build(&self) -> Result<Gitignore, Error> {
        let nignore = self.globs.iter().filter(|g| !g.is_whitelist()).count();
        let set = self.builder.build().map_err(|err| Error::Glob {
            glob: None,
            err: err.to_string(),
        })?;
        Ok(Gitignore {
            set,
            root_is_dot: self.root == Path::new("."),
            root: self.root.clone(),
            globs: self.globs.clone(),
            num_ignores: nignore as u64,
        })
    }

    pub fn build_global(mut self) -> (Gitignore, Option<Error>) {
        match gitconfig_excludes_path() {
            Some(path) if path.is_file() => {
                let mut errs = PartialErrorBuilder::default();
                errs.maybe_push_ignore_io(self.add(path));
                match self.build() {
                    Ok(gi) => (gi, errs.into_error_option()),
                    Err(err) => {
                        errs.push(err);
                        (Gitignore::empty(), errs.into_error_option())
                    }
                }
            }
            _ => (Gitignore::empty(), None),
        }
    }

    pub fn add<P: AsRef<Path>>(&mut self, path: P) -> Option<Error> {
        let path = path.as_ref();
        let mut file = match File::open(path) {
            Err(err) => return Some(Error::Io(err).with_path(path)),
            Ok(file) => file,
        };
        let mut data = Vec::new();
        let read_err = file.read_to_end(&mut data).err();
        let mut errs = PartialErrorBuilder::default();
        let complete = if read_err.is_some() {
            memchr::memrchr(b'\n', &data).map_or(0, |i| i + 1)
        } else {
            data.len()
        };
        let mut lineno = 0u64;
        let mut rest = &data[..complete];
        while !rest.is_empty() {
            lineno += 1;
            let (raw, next) = match memchr::memchr(b'\n', rest) {
                Some(i) => {
                    let line = &rest[..i];
                    (line.strip_suffix(b"\r").unwrap_or(line), &rest[i + 1..])
                }
                None => (rest, &rest[rest.len()..]),
            };
            rest = next;
            let Ok(line) = std::str::from_utf8(raw) else {
                let err = io::Error::new(
                    io::ErrorKind::InvalidData,
                    "stream did not contain valid UTF-8",
                );
                errs.push(Error::Io(err).tagged(path, lineno));
                return errs.into_error_option();
            };
            let line = if lineno == 1 {
                line.trim_start_matches('\u{feff}')
            } else {
                line
            };
            if let Err(err) = self.add_line(line) {
                errs.push(err.tagged(path, lineno));
            }
        }
        if let Some(err) = read_err {
            errs.push(Error::Io(err).tagged(path, lineno + 1));
        }
        errs.into_error_option()
    }

    #[cfg(test)]
    pub(crate) fn add_str(&mut self, gitignore: &str) -> Result<&mut GitignoreBuilder, Error> {
        for line in gitignore.lines() {
            self.add_line(line)?;
        }
        Ok(self)
    }

    pub fn add_line(&mut self, mut line: &str) -> Result<&mut GitignoreBuilder, Error> {
        if line.starts_with('#') {
            return Ok(self);
        }
        if !line.ends_with("\\ ") {
            line = line.trim_end();
        }
        if line.is_empty() {
            return Ok(self);
        }
        let mut glob = Glob {
            original: line.to_string(),
            actual: String::new(),
            is_whitelist: false,
            is_only_dir: false,
        };
        let mut is_absolute = false;
        if line.starts_with("\\!") || line.starts_with("\\#") {
            line = &line[1..];
            is_absolute = line.starts_with('/');
        } else {
            if let Some(rest) = line.strip_prefix('!') {
                glob.is_whitelist = true;
                line = rest;
            }
            if let Some(rest) = line.strip_prefix('/') {
                line = rest;
                is_absolute = true;
            }
        }
        if let Some(rest) = line.strip_suffix('/') {
            glob.is_only_dir = true;
            line = rest.strip_suffix('\\').unwrap_or(rest);
        }
        glob.actual = line.to_string();
        if !is_absolute && !line.contains('/') && !glob.has_doublestar_prefix() {
            glob.actual = format!("**/{}", glob.actual);
        }
        if glob.actual.ends_with("/**") {
            glob.actual.push_str("/*");
        }
        let parsed = GlobBuilder::new(&glob.actual)
            .literal_separator(true)
            .case_insensitive(self.case_insensitive)
            .backslash_escape(true)
            .allow_unclosed_class(self.allow_unclosed_class)
            .build()
            .map_err(|err| Error::Glob {
                glob: Some(glob.original.clone()),
                err: err.kind().to_string(),
            })?;
        self.builder.add(parsed);
        self.globs.push(glob);
        Ok(self)
    }

    pub fn case_insensitive(&mut self, yes: bool) -> &mut GitignoreBuilder {
        self.case_insensitive = yes;
        self
    }

    pub fn allow_unclosed_class(&mut self, yes: bool) -> &mut GitignoreBuilder {
        self.allow_unclosed_class = yes;
        self
    }
}

pub fn gitconfig_excludes_path() -> Option<PathBuf> {
    if let Some(path) = gitconfig_home_contents().and_then(|x| parse_excludes_file(&x)) {
        return Some(path);
    }
    if let Some(path) = gitconfig_xdg_contents().and_then(|x| parse_excludes_file(&x)) {
        return Some(path);
    }
    excludes_file_default()
}

fn read_all(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).ok().map(|_| contents)
}

fn gitconfig_home_contents() -> Option<Vec<u8>> {
    read_all(&home_dir()?.join(".gitconfig"))
}

fn xdg_config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|x| !x.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|p| p.join(".config")))
}

fn gitconfig_xdg_contents() -> Option<Vec<u8>> {
    read_all(&xdg_config_home()?.join("git/config"))
}

fn excludes_file_default() -> Option<PathBuf> {
    xdg_config_home().map(|x| x.join("git/ignore"))
}

fn parse_excludes_file(data: &[u8]) -> Option<PathBuf> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?im-u)^\s*excludesfile\s*=\s*"?\s*(\S+?)\s*"?\s*$"#)
            .expect("excludesfile regex is valid")
    });
    let caps = re.captures(data)?;
    let candidate = caps.get(1)?.as_bytes();
    std::str::from_utf8(candidate)
        .ok()
        .map(|s| PathBuf::from(expand_tilde(s)))
}

fn expand_tilde(path: &str) -> String {
    match home_dir() {
        None => path.to_string(),
        Some(home) => path.replace('~', &home.to_string_lossy()),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Gitignore, GitignoreBuilder};

    fn gi_from_str<P: AsRef<Path>>(root: P, s: &str) -> Gitignore {
        let mut builder = GitignoreBuilder::new(root);
        builder.add_str(s).unwrap();
        builder.build().unwrap()
    }

    macro_rules! ignored {
        ($name:ident, $root:expr, $gi:expr, $path:expr) => {
            ignored!($name, $root, $gi, $path, false);
        };
        ($name:ident, $root:expr, $gi:expr, $path:expr, $is_dir:expr) => {
            #[test]
            fn $name() {
                let gi = gi_from_str($root, $gi);
                assert!(gi.matched($path, $is_dir).is_ignore());
            }
        };
    }

    macro_rules! not_ignored {
        ($name:ident, $root:expr, $gi:expr, $path:expr) => {
            not_ignored!($name, $root, $gi, $path, false);
        };
        ($name:ident, $root:expr, $gi:expr, $path:expr, $is_dir:expr) => {
            #[test]
            fn $name() {
                let gi = gi_from_str($root, $gi);
                assert!(!gi.matched($path, $is_dir).is_ignore());
            }
        };
    }

    const ROOT: &str = "/home/foobar/rust/rg";

    ignored!(ig1, ROOT, "months", "months");
    ignored!(ig2, ROOT, "*.lock", "Cargo.lock");
    ignored!(ig3, ROOT, "*.rs", "src/main.rs");
    ignored!(ig4, ROOT, "src/*.rs", "src/main.rs");
    ignored!(ig5, ROOT, "/*.c", "cat-file.c");
    ignored!(ig6, ROOT, "/src/*.rs", "src/main.rs");
    ignored!(ig7, ROOT, "!src/main.rs\n*.rs", "src/main.rs");
    ignored!(ig8, ROOT, "foo/", "foo", true);
    ignored!(ig9, ROOT, "**/foo", "foo");
    ignored!(ig10, ROOT, "**/foo", "src/foo");
    ignored!(ig11, ROOT, "**/foo/**", "src/foo/bar");
    ignored!(ig12, ROOT, "**/foo/**", "wat/src/foo/bar/baz");
    ignored!(ig13, ROOT, "**/foo/bar", "foo/bar");
    ignored!(ig14, ROOT, "**/foo/bar", "src/foo/bar");
    ignored!(ig15, ROOT, "abc/**", "abc/x");
    ignored!(ig16, ROOT, "abc/**", "abc/x/y");
    ignored!(ig17, ROOT, "abc/**", "abc/x/y/z");
    ignored!(ig18, ROOT, "a/**/b", "a/b");
    ignored!(ig19, ROOT, "a/**/b", "a/x/b");
    ignored!(ig20, ROOT, "a/**/b", "a/x/y/b");
    ignored!(ig21, ROOT, r"\!xy", "!xy");
    ignored!(ig22, ROOT, r"\#foo", "#foo");
    ignored!(ig23, ROOT, "foo", "./foo");
    ignored!(ig24, ROOT, "target", "grep/target");
    ignored!(ig25, ROOT, "Cargo.lock", "./tabwriter-bin/Cargo.lock");
    ignored!(ig26, ROOT, "/foo/bar/baz", "./foo/bar/baz");
    ignored!(ig27, ROOT, "foo/", "xyz/foo", true);
    ignored!(ig28, "./src", "/llvm/", "./src/llvm", true);
    ignored!(ig29, ROOT, "node_modules/ ", "node_modules", true);
    ignored!(ig30, ROOT, "**/", "foo/bar", true);
    ignored!(ig31, ROOT, "path1/*", "path1/foo");
    ignored!(ig32, ROOT, ".a/b", ".a/b");
    ignored!(ig33, "./", ".a/b", ".a/b");
    ignored!(ig34, ".", ".a/b", ".a/b");
    ignored!(ig35, "./.", ".a/b", ".a/b");
    ignored!(ig36, "././", ".a/b", ".a/b");
    ignored!(ig37, "././.", ".a/b", ".a/b");
    ignored!(ig38, ROOT, "\\[", "[");
    ignored!(ig39, ROOT, "\\?", "?");
    ignored!(ig40, ROOT, "\\*", "*");
    ignored!(ig41, ROOT, "\\a", "a");
    ignored!(ig42, ROOT, "s*.rs", "sfoo.rs");
    ignored!(ig43, ROOT, "**", "foo.rs");
    ignored!(ig44, ROOT, "**/**/*", "a/foo.rs");

    not_ignored!(ignot1, ROOT, "amonths", "months");
    not_ignored!(ignot2, ROOT, "monthsa", "months");
    not_ignored!(ignot3, ROOT, "/src/*.rs", "src/grep/src/main.rs");
    not_ignored!(ignot4, ROOT, "/*.c", "mozilla-sha1/sha1.c");
    not_ignored!(ignot5, ROOT, "/src/*.rs", "src/grep/src/main.rs");
    not_ignored!(ignot6, ROOT, "*.rs\n!src/main.rs", "src/main.rs");
    not_ignored!(ignot7, ROOT, "foo/", "foo", false);
    not_ignored!(ignot8, ROOT, "**/foo/**", "wat/src/afoo/bar/baz");
    not_ignored!(ignot9, ROOT, "**/foo/**", "wat/src/fooa/bar/baz");
    not_ignored!(ignot10, ROOT, "**/foo/bar", "foo/src/bar");
    not_ignored!(ignot11, ROOT, "#foo", "#foo");
    not_ignored!(ignot12, ROOT, "\n\n\n", "foo");
    not_ignored!(ignot13, ROOT, "foo/**", "foo", true);
    not_ignored!(
        ignot14,
        "./third_party/protobuf",
        "m4/ltoptions.m4",
        "./third_party/protobuf/csharp/src/packages/repositories.config"
    );
    not_ignored!(ignot15, ROOT, "!/bar", "foo/bar");
    not_ignored!(ignot16, ROOT, "*\n!**/", "foo", true);
    not_ignored!(ignot17, ROOT, "src/*.rs", "src/grep/src/main.rs");
    not_ignored!(ignot18, ROOT, "path1/*", "path2/path1/foo");
    not_ignored!(ignot19, ROOT, "s*.rs", "src/foo.rs");

    fn path_string<P: AsRef<Path>>(path: P) -> String {
        path.as_ref().to_str().unwrap().to_string()
    }

    #[test]
    fn parse_excludes_file1() {
        let got = super::parse_excludes_file(b"[core]\nexcludesFile = /foo/bar").unwrap();
        assert_eq!(path_string(got), "/foo/bar");
    }

    #[test]
    fn parse_excludes_file2() {
        let got = super::parse_excludes_file(b"[core]\nexcludesFile = ~/foo/bar").unwrap();
        assert_eq!(path_string(got), super::expand_tilde("~/foo/bar"));
    }

    #[test]
    fn parse_excludes_file3() {
        assert!(super::parse_excludes_file(b"[core]\nexcludeFile = /foo/bar").is_none());
    }

    #[test]
    fn parse_excludes_file4() {
        let got = super::parse_excludes_file(b"[core]\nexcludesFile = \"~/foo/bar\"");
        assert_eq!(path_string(got.unwrap()), super::expand_tilde("~/foo/bar"));
    }

    #[test]
    fn parse_excludes_file5() {
        let data = b"[core]\nexcludesFile = \" \"~/foo/bar \" \"";
        assert!(super::parse_excludes_file(data).is_none());
    }

    #[test]
    fn regression_106() {
        gi_from_str("/", " ");
    }

    #[test]
    fn case_insensitive() {
        let gi = GitignoreBuilder::new(ROOT)
            .case_insensitive(true)
            .add_str("*.html")
            .unwrap()
            .build()
            .unwrap();
        assert!(gi.matched("foo.html", false).is_ignore());
        assert!(gi.matched("foo.HTML", false).is_ignore());
        assert!(!gi.matched("foo.htm", false).is_ignore());
        assert!(!gi.matched("foo.HTM", false).is_ignore());
    }

    ignored!(cs1, ROOT, "*.html", "foo.html");
    not_ignored!(cs2, ROOT, "*.html", "foo.HTML");
    not_ignored!(cs3, ROOT, "*.html", "foo.htm");
    not_ignored!(cs4, ROOT, "*.html", "foo.HTM");

    #[test]
    fn matched_path_or_any_parents_walks_up() {
        let gi = gi_from_str(ROOT, "/a/b/\n!/a/b/keep");
        assert!(gi.matched_path_or_any_parents("a/b/c/d", false).is_ignore());
        assert!(
            gi.matched_path_or_any_parents("a/b/keep", false)
                .is_whitelist()
        );
        assert!(gi.matched_path_or_any_parents("a/x/c", false).is_none());
    }

    #[test]
    fn bom_and_crlf_lines_are_handled() {
        let dir = std::env::temp_dir().join(format!("walker_gi_bom_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ignore");
        std::fs::write(&file, "\u{feff}foo\r\nbar\r\n").unwrap();
        let mut builder = GitignoreBuilder::new(&dir);
        assert!(builder.add(&file).is_none());
        let gi = builder.build().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(gi.matched("foo", false).is_ignore());
        assert!(gi.matched("bar", false).is_ignore());
    }

    #[test]
    fn invalid_utf8_reports_line_number() {
        let dir = std::env::temp_dir().join(format!("walker_gi_utf8_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ignore");
        std::fs::write(&file, b"foo\nb\xffr\nbaz\n").unwrap();
        let mut builder = GitignoreBuilder::new(&dir);
        let err = builder.add(&file).unwrap();
        let gi = builder.build().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            err.to_string(),
            format!(
                "{}: line 2: stream did not contain valid UTF-8",
                file.display()
            )
        );
        assert!(gi.matched("foo", false).is_ignore());
        assert!(gi.matched("baz", false).is_none());
    }
}
