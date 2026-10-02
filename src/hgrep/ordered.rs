use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use super::flags::{ColorChoice, DirAction, SearchMode, os_bytes};
use super::haystack::Haystack;
use super::hiargs::{HiArgs, Recursion};
use super::messages::{eprint_locked, err_message};
use super::out;
use super::queue::WorkQueue;
use super::worker::SearchResult;
use crate::hc_internal::grep::printer::Stats;

const BATCH: usize = 256;

enum Item {
    Search(Haystack),
    Message(String),
    Warning(String),
    Boundary,
}

struct Done {
    path: PathBuf,
    output: Vec<u8>,
    result: io::Result<SearchResult>,
}

enum Ready {
    Message(String),
    Warning(String),
    Done(Done),
}

fn operand_items(args: &HiArgs, operand: &Path, emit: &mut dyn FnMut(Item) -> bool) -> bool {
    let builder = args.haystack_builder();
    if operand.as_os_str() != "-" {
        let is_dir = operand.is_dir();
        if is_dir {
            match args.gnu.dir_action {
                Some(DirAction::Skip) => return true,
                Some(DirAction::Read) => {
                    return emit(Item::Message(format!(
                        "{}: Is a directory",
                        operand.display()
                    )));
                }
                _ => {}
            }
            let implicit_dot = args.has_implicit_path();
            if matches!(args.recursion, Recursion::Gnu { .. })
                && !implicit_dot
                && args
                    .gnu
                    .excludes
                    .skip_dir(os_bytes(operand.as_os_str()), true)
            {
                return true;
            }
            if let Some(deref) = args.gnu_fast_walk() {
                let excludes = &args.gnu.excludes;
                let walker = super::gnuwalk::Walker {
                    threads: args.threads,
                    deref,
                    excludes: (excludes.has_file_patterns() || excludes.has_dir_patterns())
                        .then_some(excludes),
                    strip_dot: implicit_dot,
                };
                let mut keep = true;
                walker.walk(operand, &mut |step| {
                    let item = match step {
                        super::gnuwalk::Step::Entry(dent) => {
                            match builder.build_checked(Ok(dent)) {
                                Ok(Some(hay)) => Item::Search(hay),
                                Ok(None) => return true,
                                Err(message) => Item::Message(message),
                            }
                        }
                        super::gnuwalk::Step::Error(message) => Item::Message(message),
                        super::gnuwalk::Step::Warning(message) => Item::Warning(message),
                        super::gnuwalk::Step::Boundary => Item::Boundary,
                    };
                    keep = emit(item);
                    keep
                });
                return keep;
            }
        } else if operand.exists()
            && args
                .gnu
                .excludes
                .skip_file(os_bytes(operand.as_os_str()), true)
        {
            return true;
        }
    }
    let walk = args.walk_builder_for(&[operand.to_path_buf()]).build();
    let haystacks = walk.filter_map(|result| match builder.build_checked(result) {
        Ok(hay) => hay.map(Item::Search),
        Err(message) => Some(Item::Message(message)),
    });
    for item in haystacks {
        if !emit(item) {
            return false;
        }
    }
    true
}

fn produce(args: &HiArgs, emit: &mut dyn FnMut(Item) -> bool) {
    for operand in &args.paths.operands {
        if !operand_items(args, operand, emit) {
            return;
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
struct Emitter<W: Write> {
    out: W,
    separator: Option<Vec<u8>>,
    printed: bool,
    stats: Option<Stats>,
    used: bool,
    matched: bool,
    quit_after_match: bool,
}

impl<W: Write> Emitter<W> {
    fn message(&mut self, text: &str) -> io::Result<()> {
        self.out.flush()?;
        err_message(text);
        Ok(())
    }

    fn warning(&mut self, text: &str) -> io::Result<()> {
        self.out.flush()?;
        super::messages::message(text);
        Ok(())
    }

    fn done(&mut self, done: Done) -> io::Result<bool> {
        let result = match done.result {
            Ok(result) => result,
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(false),
            Err(err) => {
                self.out.write_all(&done.output)?;
                self.message(&super::format_error(&format!(
                    "{}: {err}",
                    done.path.display()
                )))?;
                return Ok(true);
            }
        };
        if !done.output.is_empty() {
            if let Some(sep) = &self.separator
                && self.printed
            {
                self.out.write_all(sep)?;
                self.out.write_all(b"\n")?;
            }
            if let Some(lead) = &result.lead_separator
                && self.used
            {
                self.out.write_all(lead)?;
            }
            self.out.write_all(&done.output)?;
            self.printed = true;
        }
        if let (Some(total), Some(one)) = (self.stats.as_mut(), result.stats.as_ref()) {
            *total += one;
        }
        if let Some(fatal) = &result.fatal {
            self.out.flush()?;
            eprint_locked(fatal);
            std::process::exit(2);
        }
        self.used |= result.gnu_used;
        if let Some(error) = &result.error {
            self.message(error)?;
        }
        if let Some(message) = &result.after_message {
            self.out.flush()?;
            eprint_locked(message);
        }
        self.matched |= result.has_match;
        Ok(!(self.matched && self.quit_after_match))
    }
}

struct Ring {
    slots: Vec<Mutex<Option<Ready>>>,
    next: AtomicUsize,
    total: AtomicUsize,
    stop: AtomicBool,
    gate: Mutex<()>,
    filled: Condvar,
    freed: Condvar,
}

impl Ring {
    fn new(capacity: usize) -> Ring {
        Ring {
            slots: (0..capacity).map(|_| Mutex::new(None)).collect(),
            next: AtomicUsize::new(0),
            total: AtomicUsize::new(usize::MAX),
            stop: AtomicBool::new(false),
            gate: Mutex::new(()),
            filled: Condvar::new(),
            freed: Condvar::new(),
        }
    }

    fn gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn slot(&self, seq: usize) -> std::sync::MutexGuard<'_, Option<Ready>> {
        self.slots[seq % self.slots.len()]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn reserve(&self, seq: usize) {
        if seq < self.next.load(Ordering::Acquire) + self.slots.len() {
            return;
        }
        let mut gate = self.gate();
        while seq >= self.next.load(Ordering::Acquire) + self.slots.len()
            && !self.stop.load(Ordering::Relaxed)
        {
            gate = self
                .freed
                .wait_timeout(gate, std::time::Duration::from_millis(5))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    fn put(&self, seq: usize, ready: Ready) {
        *self.slot(seq) = Some(ready);
        if seq == self.next.load(Ordering::Acquire) {
            let _gate = self.gate();
            self.filled.notify_one();
        }
    }

    fn finish(&self, total: usize) {
        self.total.store(total, Ordering::Release);
        let _gate = self.gate();
        self.filled.notify_one();
    }

    fn take(&self) -> Option<Ready> {
        let next = self.next.load(Ordering::Acquire);
        loop {
            if let Some(ready) = self.slot(next).take() {
                return Some(ready);
            }
            if next >= self.total.load(Ordering::Acquire) {
                return None;
            }
            let gate = self.gate();
            if self.slot(next).is_some() || next >= self.total.load(Ordering::Acquire) {
                continue;
            }
            drop(
                self.filled
                    .wait_timeout(gate, std::time::Duration::from_millis(50))
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
        }
    }

    fn advance(&self) {
        self.next.fetch_add(1, Ordering::AcqRel);
        let _gate = self.gate();
        self.freed.notify_all();
    }
}

pub(crate) fn search(args: &HiArgs, mode: SearchMode) -> io::Result<bool> {
    let compiled = args.matcher().map_err(|e| io::Error::other(e.0))?;
    let sequential = args.threads <= 1 || args.paths.operands.iter().any(|p| p.as_os_str() == "-");
    if sequential {
        return search_sequential(args, mode, compiled);
    }
    let threads = args.threads;
    let started = std::time::Instant::now();
    let gnu = args.gnu.printer;
    let color = !gnu && args.color != ColorChoice::Never;
    let mut worker = args
        .search_worker(
            compiled,
            args.searcher(),
            args.printer(mode, out::Colored::new(Vec::new(), color)),
        )
        .map_err(|e| io::Error::other(e.0))?;
    worker.printer().set_defer_lead(true);
    let ring = Ring::new((threads * 512).next_power_of_two());
    let jobs: WorkQueue<Vec<(usize, Haystack)>> = WorkQueue::new();
    let mut emitter = Emitter {
        out: out::stdout(false, args.stdout_line_buffered()),
        separator: if gnu {
            None
        } else {
            args.file_separator.clone()
        },
        printed: false,
        stats: args.stats(),
        used: false,
        matched: false,
        quit_after_match: args.quit_after_match,
    };
    let outcome: io::Result<()> = std::thread::scope(|scope| {
        for _ in 0..threads {
            let mut worker = worker.clone();
            let jobs = &jobs;
            let ring = &ring;
            scope.spawn(move || {
                while let Some(batch) = jobs.pop() {
                    for (seq, haystack) in batch {
                        if ring.stop.load(Ordering::Relaxed) {
                            break;
                        }
                        worker.printer().get_mut().clear();
                        let result = worker.search(&haystack);
                        let output = std::mem::take(worker.printer().get_mut().get_mut());
                        let done = Done {
                            path: haystack.path().to_path_buf(),
                            output,
                            result,
                        };
                        ring.put(seq, Ready::Done(done));
                    }
                }
            });
        }
        {
            let jobs = &jobs;
            let ring = &ring;
            scope.spawn(move || {
                let mut next_seq = 0usize;
                let mut batch: Vec<(usize, Haystack)> = Vec::new();
                produce(args, &mut |item| {
                    if ring.stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    if matches!(item, Item::Boundary) {
                        if !batch.is_empty() {
                            jobs.push(std::mem::take(&mut batch));
                        }
                        return true;
                    }
                    let seq = next_seq;
                    next_seq += 1;
                    if seq >= ring.next.load(Ordering::Acquire) + ring.slots.len()
                        && !batch.is_empty()
                    {
                        jobs.push(std::mem::take(&mut batch));
                    }
                    ring.reserve(seq);
                    match item {
                        Item::Search(haystack) => {
                            batch.push((seq, haystack));
                            if batch.len() >= BATCH {
                                jobs.push(std::mem::take(&mut batch));
                            }
                        }
                        Item::Message(message) => ring.put(seq, Ready::Message(message)),
                        Item::Warning(message) => ring.put(seq, Ready::Warning(message)),
                        Item::Boundary => {}
                    }
                    true
                });
                if !batch.is_empty() {
                    jobs.push(batch);
                }
                jobs.close();
                ring.finish(next_seq);
            });
        }
        let mut result = Ok(());
        while let Some(ready) = ring.take() {
            let keep_going = match ready {
                Ready::Message(message) => emitter.message(&message).map(|()| true),
                Ready::Warning(message) => emitter.warning(&message).map(|()| true),
                Ready::Done(done) => emitter.done(done),
            };
            ring.advance();
            match keep_going {
                Ok(true) => {}
                Ok(false) => {
                    ring.stop.store(true, Ordering::Relaxed);
                    break;
                }
                Err(err) => {
                    ring.stop.store(true, Ordering::Relaxed);
                    result = Err(err);
                    break;
                }
            }
        }
        if ring.stop.load(Ordering::Relaxed) {
            let _gate = ring.gate();
            ring.freed.notify_all();
        }
        result
    });
    match outcome {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(emitter.matched),
        Err(err) => return Err(err),
        Ok(()) => {}
    }
    if let Some(stats) = &emitter.stats {
        let _ = super::app::print_stats(mode, stats, started, &mut emitter.out);
    }
    match emitter.out.flush() {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err),
        _ => Ok(emitter.matched),
    }
}

fn search_sequential(
    args: &HiArgs,
    mode: SearchMode,
    compiled: super::matchers::PatternMatcher,
) -> io::Result<bool> {
    let mut worker = args
        .search_worker(
            compiled,
            args.searcher(),
            args.printer(mode, out::stdout(false, args.stdout_line_buffered())),
        )
        .map_err(|e| io::Error::other(e.0))?;
    worker.printer().set_defer_lead(false);
    let mut matched = false;
    let mut failure = None;
    produce(args, &mut |item| {
        let haystack = match item {
            Item::Message(message) => {
                let _ = worker.printer().get_mut().flush();
                err_message(&message);
                return true;
            }
            Item::Warning(message) => {
                let _ = worker.printer().get_mut().flush();
                super::messages::message(&message);
                return true;
            }
            Item::Boundary => return true,
            Item::Search(haystack) => haystack,
        };
        let result = match worker.search(&haystack) {
            Ok(result) => result,
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return false,
            Err(err) => {
                let _ = worker.printer().get_mut().flush();
                err_message(&super::format_error(&format!(
                    "{}: {err}",
                    haystack.path().display()
                )));
                return true;
            }
        };
        if let Some(fatal) = &result.fatal {
            let _ = worker.printer().get_mut().flush();
            eprint_locked(fatal);
            std::process::exit(2);
        }
        if let Some(error) = &result.error {
            let _ = worker.printer().get_mut().flush();
            err_message(error);
        }
        if let Some(message) = &result.after_message {
            if let Err(err) = worker.printer().get_mut().flush() {
                failure = Some(err);
                return false;
            }
            eprint_locked(message);
        }
        matched |= result.has_match;
        !(matched && args.quit_after_match)
    });
    if let Some(err) = failure
        && err.kind() != io::ErrorKind::BrokenPipe
    {
        return Err(err);
    }
    match worker.printer().get_mut().flush() {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err),
        _ => Ok(matched),
    }
}
