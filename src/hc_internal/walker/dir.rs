use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock, Weak};

use super::fsys::{FileType, Listing, Lookup};
use super::gitignore::{Gitignore, GitignoreBuilder};
use super::overrides::Override;
use super::types::Types;
use super::{Error, Match, PartialErrorBuilder, is_hidden, strip_prefix};

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct IgnoreOptions {
    pub(crate) hidden: bool,
    pub(crate) ignore: bool,
    pub(crate) parents: bool,
    pub(crate) git_global: bool,
    pub(crate) git_ignore: bool,
    pub(crate) git_exclude: bool,
    pub(crate) ignore_case_insensitive: bool,
    pub(crate) require_git: bool,
}

pub(crate) enum Probe<'a> {
    Fs,
    Listing(&'a Listing),
}

#[derive(Debug)]
struct Shared {
    compiled: RwLock<HashMap<OsString, Weak<IgnoreInner>>>,
    overrides: Arc<Override>,
    types: Arc<Types>,
    explicit_ignores: Arc<Vec<Gitignore>>,
    custom_ignore_filenames: Arc<Vec<OsString>>,
    git_global_matcher: Arc<Gitignore>,
    opts: IgnoreOptions,
    has_any_ignore_rules: bool,
    filter_rules: bool,
    global_rules: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Ignore(Arc<IgnoreInner>);

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
struct IgnoreInner {
    shared: Arc<Shared>,
    dir: PathBuf,
    parent: Option<Ignore>,
    is_absolute_parent: bool,
    absolute_base: Option<Arc<PathBuf>>,
    custom_ignore_matcher: Gitignore,
    ignore_matcher: Gitignore,
    git_ignore_matcher: Gitignore,
    git_exclude_matcher: Gitignore,
    has_git: bool,
    any_git: bool,
    rel_rules: bool,
    abs_rules: bool,
    any_rules: bool,
}

impl IgnoreInner {
    fn own_rules(&self) -> bool {
        !self.custom_ignore_matcher.is_empty()
            || !self.ignore_matcher.is_empty()
            || !self.git_ignore_matcher.is_empty()
            || !self.git_exclude_matcher.is_empty()
    }

    fn finish(mut self) -> IgnoreInner {
        let own = self.own_rules();
        let (p_git, p_rel, p_abs) = self.parent.as_ref().map_or((false, false, false), |p| {
            (p.0.any_git, p.0.rel_rules, p.0.abs_rules)
        });
        self.any_git = self.has_git || p_git;
        if self.is_absolute_parent {
            self.rel_rules = false;
            self.abs_rules = own || p_abs;
        } else {
            self.rel_rules = own || p_rel;
            self.abs_rules = p_abs;
        }
        let sh = &self.shared;
        self.any_rules = sh.filter_rules
            || (sh.has_any_ignore_rules && (self.rel_rules || self.abs_rules || sh.global_rules));
        self
    }
}

struct Loaded {
    custom: Gitignore,
    ignore: Gitignore,
    git_ignore: Gitignore,
    git_exclude: Gitignore,
    has_git: bool,
}

impl Loaded {
    fn is_noop(&self) -> bool {
        !self.has_git
            && self.custom.is_empty()
            && self.ignore.is_empty()
            && self.git_ignore.is_empty()
            && self.git_exclude.is_empty()
    }
}

fn lookup_exists(dir: &Path, name: &OsStr, lookup: Lookup) -> bool {
    match lookup {
        Lookup::Absent => false,
        Lookup::Present(_) => true,
        Lookup::Unknown => dir.join(name).exists(),
    }
}

impl Ignore {
    pub(crate) fn path(&self) -> &Path {
        &self.0.dir
    }

    pub(crate) fn is_root(&self) -> bool {
        self.0.parent.is_none()
    }

    pub(crate) fn is_absolute_parent(&self) -> bool {
        self.0.is_absolute_parent
    }

    pub(crate) fn parents(&self) -> impl Iterator<Item = &Ignore> {
        std::iter::successors(Some(self), |ig| ig.0.parent.as_ref())
    }

    pub(crate) fn add_parents<P: AsRef<Path>>(&self, path: P) -> (Ignore, Option<Error>) {
        let opts = self.0.shared.opts;
        if !opts.parents && !opts.git_ignore && !opts.git_exclude && !opts.git_global {
            return (self.clone(), None);
        }
        if !self.is_root() {
            return (self.clone(), None);
        }
        let Ok(absolute_base) = path.as_ref().canonicalize() else {
            return (self.clone(), None);
        };
        let absolute_base = Arc::new(absolute_base);
        let parents: Vec<&Path> = absolute_base.ancestors().skip(1).collect();
        let mut errs = PartialErrorBuilder::default();
        let mut ig = self.clone();
        for parent in parents.into_iter().rev() {
            let mut compiled = self
                .0
                .shared
                .compiled
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(prebuilt) = compiled.get(parent.as_os_str()).and_then(Weak::upgrade) {
                ig = Ignore(prebuilt);
                continue;
            }
            let (loaded, err) = ig.load(parent, &Probe::Fs);
            errs.maybe_push(err);
            let mut igtmp = ig.inner(parent, loaded);
            igtmp.is_absolute_parent = true;
            igtmp.absolute_base = Some(absolute_base.clone());
            igtmp.has_git = if opts.require_git && opts.git_ignore {
                parent.join(".git").exists() || parent.join(".jj").exists()
            } else {
                false
            };
            let ig_arc = Arc::new(igtmp.finish());
            ig = Ignore(ig_arc.clone());
            compiled.insert(parent.as_os_str().to_os_string(), Arc::downgrade(&ig_arc));
        }
        (ig, errs.into_error_option())
    }

    pub(crate) fn add_child(
        &self,
        dir: &Path,
        probe: &Probe<'_>,
        keep_chain: bool,
    ) -> (Ignore, Option<Error>) {
        let (loaded, err) = self.load(dir, probe);
        if !keep_chain && !self.0.is_absolute_parent && loaded.is_noop() {
            return (self.clone(), err);
        }
        (Ignore(Arc::new(self.inner(dir, loaded).finish())), err)
    }

    fn inner(&self, dir: &Path, loaded: Loaded) -> IgnoreInner {
        IgnoreInner {
            shared: self.0.shared.clone(),
            dir: dir.to_path_buf(),
            parent: Some(self.clone()),
            is_absolute_parent: false,
            absolute_base: self.0.absolute_base.clone(),
            custom_ignore_matcher: loaded.custom,
            ignore_matcher: loaded.ignore,
            git_ignore_matcher: loaded.git_ignore,
            git_exclude_matcher: loaded.git_exclude,
            has_git: loaded.has_git,
            any_git: false,
            rel_rules: false,
            abs_rules: false,
            any_rules: false,
        }
    }

    fn load(&self, dir: &Path, probe: &Probe<'_>) -> (Loaded, Option<Error>) {
        let sh = &self.0.shared;
        let opts = sh.opts;
        let ci = opts.ignore_case_insensitive;
        let mut names: Vec<&OsStr> = vec![
            OsStr::new(".git"),
            OsStr::new(".jj"),
            OsStr::new(".ignore"),
            OsStr::new(".gitignore"),
        ];
        names.extend(sh.custom_ignore_filenames.iter().map(OsString::as_os_str));
        let lookups = match probe {
            Probe::Fs => vec![Lookup::Unknown; names.len()],
            Probe::Listing(listing) => listing.lookup_all(&names),
        };
        let git_type = if opts.require_git && (opts.git_ignore || opts.git_exclude) {
            match lookups[0] {
                Lookup::Absent => None,
                Lookup::Present(ty) => Some(ty),
                Lookup::Unknown => dir
                    .join(".git")
                    .metadata()
                    .ok()
                    .map(|md| FileType::from(md.file_type())),
            }
        } else {
            None
        };
        let has_git = git_type.is_some() || lookup_exists(dir, names[1], lookups[1]);

        let mut errs = PartialErrorBuilder::default();
        let custom_ig_matcher = if sh.custom_ignore_filenames.is_empty() {
            Gitignore::empty()
        } else {
            let present: Vec<&OsStr> = names[4..]
                .iter()
                .zip(&lookups[4..])
                .filter(|(n, l)| lookup_exists(dir, n, **l))
                .map(|(n, _)| *n)
                .collect();
            let (m, err) = create_gitignore(dir, dir, &present, ci);
            errs.maybe_push(err);
            m
        };
        let ig_matcher = if opts.ignore && lookup_exists(dir, names[2], lookups[2]) {
            let (m, err) = create_gitignore(dir, dir, &[names[2]], ci);
            errs.maybe_push(err);
            m
        } else {
            Gitignore::empty()
        };
        let gi_matcher = if opts.git_ignore && lookup_exists(dir, names[3], lookups[3]) {
            let (m, err) = create_gitignore(dir, dir, &[names[3]], ci);
            errs.maybe_push(err);
            m
        } else {
            Gitignore::empty()
        };
        let gi_exclude_matcher = if !opts.git_exclude || lookups[0] == Lookup::Absent {
            Gitignore::empty()
        } else {
            match resolve_git_commondir(dir, git_type) {
                Ok(git_dir) => {
                    let exclude = OsStr::new("info/exclude");
                    if git_dir.join(exclude).exists() {
                        let (m, err) = create_gitignore(dir, &git_dir, &[exclude], ci);
                        errs.maybe_push(err);
                        m
                    } else {
                        Gitignore::empty()
                    }
                }
                Err(err) => {
                    errs.maybe_push(err);
                    Gitignore::empty()
                }
            }
        };
        let loaded = Loaded {
            custom: custom_ig_matcher,
            ignore: ig_matcher,
            git_ignore: gi_matcher,
            git_exclude: gi_exclude_matcher,
            has_git,
        };
        (loaded, errs.into_error_option())
    }

    pub(crate) fn should_skip(&self, path: &Path, is_dir: bool, attr_hidden: bool) -> bool {
        let m = if self.0.any_rules {
            self.matched(path, is_dir)
        } else {
            Match::None
        };
        match m {
            Match::Ignore(()) => true,
            Match::Whitelist(()) => false,
            Match::None => self.0.shared.opts.hidden && (attr_hidden || is_hidden(path)),
        }
    }

    fn matched(&self, path: &Path, is_dir: bool) -> Match<()> {
        let path = strip_prefix("./", path).unwrap_or(path);
        let sh = &self.0.shared;
        if !sh.overrides.is_empty() {
            let mat = sh.overrides.matched(path, is_dir).map(|_| ());
            if !mat.is_none() {
                return mat;
            }
        }
        let mut whitelisted = Match::None;
        if sh.has_any_ignore_rules {
            let mat = self.matched_ignore(path, is_dir);
            if mat.is_ignore() {
                return mat;
            } else if mat.is_whitelist() {
                whitelisted = mat;
            }
        }
        if !sh.types.is_empty() {
            let mat = sh.types.matched(path, is_dir).map(|_| ());
            if mat.is_ignore() {
                return mat;
            } else if mat.is_whitelist() {
                whitelisted = mat;
            }
        }
        whitelisted
    }

    fn matched_ignore(&self, path: &Path, is_dir: bool) -> Match<()> {
        fn gm(gi: &Gitignore, path: &Path, is_dir: bool) -> Match<()> {
            gi.matched(path, is_dir).map(|_| ())
        }
        let sh = &self.0.shared;
        let (mut m_custom, mut m_ignore, mut m_gi, mut m_gi_exclude, mut m_explicit) = (
            Match::None,
            Match::None,
            Match::None,
            Match::None,
            Match::None,
        );
        let any_git = !sh.opts.require_git || self.0.any_git;
        let mut saw_git = false;
        if self.0.rel_rules || (sh.opts.parents && self.0.abs_rules) {
            for ig in self.parents().take_while(|ig| !ig.0.is_absolute_parent) {
                let ig = &ig.0;
                if m_custom.is_none() {
                    m_custom = gm(&ig.custom_ignore_matcher, path, is_dir);
                }
                if m_ignore.is_none() {
                    m_ignore = gm(&ig.ignore_matcher, path, is_dir);
                }
                if any_git && !saw_git && m_gi.is_none() {
                    m_gi = gm(&ig.git_ignore_matcher, path, is_dir);
                }
                if any_git && !saw_git && m_gi_exclude.is_none() {
                    m_gi_exclude = gm(&ig.git_exclude_matcher, path, is_dir);
                }
                saw_git = saw_git || ig.has_git;
            }
        }
        if sh.opts.parents
            && self.0.abs_rules
            && let Some(abs_parent_path) = self.0.absolute_base.as_deref()
        {
            let rel = match self
                .parents()
                .take_while(|ig| !ig.0.is_absolute_parent)
                .last()
            {
                None => path,
                Some(ig) if ig.0.dir.as_path() == Path::new(".") => path,
                Some(ig) => {
                    let without_dot_slash = strip_if_is_prefix("./", &ig.0.dir);
                    let relative_base = strip_if_is_prefix(without_dot_slash, path);
                    strip_if_is_prefix("/", relative_base)
                }
            };
            let path = abs_parent_path.join(rel);
            for ig in self.parents().skip_while(|ig| !ig.0.is_absolute_parent) {
                let ig = &ig.0;
                if m_custom.is_none() {
                    m_custom = gm(&ig.custom_ignore_matcher, &path, is_dir);
                }
                if m_ignore.is_none() {
                    m_ignore = gm(&ig.ignore_matcher, &path, is_dir);
                }
                if any_git && !saw_git && m_gi.is_none() {
                    m_gi = gm(&ig.git_ignore_matcher, &path, is_dir);
                }
                if any_git && !saw_git && m_gi_exclude.is_none() {
                    m_gi_exclude = gm(&ig.git_exclude_matcher, &path, is_dir);
                }
                saw_git = saw_git || ig.has_git;
            }
        }
        for gi in sh.explicit_ignores.iter().rev() {
            if !m_explicit.is_none() {
                break;
            }
            m_explicit = gm(gi, path, is_dir);
        }
        let m_global = if any_git {
            gm(&sh.git_global_matcher, path, is_dir)
        } else {
            Match::None
        };
        m_custom
            .or(m_ignore)
            .or(m_gi)
            .or(m_gi_exclude)
            .or(m_global)
            .or(m_explicit)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IgnoreBuilder {
    dir: PathBuf,
    overrides: Arc<Override>,
    types: Arc<Types>,
    explicit_ignores: Vec<Gitignore>,
    custom_ignore_filenames: Vec<OsString>,
    global_gitignores_relative_to: Option<PathBuf>,
    pub(crate) opts: IgnoreOptions,
}

impl IgnoreBuilder {
    pub(crate) fn new() -> IgnoreBuilder {
        IgnoreBuilder {
            dir: PathBuf::new(),
            overrides: Arc::new(Override::empty()),
            types: Arc::new(Types::empty()),
            explicit_ignores: vec![],
            custom_ignore_filenames: vec![],
            global_gitignores_relative_to: None,
            opts: IgnoreOptions {
                hidden: true,
                ignore: true,
                parents: true,
                git_global: true,
                git_ignore: true,
                git_exclude: true,
                ignore_case_insensitive: false,
                require_git: true,
            },
        }
    }

    pub(crate) fn build_with_cwd(&self, cwd: Option<PathBuf>) -> Ignore {
        let global_gitignores_relative_to =
            cwd.or_else(|| self.global_gitignores_relative_to.clone());
        let git_global_matcher = match (&global_gitignores_relative_to, self.opts.git_global) {
            (Some(cwd), true) => {
                let mut builder = GitignoreBuilder::new(cwd);
                let _ = builder.case_insensitive(self.opts.ignore_case_insensitive);
                builder.build_global().0
            }
            _ => Gitignore::empty(),
        };
        let opts = self.opts;
        let has_any_ignore_rules = opts.ignore
            || opts.git_global
            || opts.git_ignore
            || opts.git_exclude
            || !self.custom_ignore_filenames.is_empty()
            || !self.explicit_ignores.is_empty();
        let shared = Arc::new(Shared {
            compiled: RwLock::new(HashMap::new()),
            filter_rules: !self.overrides.is_empty() || !self.types.is_empty(),
            global_rules: !git_global_matcher.is_empty()
                || self.explicit_ignores.iter().any(|g| !g.is_empty()),
            overrides: self.overrides.clone(),
            types: self.types.clone(),
            explicit_ignores: Arc::new(self.explicit_ignores.clone()),
            custom_ignore_filenames: Arc::new(self.custom_ignore_filenames.clone()),
            git_global_matcher: Arc::new(git_global_matcher),
            opts,
            has_any_ignore_rules,
        });
        Ignore(Arc::new(
            IgnoreInner {
                shared,
                dir: self.dir.clone(),
                parent: None,
                is_absolute_parent: true,
                absolute_base: None,
                custom_ignore_matcher: Gitignore::empty(),
                ignore_matcher: Gitignore::empty(),
                git_ignore_matcher: Gitignore::empty(),
                git_exclude_matcher: Gitignore::empty(),
                has_git: false,
                any_git: false,
                rel_rules: false,
                abs_rules: false,
                any_rules: false,
            }
            .finish(),
        ))
    }

    pub(crate) fn current_dir(&mut self, cwd: impl Into<PathBuf>) -> &mut IgnoreBuilder {
        self.global_gitignores_relative_to = Some(cwd.into());
        self
    }

    pub(crate) fn overrides(&mut self, overrides: Override) -> &mut IgnoreBuilder {
        self.overrides = Arc::new(overrides);
        self
    }

    pub(crate) fn types(&mut self, types: Types) -> &mut IgnoreBuilder {
        self.types = Arc::new(types);
        self
    }

    pub(crate) fn add_ignore(&mut self, ig: Gitignore) -> &mut IgnoreBuilder {
        self.explicit_ignores.push(ig);
        self
    }

    pub(crate) fn add_custom_ignore_filename<S: AsRef<OsStr>>(
        &mut self,
        file_name: S,
    ) -> &mut IgnoreBuilder {
        self.custom_ignore_filenames
            .push(file_name.as_ref().to_os_string());
        self
    }
}

fn create_gitignore(
    dir: &Path,
    dir_for_ignorefile: &Path,
    names: &[&OsStr],
    case_insensitive: bool,
) -> (Gitignore, Option<Error>) {
    if names.is_empty() {
        return (Gitignore::empty(), None);
    }
    let mut builder = GitignoreBuilder::new(dir);
    let mut errs = PartialErrorBuilder::default();
    let _ = builder.case_insensitive(case_insensitive);
    for name in names {
        errs.maybe_push_ignore_io(builder.add(dir_for_ignorefile.join(name)));
    }
    let gi = match builder.build() {
        Ok(gi) => gi,
        Err(err) => {
            errs.push(err);
            Gitignore::empty()
        }
    };
    (gi, errs.into_error_option())
}

fn first_line(path: &Path) -> Result<Option<String>, io::Error> {
    let file = io::BufReader::new(File::open(path)?);
    file.lines().next().transpose()
}

fn resolve_git_commondir(dir: &Path, git_type: Option<FileType>) -> Result<PathBuf, Option<Error>> {
    let git_dir = dir.join(".git");
    if !git_type.is_some_and(FileType::is_file) {
        return Ok(git_dir);
    }
    let dot_git_line = match first_line(&git_dir) {
        Ok(Some(line)) => line,
        Ok(None) => return Err(None),
        Err(err) => return Err(Some(Error::Io(err).with_path(git_dir))),
    };
    let Some(real) = dot_git_line.strip_prefix("gitdir: ") else {
        return Err(None);
    };
    let real_git_dir = PathBuf::from(real);
    let commondir_file = real_git_dir.join("commondir");
    let file = match File::open(&commondir_file) {
        Ok(file) => io::BufReader::new(file),
        Err(_) => return Err(None),
    };
    let commondir_line = match file.lines().next() {
        Some(Ok(line)) => line,
        Some(Err(err)) => return Err(Some(Error::Io(err).with_path(commondir_file))),
        None => return Err(None),
    };
    Ok(if commondir_line.starts_with('.') {
        real_git_dir.join(commondir_line)
    } else {
        PathBuf::from(commondir_line)
    })
}

fn strip_if_is_prefix<'a, P: AsRef<Path> + ?Sized>(prefix: &'a P, path: &'a Path) -> &'a Path {
    strip_prefix(prefix, path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::Error;
    use super::super::gitignore::Gitignore;
    use super::super::tests::{TempDir, mkdirp, wfile};
    use super::{Ignore, IgnoreBuilder, Probe};

    fn child(ig: &Ignore, dir: &Path) -> (Ignore, Option<Error>) {
        ig.add_child(dir, &Probe::Fs, true)
    }

    fn listed_child(ig: &Ignore, dir: &Path) -> (Ignore, Option<Error>) {
        let listing = super::super::fsys::read_dir(dir).unwrap();
        ig.add_child(dir, &Probe::Listing(&listing), true)
    }

    fn matched(ig: &Ignore, path: &str, is_dir: bool) -> super::Match<()> {
        ig.matched(Path::new(path), is_dir)
    }

    fn partial(err: Error) -> Vec<Error> {
        match err {
            Error::Partial(errs) => errs,
            _ => panic!("expected partial error but got {err:?}"),
        }
    }

    fn base() -> IgnoreBuilder {
        IgnoreBuilder::new()
    }

    fn build(b: &IgnoreBuilder) -> Ignore {
        b.build_with_cwd(None)
    }

    #[test]
    fn explicit_ignore() {
        let td = TempDir::new();
        wfile(td.path().join("not-an-ignore"), "foo\n!bar");
        let (gi, err) = Gitignore::new(td.path().join("not-an-ignore"));
        assert!(err.is_none());
        let mut b = base();
        b.add_ignore(gi);
        let (ig, err) = child(&build(&b), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_ignore());
        assert!(matched(&ig, "bar", false).is_whitelist());
        assert!(matched(&ig, "baz", false).is_none());
    }

    #[test]
    fn git_exclude() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git/info"));
        wfile(td.path().join(".git/info/exclude"), "foo\n!bar");
        for f in [child, listed_child] {
            let (ig, err) = f(&build(&base()), td.path());
            assert!(err.is_none());
            assert!(matched(&ig, "foo", false).is_ignore());
            assert!(matched(&ig, "bar", false).is_whitelist());
            assert!(matched(&ig, "baz", false).is_none());
        }
    }

    #[test]
    fn gitignore() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        wfile(td.path().join(".gitignore"), "foo\n!bar");
        for f in [child, listed_child] {
            let (ig, err) = f(&build(&base()), td.path());
            assert!(err.is_none());
            assert!(matched(&ig, "foo", false).is_ignore());
            assert!(matched(&ig, "bar", false).is_whitelist());
            assert!(matched(&ig, "baz", false).is_none());
        }
    }

    #[test]
    fn gitignore_with_jj() {
        let td = TempDir::new();
        mkdirp(td.path().join(".jj"));
        wfile(td.path().join(".gitignore"), "foo\n!bar");
        for f in [child, listed_child] {
            let (ig, err) = f(&build(&base()), td.path());
            assert!(err.is_none());
            assert!(matched(&ig, "foo", false).is_ignore());
            assert!(matched(&ig, "bar", false).is_whitelist());
        }
    }

    #[test]
    fn gitignore_no_git() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "foo\n!bar");
        for f in [child, listed_child] {
            let (ig, err) = f(&build(&base()), td.path());
            assert!(err.is_none());
            assert!(matched(&ig, "foo", false).is_none());
            assert!(matched(&ig, "bar", false).is_none());
        }
    }

    #[test]
    fn gitignore_allowed_no_git() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "foo\n!bar");
        let mut b = base();
        b.opts.require_git = false;
        let (ig, err) = child(&build(&b), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_ignore());
        assert!(matched(&ig, "bar", false).is_whitelist());
        assert!(matched(&ig, "baz", false).is_none());
    }

    #[test]
    fn ignore() {
        let td = TempDir::new();
        wfile(td.path().join(".ignore"), "foo\n!bar");
        let (ig, err) = child(&build(&base()), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_ignore());
        assert!(matched(&ig, "bar", false).is_whitelist());
        assert!(matched(&ig, "baz", false).is_none());
    }

    #[test]
    fn custom_ignore() {
        let td = TempDir::new();
        wfile(td.path().join(".customignore"), "foo\n!bar");
        let mut b = base();
        b.add_custom_ignore_filename(".customignore");
        for f in [child, listed_child] {
            let (ig, err) = f(&build(&b), td.path());
            assert!(err.is_none());
            assert!(matched(&ig, "foo", false).is_ignore());
            assert!(matched(&ig, "bar", false).is_whitelist());
            assert!(matched(&ig, "baz", false).is_none());
        }
    }

    #[test]
    fn custom_ignore_over_ignore() {
        let td = TempDir::new();
        wfile(td.path().join(".ignore"), "foo");
        wfile(td.path().join(".customignore"), "!foo");
        let mut b = base();
        b.add_custom_ignore_filename(".customignore");
        let (ig, err) = child(&build(&b), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_whitelist());
    }

    #[test]
    fn custom_ignore_precedence() {
        let td = TempDir::new();
        wfile(td.path().join(".customignore1"), "foo");
        wfile(td.path().join(".customignore2"), "!foo");
        let mut b = base();
        b.add_custom_ignore_filename(".customignore1");
        b.add_custom_ignore_filename(".customignore2");
        let (ig, err) = child(&build(&b), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_whitelist());
    }

    #[test]
    fn ignore_over_gitignore() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "foo");
        wfile(td.path().join(".ignore"), "!foo");
        let (ig, err) = child(&build(&base()), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "foo", false).is_whitelist());
    }

    #[test]
    fn exclude_lowest() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "!foo");
        wfile(td.path().join(".ignore"), "!bar");
        mkdirp(td.path().join(".git/info"));
        wfile(td.path().join(".git/info/exclude"), "foo\nbar\nbaz");
        let (ig, err) = child(&build(&base()), td.path());
        assert!(err.is_none());
        assert!(matched(&ig, "baz", false).is_ignore());
        assert!(matched(&ig, "foo", false).is_whitelist());
        assert!(matched(&ig, "bar", false).is_whitelist());
    }

    #[test]
    fn errored() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "{foo");
        let (_, err) = child(&build(&base()), td.path());
        assert!(err.is_some());
    }

    #[test]
    fn errored_both() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "{foo");
        wfile(td.path().join(".ignore"), "{bar");
        let (_, err) = child(&build(&base()), td.path());
        assert_eq!(2, partial(err.expect("an error")).len());
    }

    #[test]
    fn errored_partial() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        wfile(td.path().join(".gitignore"), "{foo\nbar");
        let (ig, err) = child(&build(&base()), td.path());
        assert!(err.is_some());
        assert!(matched(&ig, "bar", false).is_ignore());
    }

    #[test]
    fn errored_partial_and_ignore() {
        let td = TempDir::new();
        wfile(td.path().join(".gitignore"), "{foo\nbar");
        wfile(td.path().join(".ignore"), "!bar");
        let (ig, err) = child(&build(&base()), td.path());
        assert!(err.is_some());
        assert!(matched(&ig, "bar", false).is_whitelist());
    }

    #[test]
    fn not_present_empty() {
        let td = TempDir::new();
        let (_, err) = child(&build(&base()), td.path());
        assert!(err.is_none());
    }

    #[test]
    fn stops_at_git_dir() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        mkdirp(td.path().join("foo/.git"));
        wfile(td.path().join(".gitignore"), "foo");
        wfile(td.path().join(".ignore"), "bar");
        let ig0 = build(&base());
        let (ig1, err) = child(&ig0, td.path());
        assert!(err.is_none());
        let (ig2, err) = child(&ig1, &ig1.path().join("foo"));
        assert!(err.is_none());
        assert!(matched(&ig1, "foo", false).is_ignore());
        assert!(matched(&ig2, "foo", false).is_none());
        assert!(matched(&ig1, "bar", false).is_ignore());
        assert!(matched(&ig2, "bar", false).is_ignore());
    }

    #[test]
    fn absolute_parent() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        mkdirp(td.path().join("foo"));
        wfile(td.path().join(".gitignore"), "bar");
        let ig0 = build(&base());
        let (ig1, err) = child(&ig0, &td.path().join("foo"));
        assert!(err.is_none());
        assert!(matched(&ig1, "bar", false).is_none());
        let ig0 = build(&base());
        let (ig1, err) = ig0.add_parents(td.path().join("foo"));
        assert!(err.is_none());
        let (ig2, err) = child(&ig1, &td.path().join("foo"));
        assert!(err.is_none());
        assert!(matched(&ig2, "bar", false).is_ignore());
    }

    #[test]
    fn absolute_parent_anchored() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        mkdirp(td.path().join("src/llvm"));
        wfile(td.path().join(".gitignore"), "/llvm/\nfoo");
        let ig0 = build(&base());
        let (ig1, err) = ig0.add_parents(td.path().join("src"));
        assert!(err.is_none());
        let (ig2, err) = child(&ig1, Path::new("src"));
        assert!(err.is_none());
        assert!(matched(&ig1, "llvm", true).is_none());
        assert!(matched(&ig2, "llvm", true).is_none());
        assert!(matched(&ig2, "src/llvm", true).is_none());
        assert!(matched(&ig2, "foo", false).is_ignore());
        assert!(matched(&ig2, "src/foo", false).is_ignore());
    }

    #[test]
    fn git_info_exclude_in_linked_worktree() {
        let td = TempDir::new();
        let git_dir = td.path().join(".git");
        mkdirp(git_dir.join("info"));
        wfile(git_dir.join("info/exclude"), "ignore_me");
        mkdirp(git_dir.join("worktrees/linked-worktree"));
        let commondir_path = git_dir.join("worktrees/linked-worktree/commondir");
        mkdirp(td.path().join("linked-worktree"));
        let worktree_git_dir_abs = format!(
            "gitdir: {}",
            git_dir.join("worktrees/linked-worktree").to_str().unwrap(),
        );
        wfile(
            td.path().join("linked-worktree/.git"),
            &worktree_git_dir_abs,
        );
        let wt = td.path().join("linked-worktree");
        wfile(&commondir_path, "../..");
        let ib = build(&base());
        for f in [child, listed_child] {
            let (ignore, err) = f(&ib, &wt);
            assert!(err.is_none());
            assert!(matched(&ignore, "ignore_me", false).is_ignore());
        }
        wfile(&commondir_path, git_dir.to_str().unwrap());
        let (ignore, err) = child(&ib, &wt);
        assert!(err.is_none());
        assert!(matched(&ignore, "ignore_me", false).is_ignore());
        std::fs::remove_file(&commondir_path).unwrap();
        let (_, err) = child(&ib, &wt);
        assert!(err.is_none());
        wfile(td.path().join("linked-worktree/.git"), "garbage");
        let (_, err) = child(&ib, &wt);
        assert!(err.is_none());
        wfile(td.path().join("linked-worktree/.git"), "gitdir: garbage");
        let (_, err) = child(&ib, &wt);
        assert!(err.is_none());
    }

    #[test]
    fn reuse_parent_when_nothing_changes() {
        let td = TempDir::new();
        mkdirp(td.path().join(".git"));
        mkdirp(td.path().join("a/b"));
        wfile(td.path().join(".gitignore"), "foo");
        let ig0 = build(&base());
        let (ig1, _) = child(&ig0, td.path());
        let (ig2, _) = ig1.add_child(&td.path().join("a"), &Probe::Fs, false);
        assert!(std::sync::Arc::ptr_eq(&ig1.0, &ig2.0));
        assert!(ig2.should_skip(&td.path().join("a/foo"), false, false));
        assert!(!ig2.should_skip(&td.path().join("a/bar"), false, false));
    }
}
