mod bitcount;
mod core;
mod glue;
mod line_buffer;
mod lines;
mod parallel;
mod sink;
#[cfg(test)]
mod testutil;

use std::cell::RefCell;
use std::cmp;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

use self::core::Core;
use super::Flags;
use super::matcher::{LineMatchKind, LineTerminator, Match, Matcher, ParallelMatcher};
use glue::{MultiLine, ReadByLine, SliceByLine};
use line_buffer::{
    BufferAllocation, DEFAULT_BUFFER_CAPACITY, LineBuffer, LineBufferBuilder, LineBufferReader,
    alloc_error,
};

pub use lines::{LineIter, LineStep};
pub use sink::{Sink, SinkContext, SinkContextKind, SinkCount, SinkError, SinkFinish, SinkMatch};

type Range = Match;

const MMAP_THRESHOLD: u64 = 1 << 20;
const PARALLEL_THRESHOLD: usize = 4 << 20;
const GUARD_STEP: usize = 1 << 20;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BinaryDetection(line_buffer::BinaryDetection);

impl BinaryDetection {
    #[must_use]
    pub fn none() -> BinaryDetection {
        BinaryDetection(line_buffer::BinaryDetection::None)
    }

    #[must_use]
    pub fn quit(binary_byte: u8) -> BinaryDetection {
        BinaryDetection(line_buffer::BinaryDetection::Quit(binary_byte))
    }

    #[must_use]
    pub fn convert(binary_byte: u8) -> BinaryDetection {
        BinaryDetection(line_buffer::BinaryDetection::Convert(binary_byte))
    }

    #[must_use]
    pub fn quit_byte(&self) -> Option<u8> {
        match self.0 {
            line_buffer::BinaryDetection::Quit(b) => Some(b),
            _ => None,
        }
    }

    #[must_use]
    pub fn convert_byte(&self) -> Option<u8> {
        match self.0 {
            line_buffer::BinaryDetection::Convert(b) => Some(b),
            _ => None,
        }
    }

    fn byte(&self) -> Option<u8> {
        match self.0 {
            line_buffer::BinaryDetection::Quit(b) | line_buffer::BinaryDetection::Convert(b) => {
                Some(b)
            }
            line_buffer::BinaryDetection::None => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MmapChoice(bool);

impl MmapChoice {
    #[must_use]
    pub fn auto() -> MmapChoice {
        MmapChoice(true)
    }

    #[must_use]
    pub fn never() -> MmapChoice {
        MmapChoice(false)
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.0
    }

    fn slice_semantics(&self) -> bool {
        self.0 && !cfg!(target_os = "macos")
    }
}

pub type TranscodeFn = dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync;

#[derive(Clone)]
struct Transcoder(Arc<TranscodeFn>);

impl fmt::Debug for Transcoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Transcoder")
    }
}

const INVERT_MATCH: u32 = 1;
const PASSTHRU: u32 = 1 << 1;
const LINE_NUMBER: u32 = 1 << 2;
const MULTI_LINE: u32 = 1 << 3;
const BOM_SNIFFING: u32 = 1 << 4;
const STOP_ON_NONMATCH: u32 = 1 << 5;
const PARALLEL: u32 = 1 << 6;

#[derive(Clone, Debug)]
pub struct Config {
    line_term: LineTerminator,
    flags: Flags,
    after_context: usize,
    before_context: usize,
    heap_limit: Option<usize>,
    mmap: MmapChoice,
    binary: BinaryDetection,
    max_matches: Option<u64>,
    transcoder: Option<Transcoder>,
    parallel_threshold: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            line_term: LineTerminator::default(),
            flags: Flags(LINE_NUMBER | BOM_SNIFFING | PARALLEL),
            after_context: 0,
            before_context: 0,
            heap_limit: None,
            mmap: MmapChoice::default(),
            binary: BinaryDetection::default(),
            max_matches: None,
            transcoder: None,
            parallel_threshold: PARALLEL_THRESHOLD,
        }
    }
}

impl Config {
    fn invert_match(&self) -> bool {
        self.flags.contains(INVERT_MATCH)
    }

    fn passthru(&self) -> bool {
        self.flags.contains(PASSTHRU)
    }

    fn line_number(&self) -> bool {
        self.flags.contains(LINE_NUMBER)
    }

    fn multi_line(&self) -> bool {
        self.flags.contains(MULTI_LINE)
    }

    fn bom_sniffing(&self) -> bool {
        self.flags.contains(BOM_SNIFFING)
    }

    fn stop_on_nonmatch(&self) -> bool {
        self.flags.contains(STOP_ON_NONMATCH)
    }

    fn parallel(&self) -> bool {
        self.flags.contains(PARALLEL)
    }

    fn max_context(&self) -> usize {
        cmp::max(self.before_context, self.after_context)
    }

    fn line_buffer(&self) -> LineBuffer {
        let mut builder = LineBufferBuilder::new();
        builder
            .line_terminator(self.line_term.as_byte())
            .binary_detection(self.binary.0);
        if let Some(limit) = self.heap_limit {
            let (capacity, additional) = if limit <= DEFAULT_BUFFER_CAPACITY {
                (limit, 0)
            } else {
                (DEFAULT_BUFFER_CAPACITY, limit - DEFAULT_BUFFER_CAPACITY)
            };
            builder
                .capacity(capacity)
                .buffer_alloc(BufferAllocation::Error(additional));
        }
        builder.build()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    SearchUnavailable,
    MismatchedLineTerminators {
        matcher: LineTerminator,
        searcher: LineTerminator,
    },
}

impl std::error::Error for ConfigError {}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ConfigError::SearchUnavailable => {
                write!(f, "grep config error: no available searchers")
            }
            ConfigError::MismatchedLineTerminators { matcher, searcher } => write!(
                f,
                "grep config error: mismatched line terminators, matcher has {matcher:?} but searcher has {searcher:?}"
            ),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SearcherBuilder {
    config: Config,
}

impl SearcherBuilder {
    #[must_use]
    pub fn new() -> SearcherBuilder {
        SearcherBuilder::default()
    }

    #[must_use]
    pub fn build(&self) -> Searcher {
        let mut config = self.config.clone();
        if config.passthru() {
            config.before_context = 0;
            config.after_context = 0;
        }
        Searcher {
            line_buffer: RefCell::new(config.line_buffer()),
            config,
            multi_line_buffer: RefCell::new(vec![]),
            file_buffer: RefCell::new(vec![]),
        }
    }

    pub fn line_terminator(&mut self, line_term: LineTerminator) -> &mut SearcherBuilder {
        self.config.line_term = line_term;
        self
    }

    pub fn invert_match(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(INVERT_MATCH, yes);
        self
    }

    pub fn line_number(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(LINE_NUMBER, yes);
        self
    }

    pub fn multi_line(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(MULTI_LINE, yes);
        self
    }

    pub fn after_context(&mut self, line_count: usize) -> &mut SearcherBuilder {
        self.config.after_context = line_count;
        self
    }

    pub fn before_context(&mut self, line_count: usize) -> &mut SearcherBuilder {
        self.config.before_context = line_count;
        self
    }

    pub fn passthru(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(PASSTHRU, yes);
        self
    }

    #[cfg(test)]
    pub fn heap_limit(&mut self, bytes: Option<usize>) -> &mut SearcherBuilder {
        self.config.heap_limit = bytes;
        self
    }

    pub fn memory_map(&mut self, strategy: MmapChoice) -> &mut SearcherBuilder {
        self.config.mmap = strategy;
        self
    }

    #[cfg(test)]
    pub fn binary_detection(&mut self, detection: BinaryDetection) -> &mut SearcherBuilder {
        self.config.binary = detection;
        self
    }

    pub fn bom_sniffing(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(BOM_SNIFFING, yes);
        self
    }

    pub fn stop_on_nonmatch(&mut self, stop_on_nonmatch: bool) -> &mut SearcherBuilder {
        self.config.flags.set(STOP_ON_NONMATCH, stop_on_nonmatch);
        self
    }

    pub fn max_matches(&mut self, limit: Option<u64>) -> &mut SearcherBuilder {
        self.config.max_matches = limit;
        self
    }

    pub fn transcoder(&mut self, transcoder: Option<Arc<TranscodeFn>>) -> &mut SearcherBuilder {
        self.config.transcoder = transcoder.map(Transcoder);
        self
    }

    #[cfg(test)]
    pub fn parallel(&mut self, yes: bool) -> &mut SearcherBuilder {
        self.config.flags.set(PARALLEL, yes);
        self
    }

    #[cfg(test)]
    pub fn parallel_threshold(&mut self, bytes: usize) -> &mut SearcherBuilder {
        self.config.parallel_threshold = bytes;
        self
    }
}

#[derive(Clone, Debug)]
pub struct Searcher {
    config: Config,
    line_buffer: RefCell<LineBuffer>,
    multi_line_buffer: RefCell<Vec<u8>>,
    file_buffer: RefCell<Vec<u8>>,
}

impl Default for Searcher {
    fn default() -> Searcher {
        Searcher::new()
    }
}

enum Contents<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl Contents<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Contents::Borrowed(b) => b,
            Contents::Owned(v) => v,
        }
    }
}

struct Decoded<'a> {
    data: Contents<'a>,
    first_read: Option<usize>,
}

fn has_bom(slice: &[u8]) -> bool {
    slice.starts_with(b"\xEF\xBB\xBF")
        || slice.starts_with(b"\xFF\xFE")
        || slice.starts_with(b"\xFE\xFF")
}

fn decode_utf16(bytes: &[u8], little: bool) -> Vec<u8> {
    let units = bytes.as_chunks::<2>().0.iter().map(|c| {
        if little {
            u16::from_le_bytes([c[0], c[1]])
        } else {
            u16::from_be_bytes([c[0], c[1]])
        }
    });
    let mut out = String::with_capacity(bytes.len());
    for r in char::decode_utf16(units) {
        out.push(r.unwrap_or('\u{FFFD}'));
    }
    if bytes.len() % 2 == 1 {
        out.push('\u{FFFD}');
    }
    out.into_bytes()
}

fn decode_contents<'a>(config: &Config, raw: &'a [u8]) -> Decoded<'a> {
    if let Some(transcoder) = &config.transcoder
        && let Some(out) = (transcoder.0)(raw)
    {
        return Decoded {
            data: Contents::Owned(out),
            first_read: None,
        };
    }
    let peek = &raw[..raw.len().min(3)];
    if config.bom_sniffing() && peek.len() >= 2 {
        let utf16 = if peek.starts_with(b"\xFF\xFE") {
            Some(true)
        } else if peek.starts_with(b"\xFE\xFF") {
            Some(false)
        } else {
            None
        };
        if let Some(little) = utf16 {
            if peek.len() < 3 {
                return Decoded {
                    data: Contents::Borrowed(&raw[2..]),
                    first_read: None,
                };
            }
            return Decoded {
                data: Contents::Owned(decode_utf16(&raw[2..], little)),
                first_read: None,
            };
        }
        if peek == b"\xEF\xBB\xBF" {
            return Decoded {
                data: Contents::Borrowed(&raw[3..]),
                first_read: None,
            };
        }
    }
    Decoded {
        data: Contents::Borrowed(raw),
        first_read: Some(3),
    }
}

#[cfg(unix)]
fn advise_will_need(map: &memmap2::Mmap) {
    let _ = map.advise(memmap2::Advice::WillNeed);
}

#[cfg(not(unix))]
fn advise_will_need(_map: &memmap2::Mmap) {}

struct Replay<S> {
    sink: S,
    seen: usize,
    skip: Option<usize>,
    binary: Option<bool>,
}

impl<S> Replay<S> {
    fn new(sink: S) -> Replay<S> {
        Replay {
            sink,
            seen: 0,
            skip: None,
            binary: None,
        }
    }

    fn resumed(sink: S, binary: Option<bool>) -> Replay<S> {
        Replay {
            sink,
            seen: 0,
            skip: Some(0),
            binary,
        }
    }

    fn rewind(&mut self) {
        self.skip = Some(self.seen);
    }

    fn pass(&mut self) -> bool {
        match &mut self.skip {
            None => {
                self.seen += 1;
                true
            }
            Some(0) => true,
            Some(left) => {
                *left -= 1;
                false
            }
        }
    }
}

impl<S: Sink> Sink for Replay<S> {
    type Error = S::Error;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, S::Error> {
        if !self.pass() {
            return Ok(true);
        }
        self.sink.matched(searcher, mat)
    }

    fn context(&mut self, searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, S::Error> {
        if !self.pass() {
            return Ok(true);
        }
        self.sink.context(searcher, ctx)
    }

    fn context_break(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        if !self.pass() {
            return Ok(true);
        }
        self.sink.context_break(searcher)
    }

    fn binary_data(&mut self, searcher: &Searcher, offset: u64) -> Result<bool, S::Error> {
        match self.binary {
            Some(keep_going) => Ok(keep_going),
            None => self.sink.binary_data(searcher, offset),
        }
    }

    fn begin(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        if self.skip.is_some() {
            return Ok(true);
        }
        self.sink.begin(searcher)
    }

    fn finish(&mut self, searcher: &Searcher, finish: &SinkFinish) -> Result<(), S::Error> {
        self.sink.finish(searcher, finish)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flow {
    Done,
    Replay,
}

fn run_core<M: Matcher, S: Sink>(core: &mut Core<'_, M, S>, buf: &[u8]) -> Result<bool, S::Error> {
    while !buf[core.pos()..].is_empty() {
        if !core.match_by_line(buf)? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct SeqCounter<'m, M>(&'m M);

impl<M: Matcher> parallel::LineCounter for SeqCounter<'_, M> {
    fn candidate(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, String> {
        self.0
            .find_candidate_line(haystack)
            .map_err(|e| e.to_string())
    }

    #[cfg(test)]
    fn is_match(&self, haystack: &[u8]) -> Result<bool, String> {
        self.0.is_match(haystack).map_err(|e| e.to_string())
    }

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Range>, String> {
        self.0.find_at(haystack, at).map_err(|e| e.to_string())
    }

    fn literal(&self) -> Option<&[u8]> {
        self.0.parallel().and_then(ParallelMatcher::par_literal)
    }
}

struct VirtualReader<'a> {
    data: &'a [u8],
    pos: usize,
    first_read: Option<usize>,
}

impl Read for VirtualReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.data.len() - self.pos;
        let limit = self.first_read.take().unwrap_or(usize::MAX);
        let n = buf.len().min(remaining).min(limit);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

enum Raw<'a> {
    Map(memmap2::Mmap),
    Buf(std::cell::RefMut<'a, Vec<u8>>),
}

impl Raw<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Raw::Map(m) => m,
            Raw::Buf(b) => b,
        }
    }
}

impl Searcher {
    #[must_use]
    pub fn new() -> Searcher {
        SearcherBuilder::new().build()
    }

    pub fn search_path<P, M, S>(&mut self, matcher: M, path: P, write_to: S) -> Result<(), S::Error>
    where
        P: AsRef<Path>,
        M: Matcher,
        S: Sink,
    {
        let file = File::open(path.as_ref()).map_err(S::Error::error_io)?;
        self.search_file(matcher, &file, write_to)
    }

    pub fn search_file<M, S>(
        &mut self,
        matcher: M,
        file: &File,
        write_to: S,
    ) -> Result<(), S::Error>
    where
        M: Matcher,
        S: Sink,
    {
        let multi_line = self.multi_line_with_matcher(&matcher);
        if !multi_line {
            self.check_config(&matcher)
                .map_err(S::Error::error_config)?;
        }
        let meta = file.metadata().map_err(S::Error::error_io)?;
        if !meta.is_file() {
            if multi_line {
                self.fill_multi_line_buffer_from_reader::<_, S>(decoding_reader(
                    &self.config,
                    file,
                )?)?;
                let buf = self.multi_line_buffer.borrow();
                return MultiLine::new(self, matcher, &buf, write_to).run();
            }
            return self.search_reader(matcher, file, write_to);
        }
        let raw = self.read_raw::<S>(file, meta.len())?;
        let raw_bytes = raw.bytes();
        let slice_semantics = self.config.mmap.slice_semantics();
        if slice_semantics && !self.slice_needs_transcoding(raw_bytes) {
            if multi_line {
                return MultiLine::new(self, matcher, raw_bytes, write_to).run();
            }
            return self.search_slice_lines(matcher, raw_bytes, write_to, true);
        }
        let decoded = decode_contents(&self.config, raw_bytes);
        let data = decoded.data.bytes();
        if multi_line {
            if let Some(limit) = self.config.heap_limit
                && data.len() >= limit
            {
                return Err(S::Error::error_io(alloc_error(limit)));
            }
            return MultiLine::new(self, matcher, data, write_to).run();
        }
        self.search_reader_semantics(matcher, data, decoded.first_read, write_to)
    }

    pub fn search_reader<M, R, S>(
        &mut self,
        matcher: M,
        read_from: R,
        write_to: S,
    ) -> Result<(), S::Error>
    where
        M: Matcher,
        R: io::Read,
        S: Sink,
    {
        self.check_config(&matcher)
            .map_err(S::Error::error_config)?;
        if self.config.transcoder.is_some() {
            let mut raw = vec![];
            let mut read_from = read_from;
            read_from
                .read_to_end(&mut raw)
                .map_err(S::Error::error_io)?;
            let decoded = decode_contents(&self.config, &raw);
            let data = decoded.data.bytes();
            if self.multi_line_with_matcher(&matcher) {
                return MultiLine::new(self, matcher, data, write_to).run();
            }
            return self.search_reader_semantics(matcher, data, decoded.first_read, write_to);
        }
        let reader = decoding_reader(&self.config, read_from)?;
        if self.multi_line_with_matcher(&matcher) {
            self.fill_multi_line_buffer_from_reader::<_, S>(reader)?;
            let buf = self.multi_line_buffer.borrow();
            MultiLine::new(self, matcher, &buf, write_to).run()
        } else {
            let mut line_buffer = self.line_buffer.borrow_mut();
            let rdr = LineBufferReader::new(reader, &mut line_buffer);
            ReadByLine::new(self, matcher, rdr, write_to).run()
        }
    }

    #[cfg(test)]
    pub fn search_slice<M, S>(
        &mut self,
        matcher: M,
        slice: &[u8],
        write_to: S,
    ) -> Result<(), S::Error>
    where
        M: Matcher,
        S: Sink,
    {
        self.check_config(&matcher)
            .map_err(S::Error::error_config)?;
        if self.slice_needs_transcoding(slice) {
            let decoded = decode_contents(&self.config, slice);
            let data = decoded.data.bytes();
            if self.multi_line_with_matcher(&matcher) {
                return MultiLine::new(self, matcher, data, write_to).run();
            }
            return self.search_reader_semantics(matcher, data, decoded.first_read, write_to);
        }
        if self.multi_line_with_matcher(&matcher) {
            MultiLine::new(self, matcher, slice, write_to).run()
        } else {
            self.search_slice_lines(matcher, slice, write_to, true)
        }
    }

    pub fn set_binary_detection(&mut self, detection: BinaryDetection) {
        self.line_buffer
            .borrow_mut()
            .set_binary_detection(detection.0);
        self.config.binary = detection;
    }

    fn read_raw<S: Sink>(&self, mut file: &File, len: u64) -> Result<Raw<'_>, S::Error> {
        if len >= MMAP_THRESHOLD
            && let Ok(map) = unsafe { memmap2::Mmap::map(file) }
        {
            advise_will_need(&map);
            return Ok(Raw::Map(map));
        }
        let mut buf = self.file_buffer.borrow_mut();
        let Ok(want) = usize::try_from(len) else {
            buf.clear();
            file.read_to_end(&mut buf).map_err(S::Error::error_io)?;
            return Ok(Raw::Buf(buf));
        };
        if want == 0 {
            buf.clear();
            file.read_to_end(&mut buf).map_err(S::Error::error_io)?;
            return Ok(Raw::Buf(buf));
        }
        if buf.len() < want {
            buf.resize(want, 0);
        } else {
            buf.truncate(want);
        }
        let mut filled = 0;
        while filled < want {
            match file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(S::Error::error_io(err)),
            }
        }
        buf.truncate(filled);
        Ok(Raw::Buf(buf))
    }

    fn search_reader_semantics<M, S>(
        &self,
        matcher: M,
        data: &[u8],
        first_read: Option<usize>,
        write_to: S,
    ) -> Result<(), S::Error>
    where
        M: Matcher,
        S: Sink,
    {
        if self.config.heap_limit.is_some() {
            return self.read_by_line(matcher, data, first_read, write_to);
        }
        let Some(byte) = self.config.binary.byte() else {
            return self.search_slice_lines(matcher, data, write_to, false);
        };
        let mode = self.bulk_count_mode(&matcher, &write_to);
        if mode != SinkCount::None {
            return self.count_reader_semantics(&matcher, data, byte, first_read, mode, write_to);
        }
        let guard = parallel::Guard {
            byte,
            window: DEFAULT_BUFFER_CAPACITY / (self.config.max_context() + 3) / 2,
        };
        let mut sink = Replay::new(write_to);
        if self.search_lines_guarded(&matcher, data, guard, &mut sink)? == Flow::Done {
            return Ok(());
        }
        sink.rewind();
        self.read_by_line(&matcher, data, first_read, sink)
    }

    fn read_by_line<M: Matcher, S: Sink>(
        &self,
        matcher: M,
        data: &[u8],
        first_read: Option<usize>,
        write_to: S,
    ) -> Result<(), S::Error> {
        let reader = VirtualReader {
            data,
            pos: 0,
            first_read,
        };
        let mut line_buffer = self.line_buffer.borrow_mut();
        let rdr = LineBufferReader::new(reader, &mut line_buffer);
        ReadByLine::new(self, matcher, rdr, write_to).run()
    }

    fn search_lines_guarded<M: Matcher, S: Sink>(
        &self,
        matcher: &M,
        data: &[u8],
        guard: parallel::Guard,
        sink: S,
    ) -> Result<Flow, S::Error> {
        let finder = matcher.parallel();
        let Some(finder) = finder.filter(|_| self.can_parallelize(data.len())) else {
            let mut core = Core::new(self, matcher, sink, false);
            return self.drive_guarded(&mut core, data, guard, None);
        };
        let threads = parallel::thread_count();
        let chunks = parallel::split_chunks(
            data,
            self.config.line_term.as_byte(),
            self.chunk_size(data.len()).max(DEFAULT_BUFFER_CAPACITY),
        );
        let shared = parallel::Shared::new(chunks.len(), threads);
        let job = parallel::Job {
            buf: data,
            finder,
            line_term: self.config.line_term,
            count_lines: self.config.line_number(),
            guard: Some(guard),
            per_line: !self.is_line_by_line_fast_with(matcher),
        };
        rayon::in_place_scope(|scope| {
            for _ in 0..threads.min(chunks.len()) {
                scope.spawn(|_| parallel::search_worker(&job, &shared, &chunks));
            }
            let mut feed = parallel::Feed::new(&job, &shared, chunks.len());
            let mut core = Core::new(self, matcher, sink, false);
            core.set_feed(&mut feed);
            let flow = self.drive_guarded(&mut core, data, guard, Some((&shared, &chunks)));
            shared.finish();
            flow
        })
    }

    fn drive_guarded<M: Matcher, S: Sink>(
        &self,
        core: &mut Core<'_, M, S>,
        data: &[u8],
        guard: parallel::Guard,
        chunks: Option<(&parallel::Shared, &[(usize, usize)])>,
    ) -> Result<Flow, S::Error> {
        if !core.begin()? {
            core.finish(0, None)?;
            return Ok(Flow::Done);
        }
        let flow = match chunks {
            None => self.drive_regions(core, data, guard)?,
            Some((shared, chunks)) => Self::drive_chunks(core, data, guard, shared, chunks)?,
        };
        if flow == Flow::Done {
            let byte_count = core.pos() as u64;
            core.finish(byte_count, None)?;
        }
        Ok(flow)
    }

    fn drive_regions<M: Matcher, S: Sink>(
        &self,
        core: &mut Core<'_, M, S>,
        data: &[u8],
        guard: parallel::Guard,
    ) -> Result<Flow, S::Error> {
        let term = self.config.line_term.as_byte();
        let mut guarded = true;
        let mut checked = 0;
        while checked < data.len() {
            let next = if guarded {
                (checked + GUARD_STEP).min(data.len())
            } else {
                data.len()
            };
            if guarded {
                match parallel::check_region(data, checked, next, term, guard) {
                    parallel::Region::Binary => return Ok(Flow::Replay),
                    parallel::Region::LongLine => {
                        if memchr::memchr(guard.byte, &data[next..]).is_some() {
                            return Ok(Flow::Replay);
                        }
                        guarded = false;
                    }
                    parallel::Region::Clean => {}
                }
            }
            checked = if guarded { next } else { data.len() };
            let limit = if checked == data.len() {
                data.len()
            } else {
                memchr::memrchr(term, &data[..checked - DEFAULT_BUFFER_CAPACITY])
                    .map_or(0, |i| i + 1)
            };
            if !run_core(core, &data[..limit])? {
                return Ok(Flow::Done);
            }
        }
        Ok(Flow::Done)
    }

    fn drive_chunks<M: Matcher, S: Sink>(
        core: &mut Core<'_, M, S>,
        data: &[u8],
        guard: parallel::Guard,
        shared: &parallel::Shared,
        chunks: &[(usize, usize)],
    ) -> Result<Flow, S::Error> {
        let mut guarded = true;
        for (k, &(start, end)) in chunks.iter().enumerate() {
            if guarded {
                let ahead = if k + 1 < chunks.len() {
                    shared.region(k + 1)
                } else {
                    parallel::Region::Clean
                };
                match shared.region(k).worst(ahead) {
                    parallel::Region::Binary => return Ok(Flow::Replay),
                    parallel::Region::LongLine => {
                        if memchr::memchr(guard.byte, &data[start..]).is_some() {
                            return Ok(Flow::Replay);
                        }
                        guarded = false;
                    }
                    parallel::Region::Clean => {}
                }
            }
            if !run_core(core, &data[..end])? {
                return Ok(Flow::Done);
            }
        }
        Ok(Flow::Done)
    }

    fn can_parallelize(&self, len: usize) -> bool {
        self.config.parallel()
            && len >= self.config.parallel_threshold
            && rayon::current_thread_index().is_none()
            && parallel::thread_count() > 1
    }

    fn chunk_size(&self, len: usize) -> usize {
        let min_chunk = (1usize << 20).min(self.config.parallel_threshold.max(1));
        (len / (parallel::thread_count() * 64)).clamp(min_chunk, 16 << 20)
    }

    fn search_slice_lines<M, S>(
        &self,
        matcher: M,
        slice: &[u8],
        write_to: S,
        binary: bool,
    ) -> Result<(), S::Error>
    where
        M: Matcher,
        S: Sink,
    {
        if !binary || self.config.binary.byte().is_none() {
            let mode = self.bulk_count_mode(&matcher, &write_to);
            if mode != SinkCount::None {
                return self.count_only_search(&matcher, slice, mode, write_to);
            }
        }
        let finder = matcher.parallel();
        let Some(finder) = finder.filter(|_| self.can_parallelize(slice.len())) else {
            return SliceByLine::new(self, &matcher, slice, write_to, binary).run();
        };
        let threads = parallel::thread_count();
        let chunks = parallel::split_chunks(
            slice,
            self.config.line_term.as_byte(),
            self.chunk_size(slice.len()),
        );
        let shared = parallel::Shared::new(chunks.len(), threads);
        let job = parallel::Job {
            buf: slice,
            finder,
            line_term: self.config.line_term,
            count_lines: self.config.line_number(),
            guard: None,
            per_line: !self.is_line_by_line_fast_with(&matcher),
        };
        rayon::in_place_scope(|scope| {
            for _ in 0..threads.min(chunks.len()) {
                scope.spawn(|_| parallel::search_worker(&job, &shared, &chunks));
            }
            let mut feed = parallel::Feed::new(&job, &shared, chunks.len());
            let result = SliceByLine::new(self, &matcher, slice, write_to, binary)
                .with_feed(&mut feed)
                .run();
            shared.finish();
            result
        })
    }

    fn bulk_count_mode<M: Matcher, S: Sink>(&self, matcher: &M, sink: &S) -> SinkCount {
        if self.config.passthru()
            || self.config.stop_on_nonmatch()
            || self.config.max_matches.is_some()
            || !self.is_line_by_line_fast_with(matcher)
        {
            return SinkCount::None;
        }
        sink.count_mode(self)
    }

    fn count_only_search<M: Matcher, S: Sink>(
        &self,
        matcher: &M,
        slice: &[u8],
        mode: SinkCount,
        mut sink: S,
    ) -> Result<(), S::Error> {
        let byte_count = if sink.begin(self)? {
            let (lines, hits) = self
                .count_matching_lines(
                    matcher,
                    slice,
                    mode == SinkCount::LinesAndMatches,
                    parallel::Counting::Plain,
                )
                .map_err(S::Error::error_message)?
                .unwrap_or((0, 0));
            sink.matched_count(self, lines, hits)?;
            slice.len() as u64
        } else {
            0
        };
        sink.finish(
            self,
            &SinkFinish {
                byte_count,
                binary_byte_offset: None,
            },
        )
    }

    fn count_reader_semantics<M: Matcher, S: Sink>(
        &self,
        matcher: &M,
        data: &[u8],
        byte: u8,
        first_read: Option<usize>,
        mode: SinkCount,
        mut sink: S,
    ) -> Result<(), S::Error> {
        if !sink.begin(self)? {
            return sink.finish(
                self,
                &SinkFinish {
                    byte_count: 0,
                    binary_byte_offset: None,
                },
            );
        }
        let with_matches = mode == SinkCount::LinesAndMatches;
        let counted = self
            .count_matching_lines(
                matcher,
                data,
                with_matches,
                parallel::Counting::Guarded(byte),
            )
            .map_err(S::Error::error_message)?;
        if let Some((lines, hits)) = counted {
            sink.matched_count(self, lines, hits)?;
            return sink.finish(
                self,
                &SinkFinish {
                    byte_count: data.len() as u64,
                    binary_byte_offset: None,
                },
            );
        }
        if self.config.binary.convert_byte() != Some(byte) {
            return self.read_by_line(matcher, data, first_read, Replay::resumed(sink, None));
        }
        let offset = memchr::memchr(byte, data).unwrap_or(data.len()) as u64;
        if !sink.binary_data(self, offset)? {
            return self.read_by_line(
                matcher,
                data,
                first_read,
                Replay::resumed(sink, Some(false)),
            );
        }
        let (lines, hits) = self
            .count_matching_lines(
                matcher,
                data,
                with_matches,
                parallel::Counting::Converted(byte),
            )
            .map_err(S::Error::error_message)?
            .unwrap_or((0, 0));
        sink.matched_count(self, lines, hits)?;
        sink.finish(
            self,
            &SinkFinish {
                byte_count: data.len() as u64,
                binary_byte_offset: Some(offset),
            },
        )
    }

    fn count_matching_lines<M: Matcher>(
        &self,
        matcher: &M,
        slice: &[u8],
        with_matches: bool,
        mode: parallel::Counting,
    ) -> Result<Option<(u64, u64)>, String> {
        let line_term = self.config.line_term;
        let invert = self.config.invert_match();
        let spec = parallel::CountSpec {
            buf: slice,
            line_term,
            with_matches: with_matches && !invert,
            with_lines: invert,
            mode,
        };
        let out = match matcher
            .parallel()
            .filter(|_| self.can_parallelize(slice.len()))
        {
            Some(finder) => {
                let chunks = parallel::split_chunks(
                    slice,
                    line_term.as_byte(),
                    self.chunk_size(slice.len()),
                );
                parallel::count_parallel(spec, finder, &chunks)?
            }
            None => parallel::count_sequential(spec, &SeqCounter(matcher))?,
        };
        Ok(out.map(|out| {
            if invert {
                let lines = out.lines - out.matched;
                (lines, lines)
            } else {
                (out.matched, out.matches)
            }
        }))
    }

    fn is_line_by_line_fast_with<M: Matcher>(&self, matcher: &M) -> bool {
        if let Some(line_term) = matcher.line_terminator() {
            if line_term.as_byte() == b'\x00' {
                return false;
            }
            if line_term == self.config.line_term {
                return true;
            }
        }
        matcher
            .non_matching_bytes()
            .is_some_and(|nm| nm.contains(self.config.line_term.as_byte()))
    }

    fn check_config<M: Matcher>(&self, matcher: M) -> Result<(), ConfigError> {
        if self.config.heap_limit == Some(0) && !self.config.mmap.is_enabled() {
            return Err(ConfigError::SearchUnavailable);
        }
        let Some(matcher_line_term) = matcher.line_terminator() else {
            return Ok(());
        };
        if matcher_line_term != self.config.line_term {
            return Err(ConfigError::MismatchedLineTerminators {
                matcher: matcher_line_term,
                searcher: self.config.line_term,
            });
        }
        Ok(())
    }

    fn slice_needs_transcoding(&self, slice: &[u8]) -> bool {
        self.config.transcoder.is_some() || (self.config.bom_sniffing() && has_bom(slice))
    }

    fn fill_multi_line_buffer_from_reader<R: io::Read, S: Sink>(
        &self,
        mut read_from: R,
    ) -> Result<(), S::Error> {
        let mut buf = self.multi_line_buffer.borrow_mut();
        buf.clear();
        let Some(heap_limit) = self.config.heap_limit else {
            read_from
                .read_to_end(&mut buf)
                .map_err(S::Error::error_io)?;
            return Ok(());
        };
        if heap_limit == 0 {
            return Err(S::Error::error_io(alloc_error(heap_limit)));
        }
        buf.resize(cmp::min(DEFAULT_BUFFER_CAPACITY, heap_limit), 0);
        let mut pos = 0;
        loop {
            let nread = match read_from.read(&mut buf[pos..]) {
                Ok(nread) => nread,
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(S::Error::error_io(err)),
            };
            if nread == 0 {
                buf.resize(pos, 0);
                return Ok(());
            }
            pos += nread;
            if buf[pos..].is_empty() {
                let additional = heap_limit - buf.len();
                if additional == 0 {
                    return Err(S::Error::error_io(alloc_error(heap_limit)));
                }
                let limit = buf.len() + additional;
                let doubled = 2 * buf.len();
                buf.resize(cmp::min(doubled, limit), 0);
            }
        }
    }
}

fn decoding_reader<R: io::Read, E: SinkError>(
    config: &Config,
    mut rdr: R,
) -> Result<BomReader<R>, E> {
    let mut peek = [0u8; 3];
    let mut n = 0;
    while n < 3 {
        match rdr.read(&mut peek[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(E::error_io(e)),
        }
    }
    let peeked = &peek[..n];
    if config.bom_sniffing()
        && n >= 2
        && (peeked.starts_with(b"\xFF\xFE") || peeked.starts_with(b"\xFE\xFF"))
    {
        if n < 3 {
            return Ok(BomReader::Pass {
                rdr,
                head: vec![],
                head_pos: 0,
            });
        }
        let little = peeked[0] == 0xFF;
        let mut rest = vec![peeked[2]];
        rdr.read_to_end(&mut rest).map_err(E::error_io)?;
        return Ok(BomReader::Decoded {
            data: decode_utf16(&rest, little),
            pos: 0,
        });
    }
    let head = if config.bom_sniffing() && peeked == b"\xEF\xBB\xBF" {
        vec![]
    } else {
        peeked.to_vec()
    };
    Ok(BomReader::Pass {
        rdr,
        head,
        head_pos: 0,
    })
}

enum BomReader<R> {
    Pass {
        rdr: R,
        head: Vec<u8>,
        head_pos: usize,
    },
    Decoded {
        data: Vec<u8>,
        pos: usize,
    },
}

impl<R: io::Read> Read for BomReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            BomReader::Pass {
                rdr,
                head,
                head_pos,
            } => {
                if *head_pos < head.len() {
                    let n = buf.len().min(head.len() - *head_pos);
                    buf[..n].copy_from_slice(&head[*head_pos..*head_pos + n]);
                    *head_pos += n;
                    return Ok(n);
                }
                rdr.read(buf)
            }
            BomReader::Decoded { data, pos } => {
                let n = buf.len().min(data.len() - *pos);
                buf[..n].copy_from_slice(&data[*pos..*pos + n]);
                *pos += n;
                Ok(n)
            }
        }
    }
}

impl Searcher {
    #[inline]
    #[must_use]
    pub fn line_terminator(&self) -> LineTerminator {
        self.config.line_term
    }

    #[inline]
    #[must_use]
    pub fn binary_detection(&self) -> &BinaryDetection {
        &self.config.binary
    }

    #[inline]
    #[must_use]
    pub fn invert_match(&self) -> bool {
        self.config.invert_match()
    }

    #[inline]
    #[must_use]
    pub fn multi_line(&self) -> bool {
        self.config.multi_line()
    }

    pub fn multi_line_with_matcher<M: Matcher>(&self, matcher: M) -> bool {
        if !self.multi_line() {
            return false;
        }
        if let Some(line_term) = matcher.line_terminator()
            && line_term == self.line_terminator()
        {
            return false;
        }
        if let Some(non_matching) = matcher.non_matching_bytes()
            && non_matching.contains(self.line_terminator().as_byte())
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{KitchenSink, RegexMatcher};
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn config_error_heap_limit() {
        let matcher = RegexMatcher::new("");
        let sink = KitchenSink::new();
        let mut searcher = SearcherBuilder::new().heap_limit(Some(0)).build();
        let res = searcher.search_slice(matcher, &[], sink);
        assert!(res.is_err());
    }

    #[test]
    fn config_error_line_terminator() {
        let mut matcher = RegexMatcher::new("");
        matcher.set_line_term(Some(LineTerminator::byte(b'z')));
        let sink = KitchenSink::new();
        let mut searcher = Searcher::new();
        let res = searcher.search_slice(matcher, &[], sink);
        assert!(res.is_err());
    }

    #[test]
    fn transcoder_hook() {
        let matcher = RegexMatcher::new("needle");
        let upper: Arc<TranscodeFn> = Arc::new(|raw: &[u8]| {
            if raw.starts_with(b"ENC:") {
                Some(
                    raw[4..]
                        .iter()
                        .map(|b| if *b == b'#' { b'e' } else { *b })
                        .collect(),
                )
            } else {
                None
            }
        });
        let mut builder = SearcherBuilder::new();
        builder.transcoder(Some(upper));
        let run = |haystack: &[u8]| {
            let mut sink = KitchenSink::new();
            builder
                .build()
                .search_slice(&matcher, haystack, &mut sink)
                .unwrap();
            String::from_utf8(sink.as_bytes().to_vec()).unwrap()
        };
        assert_eq!(run(b"ENC:x\nn##dl#\n"), "2:2:needle\n\nbyte count:9\n");
        assert_eq!(
            run(b"plain needle\n"),
            "1:0:plain needle\n\nbyte count:13\n"
        );
        assert_eq!(run(b"\xEF\xBB\xBFneedle\n"), "1:0:needle\n\nbyte count:7\n");
        let mut sink = KitchenSink::new();
        builder
            .build()
            .search_reader(&matcher, &b"ENC:n##dl#"[..], &mut sink)
            .unwrap();
        assert_eq!(sink.as_bytes(), b"1:0:needle\nbyte count:6\n");
    }

    #[test]
    fn uft8_bom_sniffing() {
        let matcher = RegexMatcher::new("foo");
        let haystack: &[u8] = &[0xef, 0xbb, 0xbf, 0x66, 0x6f, 0x6f];
        let mut sink = KitchenSink::new();
        let mut searcher = SearcherBuilder::new().build();
        let res = searcher.search_slice(matcher, haystack, &mut sink);
        assert!(res.is_ok());
        let sink_output = String::from_utf8(sink.as_bytes().to_vec()).unwrap();
        assert_eq!(sink_output, "1:0:foo\nbyte count:3\n");
    }

    fn big_haystack() -> String {
        let mut haystack = String::new();
        for i in 0..20_000 {
            let _ = writeln!(
                haystack,
                "line {i} {}",
                if i % 97 == 0 { "needle" } else { "hay" }
            );
        }
        haystack
    }

    fn run(builder: &SearcherBuilder, matcher: &RegexMatcher, haystack: &str) -> String {
        let mut sink = KitchenSink::new();
        builder
            .build()
            .search_slice(matcher, haystack.as_bytes(), &mut sink)
            .unwrap();
        String::from_utf8(sink.as_bytes().to_vec()).unwrap()
    }

    struct CountSink {
        bulk: bool,
        count: u64,
        bytes: u64,
    }

    impl Sink for CountSink {
        type Error = std::io::Error;

        fn matched(&mut self, _: &Searcher, _: &SinkMatch<'_>) -> Result<bool, std::io::Error> {
            self.count += 1;
            Ok(true)
        }

        fn count_mode(&self, _: &Searcher) -> SinkCount {
            if self.bulk {
                SinkCount::Lines
            } else {
                SinkCount::None
            }
        }

        fn matched_count(
            &mut self,
            _: &Searcher,
            count: u64,
            _: u64,
        ) -> Result<bool, std::io::Error> {
            self.count += count;
            Ok(true)
        }

        fn finish(&mut self, _: &Searcher, finish: &SinkFinish) -> Result<(), std::io::Error> {
            self.bytes = finish.byte_count();
            Ok(())
        }
    }

    #[test]
    fn bulk_count_matches_per_line_count() {
        let haystack = format!("{}tail without newline needle", big_haystack());
        for candidates in [false, true] {
            let mut matcher = RegexMatcher::new("needle|^$|line 1999[0-9]");
            matcher.set_line_term(Some(LineTerminator::byte(b'\n')));
            matcher.every_line_is_candidate(candidates);
            for invert in [false, true] {
                for parallel in [false, true] {
                    let mut results = vec![];
                    for bulk in [false, true] {
                        let mut sink = CountSink {
                            bulk,
                            count: 0,
                            bytes: 0,
                        };
                        SearcherBuilder::new()
                            .invert_match(invert)
                            .parallel(parallel)
                            .parallel_threshold(1000)
                            .build()
                            .search_slice(&matcher, haystack.as_bytes(), &mut sink)
                            .unwrap();
                        results.push((sink.count, sink.bytes));
                    }
                    assert_eq!(results[0], results[1], "{candidates} {invert} {parallel}");
                }
            }
        }
    }

    #[derive(Default)]
    struct Trace(String);

    impl Sink for Trace {
        type Error = std::io::Error;

        fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, std::io::Error> {
            let _ = writeln!(
                self.0,
                "matched {:?} {}",
                mat.line_number(),
                mat.absolute_byte_offset()
            );
            Ok(true)
        }

        fn context(&mut self, _: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, std::io::Error> {
            let _ = writeln!(
                self.0,
                "context {:?} {:?} {}",
                ctx.kind(),
                ctx.line_number(),
                ctx.absolute_byte_offset()
            );
            Ok(true)
        }

        fn context_break(&mut self, _: &Searcher) -> Result<bool, std::io::Error> {
            self.0.push_str("break\n");
            Ok(true)
        }

        fn binary_data(&mut self, _: &Searcher, offset: u64) -> Result<bool, std::io::Error> {
            let _ = writeln!(self.0, "binary {offset}");
            Ok(true)
        }

        fn begin(&mut self, _: &Searcher) -> Result<bool, std::io::Error> {
            self.0.push_str("begin\n");
            Ok(true)
        }

        fn finish(&mut self, _: &Searcher, finish: &SinkFinish) -> Result<(), std::io::Error> {
            let _ = writeln!(
                self.0,
                "finish {} {:?}",
                finish.byte_count(),
                finish.binary_byte_offset()
            );
            Ok(())
        }
    }

    fn guarded_haystack(nul: Option<usize>, long: Option<usize>) -> Vec<u8> {
        let mut data = Vec::new();
        let mut i = 0u32;
        while data.len() < 3 << 20 {
            let word = if i.is_multiple_of(89) {
                "needle"
            } else {
                "hay"
            };
            data.extend_from_slice(format!("line {i} {word}\n").as_bytes());
            i += 1;
        }
        if let Some(at) = long {
            for b in &mut data[at..at + 40_000] {
                if *b == b'\n' {
                    *b = b'x';
                }
            }
        }
        if let Some(at) = nul {
            data[at] = 0;
        }
        data
    }

    #[test]
    fn guarded_search_matches_reader() {
        let nuls = [
            None,
            Some(100),
            Some(20_000),
            Some(40_000),
            Some(65_000),
            Some(70_000),
            Some(99_000),
            Some(130_000),
            Some((1 << 20) - 10),
            Some((1 << 20) + 70_000),
            Some(2_097_160),
            Some(2_150_000),
            Some(2_500_000),
            Some((3 << 20) - 5),
        ];
        for nul in nuls {
            for long in [None, Some(1_900_000)] {
                let data = guarded_haystack(nul, long);
                for (pattern, before, after, invert, max) in [
                    ("needle", 0, 0, false, None),
                    ("line", 0, 0, false, None),
                    ("needle", 2, 1, false, None),
                    ("hay", 0, 0, true, None),
                    ("needle", 0, 2, false, Some(30)),
                ] {
                    let mut matcher = RegexMatcher::new(pattern);
                    matcher.set_line_term(Some(LineTerminator::byte(b'\n')));
                    for detection in [BinaryDetection::quit(0), BinaryDetection::convert(0)] {
                        if pattern == "line" && detection.convert_byte().is_some() {
                            continue;
                        }
                        for parallel in [false, true] {
                            let mut builder = SearcherBuilder::new();
                            builder
                                .before_context(before)
                                .after_context(after)
                                .invert_match(invert)
                                .max_matches(max)
                                .binary_detection(detection.clone())
                                .parallel(parallel)
                                .parallel_threshold(1000);
                            let label = format!(
                                "{pattern} {nul:?} {long:?} {before} {after} {invert} {max:?} {detection:?} {parallel}"
                            );
                            let searcher = builder.build();
                            let reference = builder.heap_limit(Some(1 << 40)).build();
                            let mut got = Trace::default();
                            searcher
                                .search_reader_semantics(&matcher, &data, Some(3), &mut got)
                                .unwrap();
                            let mut want = Trace::default();
                            reference
                                .search_reader_semantics(&matcher, &data, Some(3), &mut want)
                                .unwrap();
                            assert_eq!(want.0, got.0, "{label}");
                            if max.is_none() {
                                let mut got = CountSink {
                                    bulk: true,
                                    count: 0,
                                    bytes: 0,
                                };
                                searcher
                                    .search_reader_semantics(&matcher, &data, Some(3), &mut got)
                                    .unwrap();
                                let mut want = CountSink {
                                    bulk: true,
                                    count: 0,
                                    bytes: 0,
                                };
                                reference
                                    .search_reader_semantics(&matcher, &data, Some(3), &mut want)
                                    .unwrap();
                                assert_eq!(
                                    (want.count, want.bytes),
                                    (got.count, got.bytes),
                                    "{label}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn trace(
        builder: &SearcherBuilder,
        matcher: &RegexMatcher,
        data: &[u8],
        reader: bool,
    ) -> String {
        let mut sink = Trace::default();
        if reader {
            builder
                .build()
                .search_reader_semantics(matcher, data, Some(3), &mut sink)
                .unwrap();
        } else {
            builder
                .build()
                .search_slice(matcher, data, &mut sink)
                .unwrap();
        }
        sink.0
    }

    #[test]
    fn parallel_per_line_matches_sequential() {
        let clean = big_haystack().into_bytes();
        let mut binary = clean.clone();
        let at = clean.len() * 3 / 4;
        binary[at] = 0;
        let per_line = RegexMatcher::new("needle|line 1999[0-9]");
        assert!(per_line.line_terminator().is_none());
        let mut by_candidate = per_line.clone();
        by_candidate.set_line_term(Some(LineTerminator::byte(b'\n')));
        for (matcher, data) in [
            (&per_line, &clean),
            (&per_line, &binary),
            (&by_candidate, &clean),
            (&by_candidate, &binary),
        ] {
            for (before, after, invert, max, passthru, stop) in [
                (0, 0, false, None, false, false),
                (2, 3, false, None, false, false),
                (0, 0, true, None, false, false),
                (1, 1, true, Some(50), false, false),
                (3, 0, false, Some(7), false, false),
                (0, 4, false, Some(3), false, false),
                (0, 0, false, None, true, false),
                (0, 1, false, None, false, true),
            ] {
                for line_number in [false, true] {
                    for detection in [
                        BinaryDetection::none(),
                        BinaryDetection::quit(0),
                        BinaryDetection::convert(0),
                    ] {
                        let mut builder = SearcherBuilder::new();
                        builder
                            .before_context(before)
                            .after_context(after)
                            .invert_match(invert)
                            .max_matches(max)
                            .passthru(passthru)
                            .stop_on_nonmatch(stop)
                            .line_number(line_number)
                            .binary_detection(detection.clone())
                            .parallel(false);
                        let expected = [
                            trace(&builder, matcher, data, false),
                            trace(&builder, matcher, data, true),
                        ];
                        builder.parallel(true).parallel_threshold(1000);
                        let got = [
                            trace(&builder, matcher, data, false),
                            trace(&builder, matcher, data, true),
                        ];
                        assert_eq!(
                            expected,
                            got,
                            "{before} {after} {invert} {max:?} {passthru} {stop} {line_number} {detection:?} {}",
                            data.len()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn parallel_matches_sequential() {
        let haystack = big_haystack();
        let mut matcher = RegexMatcher::new("needle|line 1999[0-9]");
        matcher.set_line_term(Some(LineTerminator::byte(b'\n')));
        for (before, after, invert, max) in [
            (0, 0, false, None),
            (2, 3, false, None),
            (0, 0, true, None),
            (1, 1, true, Some(50)),
            (3, 0, false, Some(7)),
            (0, 4, false, Some(3)),
        ] {
            for line_number in [false, true] {
                let mut builder = SearcherBuilder::new();
                builder
                    .before_context(before)
                    .after_context(after)
                    .invert_match(invert)
                    .max_matches(max)
                    .line_number(line_number)
                    .parallel(false);
                let expected = run(&builder, &matcher, &haystack);
                builder.parallel(true).parallel_threshold(1000);
                let got = run(&builder, &matcher, &haystack);
                assert_eq!(
                    expected, got,
                    "{before} {after} {invert} {max:?} {line_number}"
                );
            }
        }
    }
}
