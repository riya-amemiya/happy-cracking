use std::cell::{Cell, RefCell};
use std::cmp;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::super::Flags;
use super::super::bytes::debug_bytes;
use super::super::matcher::{Match, Matcher};
use super::super::searcher::{LineStep, Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use super::color::ColorSpecs;
use super::counter::CounterWriter;
use super::hyperlink::{self, HyperlinkConfig};
use super::stats::Stats;
use super::termcolor::{ColorSpec, WriteColor};
use super::util::{
    DecimalFormatter, PrinterPath, Replacer, Sunk, find_iter_at_in_context, grapheme_end_offsets,
    trim_ascii_prefix, trim_line_terminator,
};

const STATS: u32 = 1;
const HEADING: u32 = 1 << 1;
const PATH: u32 = 1 << 2;
const ONLY_MATCHING: u32 = 1 << 3;
const PER_MATCH: u32 = 1 << 4;
const PER_MATCH_ONE_LINE: u32 = 1 << 5;
const MAX_COLUMNS_PREVIEW: u32 = 1 << 6;
const COLUMN: u32 = 1 << 7;
const BYTE_OFFSET: u32 = 1 << 8;
const TRIM_ASCII: u32 = 1 << 9;

#[derive(Debug, Clone)]
struct Config {
    colors: ColorSpecs,
    hyperlink: HyperlinkConfig,
    flags: Flags,
    replacement: Arc<Option<Vec<u8>>>,
    max_columns: Option<u64>,
    separator_search: Arc<Option<Vec<u8>>>,
    separator_context: Arc<Option<Vec<u8>>>,
    separator_field_match: Arc<Vec<u8>>,
    separator_field_context: Arc<Vec<u8>>,
    separator_path: Option<u8>,
    path_terminator: Option<u8>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            colors: ColorSpecs::default(),
            hyperlink: HyperlinkConfig::default(),
            flags: Flags(PATH),
            replacement: Arc::new(None),
            max_columns: None,
            separator_search: Arc::new(None),
            separator_context: Arc::new(Some(b"--".to_vec())),
            separator_field_match: Arc::new(b":".to_vec()),
            separator_field_context: Arc::new(b"-".to_vec()),
            separator_path: None,
            path_terminator: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct StandardBuilder {
    config: Config,
}

impl StandardBuilder {
    #[must_use]
    pub fn new() -> StandardBuilder {
        StandardBuilder::default()
    }

    pub fn build<W: WriteColor>(&self, wtr: W) -> Standard<W> {
        Standard {
            config: self.config.clone(),
            wtr: RefCell::new(CounterWriter::new(wtr)),
            matches: vec![],
        }
    }

    pub fn color_specs(&mut self, specs: ColorSpecs) -> &mut StandardBuilder {
        self.config.colors = specs;
        self
    }

    pub fn hyperlink(&mut self, config: HyperlinkConfig) -> &mut StandardBuilder {
        self.config.hyperlink = config;
        self
    }

    pub fn stats(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(STATS, yes);
        self
    }

    pub fn heading(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(HEADING, yes);
        self
    }

    pub fn path(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(PATH, yes);
        self
    }

    pub fn only_matching(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(ONLY_MATCHING, yes);
        self
    }

    pub fn per_match(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(PER_MATCH, yes);
        self
    }

    pub fn per_match_one_line(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(PER_MATCH_ONE_LINE, yes);
        self
    }

    pub fn replacement(&mut self, replacement: Option<Vec<u8>>) -> &mut StandardBuilder {
        self.config.replacement = Arc::new(replacement);
        self
    }

    pub fn max_columns(&mut self, limit: Option<u64>) -> &mut StandardBuilder {
        self.config.max_columns = limit;
        self
    }

    pub fn max_columns_preview(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(MAX_COLUMNS_PREVIEW, yes);
        self
    }

    pub fn column(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(COLUMN, yes);
        self
    }

    pub fn byte_offset(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(BYTE_OFFSET, yes);
        self
    }

    pub fn trim_ascii(&mut self, yes: bool) -> &mut StandardBuilder {
        self.config.flags.set(TRIM_ASCII, yes);
        self
    }

    pub fn separator_search(&mut self, sep: Option<Vec<u8>>) -> &mut StandardBuilder {
        self.config.separator_search = Arc::new(sep);
        self
    }

    pub fn separator_context(&mut self, sep: Option<Vec<u8>>) -> &mut StandardBuilder {
        self.config.separator_context = Arc::new(sep);
        self
    }

    pub fn separator_field_match(&mut self, sep: Vec<u8>) -> &mut StandardBuilder {
        self.config.separator_field_match = Arc::new(sep);
        self
    }

    pub fn separator_field_context(&mut self, sep: Vec<u8>) -> &mut StandardBuilder {
        self.config.separator_field_context = Arc::new(sep);
        self
    }

    pub fn separator_path(&mut self, sep: Option<u8>) -> &mut StandardBuilder {
        self.config.separator_path = sep;
        self
    }

    pub fn path_terminator(&mut self, terminator: Option<u8>) -> &mut StandardBuilder {
        self.config.path_terminator = terminator;
        self
    }
}

#[derive(Clone, Debug)]
pub struct Standard<W> {
    config: Config,
    wtr: RefCell<CounterWriter<W>>,
    matches: Vec<Match>,
}

impl<W: WriteColor> Standard<W> {
    pub fn sink<M: Matcher>(&mut self, matcher: M) -> StandardSink<'static, '_, M, W> {
        let interpolator = hyperlink::Interpolator::new(&self.config.hyperlink);
        let stats = if self.config.flags.contains(STATS) {
            Some(Stats::new())
        } else {
            None
        };
        let needs_match_granularity = self.needs_match_granularity();
        StandardSink {
            matcher,
            standard: self,
            replacer: Replacer::new(),
            interpolator,
            path: None,
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            stats,
            needs_match_granularity,
        }
    }

    pub fn sink_with_path<'p, 's, M, P>(
        &'s mut self,
        matcher: M,
        path: &'p P,
    ) -> StandardSink<'p, 's, M, W>
    where
        M: Matcher,
        P: ?Sized + AsRef<Path>,
    {
        if !self.config.flags.contains(PATH) {
            return self.sink(matcher);
        }
        let interpolator = hyperlink::Interpolator::new(&self.config.hyperlink);
        let stats = if self.config.flags.contains(STATS) {
            Some(Stats::new())
        } else {
            None
        };
        let ppath = PrinterPath::new(path.as_ref()).with_separator(self.config.separator_path);
        let needs_match_granularity = self.needs_match_granularity();
        StandardSink {
            matcher,
            standard: self,
            replacer: Replacer::new(),
            interpolator,
            path: Some(ppath),
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            stats,
            needs_match_granularity,
        }
    }

    fn needs_match_granularity(&self) -> bool {
        let supports_color = self.wtr.borrow().supports_color();
        let match_colored = !self.config.colors.matched().is_none();
        (supports_color && match_colored)
            || self.config.flags.contains(COLUMN)
            || self.config.replacement.is_some()
            || self.config.flags.contains(PER_MATCH)
            || self.config.flags.contains(ONLY_MATCHING)
            || self.config.flags.contains(STATS)
    }
}

impl<W> Standard<W> {
    pub fn get_mut(&mut self) -> &mut W {
        self.wtr.get_mut().get_mut()
    }
}

#[derive(Debug)]
pub struct StandardSink<'p, 's, M: Matcher, W> {
    matcher: M,
    standard: &'s mut Standard<W>,
    replacer: Replacer<M>,
    interpolator: hyperlink::Interpolator,
    path: Option<PrinterPath<'p>>,
    start_time: Instant,
    match_count: u64,
    binary_byte_offset: Option<u64>,
    stats: Option<Stats>,
    needs_match_granularity: bool,
}

impl<M: Matcher, W: WriteColor> StandardSink<'_, '_, M, W> {
    pub fn has_match(&self) -> bool {
        self.match_count > 0
    }

    #[cfg(test)]
    pub fn binary_byte_offset(&self) -> Option<u64> {
        self.binary_byte_offset
    }

    pub fn stats(&self) -> Option<&Stats> {
        self.stats.as_ref()
    }

    fn record_matches(
        &mut self,
        searcher: &Searcher,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> io::Result<()> {
        self.standard.matches.clear();
        if !self.needs_match_granularity {
            return Ok(());
        }
        let matches = &mut self.standard.matches;
        find_iter_at_in_context(searcher, &self.matcher, bytes, range.clone(), |m| {
            matches.push(Match::new(m.start() - range.start, m.end() - range.start));
            true
        })?;
        if matches
            .last()
            .is_some_and(|m| m.is_empty() && m.start() >= range.end)
        {
            matches.pop();
        }
        Ok(())
    }

    fn replace(
        &mut self,
        searcher: &Searcher,
        bytes: &[u8],
        range: std::ops::Range<usize>,
    ) -> io::Result<()> {
        self.replacer.clear();
        if let Some(replacement) = self.standard.config.replacement.as_ref().as_ref() {
            self.replacer
                .replace_all(searcher, &self.matcher, bytes, range, replacement)?;
        }
        Ok(())
    }
}

impl<M: Matcher, W: WriteColor> Sink for StandardSink<'_, '_, M, W> {
    type Error = io::Error;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        self.match_count += 1;
        self.record_matches(searcher, mat.buffer(), mat.bytes_range_in_buffer())?;
        self.replace(searcher, mat.buffer(), mat.bytes_range_in_buffer())?;
        if let Some(ref mut stats) = self.stats {
            stats.add_matches(self.standard.matches.len() as u64);
            stats.add_matched_lines(mat.lines().count() as u64);
        }
        if searcher.binary_detection().convert_byte().is_some() && self.binary_byte_offset.is_some()
        {
            return Ok(false);
        }
        StandardImpl::from_match(searcher, self, mat).sink()?;
        Ok(true)
    }

    fn context(&mut self, searcher: &Searcher, ctx: &SinkContext<'_>) -> Result<bool, io::Error> {
        self.standard.matches.clear();
        self.replacer.clear();
        if searcher.invert_match() {
            self.record_matches(searcher, ctx.bytes(), 0..ctx.bytes().len())?;
            self.replace(searcher, ctx.bytes(), 0..ctx.bytes().len())?;
        }
        if searcher.binary_detection().convert_byte().is_some() && self.binary_byte_offset.is_some()
        {
            return Ok(false);
        }
        StandardImpl::from_context(searcher, self, ctx).sink()?;
        Ok(true)
    }

    fn context_break(&mut self, searcher: &Searcher) -> Result<bool, io::Error> {
        StandardImpl::new(searcher, self).write_context_separator()?;
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        binary_byte_offset: u64,
    ) -> Result<bool, io::Error> {
        self.binary_byte_offset = Some(binary_byte_offset);
        Ok(true)
    }

    fn begin(&mut self, _searcher: &Searcher) -> Result<bool, io::Error> {
        self.standard.wtr.borrow_mut().reset_count();
        self.start_time = Instant::now();
        self.match_count = 0;
        self.binary_byte_offset = None;
        Ok(true)
    }

    fn finish(&mut self, searcher: &Searcher, finish: &SinkFinish) -> Result<(), io::Error> {
        if let Some(offset) = self.binary_byte_offset {
            StandardImpl::new(searcher, self).write_binary_message(offset)?;
        }
        if let Some(stats) = self.stats.as_mut() {
            stats.add_elapsed(self.start_time.elapsed());
            stats.add_searches(1);
            if self.match_count > 0 {
                stats.add_searches_with_match(1);
            }
            stats.add_bytes_searched(finish.byte_count());
            stats.add_bytes_printed(self.standard.wtr.borrow().count());
        }
        Ok(())
    }
}

struct StandardImpl<'a, M: Matcher, W> {
    searcher: &'a Searcher,
    sink: &'a StandardSink<'a, 'a, M, W>,
    sunk: Sunk<'a>,
    in_color_match: Cell<bool>,
}

impl<'a, M: Matcher, W: WriteColor> StandardImpl<'a, M, W> {
    fn new(searcher: &'a Searcher, sink: &'a StandardSink<'_, '_, M, W>) -> StandardImpl<'a, M, W> {
        StandardImpl {
            searcher,
            sink,
            sunk: Sunk::empty(),
            in_color_match: Cell::new(false),
        }
    }

    fn from_match(
        searcher: &'a Searcher,
        sink: &'a StandardSink<'_, '_, M, W>,
        mat: &'a SinkMatch<'a>,
    ) -> StandardImpl<'a, M, W> {
        StandardImpl {
            sunk: Sunk::from_sink_match(mat, &sink.standard.matches, sink.replacer.replacement()),
            ..StandardImpl::new(searcher, sink)
        }
    }

    fn from_context(
        searcher: &'a Searcher,
        sink: &'a StandardSink<'_, '_, M, W>,
        ctx: &'a SinkContext<'a>,
    ) -> StandardImpl<'a, M, W> {
        StandardImpl {
            sunk: Sunk::from_sink_context(ctx, &sink.standard.matches, sink.replacer.replacement()),
            ..StandardImpl::new(searcher, sink)
        }
    }

    fn sink(&self) -> io::Result<()> {
        self.write_search_prelude()?;
        let multi = self.multi_line() && !self.is_context();
        match (self.sunk.matches().is_empty(), multi) {
            (true, true) => self.sink_fast_multi_line(),
            (true, false) => self.sink_fast(),
            (false, true) => self.sink_slow_multi_line(),
            (false, false) => self.sink_slow(),
        }
    }

    fn sink_fast(&self) -> io::Result<()> {
        self.write_prelude(
            self.sunk.absolute_byte_offset(),
            self.sunk.line_number(),
            None,
        )?;
        self.write_line(self.sunk.bytes())
    }

    fn sink_fast_multi_line(&self) -> io::Result<()> {
        let line_term = self.searcher.line_terminator().as_byte();
        let mut absolute_byte_offset = self.sunk.absolute_byte_offset();
        for (i, line) in self.sunk.lines(line_term).enumerate() {
            self.write_prelude(
                absolute_byte_offset,
                self.sunk.line_number().map(|n| n + i as u64),
                None,
            )?;
            absolute_byte_offset += line.len() as u64;
            self.write_line(line)?;
        }
        Ok(())
    }

    fn sink_slow(&self) -> io::Result<()> {
        if self.config().flags.contains(ONLY_MATCHING) {
            for &m in self.sunk.matches() {
                self.write_prelude(
                    self.sunk.absolute_byte_offset() + m.start() as u64,
                    self.sunk.line_number(),
                    Some(m.start() as u64 + 1),
                )?;
                let buf = &self.sunk.bytes()[m];
                self.write_colored_line(&[Match::new(0, buf.len())], buf)?;
            }
        } else if self.config().flags.contains(PER_MATCH) {
            for &m in self.sunk.matches() {
                self.write_prelude(
                    self.sunk.absolute_byte_offset() + m.start() as u64,
                    self.sunk.line_number(),
                    Some(m.start() as u64 + 1),
                )?;
                self.write_colored_line(&[m], self.sunk.bytes())?;
            }
        } else {
            self.write_prelude(
                self.sunk.absolute_byte_offset(),
                self.sunk.line_number(),
                Some(self.sunk.matches()[0].start() as u64 + 1),
            )?;
            self.write_colored_line(self.sunk.matches(), self.sunk.bytes())?;
        }
        Ok(())
    }

    fn sink_slow_multi_line(&self) -> io::Result<()> {
        if self.config().flags.contains(ONLY_MATCHING) {
            return self.sink_slow_multi_line_only_matching();
        } else if self.config().flags.contains(PER_MATCH) {
            return self.sink_slow_multi_per_match();
        }
        let line_term = self.searcher.line_terminator().as_byte();
        let bytes = self.sunk.bytes();
        let matches = self.sunk.matches();
        let mut midx = 0;
        let mut count = 0;
        let mut stepper = LineStep::new(line_term, 0, bytes.len());
        while let Some((start, end)) = stepper.next(bytes) {
            let mut line = Match::new(start, end);
            self.write_prelude(
                self.sunk.absolute_byte_offset() + line.start() as u64,
                self.sunk.line_number().map(|n| n + count),
                Some(matches[0].start() as u64 + 1),
            )?;
            count += 1;
            self.trim_ascii_prefix(bytes, &mut line);
            if self.exceeds_max_columns(&bytes[line]) {
                self.write_exceeded_line(bytes, line, matches, &mut midx)?;
            } else {
                self.write_colored_matches(bytes, line, matches, &mut midx)?;
                self.write_line_term()?;
            }
        }
        Ok(())
    }

    fn sink_slow_multi_line_only_matching(&self) -> io::Result<()> {
        let line_term = self.searcher.line_terminator().as_byte();
        let spec = self.config().colors.matched();
        let bytes = self.sunk.bytes();
        let matches = self.sunk.matches();
        let mut midx = 0;
        let mut count = 0;
        let mut stepper = LineStep::new(line_term, 0, bytes.len());
        while let Some((start, end)) = stepper.next(bytes) {
            let mut line = Match::new(start, end);
            self.trim_line_terminator(bytes, &mut line);
            self.trim_ascii_prefix(bytes, &mut line);
            while !line.is_empty() {
                if matches[midx].end() <= line.start() {
                    if midx + 1 < matches.len() {
                        midx += 1;
                        continue;
                    }
                    break;
                }
                let m = matches[midx];
                if line.start() < m.start() {
                    let upto = cmp::min(line.end(), m.start());
                    line = line.with_start(upto);
                } else {
                    let upto = cmp::min(line.end(), m.end());
                    self.write_prelude(
                        self.sunk.absolute_byte_offset() + m.start() as u64,
                        self.sunk.line_number().map(|n| n + count),
                        Some(m.start() as u64 + 1),
                    )?;
                    let this_line = line.with_end(upto);
                    line = line.with_start(upto);
                    if self.exceeds_max_columns(&bytes[this_line]) {
                        self.write_exceeded_line(bytes, this_line, matches, &mut midx)?;
                    } else {
                        self.write_spec(spec, &bytes[this_line])?;
                        self.write_line_term()?;
                    }
                }
            }
            count += 1;
        }
        Ok(())
    }

    fn sink_slow_multi_per_match(&self) -> io::Result<()> {
        let line_term = self.searcher.line_terminator().as_byte();
        let spec = self.config().colors.matched();
        let bytes = self.sunk.bytes();
        for &m in self.sunk.matches() {
            let mut count = 0;
            let mut stepper = LineStep::new(line_term, 0, bytes.len());
            while let Some((start, end)) = stepper.next(bytes) {
                let mut line = Match::new(start, end);
                if line.start() >= m.end() {
                    break;
                } else if line.end() <= m.start() {
                    count += 1;
                    continue;
                }
                self.write_prelude(
                    self.sunk.absolute_byte_offset() + line.start() as u64,
                    self.sunk.line_number().map(|n| n + count),
                    Some(m.start().saturating_sub(line.start()) as u64 + 1),
                )?;
                count += 1;
                self.trim_line_terminator(bytes, &mut line);
                self.trim_ascii_prefix(bytes, &mut line);
                if self.exceeds_max_columns(&bytes[line]) {
                    self.write_exceeded_line(bytes, line, &[m], &mut 0)?;
                    continue;
                }
                while !line.is_empty() {
                    if m.end() <= line.start() {
                        self.write(&bytes[line])?;
                        line = line.with_start(line.end());
                    } else if line.start() < m.start() {
                        let upto = cmp::min(line.end(), m.start());
                        self.write(&bytes[line.with_end(upto)])?;
                        line = line.with_start(upto);
                    } else {
                        let upto = cmp::min(line.end(), m.end());
                        self.write_spec(spec, &bytes[line.with_end(upto)])?;
                        line = line.with_start(upto);
                    }
                }
                self.write_line_term()?;
                if self.config().flags.contains(PER_MATCH_ONE_LINE) {
                    break;
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn write_prelude(
        &self,
        absolute_byte_offset: u64,
        line_number: Option<u64>,
        column: Option<u64>,
    ) -> io::Result<()> {
        let mut prelude = PreludeWriter::new(self);
        prelude.start(line_number, column)?;
        prelude.write_path()?;
        prelude.write_line_number(line_number)?;
        prelude.write_column_number(column)?;
        prelude.write_byte_offset(absolute_byte_offset)?;
        prelude.end()
    }

    #[inline]
    fn write_line(&self, line: &[u8]) -> io::Result<()> {
        let line = if self.config().flags.contains(TRIM_ASCII) {
            let lineterm = self.searcher.line_terminator();
            let range = trim_ascii_prefix(lineterm, line, Match::new(0, line.len()));
            &line[range]
        } else {
            line
        };
        if self.exceeds_max_columns(line) {
            let range = Match::new(0, line.len());
            self.write_exceeded_line(line, range, self.sunk.matches(), &mut 0)?;
        } else {
            self.write(line)?;
            if !self.has_line_terminator(line) {
                self.write_line_term()?;
            }
        }
        Ok(())
    }

    fn write_colored_line(&self, matches: &[Match], bytes: &[u8]) -> io::Result<()> {
        let spec = self.config().colors.matched();
        if !self.wtr().borrow().supports_color() || spec.is_none() {
            return self.write_line(bytes);
        }
        let mut line = Match::new(0, bytes.len());
        self.trim_ascii_prefix(bytes, &mut line);
        if self.exceeds_max_columns(bytes) {
            self.write_exceeded_line(bytes, line, matches, &mut 0)
        } else {
            self.write_colored_matches(bytes, line, matches, &mut 0)?;
            self.write_line_term()
        }
    }

    fn write_colored_matches(
        &self,
        bytes: &[u8],
        mut line: Match,
        matches: &[Match],
        match_index: &mut usize,
    ) -> io::Result<()> {
        self.trim_line_terminator(bytes, &mut line);
        if matches.is_empty() {
            self.write(&bytes[line])?;
            return Ok(());
        }
        self.start_line_highlight()?;
        while !line.is_empty() {
            if matches[*match_index].end() <= line.start() {
                if *match_index + 1 < matches.len() {
                    *match_index += 1;
                    continue;
                }
                self.end_color_match()?;
                self.write(&bytes[line])?;
                break;
            }
            let m = matches[*match_index];
            if line.start() < m.start() {
                let upto = cmp::min(line.end(), m.start());
                self.end_color_match()?;
                self.write(&bytes[line.with_end(upto)])?;
                line = line.with_start(upto);
            } else {
                let upto = cmp::min(line.end(), m.end());
                self.start_color_match()?;
                self.write(&bytes[line.with_end(upto)])?;
                line = line.with_start(upto);
            }
        }
        self.end_color_match()?;
        self.end_line_highlight()?;
        Ok(())
    }

    fn write_exceeded_line(
        &self,
        bytes: &[u8],
        mut line: Match,
        matches: &[Match],
        match_index: &mut usize,
    ) -> io::Result<()> {
        if self.config().flags.contains(MAX_COLUMNS_PREVIEW) {
            let original = line;
            let limit =
                usize::try_from(self.config().max_columns.unwrap_or(0)).unwrap_or(usize::MAX);
            let end = grapheme_end_offsets(&bytes[line], limit).unwrap_or(0) + line.start();
            line = line.with_end(end);
            self.write_colored_matches(bytes, line, matches, match_index)?;
            if matches.is_empty() {
                self.write(b" [... omitted end of long line]")?;
            } else {
                let remaining = matches
                    .iter()
                    .filter(|m| m.start() >= line.end() && m.start() < original.end())
                    .count();
                let tense = if remaining == 1 { "match" } else { "matches" };
                write!(self.wtr().borrow_mut(), " [... {remaining} more {tense}]")?;
            }
            self.write_line_term()?;
            return Ok(());
        }
        if self.sunk.original_matches().is_empty() || self.config().flags.contains(ONLY_MATCHING) {
            if self.is_context() {
                self.write(b"[Omitted long context line]")?;
            } else {
                self.write(b"[Omitted long matching line]")?;
            }
        } else {
            write!(
                self.wtr().borrow_mut(),
                "[Omitted long line with {} matches]",
                self.sunk.original_matches().len()
            )?;
        }
        self.write_line_term()?;
        Ok(())
    }

    fn write_path_line(&self) -> io::Result<()> {
        if let Some(path) = self.path() {
            self.write_path_hyperlink(path)?;
            if let Some(term) = self.config().path_terminator {
                self.write(&[term])?;
            } else {
                self.write_line_term()?;
            }
        }
        Ok(())
    }

    fn write_search_prelude(&self) -> io::Result<()> {
        let this_search_written = self.wtr().borrow().count() > 0;
        if this_search_written {
            return Ok(());
        }
        if let Some(ref sep) = *self.config().separator_search {
            let ever_written = self.wtr().borrow().total_count() > 0;
            if ever_written {
                self.write(sep)?;
                self.write_line_term()?;
            }
        }
        if self.config().flags.contains(HEADING) {
            self.write_path_line()?;
        }
        Ok(())
    }

    fn write_binary_message(&self, offset: u64) -> io::Result<()> {
        if !self.sink.has_match() {
            return Ok(());
        }
        let bin = self.searcher.binary_detection();
        if let Some(byte) = bin.quit_byte() {
            if let Some(path) = self.path() {
                self.write_path_hyperlink(path)?;
                self.write(b": ")?;
            }
            let remainder = format!(
                "WARNING: stopped searching binary file after match (found {} byte around offset {offset})\n",
                debug_bytes(&[byte]),
            );
            self.write(remainder.as_bytes())?;
        } else if let Some(byte) = bin.convert_byte() {
            if let Some(path) = self.path() {
                self.write_path_hyperlink(path)?;
                self.write(b": ")?;
            }
            let remainder = format!(
                "binary file matches (found {} byte around offset {offset})\n",
                debug_bytes(&[byte]),
            );
            self.write(remainder.as_bytes())?;
        }
        Ok(())
    }

    fn write_context_separator(&self) -> io::Result<()> {
        if let Some(ref sep) = *self.config().separator_context {
            self.write(sep)?;
            self.write_line_term()?;
        }
        Ok(())
    }

    fn write_line_term(&self) -> io::Result<()> {
        self.write(self.searcher.line_terminator().as_bytes())
    }

    fn write_spec(&self, spec: &ColorSpec, buf: &[u8]) -> io::Result<()> {
        let mut wtr = self.wtr().borrow_mut();
        wtr.set_color(spec)?;
        wtr.write_all(buf)?;
        wtr.reset()?;
        Ok(())
    }

    fn write_path(&self, path: &PrinterPath<'_>) -> io::Result<()> {
        let mut wtr = self.wtr().borrow_mut();
        wtr.set_color(self.config().colors.path())?;
        wtr.write_all(path.as_bytes())?;
        wtr.reset()
    }

    fn write_path_hyperlink(&self, path: &PrinterPath<'_>) -> io::Result<()> {
        let status = self.start_hyperlink(path, None, None)?;
        self.write_path(path)?;
        self.end_hyperlink(status)
    }

    fn start_hyperlink(
        &self,
        path: &PrinterPath<'_>,
        line_number: Option<u64>,
        column: Option<u64>,
    ) -> io::Result<hyperlink::InterpolatorStatus> {
        if !self.sink.interpolator.enabled(&*self.wtr().borrow()) {
            return Ok(hyperlink::InterpolatorStatus::inactive());
        }
        let Some(hyperpath) = path.as_hyperlink() else {
            return Ok(hyperlink::InterpolatorStatus::inactive());
        };
        let values = hyperlink::Values::new(hyperpath)
            .line(line_number)
            .column(column);
        self.sink
            .interpolator
            .begin(&values, &mut *self.wtr().borrow_mut())
    }

    fn end_hyperlink(&self, status: hyperlink::InterpolatorStatus) -> io::Result<()> {
        status.finish(&mut *self.wtr().borrow_mut())
    }

    fn start_color_match(&self) -> io::Result<()> {
        if self.in_color_match.get() {
            return Ok(());
        }
        self.wtr()
            .borrow_mut()
            .set_color(self.config().colors.matched())?;
        self.in_color_match.set(true);
        Ok(())
    }

    fn end_color_match(&self) -> io::Result<()> {
        if !self.in_color_match.get() {
            return Ok(());
        }
        if self.highlight_on() {
            self.wtr()
                .borrow_mut()
                .set_color(self.config().colors.highlight())?;
        } else {
            self.wtr().borrow_mut().reset()?;
        }
        self.in_color_match.set(false);
        Ok(())
    }

    fn highlight_on(&self) -> bool {
        !self.config().colors.highlight().is_none() && !self.is_context()
    }

    fn start_line_highlight(&self) -> io::Result<()> {
        if self.highlight_on() {
            self.wtr()
                .borrow_mut()
                .set_color(self.config().colors.highlight())?;
        }
        Ok(())
    }

    fn end_line_highlight(&self) -> io::Result<()> {
        if self.highlight_on() {
            self.wtr().borrow_mut().reset()?;
        }
        Ok(())
    }

    fn write(&self, buf: &[u8]) -> io::Result<()> {
        self.wtr().borrow_mut().write_all(buf)
    }

    fn trim_line_terminator(&self, buf: &[u8], line: &mut Match) {
        trim_line_terminator(self.searcher, buf, line);
    }

    fn has_line_terminator(&self, buf: &[u8]) -> bool {
        self.searcher.line_terminator().is_suffix(buf)
    }

    fn is_context(&self) -> bool {
        self.sunk.context_kind().is_some()
    }

    fn config(&self) -> &'a Config {
        &self.sink.standard.config
    }

    fn wtr(&self) -> &'a RefCell<CounterWriter<W>> {
        &self.sink.standard.wtr
    }

    fn path(&self) -> Option<&'a PrinterPath<'a>> {
        self.sink.path.as_ref()
    }

    fn separator_field(&self) -> &[u8] {
        if self.is_context() {
            &self.config().separator_field_context
        } else {
            &self.config().separator_field_match
        }
    }

    fn exceeds_max_columns(&self, line: &[u8]) -> bool {
        self.config()
            .max_columns
            .is_some_and(|m| line.len() as u64 > m)
    }

    fn multi_line(&self) -> bool {
        self.searcher.multi_line_with_matcher(&self.sink.matcher)
    }

    fn trim_ascii_prefix(&self, slice: &[u8], range: &mut Match) {
        if !self.config().flags.contains(TRIM_ASCII) {
            return;
        }
        let lineterm = self.searcher.line_terminator();
        *range = trim_ascii_prefix(lineterm, slice, *range);
    }
}

struct PreludeWriter<'a, M: Matcher, W> {
    std: &'a StandardImpl<'a, M, W>,
    next_separator: PreludeSeparator,
    field_separator: &'a [u8],
    interp_status: hyperlink::InterpolatorStatus,
}

enum PreludeSeparator {
    None,
    FieldSeparator,
    PathTerminator,
}

impl<'a, M: Matcher, W: WriteColor> PreludeWriter<'a, M, W> {
    #[inline]
    fn new(std: &'a StandardImpl<'a, M, W>) -> PreludeWriter<'a, M, W> {
        PreludeWriter {
            std,
            next_separator: PreludeSeparator::None,
            field_separator: std.separator_field(),
            interp_status: hyperlink::InterpolatorStatus::inactive(),
        }
    }

    #[inline]
    fn start(&mut self, line_number: Option<u64>, column: Option<u64>) -> io::Result<()> {
        let Some(path) = self.std.path() else {
            return Ok(());
        };
        if self.config().hyperlink.format().is_line_dependent()
            || !self.config().flags.contains(HEADING)
        {
            self.interp_status = self.std.start_hyperlink(path, line_number, column)?;
        }
        Ok(())
    }

    #[inline]
    fn end(&mut self) -> io::Result<()> {
        self.std.end_hyperlink(std::mem::replace(
            &mut self.interp_status,
            hyperlink::InterpolatorStatus::inactive(),
        ))?;
        self.write_separator()
    }

    #[inline]
    fn write_path(&mut self) -> io::Result<()> {
        if self.config().flags.contains(HEADING) {
            return Ok(());
        }
        let Some(path) = self.std.path() else {
            return Ok(());
        };
        self.write_separator()?;
        self.std.write_path(path)?;
        self.next_separator = if self.config().path_terminator.is_some() {
            PreludeSeparator::PathTerminator
        } else {
            PreludeSeparator::FieldSeparator
        };
        Ok(())
    }

    #[inline]
    fn write_line_number(&mut self, line: Option<u64>) -> io::Result<()> {
        let Some(line_number) = line else {
            return Ok(());
        };
        self.write_separator()?;
        let n = DecimalFormatter::new(line_number);
        self.std
            .write_spec(self.config().colors.line(), n.as_bytes())?;
        self.next_separator = PreludeSeparator::FieldSeparator;
        Ok(())
    }

    #[inline]
    fn write_column_number(&mut self, column: Option<u64>) -> io::Result<()> {
        if !self.config().flags.contains(COLUMN) {
            return Ok(());
        }
        let Some(column_number) = column else {
            return Ok(());
        };
        self.write_separator()?;
        let n = DecimalFormatter::new(column_number);
        self.std
            .write_spec(self.config().colors.column(), n.as_bytes())?;
        self.next_separator = PreludeSeparator::FieldSeparator;
        Ok(())
    }

    #[inline]
    fn write_byte_offset(&mut self, offset: u64) -> io::Result<()> {
        if !self.config().flags.contains(BYTE_OFFSET) {
            return Ok(());
        }
        self.write_separator()?;
        let n = DecimalFormatter::new(offset);
        self.std
            .write_spec(self.config().colors.column(), n.as_bytes())?;
        self.next_separator = PreludeSeparator::FieldSeparator;
        Ok(())
    }

    #[inline]
    fn write_separator(&mut self) -> io::Result<()> {
        match self.next_separator {
            PreludeSeparator::None => {}
            PreludeSeparator::FieldSeparator => {
                self.std.write(self.field_separator)?;
            }
            PreludeSeparator::PathTerminator => {
                if let Some(term) = self.config().path_terminator {
                    self.std.write(&[term])?;
                }
            }
        }
        self.next_separator = PreludeSeparator::None;
        Ok(())
    }

    #[inline]
    fn config(&self) -> &Config {
        self.std.config()
    }
}
#[cfg(test)]
mod tests {
    use super::super::super::matcher::LineTerminator;
    use super::super::super::regex::{RegexMatcher, RegexMatcherBuilder};
    use super::super::super::searcher::SearcherBuilder;
    use super::super::termcolor::{Ansi, NoColor};

    use super::{ColorSpecs, Standard, StandardBuilder};

    const SHERLOCK: &str = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.\
";

    fn printer_contents(printer: &mut Standard<NoColor<Vec<u8>>>) -> String {
        String::from_utf8(printer.get_mut().get_ref().to_owned()).unwrap()
    }

    fn printer_contents_ansi(printer: &mut Standard<Ansi<Vec<u8>>>) -> String {
        String::from_utf8(printer.get_mut().get_ref().to_owned()).unwrap()
    }

    #[test]
    fn reports_match() {
        let matcher = RegexMatcher::new("Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        let mut sink = printer.sink(&matcher);
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
            .unwrap();
        assert!(sink.has_match());

        let matcher = RegexMatcher::new("zzzzz").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        let mut sink = printer.sink(&matcher);
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
            .unwrap();
        assert!(!sink.has_match());
    }

    #[test]
    fn reports_binary() {
        use super::super::super::searcher::BinaryDetection;

        let matcher = RegexMatcher::new("Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        let mut sink = printer.sink(&matcher);
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
            .unwrap();
        assert!(sink.binary_byte_offset().is_none());

        let matcher = RegexMatcher::new(".+").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        let mut sink = printer.sink(&matcher);
        SearcherBuilder::new()
            .line_number(false)
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .build()
            .search_reader(&matcher, &b"abc\x00"[..], &mut sink)
            .unwrap();
        assert_eq!(sink.binary_byte_offset(), Some(3));
    }

    #[test]
    fn reports_stats() {
        use std::time::Duration;

        let matcher = RegexMatcher::new("Sherlock|opposed").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .build(NoColor::new(vec![]));
        let stats = {
            let mut sink = printer.sink(&matcher);
            SearcherBuilder::new()
                .line_number(false)
                .build()
                .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
                .unwrap();
            sink.stats().unwrap().clone()
        };
        let buf = printer_contents(&mut printer);

        assert!(stats.elapsed() > Duration::default());
        assert_eq!(stats.searches(), 1);
        assert_eq!(stats.searches_with_match(), 1);
        assert_eq!(stats.bytes_searched(), SHERLOCK.len() as u64);
        assert_eq!(stats.bytes_printed(), buf.len() as u64);
        assert_eq!(stats.matched_lines(), 2);
        assert_eq!(stats.matches(), 3);
    }

    #[test]
    fn reports_stats_multiple() {
        use std::time::Duration;

        let matcher = RegexMatcher::new("Sherlock|opposed").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .build(NoColor::new(vec![]));
        let stats = {
            let mut sink = printer.sink(&matcher);
            SearcherBuilder::new()
                .line_number(false)
                .build()
                .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
                .unwrap();
            SearcherBuilder::new()
                .line_number(false)
                .build()
                .search_reader(&matcher, &b"zzzzzzzzzz"[..], &mut sink)
                .unwrap();
            SearcherBuilder::new()
                .line_number(false)
                .build()
                .search_reader(&matcher, SHERLOCK.as_bytes(), &mut sink)
                .unwrap();
            sink.stats().unwrap().clone()
        };
        let buf = printer_contents(&mut printer);

        assert!(stats.elapsed() > Duration::default());
        assert_eq!(stats.searches(), 3);
        assert_eq!(stats.searches_with_match(), 2);
        assert_eq!(stats.bytes_searched(), 10 + 2 * SHERLOCK.len() as u64);
        assert_eq!(stats.bytes_printed(), buf.len() as u64);
        assert_eq!(stats.matched_lines(), 4);
        assert_eq!(stats.matches(), 6);
    }

    #[test]
    fn context_break() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .separator_context(Some(b"--abc--".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--abc--
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn context_break_multiple_no_heading() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .separator_search(Some(b"--xyz--".to_vec()))
            .separator_context(Some(b"--abc--".to_vec()))
            .build(NoColor::new(vec![]));

        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();
        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--abc--
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
--xyz--
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--abc--
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn context_break_multiple_heading() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .heading(true)
            .separator_search(Some(b"--xyz--".to_vec()))
            .separator_context(Some(b"--abc--".to_vec()))
            .build(NoColor::new(vec![]));

        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();
        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--abc--
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
--xyz--
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--abc--
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn path() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .path(false)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the Doctor Watsons of this world, as opposed to the Sherlock
5:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn separator_field() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .separator_field_match(b"!!".to_vec())
            .separator_field_context(b"^^".to_vec())
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
sherlock!!For the Doctor Watsons of this world, as opposed to the Sherlock
sherlock^^Holmeses, success in the province of detective work must always
--
sherlock^^can extract a clew from a wisp of straw or a flake of cigar ash;
sherlock!!but Doctor Watson has to have it taken out for him and dusted,
sherlock^^and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn separator_path() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .separator_path(Some(b'Z'))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "books/sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
booksZsherlock:For the Doctor Watsons of this world, as opposed to the Sherlock
booksZsherlock:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn path_terminator() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .path_terminator(Some(b'Z'))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "books/sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
books/sherlockZFor the Doctor Watsons of this world, as opposed to the Sherlock
books/sherlockZbut Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn heading() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .heading(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
sherlock
For the Doctor Watsons of this world, as opposed to the Sherlock
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn no_heading() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .heading(false)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
sherlock:For the Doctor Watsons of this world, as opposed to the Sherlock
sherlock:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn no_heading_multiple() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .heading(false)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let matcher = RegexMatcher::new("Sherlock").unwrap();
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
sherlock:For the Doctor Watsons of this world, as opposed to the Sherlock
sherlock:but Doctor Watson has to have it taken out for him and dusted,
sherlock:For the Doctor Watsons of this world, as opposed to the Sherlock
sherlock:be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn heading_multiple() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .heading(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let matcher = RegexMatcher::new("Sherlock").unwrap();
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(
                &matcher,
                SHERLOCK.as_bytes(),
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
sherlock
For the Doctor Watsons of this world, as opposed to the Sherlock
but Doctor Watson has to have it taken out for him and dusted,
sherlock
For the Doctor Watsons of this world, as opposed to the Sherlock
be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn trim_ascii() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .trim_ascii(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, "   Watson".as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
Watson
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn trim_ascii_multi_line() {
        let matcher = RegexMatcher::new("(?s:.{0})Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .trim_ascii(true)
            .stats(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .build()
            .search_reader(&matcher, "   Watson".as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
Watson
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn trim_ascii_with_line_term() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .trim_ascii(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .before_context(1)
            .build()
            .search_reader(&matcher, "\n   Watson".as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1-
2:Watson
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn line_number() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the Doctor Watsons of this world, as opposed to the Sherlock
5:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn line_number_multi_line() {
        let matcher = RegexMatcher::new("(?s)Watson.+Watson").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .multi_line(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the Doctor Watsons of this world, as opposed to the Sherlock
2:Holmeses, success in the province of detective work must always
3:be, to a very large extent, the result of luck. Sherlock Holmes
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn column_number() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
16:For the Doctor Watsons of this world, as opposed to the Sherlock
12:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn column_number_multi_line() {
        let matcher = RegexMatcher::new("(?s)Watson.+Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
16:For the Doctor Watsons of this world, as opposed to the Sherlock
16:Holmeses, success in the province of detective work must always
16:be, to a very large extent, the result of luck. Sherlock Holmes
16:can extract a clew from a wisp of straw or a flake of cigar ash;
16:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn byte_offset() {
        let matcher = RegexMatcher::new("Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .byte_offset(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
258:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn byte_offset_multi_line() {
        let matcher = RegexMatcher::new("(?s)Watson.+Watson").unwrap();
        let mut printer = StandardBuilder::new()
            .byte_offset(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns() {
        let matcher = RegexMatcher::new("ash|dusted").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(63))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
[Omitted long matching line]
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_preview() {
        let matcher = RegexMatcher::new("exhibited|dusted").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(46))
            .max_columns_preview(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
but Doctor Watson has to have it taken out for [... omitted end of long line]
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_with_count() {
        let matcher = RegexMatcher::new("cigar|ash|dusted").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .max_columns(Some(63))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
[Omitted long line with 2 matches]
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_with_count_preview_no_match() {
        let matcher = RegexMatcher::new("exhibited|has to have it").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .max_columns(Some(46))
            .max_columns_preview(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
but Doctor Watson has to have it taken out for [... 0 more matches]
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_with_count_preview_one_match() {
        let matcher = RegexMatcher::new("exhibited|dusted").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .max_columns(Some(46))
            .max_columns_preview(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
but Doctor Watson has to have it taken out for [... 1 more match]
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_with_count_preview_two_matches() {
        let matcher = RegexMatcher::new("exhibited|dusted|has to have it").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .max_columns(Some(46))
            .max_columns_preview(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
but Doctor Watson has to have it taken out for [... 1 more match]
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_multi_line() {
        let matcher = RegexMatcher::new("(?s)ash.+dusted").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(63))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
[Omitted long matching line]
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_columns_multi_line_preview() {
        let matcher = RegexMatcher::new("(?s)clew|cigar ash.+have it|exhibited").unwrap();
        let mut printer = StandardBuilder::new()
            .stats(true)
            .max_columns(Some(46))
            .max_columns_preview(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
can extract a clew from a wisp of straw or a f [... 1 more match]
but Doctor Watson has to have it taken out for [... 0 more matches]
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches() {
        let matcher = RegexMatcher::new("Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_context() {
        let matcher = RegexMatcher::new("Doctor Watsons").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .max_matches(Some(1))
            .line_number(false)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
";
        assert_eq_printed!(expected, got);

        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .max_matches(Some(1))
            .line_number(false)
            .after_context(4)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);

        let matcher = RegexMatcher::new("Doctor Watsons|but Doctor").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .max_matches(Some(2))
            .line_number(false)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);

        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .max_matches(Some(2))
            .line_number(false)
            .after_context(4)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_context_invert() {
        let matcher = RegexMatcher::new("success|extent|clew|dusted|exhibited").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .invert_match(true)
            .max_matches(Some(1))
            .line_number(false)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
";
        assert_eq_printed!(expected, got);

        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .invert_match(true)
            .max_matches(Some(1))
            .line_number(false)
            .after_context(4)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);

        let matcher = RegexMatcher::new("success|extent|clew|exhibited").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .invert_match(true)
            .max_matches(Some(2))
            .line_number(false)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
--
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);

        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .invert_match(true)
            .max_matches(Some(2))
            .line_number(false)
            .after_context(4)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_multi_line1() {
        let matcher = RegexMatcher::new("(?s:.{0})Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_multi_line2() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_multi_line3() {
        let matcher = RegexMatcher::new(r"line 2\nline 3").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .max_matches(Some(1))
            .build()
            .search_reader(
                &matcher,
                "line 2\nline 3 x\nline 2\nline 3\n".as_bytes(),
                printer.sink(&matcher),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
line 2
line 3 x
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn max_matches_multi_line4() {
        let matcher = RegexMatcher::new(r"line 2\nline 3|x\nline 2\n").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .multi_line(true)
            .max_matches(Some(1))
            .build()
            .search_reader(
                &matcher,
                "line 2\nline 3 x\nline 2\nline 3 x\n".as_bytes(),
                printer.sink(&matcher),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
line 2
line 3 x
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching() {
        let matcher = RegexMatcher::new("Doctor Watsons|Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:Doctor Watsons
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_multi_line1() {
        let matcher = RegexMatcher::new(r"(?s:.{0})(Doctor Watsons|Sherlock)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:Doctor Watsons
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_multi_line2() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:Watsons of this world, as opposed to the Sherlock
2:16:Holmeses
5:12:Watson has to have it taken out for him and dusted,
6:12:and exhibited clearly
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns() {
        let matcher = RegexMatcher::new("Doctor Watsons|Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(10))
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:[Omitted long matching line]
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns_preview() {
        let matcher = RegexMatcher::new("Doctor Watsons|Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(10))
            .max_columns_preview(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:Doctor Wat [... 0 more matches]
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns_multi_line1() {
        let matcher = RegexMatcher::new(r"(?s:.{0})(Doctor Watsons|Sherlock)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(10))
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:[Omitted long matching line]
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns_preview_multi_line1() {
        let matcher = RegexMatcher::new(r"(?s:.{0})(Doctor Watsons|Sherlock)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(10))
            .max_columns_preview(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:Doctor Wat [... 0 more matches]
1:57:Sherlock
3:49:Sherlock
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns_multi_line2() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(50))
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:Watsons of this world, as opposed to the Sherlock
2:16:Holmeses
5:12:[Omitted long matching line]
6:12:and exhibited clearly
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn only_matching_max_columns_preview_multi_line2() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .max_columns(Some(50))
            .max_columns_preview(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:Watsons of this world, as opposed to the Sherlock
2:16:Holmeses
5:12:Watson has to have it taken out for him and dusted [... 0 more matches]
6:12:and exhibited clearly
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match() {
        let matcher = RegexMatcher::new("Doctor Watsons|Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:For the Doctor Watsons of this world, as opposed to the Sherlock
1:57:For the Doctor Watsons of this world, as opposed to the Sherlock
3:49:be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line1() {
        let matcher = RegexMatcher::new(r"(?s:.{0})(Doctor Watsons|Sherlock)").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:For the Doctor Watsons of this world, as opposed to the Sherlock
1:57:For the Doctor Watsons of this world, as opposed to the Sherlock
3:49:be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line2() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:For the Doctor Watsons of this world, as opposed to the Sherlock
2:1:Holmeses, success in the province of detective work must always
5:12:but Doctor Watson has to have it taken out for him and dusted,
6:1:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line3() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?Holmeses|always.+?be").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:For the Doctor Watsons of this world, as opposed to the Sherlock
2:1:Holmeses, success in the province of detective work must always
2:58:Holmeses, success in the province of detective work must always
3:1:be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line1_only_first_line() {
        let matcher = RegexMatcher::new(r"(?s:.{0})(Doctor Watsons|Sherlock)").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .per_match_one_line(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:9:For the Doctor Watsons of this world, as opposed to the Sherlock
1:57:For the Doctor Watsons of this world, as opposed to the Sherlock
3:49:be, to a very large extent, the result of luck. Sherlock Holmes
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line2_only_first_line() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?(Holmeses|clearly)").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .per_match_one_line(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:For the Doctor Watsons of this world, as opposed to the Sherlock
5:12:but Doctor Watson has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn per_match_multi_line3_only_first_line() {
        let matcher = RegexMatcher::new(r"(?s)Watson.+?Holmeses|always.+?be").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .per_match_one_line(true)
            .column(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:16:For the Doctor Watsons of this world, as opposed to the Sherlock
2:58:Holmeses, success in the province of detective work must always
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_passthru() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .passthru(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the doctah Watsons MD of this world, as opposed to the doctah  MD
2-Holmeses, success in the province of detective work must always
3:be, to a very large extent, the result of luck. doctah  MD Holmes
4-can extract a clew from a wisp of straw or a flake of cigar ash;
5:but doctah Watson MD has to have it taken out for him and dusted,
6-and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the doctah Watsons MD of this world, as opposed to the doctah  MD
3:be, to a very large extent, the result of luck. doctah  MD Holmes
5:but doctah Watson MD has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_multi_line() {
        let matcher = RegexMatcher::new(r"\n").unwrap();
        let mut printer = StandardBuilder::new()
            .replacement(Some(b"?".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .multi_line(true)
            .build()
            .search_reader(
                &matcher,
                "hello\nworld\n".as_bytes(),
                printer.sink(&matcher),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "1:hello?world?\n";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_multi_line_diff_line_term() {
        let matcher = RegexMatcherBuilder::new()
            .line_terminator(Some(b'\x00'))
            .build(r"\n")
            .unwrap();
        let mut printer = StandardBuilder::new()
            .replacement(Some(b"?".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_terminator(LineTerminator::byte(b'\x00'))
            .line_number(true)
            .multi_line(true)
            .build()
            .search_reader(
                &matcher,
                "hello\nworld\n".as_bytes(),
                printer.sink(&matcher),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "1:hello?world?\x00";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_multi_line_combine_lines() {
        let matcher = RegexMatcher::new(r"\n(.)?").unwrap();
        let mut printer = StandardBuilder::new()
            .replacement(Some(b"?$1".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .multi_line(true)
            .build()
            .search_reader(
                &matcher,
                "hello\nworld\n".as_bytes(),
                printer.sink(&matcher),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "1:hello?world?\n";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_max_columns() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(67))
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:[Omitted long line with 2 matches]
3:be, to a very large extent, the result of luck. doctah  MD Holmes
5:but doctah Watson MD has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_max_columns_preview1() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(67))
            .max_columns_preview(true)
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the doctah Watsons MD of this world, as opposed to the doctah   [... 0 more matches]
3:be, to a very large extent, the result of luck. doctah  MD Holmes
5:but doctah Watson MD has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_max_columns_preview2() {
        let matcher = RegexMatcher::new("exhibited|dusted|has to have it").unwrap();
        let mut printer = StandardBuilder::new()
            .max_columns(Some(43))
            .max_columns_preview(true)
            .replacement(Some(b"xxx".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(false)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
but Doctor Watson xxx taken out for him and [... 1 more match]
and xxx clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_only_matching() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:doctah Watsons MD
1:doctah  MD
3:doctah  MD
5:doctah Watson MD
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn replacement_per_match() {
        let matcher = RegexMatcher::new(r"Sherlock|Doctor (\w+)").unwrap();
        let mut printer = StandardBuilder::new()
            .per_match(true)
            .replacement(Some(b"doctah $1 MD".to_vec()))
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1:For the doctah Watsons MD of this world, as opposed to the doctah  MD
1:For the doctah Watsons MD of this world, as opposed to the doctah  MD
3:be, to a very large extent, the result of luck. doctah  MD Holmes
5:but doctah Watson MD has to have it taken out for him and dusted,
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert() {
        let matcher = RegexMatcher::new(r"Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .invert_match(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
2:Holmeses, success in the province of detective work must always
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert_multi_line() {
        let matcher = RegexMatcher::new(r"(?s:.{0})Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .invert_match(true)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
2:Holmeses, success in the province of detective work must always
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert_context() {
        let matcher = RegexMatcher::new(r"Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .invert_match(true)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1-For the Doctor Watsons of this world, as opposed to the Sherlock
2:Holmeses, success in the province of detective work must always
3-be, to a very large extent, the result of luck. Sherlock Holmes
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert_context_multi_line() {
        let matcher = RegexMatcher::new(r"(?s:.{0})Sherlock").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .invert_match(true)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1-For the Doctor Watsons of this world, as opposed to the Sherlock
2:Holmeses, success in the province of detective work must always
3-be, to a very large extent, the result of luck. Sherlock Holmes
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert_context_only_matching() {
        let matcher = RegexMatcher::new(r"Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .line_number(true)
            .invert_match(true)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1-Sherlock
2:Holmeses, success in the province of detective work must always
3-Sherlock
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn invert_context_only_matching_multi_line() {
        let matcher = RegexMatcher::new(r"(?s:.{0})Sherlock").unwrap();
        let mut printer = StandardBuilder::new()
            .only_matching(true)
            .build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .multi_line(true)
            .line_number(true)
            .invert_match(true)
            .before_context(1)
            .after_context(1)
            .build()
            .search_reader(&matcher, SHERLOCK.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "\
1-Sherlock
2:Holmeses, success in the province of detective work must always
3-Sherlock
4:can extract a clew from a wisp of straw or a flake of cigar ash;
5:but Doctor Watson has to have it taken out for him and dusted,
6:and exhibited clearly, with a label attached.
";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn regression_search_empty_with_crlf() {
        let matcher = RegexMatcherBuilder::new().crlf(true).build(r"x?").unwrap();
        let mut printer = StandardBuilder::new()
            .color_specs(ColorSpecs::default_with_color())
            .build(Ansi::new(vec![]));
        SearcherBuilder::new()
            .line_terminator(LineTerminator::crlf())
            .build()
            .search_reader(&matcher, &b"\n"[..], printer.sink(&matcher))
            .unwrap();

        let got = printer_contents_ansi(&mut printer);
        assert_ne!(got.len(), 0);
    }

    #[test]
    fn regression_after_context_with_match() {
        let haystack = "\
a
b
c
d
e
d
e
d
e
d
e
";

        let matcher = RegexMatcherBuilder::new().build(r"d").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        SearcherBuilder::new()
            .max_matches(Some(1))
            .line_number(true)
            .after_context(2)
            .build()
            .search_reader(&matcher, haystack.as_bytes(), printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        let expected = "4:d\n5-e\n6:d\n";
        assert_eq_printed!(expected, got);
    }

    #[test]
    fn regression_crlf_preserve() {
        let haystack = "hello\nworld\r\n";
        let matcher = RegexMatcherBuilder::new().crlf(true).build(r".").unwrap();
        let mut printer = StandardBuilder::new().build(NoColor::new(vec![]));
        let mut searcher = SearcherBuilder::new()
            .line_number(false)
            .line_terminator(LineTerminator::crlf())
            .build();

        searcher
            .search_reader(&matcher, haystack.as_bytes(), printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);
        let expected = "hello\nworld\r\n";
        assert_eq_printed!(expected, got);

        let mut printer = StandardBuilder::new()
            .replacement(Some(b"$0".to_vec()))
            .build(NoColor::new(vec![]));
        searcher
            .search_reader(&matcher, haystack.as_bytes(), printer.sink(&matcher))
            .unwrap();
        let got = printer_contents(&mut printer);
        let expected = "hello\nworld\r\n";
        assert_eq_printed!(expected, got);
    }
}
