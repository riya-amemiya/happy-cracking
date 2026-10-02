use std::cell::OnceCell;
use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use regex::bytes::Regex;

use super::{Config, Exit, Set, fsx, merge_exits, output, print_error};
use crate::hc_internal::walker::overrides::OverrideBuilder;
use crate::hc_internal::walker::{
    DirEntry, Error, FileType, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState,
};

const MAX_BUFFER_LENGTH: usize = 1000;
const DEFAULT_MAX_BUFFER_TIME: Duration = Duration::from_millis(100);
const FLUSH_AT: usize = 64 * 1024;
const BATCH_CHUNK: usize = 256;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

#[cfg(unix)]
mod sigint {
    use std::ffi::c_int;
    use std::sync::atomic::Ordering;

    const SIGINT: c_int = 2;
    const SIG_DFL: usize = 0;
    const SIG_ERR: usize = usize::MAX;

    unsafe extern "C" {
        fn signal(signum: c_int, handler: usize) -> usize;
        fn raise(sig: c_int) -> c_int;
    }

    extern "C" fn on_interrupt(_: c_int) {
        if super::INTERRUPTED.swap(true, Ordering::Relaxed) {
            reraise();
        }
    }

    pub(super) fn install() {
        unsafe {
            signal(SIGINT, on_interrupt as extern "C" fn(c_int) as usize);
        }
    }

    pub(super) fn reraise() {
        unsafe {
            if signal(SIGINT, SIG_DFL) != SIG_ERR {
                raise(SIGINT);
            }
        }
    }
}

#[cfg(not(unix))]
mod sigint {
    pub(super) fn install() {}

    pub(super) fn reraise() {}
}

pub(super) fn reraise_interrupt() {
    sigint::reraise();
}

enum Inner {
    Normal(DirEntry),
    Broken(PathBuf),
}

pub(super) struct Entry {
    inner: Inner,
    metadata: OnceCell<Option<Metadata>>,
}

impl Entry {
    fn new(inner: Inner) -> Self {
        Entry {
            inner,
            metadata: OnceCell::new(),
        }
    }

    pub(super) fn path(&self) -> &Path {
        match &self.inner {
            Inner::Normal(e) => e.path(),
            Inner::Broken(p) => p,
        }
    }

    fn into_path(self) -> PathBuf {
        match self.inner {
            Inner::Normal(e) => e.into_path(),
            Inner::Broken(p) => p,
        }
    }

    pub(super) fn stripped_path(&self, config: &Config) -> &Path {
        if config.is(Set::StripCwdPrefix) {
            fsx::strip_current_dir(self.path())
        } else {
            self.path()
        }
    }

    fn into_stripped_path(self, config: &Config) -> PathBuf {
        if config.is(Set::StripCwdPrefix) {
            self.stripped_path(config).to_path_buf()
        } else {
            self.into_path()
        }
    }

    pub(super) fn file_type(&self) -> Option<FileType> {
        match &self.inner {
            Inner::Normal(e) => e.file_type(),
            Inner::Broken(_) => self.metadata().map(|m| FileType::from(m.file_type())),
        }
    }

    pub(super) fn metadata(&self) -> Option<&Metadata> {
        self.metadata
            .get_or_init(|| match &self.inner {
                Inner::Normal(e) => e.metadata().ok(),
                Inner::Broken(p) => p.symlink_metadata().ok(),
            })
            .as_ref()
    }

    fn depth(&self) -> Option<usize> {
        match &self.inner {
            Inner::Normal(e) => Some(e.depth()),
            Inner::Broken(_) => None,
        }
    }

    pub(super) fn color_name(&self) -> &OsStr {
        match &self.inner {
            Inner::Normal(e) => e.file_name(),
            Inner::Broken(p) => p
                .components()
                .next_back()
                .map_or(p.as_os_str(), |c| c.as_os_str()),
        }
    }

    fn is_file_following(&self) -> bool {
        match self.file_type() {
            Some(ft) if !ft.is_symlink() => ft.is_file(),
            _ => self.path().is_file(),
        }
    }
}

#[cfg(all(unix, not(target_os = "android")))]
const EXEC_CHECK: rustix::fs::AtFlags = rustix::fs::AtFlags::EACCESS;

#[cfg(target_os = "android")]
const EXEC_CHECK: rustix::fs::AtFlags = rustix::fs::AtFlags::empty();

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use rustix::fs::{Access, CWD, accessat};
    accessat(CWD, path, Access::EXEC_OK, EXEC_CHECK).is_ok()
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.exists()
}

fn is_empty(entry: &Entry) -> bool {
    match entry.file_type() {
        Some(ft) if ft.is_dir() => {
            fs::read_dir(entry.path()).is_ok_and(|mut entries| entries.next().is_none())
        }
        Some(ft) if ft.is_file() => entry.metadata().is_some_and(|m| m.len() == 0),
        _ => false,
    }
}

impl super::filter::FileTypes {
    fn should_ignore(self, entry: &Entry) -> bool {
        use super::cli::FileType as T;
        let Some(t) = entry.file_type() else {
            return true;
        };
        (!self.has(T::File) && t.is_file())
            || (!self.has(T::Directory) && t.is_dir())
            || (!self.has(T::Symlink) && t.is_symlink())
            || (!self.has(T::BlockDevice) && t.is_block_device())
            || (!self.has(T::CharDevice) && t.is_char_device())
            || (!self.has(T::Socket) && t.is_socket())
            || (!self.has(T::Pipe) && t.is_fifo())
            || (self.has(T::Executable) && !executable(entry.path()))
            || (self.has(T::Empty) && !is_empty(entry))
            || !(t.is_file()
                || t.is_dir()
                || t.is_symlink()
                || t.is_block_device()
                || t.is_char_device()
                || t.is_socket()
                || t.is_fifo())
    }
}

#[derive(Clone)]
pub(super) struct Extensions {
    set: regex::bytes::RegexSet,
    ascii: Option<Vec<Vec<u8>>>,
}

impl Extensions {
    pub(super) fn new(exts: &[String]) -> Result<Self, regex::Error> {
        let trimmed: Vec<&str> = exts.iter().map(|e| e.trim_start_matches('.')).collect();
        let set = regex::bytes::RegexSetBuilder::new(
            trimmed.iter().map(|e| format!(r".\.{}$", regex::escape(e))),
        )
        .case_insensitive(true)
        .build()?;
        let ascii = trimmed.iter().all(|e| e.is_ascii()).then(|| {
            trimmed
                .iter()
                .map(|e| e.to_ascii_lowercase().into_bytes())
                .collect()
        });
        Ok(Extensions { set, ascii })
    }

    fn is_match(&self, name: &[u8]) -> bool {
        match &self.ascii {
            Some(exts) if name.is_ascii() => exts.iter().any(|ext| {
                name.len() >= ext.len() + 2 && {
                    let dot = name.len() - ext.len() - 1;
                    name[dot] == b'.'
                        && name[dot - 1] != b'\n'
                        && name[dot + 1..].eq_ignore_ascii_case(ext)
                }
            }),
            _ => self.set.is_match(name),
        }
    }
}

struct Buffer {
    entries: Vec<(PathBuf, Vec<u8>)>,
    streaming: bool,
    done: bool,
}

struct Shared<'a> {
    config: &'a Config,
    patterns: &'a [Regex],
    cwd: Option<PathBuf>,
    quit: AtomicBool,
    streaming: AtomicBool,
    count: AtomicUsize,
    has_results: AtomicBool,
    failed: AtomicBool,
    buffer: Mutex<Buffer>,
    wake: Condvar,
    deadline: Instant,
    batch_tx: Mutex<Option<mpsc::Sender<Vec<PathBuf>>>>,
    exits: Mutex<Exit>,
}

fn write_out(shared: &Shared, bytes: &[u8]) {
    if bytes.is_empty() || shared.failed.load(Ordering::Relaxed) {
        return;
    }
    let result = io::stdout().lock().write_all(bytes);
    if let Err(e) = result {
        if e.kind() != io::ErrorKind::BrokenPipe {
            print_error(&format!("Could not write to output: {e}"));
        }
        shared.failed.store(true, Ordering::Relaxed);
        shared.quit.store(true, Ordering::Relaxed);
    }
}

fn start_streaming(shared: &Shared, buffer: &mut Buffer) {
    if interrupted() {
        buffer.entries.truncate(1);
    }
    buffer.streaming = true;
    shared.streaming.store(true, Ordering::Relaxed);
    let mut all = Vec::new();
    for (_, bytes) in buffer.entries.drain(..) {
        all.extend_from_slice(&bytes);
    }
    write_out(shared, &all);
}

struct Visitor<'s, 'a> {
    shared: &'s Shared<'a>,
    patterns: Vec<Regex>,
    extensions: Option<Extensions>,
    local: Vec<u8>,
    scratch: Vec<u8>,
    batch: Vec<PathBuf>,
    batch_tx: Option<mpsc::Sender<Vec<PathBuf>>>,
    exit: Exit,
}

impl Drop for Visitor<'_, '_> {
    fn drop(&mut self) {
        write_out(self.shared, &self.local);
        self.send_batch();
        let mut exits = self.shared.exits.lock().unwrap();
        *exits = merge_exits([*exits, self.exit]);
    }
}

impl Visitor<'_, '_> {
    fn send_batch(&mut self) {
        if let Some(tx) = &self.batch_tx
            && !self.batch.is_empty()
        {
            let _ = tx.send(std::mem::take(&mut self.batch));
        }
    }

    fn emit(&mut self, entry: Entry) -> bool {
        let shared = self.shared;
        let config = shared.config;
        if let Some(cmd) = &config.command {
            if cmd.in_batch_mode() {
                self.batch.push(entry.into_stripped_path(config));
                if self.batch.len() >= BATCH_CHUNK {
                    self.send_batch();
                }
            } else {
                let code = cmd.execute(
                    entry.stripped_path(config),
                    config.path_separator.as_deref(),
                    config.is(Set::NullSeparator),
                    config.threads > 1,
                );
                self.exit = merge_exits([self.exit, code]);
            }
            return true;
        }
        if config.is(Set::Quiet) {
            shared.has_results.store(true, Ordering::Relaxed);
            shared.quit.store(true, Ordering::Relaxed);
            return false;
        }
        let last = match config.max_results {
            Some(max) => {
                let n = shared.count.fetch_add(1, Ordering::Relaxed);
                if n >= max {
                    shared.quit.store(true, Ordering::Relaxed);
                    return false;
                }
                n + 1 >= max
            }
            None => false,
        };
        if shared.streaming.load(Ordering::Relaxed) {
            output::print_entry(&mut self.local, &entry, config);
        } else {
            self.scratch.clear();
            output::print_entry(&mut self.scratch, &entry, config);
            let mut buffer = shared.buffer.lock().unwrap();
            if buffer.streaming {
                drop(buffer);
                self.local.extend_from_slice(&self.scratch);
            } else {
                buffer.entries.push((
                    entry.path().to_path_buf(),
                    std::mem::take(&mut self.scratch),
                ));
                if buffer.entries.len() > MAX_BUFFER_LENGTH || Instant::now() >= shared.deadline {
                    start_streaming(shared, &mut buffer);
                }
            }
        }
        if self.local.len() >= FLUSH_AT {
            write_out(shared, &self.local);
            self.local.clear();
        }
        if last {
            shared.quit.store(true, Ordering::Relaxed);
            return false;
        }
        true
    }

    fn matches(&self, entry: &Entry) -> bool {
        let shared = self.shared;
        let config = shared.config;
        let path = entry.path();
        if !self.patterns.is_empty() {
            let ok = if let Some(cwd) = &shared.cwd {
                let abs = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    cwd.join(path.strip_prefix(".").unwrap_or(path))
                };
                let bytes = fsx::osstr_bytes(abs.as_os_str());
                self.patterns.iter().all(|p| p.is_match(&bytes))
            } else {
                let name = fsx::osstr_bytes(path.file_name().unwrap_or(path.as_os_str()));
                self.patterns.iter().all(|p| p.is_match(&name))
            };
            if !ok {
                return false;
            }
        }
        if let Some(exts) = &self.extensions {
            match path.file_name() {
                Some(name) if exts.is_match(&fsx::osstr_bytes(name)) => {}
                _ => return false,
            }
        }
        if config.file_types.is_some_and(|ft| ft.should_ignore(entry)) {
            return false;
        }
        #[cfg(unix)]
        if let Some(owner) = config.owner_constraint {
            match entry.metadata() {
                Some(md) if owner.matches(md) => {}
                _ => return false,
            }
        }
        if !config.size_constraints.is_empty() {
            if !entry.is_file_following() {
                return false;
            }
            match entry.metadata() {
                Some(md) => {
                    let len = md.len();
                    if config.size_constraints.iter().any(|sc| !sc.is_within(len)) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        if !config.time_constraints.is_empty() {
            let Some(modified) = entry.metadata().and_then(|m| m.modified().ok()) else {
                return false;
            };
            if !config
                .time_constraints
                .iter()
                .all(|tf| tf.applies_to(modified))
            {
                return false;
            }
        }
        true
    }

    fn report(&mut self, err: &Error) -> WalkState {
        if self.shared.config.is(Set::ShowErrors) {
            print_error(&err.to_string());
        }
        WalkState::Continue
    }
}

impl ParallelVisitor for Visitor<'_, '_> {
    fn visit(&mut self, result: Result<DirEntry, Error>) -> WalkState {
        let shared = self.shared;
        let config = shared.config;
        if shared.quit.load(Ordering::Relaxed) || interrupted() {
            return WalkState::Quit;
        }
        let entry = match result {
            Ok(e) => {
                if !config.ignore_contain.is_empty()
                    && matches!(e.file_type(), Some(t) if t.is_dir())
                    && config
                        .ignore_contain
                        .iter()
                        .any(|ic| e.path().join(ic).exists())
                {
                    return WalkState::Skip;
                }
                if e.depth() == 0 {
                    return WalkState::Continue;
                }
                Entry::new(Inner::Normal(e))
            }
            Err(Error::WithPath { path, err }) => match *err {
                Error::Io(ref io_error)
                    if io_error.kind() == io::ErrorKind::NotFound
                        && path
                            .symlink_metadata()
                            .is_ok_and(|m| m.file_type().is_symlink()) =>
                {
                    Entry::new(Inner::Broken(path))
                }
                inner => {
                    return self.report(&Error::WithPath {
                        path,
                        err: Box::new(inner),
                    });
                }
            },
            Err(err) => return self.report(&err),
        };
        if let Some(min_depth) = config.min_depth
            && entry.depth().is_none_or(|d| d < min_depth)
        {
            return WalkState::Continue;
        }
        if !self.matches(&entry) {
            return WalkState::Continue;
        }
        if !self.emit(entry) {
            return WalkState::Quit;
        }
        if config.is(Set::Prune) {
            return WalkState::Skip;
        }
        WalkState::Continue
    }
}

struct Builder<'s, 'a> {
    shared: &'s Shared<'a>,
}

impl<'s> ParallelVisitorBuilder<'s> for Builder<'s, '_> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 's> {
        Box::new(Visitor {
            shared: self.shared,
            patterns: self.shared.patterns.to_vec(),
            extensions: self.shared.config.extensions.clone(),
            local: Vec::new(),
            scratch: Vec::new(),
            batch: Vec::new(),
            batch_tx: self.shared.batch_tx.lock().unwrap().clone(),
            exit: Exit::Success,
        })
    }
}

fn build_walker(paths: &[PathBuf], config: &Config) -> Result<WalkBuilder> {
    let first_path = &paths[0];
    let mut overrides = OverrideBuilder::new(first_path);
    for pattern in &config.exclude_patterns {
        overrides
            .add(pattern)
            .map_err(|e| anyhow!("Malformed exclude pattern: {e}"))?;
    }
    let overrides = overrides
        .build()
        .map_err(|_| anyhow!("Mismatch in exclude patterns"))?;
    let mut builder = WalkBuilder::new(first_path);
    builder
        .hidden(config.is(Set::IgnoreHidden))
        .ignore(config.is(Set::ReadFdignore))
        .parents(
            config.is(Set::ReadParentIgnore)
                && (config.is(Set::ReadFdignore) || config.is(Set::ReadVcsignore)),
        )
        .git_ignore(config.is(Set::ReadVcsignore))
        .git_global(config.is(Set::ReadVcsignore))
        .git_exclude(config.is(Set::ReadVcsignore))
        .require_git(config.is(Set::RequireGit))
        .overrides(overrides)
        .follow_links(config.is(Set::FollowLinks))
        .same_file_system(config.is(Set::OneFileSystem))
        .max_depth(config.max_depth);
    if config.is(Set::ReadFdignore) {
        builder.add_custom_ignore_filename(".fdignore");
    }
    if config.is(Set::ReadGlobalIgnore)
        && let Some(dir) = fsx::config_dir()
    {
        let global = dir.join("fd").join("ignore");
        if global.is_file() {
            match builder.add_ignore(global) {
                Some(Error::Partial(_)) | None => {}
                Some(err) => {
                    print_error(&format!("Malformed pattern in global ignore file. {err}."));
                }
            }
        }
    }
    for ignore_file in &config.ignore_files {
        match builder.add_ignore(ignore_file) {
            Some(Error::Partial(_)) | None => {}
            Some(err) => print_error(&format!("Malformed pattern in custom ignore file. {err}.")),
        }
    }
    for path in &paths[1..] {
        builder.add(path);
    }
    builder.threads(if config.max_results.is_some() {
        1
    } else {
        config.threads
    });
    Ok(builder)
}

pub(super) fn scan(paths: &[PathBuf], patterns: &[Regex], config: &Config) -> Result<Exit> {
    let builder = build_walker(paths, config)?;
    let cwd = if config.is(Set::FullPath) {
        Some(std::env::current_dir()?)
    } else {
        None
    };
    let shared = Shared {
        config,
        patterns,
        cwd,
        quit: AtomicBool::new(false),
        streaming: AtomicBool::new(false),
        count: AtomicUsize::new(0),
        has_results: AtomicBool::new(false),
        failed: AtomicBool::new(false),
        buffer: Mutex::new(Buffer {
            entries: Vec::new(),
            streaming: false,
            done: false,
        }),
        wake: Condvar::new(),
        deadline: Instant::now() + config.max_buffer_time.unwrap_or(DEFAULT_MAX_BUFFER_TIME),
        batch_tx: Mutex::new(None),
        exits: Mutex::new(Exit::Success),
    };
    let printing = config.command.is_none() && !config.is(Set::Quiet);
    if config.ls_colors.is_some() && config.command.is_none() {
        sigint::install();
    }
    let batch_cmd = config.command.as_ref().filter(|c| c.in_batch_mode());
    let batch_exit = std::thread::scope(|scope| {
        let executor = batch_cmd.map(|cmd| {
            let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
            *shared.batch_tx.lock().unwrap() = Some(tx);
            scope.spawn(move || {
                cmd.execute_batch(
                    rx.into_iter().flatten(),
                    config.batch_size,
                    config.path_separator.as_deref(),
                )
            })
        });
        if printing {
            scope.spawn(|| {
                let mut buffer = shared.buffer.lock().unwrap();
                loop {
                    if buffer.done || buffer.streaming {
                        return;
                    }
                    let now = Instant::now();
                    if now >= shared.deadline {
                        start_streaming(&shared, &mut buffer);
                        return;
                    }
                    buffer = shared
                        .wake
                        .wait_timeout(buffer, shared.deadline - now)
                        .unwrap()
                        .0;
                }
            });
        }
        builder
            .build_parallel()
            .visit(&mut Builder { shared: &shared });
        let mut buffer = shared.buffer.lock().unwrap();
        buffer.done = true;
        shared.wake.notify_all();
        if !buffer.streaming {
            buffer.entries.sort_by(|a, b| a.0.cmp(&b.0));
            start_streaming(&shared, &mut buffer);
        }
        drop(buffer);
        shared.batch_tx.lock().unwrap().take();
        executor.map(|handle| handle.join().unwrap_or(Exit::GeneralError))
    });
    if interrupted() {
        let _ = io::stdout().lock().flush();
        return Ok(Exit::KilledBySigint);
    }
    if printing && io::stdout().lock().flush().is_err() {
        return Ok(Exit::GeneralError);
    }
    if let Some(exit) = batch_exit {
        return Ok(exit);
    }
    if config.command.is_some() {
        return Ok(*shared.exits.lock().unwrap());
    }
    if config.is(Set::Quiet) {
        return Ok(Exit::HasResults(shared.has_results.load(Ordering::Relaxed)));
    }
    if shared.failed.load(Ordering::Relaxed) {
        return Ok(Exit::GeneralError);
    }
    Ok(Exit::Success)
}
