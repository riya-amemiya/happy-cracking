use std::collections::HashMap;
use std::path::Path;

use super::default_types::DEFAULT_TYPES;
use super::glob::{Candidate, GlobBuilder, GlobSet, GlobSetBuilder};
use super::{Error, Match, file_name};

#[derive(Clone, Debug)]
pub struct Glob;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTypeDef {
    name: String,
    globs: Vec<String>,
}

impl FileTypeDef {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn globs(&self) -> &[String] {
        &self.globs
    }
}

#[derive(Clone, Debug)]
pub struct Types {
    defs: Vec<FileTypeDef>,
    selections: Vec<Selection<FileTypeDef>>,
    has_selected: bool,
    glob_to_selection: Vec<usize>,
    set: GlobSet,
}

#[derive(Clone, Debug)]
enum Selection<T> {
    Select(String, T),
    Negate(String, T),
}

impl<T> Selection<T> {
    fn is_negated(&self) -> bool {
        matches!(self, Selection::Negate(..))
    }

    fn name(&self) -> &str {
        match self {
            Selection::Select(name, _) | Selection::Negate(name, _) => name,
        }
    }

    fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Selection<U> {
        match self {
            Selection::Select(name, inner) => Selection::Select(name, f(inner)),
            Selection::Negate(name, inner) => Selection::Negate(name, f(inner)),
        }
    }
}

impl Types {
    pub fn empty() -> Types {
        Types {
            defs: vec![],
            selections: vec![],
            has_selected: false,
            glob_to_selection: vec![],
            set: GlobSet::empty(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    pub fn definitions(&self) -> &[FileTypeDef] {
        &self.defs
    }

    pub fn matched<P: AsRef<Path>>(&self, path: P, is_dir: bool) -> Match<Glob> {
        if is_dir || self.set.is_empty() {
            return Match::None;
        }
        let Some(name) = file_name(path.as_ref()) else {
            return if self.has_selected {
                Match::Ignore(Glob)
            } else {
                Match::None
            };
        };
        if let Some(i) = self.set.last_match(&Candidate::new(name), &|_| true) {
            let sel = &self.selections[self.glob_to_selection[i]];
            return if sel.is_negated() {
                Match::Ignore(Glob)
            } else {
                Match::Whitelist(Glob)
            };
        }
        if self.has_selected {
            Match::Ignore(Glob)
        } else {
            Match::None
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TypesBuilder {
    types: HashMap<String, FileTypeDef>,
    selections: Vec<Selection<()>>,
}

impl TypesBuilder {
    pub fn new() -> TypesBuilder {
        TypesBuilder::default()
    }

    pub fn build(&self) -> Result<Types, Error> {
        let defs = self.definitions();
        let has_selected = self.selections.iter().any(|s| !s.is_negated());
        let mut selections = vec![];
        let mut glob_to_selection = vec![];
        let mut build_set = GlobSetBuilder::new();
        for (isel, selection) in self.selections.iter().enumerate() {
            let Some(def) = self.types.get(selection.name()) else {
                return Err(Error::UnrecognizedFileType(selection.name().to_string()));
            };
            for glob in &def.globs {
                build_set.add(
                    GlobBuilder::new(glob)
                        .literal_separator(true)
                        .build()
                        .map_err(|err| Error::Glob {
                            glob: Some(glob.clone()),
                            err: err.kind().to_string(),
                        })?,
                );
                glob_to_selection.push(isel);
            }
            selections.push(selection.clone().map(|()| def.clone()));
        }
        let set = build_set.build().map_err(|err| Error::Glob {
            glob: None,
            err: err.to_string(),
        })?;
        Ok(Types {
            defs,
            selections,
            has_selected,
            glob_to_selection,
            set,
        })
    }

    pub fn definitions(&self) -> Vec<FileTypeDef> {
        let mut defs: Vec<FileTypeDef> = self
            .types
            .values()
            .map(|def| {
                let mut def = def.clone();
                def.globs.sort();
                def
            })
            .collect();
        defs.sort_by(|a, b| a.name().cmp(b.name()));
        defs
    }

    pub fn select(&mut self, name: &str) -> &mut TypesBuilder {
        if name == "all" {
            for name in self.types.keys() {
                self.selections.push(Selection::Select(name.clone(), ()));
            }
        } else {
            self.selections
                .push(Selection::Select(name.to_string(), ()));
        }
        self
    }

    pub fn negate(&mut self, name: &str) -> &mut TypesBuilder {
        if name == "all" {
            for name in self.types.keys() {
                self.selections.push(Selection::Negate(name.clone(), ()));
            }
        } else {
            self.selections
                .push(Selection::Negate(name.to_string(), ()));
        }
        self
    }

    pub fn clear(&mut self, name: &str) -> &mut TypesBuilder {
        self.types.remove(name);
        self
    }

    pub fn add(&mut self, name: &str, glob: &str) -> Result<(), Error> {
        if name == "all" || !name.chars().all(char::is_alphanumeric) {
            return Err(Error::InvalidDefinition);
        }
        self.types
            .entry(name.to_string())
            .or_insert_with(|| FileTypeDef {
                name: name.to_string(),
                globs: vec![],
            })
            .globs
            .push(glob.to_string());
        Ok(())
    }

    pub fn add_def(&mut self, def: &str) -> Result<(), Error> {
        let parts: Vec<&str> = def.split(':').collect();
        match parts.as_slice() {
            [name, glob] => {
                if name.is_empty() || glob.is_empty() {
                    return Err(Error::InvalidDefinition);
                }
                self.add(name, glob)
            }
            [name, include, types_string] => {
                if name.is_empty() || *include != "include" || types_string.is_empty() {
                    return Err(Error::InvalidDefinition);
                }
                let types = types_string.split(',');
                if types.clone().any(|t| !self.types.contains_key(t)) {
                    return Err(Error::InvalidDefinition);
                }
                for type_name in types {
                    let globs = self.types[type_name].globs.clone();
                    for glob in globs {
                        self.add(name, &glob)?;
                    }
                }
                Ok(())
            }
            _ => Err(Error::InvalidDefinition),
        }
    }

    pub fn add_defaults(&mut self) -> &mut TypesBuilder {
        for &(names, exts) in DEFAULT_TYPES {
            for name in names {
                for ext in exts {
                    self.add(name, ext)
                        .expect("adding a default type should never fail");
                }
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::super::default_types::DEFAULT_TYPES;
    use super::TypesBuilder;

    macro_rules! matched {
        ($name:ident, $types:expr, $sel:expr, $selnot:expr, $path:expr) => {
            matched!($name, $types, $sel, $selnot, $path, true);
        };
        (not, $name:ident, $types:expr, $sel:expr, $selnot:expr, $path:expr) => {
            matched!($name, $types, $sel, $selnot, $path, false);
        };
        ($name:ident, $types:expr, $sel:expr, $selnot:expr, $path:expr, $matched:expr) => {
            #[test]
            fn $name() {
                let mut btypes = TypesBuilder::new();
                for tydef in $types {
                    btypes.add_def(tydef).unwrap();
                }
                for sel in $sel {
                    btypes.select(sel);
                }
                for selnot in $selnot {
                    btypes.negate(selnot);
                }
                let types = btypes.build().unwrap();
                let mat = types.matched($path, false);
                assert_eq!($matched, !mat.is_ignore());
            }
        };
    }

    fn types() -> Vec<&'static str> {
        vec![
            "html:*.html",
            "html:*.htm",
            "rust:*.rs",
            "js:*.js",
            "py:*.py",
            "python:*.py",
            "foo:*.{rs,foo}",
            "combo:include:html,rust",
        ]
    }

    const NONE: [&str; 0] = [];

    matched!(match1, types(), ["rust"], NONE, "lib.rs");
    matched!(match2, types(), ["html"], NONE, "index.html");
    matched!(match3, types(), ["html"], NONE, "index.htm");
    matched!(match4, types(), ["html", "rust"], NONE, "main.rs");
    matched!(match5, types(), NONE, NONE, "index.html");
    matched!(match6, types(), NONE, ["rust"], "index.html");
    matched!(match7, types(), ["foo"], ["rust"], "main.foo");
    matched!(match8, types(), ["combo"], NONE, "index.html");
    matched!(match9, types(), ["combo"], NONE, "lib.rs");
    matched!(match10, types(), ["py"], NONE, "main.py");
    matched!(match11, types(), ["python"], NONE, "main.py");

    matched!(not, matchnot1, types(), ["rust"], NONE, "index.html");
    matched!(not, matchnot2, types(), NONE, ["rust"], "main.rs");
    matched!(not, matchnot3, types(), ["foo"], ["rust"], "main.rs");
    matched!(not, matchnot4, types(), ["rust"], ["foo"], "main.rs");
    matched!(not, matchnot5, types(), ["rust"], ["foo"], "main.foo");
    matched!(not, matchnot6, types(), ["combo"], NONE, "leftpad.js");
    matched!(not, matchnot7, types(), ["py"], NONE, "index.html");
    matched!(not, matchnot8, types(), ["python"], NONE, "doc.md");

    #[test]
    fn test_invalid_defs() {
        let mut btypes = TypesBuilder::new();
        for tydef in types() {
            btypes.add_def(tydef).unwrap();
        }
        let original_defs = btypes.definitions();
        for def in ["combo:include:html,qwerty", "combo:foobar:html,rust", ""] {
            assert!(btypes.add_def(def).is_err());
            assert_eq!(btypes.definitions(), original_defs);
        }
    }

    #[test]
    fn default_types_are_sorted() {
        let names: Vec<&str> = DEFAULT_TYPES
            .iter()
            .map(|(aliases, _)| aliases[0])
            .collect();
        assert!(names.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn error_messages() {
        let mut btypes = TypesBuilder::new();
        btypes.add_defaults();
        btypes.select("nope");
        assert_eq!(
            btypes.build().err().unwrap().to_string(),
            "unrecognized file type: nope"
        );
        assert_eq!(
            btypes.add_def("bad-name:*.x").err().unwrap().to_string(),
            "invalid definition (format is type:glob, e.g., html:*.html)"
        );
    }

    #[test]
    fn all_selects_every_type() {
        let mut btypes = TypesBuilder::new();
        btypes.add_defaults();
        btypes.select("all");
        btypes.negate("rust");
        let types = btypes.build().unwrap();
        assert!(types.matched("a.c", false).is_whitelist());
        assert!(types.matched("a.rs", false).is_ignore());
        assert!(types.matched("unknown.zzzz", false).is_ignore());
        assert!(types.matched("dir", true).is_none());
    }
}
