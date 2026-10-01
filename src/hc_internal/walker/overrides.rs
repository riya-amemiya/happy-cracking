use std::path::Path;

use super::gitignore::{Gitignore, GitignoreBuilder};
use super::{Error, Match};

#[derive(Clone, Debug)]
pub struct Glob;

#[derive(Clone, Debug)]
pub struct Override(Gitignore);

impl Override {
    pub fn empty() -> Override {
        Override(Gitignore::empty())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn num_whitelists(&self) -> u64 {
        self.0.num_ignores()
    }

    pub fn matched<P: AsRef<Path>>(&self, path: P, is_dir: bool) -> Match<Glob> {
        if self.is_empty() {
            return Match::None;
        }
        let mat = self.0.matched(path, is_dir).invert();
        if mat.is_none() && self.num_whitelists() > 0 && !is_dir {
            return Match::Ignore(Glob);
        }
        mat.map(|_| Glob)
    }
}

#[derive(Clone, Debug)]
pub struct OverrideBuilder {
    builder: GitignoreBuilder,
}

impl OverrideBuilder {
    pub fn new<P: AsRef<Path>>(path: P) -> OverrideBuilder {
        let mut builder = GitignoreBuilder::new(path);
        builder.allow_unclosed_class(false);
        OverrideBuilder { builder }
    }

    pub fn build(&self) -> Result<Override, Error> {
        Ok(Override(self.builder.build()?))
    }

    pub fn add(&mut self, glob: &str) -> Result<&mut OverrideBuilder, Error> {
        self.builder.add_line(glob)?;
        Ok(self)
    }

    pub fn case_insensitive(&mut self, yes: bool) -> &mut OverrideBuilder {
        self.builder.case_insensitive(yes);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Override, OverrideBuilder};

    const ROOT: &str = "/home/andrew/foo";

    fn ov(globs: &[&str]) -> Override {
        let mut builder = OverrideBuilder::new(ROOT);
        for glob in globs {
            builder.add(glob).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn empty() {
        let ov = ov(&[]);
        assert!(ov.matched("a.foo", false).is_none());
        assert!(ov.matched("a", false).is_none());
        assert!(ov.matched("", false).is_none());
    }

    #[test]
    fn simple() {
        let ov = ov(&["*.foo", "!*.bar"]);
        assert!(ov.matched("a.foo", false).is_whitelist());
        assert!(ov.matched("a.foo", true).is_whitelist());
        assert!(ov.matched("a.rs", false).is_ignore());
        assert!(ov.matched("a.rs", true).is_none());
        assert!(ov.matched("a.bar", false).is_ignore());
        assert!(ov.matched("a.bar", true).is_ignore());
    }

    #[test]
    fn only_ignores() {
        let ov = ov(&["!*.bar"]);
        assert!(ov.matched("a.rs", false).is_none());
        assert!(ov.matched("a.rs", true).is_none());
        assert!(ov.matched("a.bar", false).is_ignore());
        assert!(ov.matched("a.bar", true).is_ignore());
    }

    #[test]
    fn precedence() {
        let ov = ov(&["*.foo", "!*.bar.foo"]);
        assert!(ov.matched("a.foo", false).is_whitelist());
        assert!(ov.matched("a.baz", false).is_ignore());
        assert!(ov.matched("a.bar.foo", false).is_ignore());
    }

    #[test]
    fn gitignore() {
        let ov = ov(&["/foo", "bar/*.rs", "baz/**"]);
        assert!(ov.matched("bar/lib.rs", false).is_whitelist());
        assert!(ov.matched("bar/wat/lib.rs", false).is_ignore());
        assert!(ov.matched("wat/bar/lib.rs", false).is_ignore());
        assert!(ov.matched("foo", false).is_whitelist());
        assert!(ov.matched("wat/foo", false).is_ignore());
        assert!(ov.matched("baz", false).is_ignore());
        assert!(ov.matched("baz/a", false).is_whitelist());
        assert!(ov.matched("baz/a/b", false).is_whitelist());
    }

    #[test]
    fn allow_directories() {
        let ov = ov(&["*.rs"]);
        assert!(ov.matched("foo.rs", false).is_whitelist());
        assert!(ov.matched("foo.c", false).is_ignore());
        assert!(ov.matched("foo", false).is_ignore());
        assert!(ov.matched("foo", true).is_none());
        assert!(ov.matched("src/foo.rs", false).is_whitelist());
        assert!(ov.matched("src/foo.c", false).is_ignore());
        assert!(ov.matched("src/foo", false).is_ignore());
        assert!(ov.matched("src/foo", true).is_none());
    }

    #[test]
    fn absolute_path() {
        let ov = ov(&["!/bar"]);
        assert!(ov.matched("./foo/bar", false).is_none());
    }

    #[test]
    fn case_insensitive() {
        let ov = OverrideBuilder::new(ROOT)
            .case_insensitive(true)
            .add("*.html")
            .unwrap()
            .build()
            .unwrap();
        assert!(ov.matched("foo.html", false).is_whitelist());
        assert!(ov.matched("foo.HTML", false).is_whitelist());
        assert!(ov.matched("foo.htm", false).is_ignore());
        assert!(ov.matched("foo.HTM", false).is_ignore());
    }

    #[test]
    fn default_case_sensitive() {
        let ov = OverrideBuilder::new(ROOT)
            .add("*.html")
            .unwrap()
            .build()
            .unwrap();
        assert!(ov.matched("foo.html", false).is_whitelist());
        assert!(ov.matched("foo.HTML", false).is_ignore());
        assert!(ov.matched("foo.htm", false).is_ignore());
        assert!(ov.matched("foo.HTM", false).is_ignore());
    }

    #[test]
    fn unclosed_class_is_an_error() {
        let err = OverrideBuilder::new(ROOT).add("[abc").err().unwrap();
        assert_eq!(
            err.to_string(),
            "error parsing glob '[abc': unclosed character class; missing ']'"
        );
    }
}
