use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use memchr::memchr;

use super::super::matcher::{LineMatchKind, LineTerminator, ParallelMatcher};
use super::Range;
use super::bitcount;
use super::core::LineFeed;
use super::lines::{self, LineStep};

const PANICKED: &str = "search worker panicked";

pub(crate) fn thread_count() -> usize {
    rayon::current_num_threads()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn run_workers<F: Fn() + Sync>(workers: usize, work: F) {
    rayon::in_place_scope(|scope| {
        for _ in 1..workers {
            scope.spawn(|_| work());
        }
        work();
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Region {
    Clean,
    LongLine,
    Binary,
}

impl Region {
    pub(crate) fn worst(self, other: Region) -> Region {
        match (self, other) {
            (Region::Binary, _) | (_, Region::Binary) => Region::Binary,
            (Region::LongLine, _) | (_, Region::LongLine) => Region::LongLine,
            _ => Region::Clean,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Guard {
    pub(crate) byte: u8,
    pub(crate) window: usize,
}

pub(crate) fn check_region(
    data: &[u8],
    start: usize,
    end: usize,
    term: u8,
    guard: Guard,
) -> Region {
    if memchr(guard.byte, &data[start..end]).is_some() {
        return Region::Binary;
    }
    let mut window = start.div_ceil(guard.window) * guard.window;
    while window < end && window + guard.window <= data.len() {
        if memchr(term, &data[window..window + guard.window]).is_none() {
            return Region::LongLine;
        }
        window += guard.window;
    }
    Region::Clean
}

struct ChunkOut {
    lines: Vec<Range>,
    rel: Vec<u64>,
    terms: u64,
}

struct State {
    slots: Vec<Option<Result<ChunkOut, String>>>,
    regions: Vec<Option<Region>>,
    next: usize,
    current: usize,
    main_waits: Option<usize>,
    idle: usize,
}

pub(crate) struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    space: Condvar,
    stop: AtomicBool,
    window: usize,
}

impl Shared {
    pub(crate) fn new(chunks: usize, window: usize) -> Shared {
        Shared {
            state: Mutex::new(State {
                slots: (0..chunks).map(|_| None).collect(),
                regions: vec![None; chunks],
                next: 0,
                current: 0,
                main_waits: None,
                idle: 0,
            }),
            ready: Condvar::new(),
            space: Condvar::new(),
            stop: AtomicBool::new(false),
            window: window.max(1),
        }
    }

    pub(crate) fn finish(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let _state = lock(&self.state);
        self.space.notify_all();
    }

    fn claim(&self) -> Option<usize> {
        let mut state = lock(&self.state);
        loop {
            if self.stop.load(Ordering::Relaxed) || state.next >= state.slots.len() {
                return None;
            }
            if state.next < state.current + self.window {
                state.next += 1;
                return Some(state.next - 1);
            }
            state.idle += 1;
            state = self
                .space
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
            state.idle -= 1;
        }
    }

    fn store(&self, k: usize, region: Region, result: Result<ChunkOut, String>) {
        let mut state = lock(&self.state);
        state.slots[k] = Some(result);
        state.regions[k] = Some(region);
        if state.main_waits == Some(k) {
            self.ready.notify_one();
        }
    }

    pub(crate) fn region(&self, k: usize) -> Region {
        let mut state = lock(&self.state);
        loop {
            if let Some(region) = state.regions[k] {
                state.main_waits = None;
                return region;
            }
            state.main_waits = Some(k);
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn take(&self, k: usize) -> Result<ChunkOut, String> {
        let mut state = lock(&self.state);
        loop {
            if let Some(result) = state.slots[k].take() {
                state.main_waits = None;
                state.current = k + 1;
                if state.idle > 0 {
                    self.space.notify_one();
                }
                return result;
            }
            state.main_waits = Some(k);
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

pub(crate) fn split_chunks(buf: &[u8], term: u8, chunk_size: usize) -> Vec<(usize, usize)> {
    let mut chunks = vec![];
    let mut start = 0usize;
    while start < buf.len() {
        let want = (start + chunk_size).min(buf.len());
        let end = if want >= buf.len() {
            buf.len()
        } else {
            memchr(term, &buf[want..]).map_or(buf.len(), |i| want + i + 1)
        };
        chunks.push((start, end));
        start = end;
    }
    chunks
}

pub(crate) struct Job<'a> {
    pub(crate) buf: &'a [u8],
    pub(crate) finder: &'a dyn ParallelMatcher,
    pub(crate) line_term: LineTerminator,
    pub(crate) count_lines: bool,
    pub(crate) guard: Option<Guard>,
    pub(crate) per_line: bool,
}

impl ChunkOut {
    fn empty() -> ChunkOut {
        ChunkOut {
            lines: vec![],
            rel: vec![],
            terms: 0,
        }
    }

    fn record(&mut self, job: &Job<'_>, counted: &mut usize, line: Range) {
        if job.count_lines {
            self.terms += lines::count(&job.buf[*counted..line.start()], job.line_term.as_byte());
            *counted = line.start();
            self.rel.push(self.terms);
        }
        self.lines.push(line);
    }

    fn close(mut self, job: &Job<'_>, counted: usize, end: usize) -> ChunkOut {
        if job.count_lines {
            self.terms += lines::count(&job.buf[counted..end], job.line_term.as_byte());
        }
        self
    }
}

fn search_chunk(
    job: &Job<'_>,
    finder: &dyn ParallelMatcher,
    start: usize,
    end: usize,
) -> Result<ChunkOut, String> {
    let buf = job.buf;
    let term = job.line_term.as_byte();
    let mut out = ChunkOut::empty();
    let mut counted = start;
    let mut pos = start;
    while pos < end {
        let line = match finder.par_candidate_line(&buf[pos..end])? {
            None => break,
            Some(LineMatchKind::Confirmed(i)) => {
                let line = lines::locate(buf, term, Range::zero(pos + i));
                if line.start() >= end {
                    break;
                }
                line
            }
            #[cfg(test)]
            Some(LineMatchKind::Candidate(i)) => {
                let line = lines::locate(buf, term, Range::zero(pos + i));
                if line.start() >= end {
                    break;
                }
                let slice = lines::without_terminator(&buf[line], job.line_term);
                if !finder.par_is_match(slice)? {
                    pos = line.end();
                    continue;
                }
                line
            }
        };
        out.record(job, &mut counted, line);
        pos = line.end();
    }
    Ok(out.close(job, counted, end))
}

fn search_lines_chunk(
    job: &Job<'_>,
    finder: &dyn ParallelMatcher,
    start: usize,
    end: usize,
) -> Result<ChunkOut, String> {
    let mut out = ChunkOut::empty();
    let mut counted = start;
    let mut stepper = LineStep::new(job.line_term.as_byte(), start, end);
    while let Some(line) = stepper.next_match(job.buf) {
        if finder.par_is_match(lines::without_terminator(&job.buf[line], job.line_term))? {
            out.record(job, &mut counted, line);
        }
    }
    Ok(out.close(job, counted, end))
}

pub(crate) fn search_worker(job: &Job<'_>, shared: &Shared, chunks: &[(usize, usize)]) {
    let fork = job.finder.par_fork();
    let finder = fork.as_deref().unwrap_or(job.finder);
    let term = job.line_term.as_byte();
    while let Some(k) = shared.claim() {
        let (start, end) = chunks[k];
        let region = job.guard.map_or(Region::Clean, |guard| {
            check_region(job.buf, start, end, term, guard)
        });
        let result = if region == Region::Binary {
            Ok(ChunkOut::empty())
        } else {
            catch_unwind(AssertUnwindSafe(|| {
                if job.per_line {
                    search_lines_chunk(job, finder, start, end)
                } else {
                    search_chunk(job, finder, start, end)
                }
            }))
            .unwrap_or_else(|_| Err(PANICKED.to_string()))
        };
        shared.store(k, region, result);
    }
}

pub(crate) struct Feed<'a> {
    job: &'a Job<'a>,
    shared: &'a Shared,
    chunks: usize,
    current: usize,
    out: Option<ChunkOut>,
    idx: usize,
    base_terms: u64,
}

impl<'a> Feed<'a> {
    pub(crate) fn new(job: &'a Job<'a>, shared: &'a Shared, chunks: usize) -> Feed<'a> {
        Feed {
            job,
            shared,
            chunks,
            current: 0,
            out: None,
            idx: 0,
            base_terms: 0,
        }
    }

    fn load_current(&mut self) -> Result<bool, String> {
        if self.out.is_some() {
            return Ok(true);
        }
        if self.current >= self.chunks {
            return Ok(false);
        }
        self.out = Some(self.shared.take(self.current)?);
        self.idx = 0;
        Ok(true)
    }
}

impl LineFeed for Feed<'_> {
    fn peek(&mut self, pos: usize, end: usize) -> Result<Option<(Range, Option<u64>)>, String> {
        loop {
            if !self.load_current()? {
                return Ok(None);
            }
            let out = self.out.as_ref().unwrap();
            while self.idx < out.lines.len() && out.lines[self.idx].start() < pos {
                self.idx += 1;
            }
            if self.idx < out.lines.len() {
                let line = out.lines[self.idx];
                if line.start() >= end {
                    return Ok(None);
                }
                let number = if self.job.count_lines {
                    Some(1 + self.base_terms + out.rel[self.idx])
                } else {
                    None
                };
                return Ok(Some((line, number)));
            }
            self.base_terms += out.terms;
            self.out = None;
            self.current += 1;
        }
    }

    fn consume(&mut self) {
        self.idx += 1;
    }
}

pub(crate) fn count_lines_in(buf: &[u8], start: usize, end: usize, term: u8) -> u64 {
    lines::count(&buf[start..end], term) + u64::from(end > start && buf[end - 1] != term)
}

pub(crate) fn count_matches_at<F>(
    haystack: &[u8],
    at: usize,
    limit: usize,
    mut find_at: F,
) -> Result<u64, String>
where
    F: FnMut(&[u8], usize) -> Result<Option<Range>, String>,
{
    let mut count = 0;
    let mut last_end = at;
    let mut last_match = None;
    while last_end <= haystack.len() {
        let Some(m) = find_at(haystack, last_end)? else {
            break;
        };
        if m.start() >= limit {
            break;
        }
        if m.is_empty() {
            last_end = m.end() + 1;
            if Some(m.end()) == last_match {
                continue;
            }
        } else {
            last_end = m.end();
        }
        last_match = Some(m.end());
        count += 1;
    }
    Ok(count)
}

pub(crate) struct CountOut {
    pub(crate) matched: u64,
    pub(crate) matches: u64,
    pub(crate) lines: u64,
}

pub(crate) trait LineCounter {
    fn candidate(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String>;

    #[cfg(test)]
    fn is_match(&self, haystack: &[u8]) -> Result<bool, String>;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Range>, String>;

    fn literal(&self) -> Option<&[u8]>;
}

impl LineCounter for &dyn ParallelMatcher {
    fn candidate(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        self.par_candidate_line(haystack)
    }

    #[cfg(test)]
    fn is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        self.par_is_match(haystack)
    }

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Range>, String> {
        self.par_find_at(haystack, at)
    }

    fn literal(&self) -> Option<&[u8]> {
        self.par_literal()
    }
}

pub(crate) fn count_chunk<C: LineCounter>(
    buf: &[u8],
    line_term: LineTerminator,
    start: usize,
    end: usize,
    counter: &C,
    with_matches: bool,
    with_lines: bool,
) -> Result<CountOut, String> {
    let term = line_term.as_byte();
    if !with_matches
        && let Some(needle) = counter.literal()
        && !(line_term.is_crlf() && needle.contains(&b'\r'))
        && let Some((lines, matched)) =
            bitcount::literal_line_counts(&buf[start..end], needle, term)
    {
        return Ok(CountOut {
            matched,
            matches: 0,
            lines: if with_lines { lines } else { 0 },
        });
    }
    let mut out = CountOut {
        matched: 0,
        matches: 0,
        lines: if with_lines {
            count_lines_in(buf, start, end, term)
        } else {
            0
        },
    };
    let mut pos = start;
    while pos < end {
        let line = match counter.candidate(&buf[pos..end])? {
            None => break,
            Some(LineMatchKind::Confirmed(i)) => {
                let at = pos + i;
                if at >= end && buf[at - 1] == term {
                    break;
                }
                if !with_matches {
                    out.matched += 1;
                    pos = memchr(term, &buf[at..end]).map_or(end, |k| at + k + 1);
                    continue;
                }
                lines::locate(buf, term, Range::zero(at))
            }
            #[cfg(test)]
            Some(LineMatchKind::Candidate(i)) => {
                let line = lines::locate(buf, term, Range::zero(pos + i));
                if line.start() >= end {
                    break;
                }
                if !counter.is_match(lines::without_terminator(&buf[line], line_term))? {
                    pos = line.end();
                    continue;
                }
                line
            }
        };
        out.matched += 1;
        if with_matches {
            let mut trimmed = line.end();
            if line_term.is_suffix(&buf[..trimmed]) {
                trimmed -= 1;
                if line_term.is_crlf() && trimmed > 0 && buf[trimmed - 1] == b'\r' {
                    trimmed -= 1;
                }
            }
            let count = count_matches_at(&buf[..trimmed], line.start(), line.end(), |hay, at| {
                counter.find_at(hay, at)
            })?;
            out.matches += count.max(1);
        }
        pos = line.end();
    }
    Ok(out)
}

pub(crate) fn count_chunk_converted<C: LineCounter>(
    chunk: &[u8],
    line_term: LineTerminator,
    byte: u8,
    scratch: &mut Vec<u8>,
    counter: &C,
    with_matches: bool,
    with_lines: bool,
) -> Result<CountOut, String> {
    let term = line_term.as_byte();
    scratch.clear();
    scratch.extend(chunk.iter().map(|&b| if b == byte { term } else { b }));
    count_chunk(
        scratch,
        line_term,
        0,
        scratch.len(),
        counter,
        with_matches,
        with_lines,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Counting {
    Plain,
    Converted(u8),
    Guarded(u8),
}

#[derive(Clone, Copy)]
pub(crate) struct CountSpec<'a> {
    pub(crate) buf: &'a [u8],
    pub(crate) line_term: LineTerminator,
    pub(crate) with_matches: bool,
    pub(crate) with_lines: bool,
    pub(crate) mode: Counting,
}

impl CountOut {
    fn zero() -> CountOut {
        CountOut {
            matched: 0,
            matches: 0,
            lines: 0,
        }
    }

    fn add(&mut self, other: &CountOut) {
        self.matched += other.matched;
        self.matches += other.matches;
        self.lines += other.lines;
    }
}

fn count_piece<C: LineCounter>(
    spec: CountSpec<'_>,
    start: usize,
    end: usize,
    counter: &C,
    scratch: &mut Vec<u8>,
) -> Result<Option<CountOut>, String> {
    let CountSpec {
        buf,
        line_term,
        with_matches,
        with_lines,
        mode,
    } = spec;
    match mode {
        Counting::Plain => count_chunk(
            buf,
            line_term,
            start,
            end,
            counter,
            with_matches,
            with_lines,
        )
        .map(Some),
        Counting::Converted(byte) => count_chunk_converted(
            &buf[start..end],
            line_term,
            byte,
            scratch,
            counter,
            with_matches,
            with_lines,
        )
        .map(Some),
        Counting::Guarded(byte) => {
            if memchr(byte, &buf[start..end]).is_some() {
                return Ok(None);
            }
            count_chunk(
                buf,
                line_term,
                start,
                end,
                counter,
                with_matches,
                with_lines,
            )
            .map(Some)
        }
    }
}

pub(crate) fn count_sequential<C: LineCounter>(
    spec: CountSpec<'_>,
    counter: &C,
) -> Result<Option<CountOut>, String> {
    if spec.mode == Counting::Plain {
        return count_piece(spec, 0, spec.buf.len(), counter, &mut vec![]);
    }
    let mut total = CountOut::zero();
    let mut scratch = vec![];
    for (start, end) in split_chunks(spec.buf, spec.line_term.as_byte(), 1 << 20) {
        let Some(out) = count_piece(spec, start, end, counter, &mut scratch)? else {
            return Ok(None);
        };
        total.add(&out);
    }
    Ok(Some(total))
}

pub(crate) fn count_parallel(
    spec: CountSpec<'_>,
    matcher: &dyn ParallelMatcher,
    chunks: &[(usize, usize)],
) -> Result<Option<CountOut>, String> {
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let total = Mutex::new(Ok(Some(CountOut::zero())));
    run_workers(thread_count().min(chunks.len()), || {
        let fork = matcher.par_fork();
        let finder = fork.as_deref().unwrap_or(matcher);
        let mut acc = CountOut::zero();
        let mut outcome = Ok(Some(()));
        let mut scratch = vec![];
        while !failed.load(Ordering::Relaxed) {
            let k = next.fetch_add(1, Ordering::Relaxed);
            if k >= chunks.len() {
                break;
            }
            let (start, end) = chunks[k];
            let result = catch_unwind(AssertUnwindSafe(|| {
                count_piece(spec, start, end, &finder, &mut scratch)
            }))
            .unwrap_or_else(|_| Err(PANICKED.to_string()));
            match result {
                Ok(Some(out)) => acc.add(&out),
                Ok(None) => {
                    failed.store(true, Ordering::Relaxed);
                    outcome = Ok(None);
                    break;
                }
                Err(err) => {
                    failed.store(true, Ordering::Relaxed);
                    outcome = Err(err);
                    break;
                }
            }
        }
        let mut total = lock(&total);
        match (&mut *total, outcome) {
            (Ok(_), Err(err)) => *total = Err(err),
            (Ok(Some(_)), Ok(None)) => *total = Ok(None),
            (Ok(Some(sum)), Ok(Some(()))) => sum.add(&acc),
            _ => {}
        }
    });
    total.into_inner().unwrap_or_else(PoisonError::into_inner)
}
