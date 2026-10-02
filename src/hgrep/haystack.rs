use std::path::Path;

use crate::hc_internal::walker::{DirEntry, Error as WalkError};

use super::flags::DevAction;
use super::hiargs::Recursion;
use super::messages;

#[derive(Clone, Debug)]
pub(crate) struct HaystackBuilder {
    strip_dot_prefix: bool,
    dev_action: Option<DevAction>,
    recursion: Recursion,
}

#[derive(Clone, Debug)]
pub(crate) struct Haystack {
    dent: DirEntry,
    strip_dot_prefix: bool,
}

pub(crate) fn report_walk_error(err: &WalkError) {
    messages::err_message(&super::format_error(&err.to_string()));
}

impl HaystackBuilder {
    pub(crate) fn new(
        strip_dot_prefix: bool,
        dev_action: Option<DevAction>,
        recursion: Recursion,
    ) -> HaystackBuilder {
        HaystackBuilder {
            strip_dot_prefix,
            dev_action,
            recursion,
        }
    }

    pub(crate) fn build_checked(
        &self,
        result: Result<DirEntry, WalkError>,
    ) -> Result<Option<Haystack>, String> {
        match result {
            Ok(dent) => Ok(self.build(dent)),
            Err(err) => Err(super::format_parallel_error(&err.to_string())),
        }
    }

    pub(crate) fn build_from_result(
        &self,
        result: Result<DirEntry, WalkError>,
    ) -> Option<Haystack> {
        match result {
            Ok(dent) => self.build(dent),
            Err(err) => {
                report_walk_error(&err);
                None
            }
        }
    }

    fn build(&self, dent: DirEntry) -> Option<Haystack> {
        let hay = Haystack {
            dent,
            strip_dot_prefix: self.strip_dot_prefix,
        };
        if let Some(err) = hay.dent.error() {
            messages::ignore_message(&super::format_error(&err.to_string()));
        }
        if hay.is_explicit() {
            if self.dev_action == Some(DevAction::Skip) && hay.is_special() {
                return None;
            }
            return Some(hay);
        }
        if hay.is_file() {
            return Some(hay);
        }
        if self.dev_action == Some(DevAction::Read)
            && matches!(self.recursion, Recursion::Gnu { .. })
            && hay.is_special()
        {
            return Some(hay);
        }
        None
    }
}

impl Haystack {
    pub(crate) fn path(&self) -> &Path {
        let path = self.dent.path();
        if self.strip_dot_prefix && path.starts_with("./") {
            path.strip_prefix("./").unwrap_or(path)
        } else {
            path
        }
    }

    pub(crate) fn depth(&self) -> usize {
        self.dent.depth()
    }

    pub(crate) fn is_regular(&self) -> bool {
        self.dent
            .file_type()
            .is_some_and(crate::hc_internal::walker::FileType::is_file)
    }

    pub(crate) fn is_stdin(&self) -> bool {
        self.dent.is_stdin()
    }

    pub(crate) fn is_explicit(&self) -> bool {
        self.is_stdin() || (self.dent.depth() == 0 && !self.is_dir())
    }

    fn is_dir(&self) -> bool {
        let Some(ft) = self.dent.file_type() else {
            return false;
        };
        ft.is_dir() || (self.dent.path_is_symlink() && self.dent.path().is_dir())
    }

    fn is_file(&self) -> bool {
        self.dent
            .file_type()
            .is_some_and(crate::hc_internal::walker::FileType::is_file)
    }

    fn is_special(&self) -> bool {
        self.dent
            .file_type()
            .is_some_and(|ft| !ft.is_file() && !ft.is_dir() && !ft.is_symlink())
            || (self.dent.path_is_symlink()
                && std::fs::metadata(self.dent.path())
                    .is_ok_and(|md| !md.is_file() && !md.is_dir()))
    }
}
