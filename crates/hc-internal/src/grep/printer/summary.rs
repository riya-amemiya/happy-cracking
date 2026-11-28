use std::cell::RefCell;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::super::matcher::Matcher;
use super::super::searcher::{Searcher, Sink, SinkCount, SinkError, SinkFinish, SinkMatch};
use super::color::ColorSpecs;
use super::counter::CounterWriter;
use super::hyperlink::{self, HyperlinkConfig};
use super::stats::Stats;
#[cfg(test)]
use super::termcolor::NoColor;
use super::termcolor::{ColorSpec, WriteColor};
use super::util::{DecimalFormatter, PrinterPath, find_iter_at_in_context};

#[derive(Debug, Clone)]
struct Config {
    kind: SummaryKind,
    colors: ColorSpecs,
    hyperlink: HyperlinkConfig,
    stats: bool,
    path: bool,
    exclude_zero: bool,
    separator_field: Arc<Vec<u8>>,
    separator_path: Option<u8>,
    path_terminator: Option<u8>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            kind: SummaryKind::Count,
            colors: ColorSpecs::default(),
            hyperlink: HyperlinkConfig::default(),
            stats: false,
            path: true,
            exclude_zero: true,
            separator_field: Arc::new(b":".to_vec()),
            separator_path: None,
            path_terminator: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SummaryKind {
    Count,
    CountMatches,
    PathWithMatch,
    PathWithoutMatch,
    QuietWithMatch,
    QuietWithoutMatch,
}

impl SummaryKind {
    fn requires_path(self) -> bool {
        matches!(
            self,
            SummaryKind::PathWithMatch | SummaryKind::PathWithoutMatch
        )
    }

    fn requires_stats(self) -> bool {
        matches!(self, SummaryKind::CountMatches)
    }

    fn quit_early(self) -> bool {
        matches!(
            self,
            SummaryKind::PathWithMatch | SummaryKind::QuietWithMatch
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct SummaryBuilder {
    config: Config,
}

impl SummaryBuilder {
    #[must_use]
    pub fn new() -> SummaryBuilder {
        SummaryBuilder::default()
    }

    pub fn build<W: WriteColor>(&self, wtr: W) -> Summary<W> {
        Summary {
            config: self.config.clone(),
            wtr: RefCell::new(CounterWriter::new(wtr)),
        }
    }

    #[cfg(test)]
    pub fn build_no_color<W: io::Write>(&self, wtr: W) -> Summary<NoColor<W>> {
        self.build(NoColor::new(wtr))
    }

    pub fn kind(&mut self, kind: SummaryKind) -> &mut SummaryBuilder {
        self.config.kind = kind;
        self
    }

    pub fn color_specs(&mut self, specs: ColorSpecs) -> &mut SummaryBuilder {
        self.config.colors = specs;
        self
    }

    pub fn hyperlink(&mut self, config: HyperlinkConfig) -> &mut SummaryBuilder {
        self.config.hyperlink = config;
        self
    }

    pub fn stats(&mut self, yes: bool) -> &mut SummaryBuilder {
        self.config.stats = yes;
        self
    }

    pub fn path(&mut self, yes: bool) -> &mut SummaryBuilder {
        self.config.path = yes;
        self
    }

    pub fn exclude_zero(&mut self, yes: bool) -> &mut SummaryBuilder {
        self.config.exclude_zero = yes;
        self
    }

    pub fn separator_field(&mut self, sep: Vec<u8>) -> &mut SummaryBuilder {
        self.config.separator_field = Arc::new(sep);
        self
    }

    pub fn separator_path(&mut self, sep: Option<u8>) -> &mut SummaryBuilder {
        self.config.separator_path = sep;
        self
    }

    pub fn path_terminator(&mut self, terminator: Option<u8>) -> &mut SummaryBuilder {
        self.config.path_terminator = terminator;
        self
    }
}

#[derive(Clone, Debug)]
pub struct Summary<W> {
    config: Config,
    wtr: RefCell<CounterWriter<W>>,
}

impl<W: WriteColor> Summary<W> {
    pub fn sink<M: Matcher>(&mut self, matcher: M) -> SummarySink<'static, '_, M, W> {
        let interpolator = hyperlink::Interpolator::new(&self.config.hyperlink);
        let stats = if self.config.stats || self.config.kind.requires_stats() {
            Some(Stats::new())
        } else {
            None
        };
        SummarySink {
            matcher,
            summary: self,
            interpolator,
            path: None,
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            stats,
        }
    }

    pub fn sink_with_path<'p, 's, M, P>(
        &'s mut self,
        matcher: M,
        path: &'p P,
    ) -> SummarySink<'p, 's, M, W>
    where
        M: Matcher,
        P: ?Sized + AsRef<Path>,
    {
        if !self.config.path && !self.config.kind.requires_path() {
            return self.sink(matcher);
        }
        let interpolator = hyperlink::Interpolator::new(&self.config.hyperlink);
        let stats = if self.config.stats || self.config.kind.requires_stats() {
            Some(Stats::new())
        } else {
            None
        };
        let ppath = PrinterPath::new(path.as_ref()).with_separator(self.config.separator_path);
        SummarySink {
            matcher,
            summary: self,
            interpolator,
            path: Some(ppath),
            start_time: Instant::now(),
            match_count: 0,
            binary_byte_offset: None,
            stats,
        }
    }
}

impl<W> Summary<W> {
    pub fn get_mut(&mut self) -> &mut W {
        self.wtr.get_mut().get_mut()
    }

    #[cfg(test)]
    pub fn into_inner(self) -> W {
        self.wtr.into_inner().into_inner()
    }
}

#[derive(Debug)]
pub struct SummarySink<'p, 's, M: Matcher, W> {
    matcher: M,
    summary: &'s mut Summary<W>,
    interpolator: hyperlink::Interpolator,
    path: Option<PrinterPath<'p>>,
    start_time: Instant,
    match_count: u64,
    binary_byte_offset: Option<u64>,
    stats: Option<Stats>,
}

impl<M: Matcher, W: WriteColor> SummarySink<'_, '_, M, W> {
    pub fn has_match(&self) -> bool {
        match self.summary.config.kind {
            SummaryKind::PathWithoutMatch | SummaryKind::QuietWithoutMatch => self.match_count == 0,
            _ => self.match_count > 0,
        }
    }

    #[cfg(test)]
    pub fn binary_byte_offset(&self) -> Option<u64> {
        self.binary_byte_offset
    }

    pub fn stats(&self) -> Option<&Stats> {
        self.stats.as_ref()
    }

    fn multi_line(&self, searcher: &Searcher) -> bool {
        searcher.multi_line_with_matcher(&self.matcher)
    }

    fn write_path_line(&mut self, searcher: &Searcher) -> io::Result<()> {
        if self.path.is_some() {
            self.write_path()?;
            if let Some(term) = self.summary.config.path_terminator {
                self.write(&[term])?;
            } else {
                self.write_line_term(searcher)?;
            }
        }
        Ok(())
    }

    fn write_path_field(&mut self) -> io::Result<()> {
        if self.path.is_some() {
            self.write_path()?;
            if let Some(term) = self.summary.config.path_terminator {
                self.write(&[term])?;
            } else {
                self.write(&self.summary.config.separator_field)?;
            }
        }
        Ok(())
    }

    fn write_path(&mut self) -> io::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let enabled = self.interpolator.enabled(&*self.summary.wtr.borrow());
        let status = match enabled.then(|| path.as_hyperlink()).flatten() {
            Some(hyperpath) => {
                let values = hyperlink::Values::new(hyperpath);
                self.interpolator
                    .begin(&values, &mut *self.summary.wtr.borrow_mut())?
            }
            None => hyperlink::InterpolatorStatus::inactive(),
        };
        self.write_spec(self.summary.config.colors.path(), path.as_bytes())?;
        status.finish(&mut *self.summary.wtr.borrow_mut())
    }

    fn write_line_term(&self, searcher: &Searcher) -> io::Result<()> {
        self.write(searcher.line_terminator().as_bytes())
    }

    fn write_spec(&self, spec: &ColorSpec, buf: &[u8]) -> io::Result<()> {
        self.summary.wtr.borrow_mut().set_color(spec)?;
        self.write(buf)?;
        self.summary.wtr.borrow_mut().reset()?;
        Ok(())
    }

    fn write(&self, buf: &[u8]) -> io::Result<()> {
        self.summary.wtr.borrow_mut().write_all(buf)
    }
}

impl<M: Matcher, W: WriteColor> Sink for SummarySink<'_, '_, M, W> {
    type Error = io::Error;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        let is_multi_line = self.multi_line(searcher);
        let sink_match_count = if self.stats.is_none() && !is_multi_line {
            1
        } else {
            let mut count = 0;
            find_iter_at_in_context(
                searcher,
                &self.matcher,
                mat.buffer(),
                mat.bytes_range_in_buffer(),
                |_| {
                    count += 1;
                    true
                },
            )?;
            count.max(1)
        };
        if is_multi_line {
            self.match_count += sink_match_count;
        } else {
            self.match_count += 1;
        }
        if let Some(ref mut stats) = self.stats {
            stats.add_matches(sink_match_count);
            stats.add_matched_lines(mat.lines().count() as u64);
        } else if self.summary.config.kind.quit_early() {
            return Ok(false);
        }
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        _binary_byte_offset: u64,
    ) -> Result<bool, io::Error> {
        Ok(true)
    }

    fn count_mode(&self, searcher: &Searcher) -> SinkCount {
        if self.multi_line(searcher) {
            return SinkCount::None;
        }
        if self.stats.is_some() {
            return SinkCount::LinesAndMatches;
        }
        match self.summary.config.kind {
            SummaryKind::Count | SummaryKind::PathWithoutMatch | SummaryKind::QuietWithoutMatch => {
                SinkCount::Lines
            }
            _ => SinkCount::None,
        }
    }

    fn matched_count(
        &mut self,
        _searcher: &Searcher,
        lines: u64,
        matches: u64,
    ) -> Result<bool, io::Error> {
        self.match_count += lines;
        if let Some(ref mut stats) = self.stats {
            stats.add_matches(matches);
            stats.add_matched_lines(lines);
        }
        Ok(true)
    }

    fn begin(&mut self, _searcher: &Searcher) -> Result<bool, io::Error> {
        if self.path.is_none() && self.summary.config.kind.requires_path() {
            return Err(io::Error::error_message(format!(
                "output kind {:?} requires a file path",
                self.summary.config.kind
            )));
        }
        self.summary.wtr.borrow_mut().reset_count();
        self.start_time = Instant::now();
        self.match_count = 0;
        self.binary_byte_offset = None;
        Ok(true)
    }

    fn finish(&mut self, searcher: &Searcher, finish: &SinkFinish) -> Result<(), io::Error> {
        self.binary_byte_offset = finish.binary_byte_offset();
        if let Some(ref mut stats) = self.stats {
            stats.add_elapsed(self.start_time.elapsed());
            stats.add_searches(1);
            if self.match_count > 0 {
                stats.add_searches_with_match(1);
            }
            stats.add_bytes_searched(finish.byte_count());
            stats.add_bytes_printed(self.summary.wtr.borrow().count());
        }
        if self.binary_byte_offset.is_some() && searcher.binary_detection().quit_byte().is_some() {
            self.match_count = 0;
            return Ok(());
        }
        let show_count = !self.summary.config.exclude_zero || self.match_count > 0;
        match self.summary.config.kind {
            SummaryKind::Count => {
                if show_count {
                    self.write_path_field()?;
                    self.write(DecimalFormatter::new(self.match_count).as_bytes())?;
                    self.write_line_term(searcher)?;
                }
            }
            SummaryKind::CountMatches => {
                if show_count {
                    self.write_path_field()?;
                    let matches = self
                        .stats
                        .as_ref()
                        .expect("CountMatches should enable stats tracking")
                        .matches();
                    self.write(DecimalFormatter::new(matches).as_bytes())?;
                    self.write_line_term(searcher)?;
                }
            }
            SummaryKind::PathWithMatch => {
                if self.match_count > 0 {
                    self.write_path_line(searcher)?;
                }
            }
            SummaryKind::PathWithoutMatch => {
                if self.match_count == 0 {
                    self.write_path_line(searcher)?;
                }
            }
            SummaryKind::QuietWithMatch | SummaryKind::QuietWithoutMatch => {}
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::super::super::regex::RegexMatcher;
    use super::super::super::searcher::SearcherBuilder;
    use super::super::termcolor::NoColor;

    use super::{Summary, SummaryBuilder, SummaryKind};

    const SHERLOCK: &[u8] = b"\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.
";

    fn printer_contents(printer: &mut Summary<NoColor<Vec<u8>>>) -> String {
        String::from_utf8(printer.get_mut().get_ref().to_owned()).unwrap()
    }

    #[test]
    fn path_with_match_error() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithMatch)
            .build_no_color(vec![]);
        let res = SearcherBuilder::new().build().search_reader(
            &matcher,
            SHERLOCK,
            printer.sink(&matcher),
        );
        assert!(res.is_err());
    }

    #[test]
    fn path_without_match_error() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithoutMatch)
            .build_no_color(vec![]);
        let res = SearcherBuilder::new().build().search_reader(
            &matcher,
            SHERLOCK,
            printer.sink(&matcher),
        );
        assert!(res.is_err());
    }

    #[test]
    fn count_no_path() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(&matcher, SHERLOCK, printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("2\n", got);
    }

    #[test]
    fn count_no_path_even_with_path() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .path(false)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("2\n", got);
    }

    #[test]
    fn count_path() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock:2\n", got);
    }

    #[test]
    fn count_path_with_zero() {
        let matcher = RegexMatcher::new(r"NO MATCH").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .exclude_zero(false)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock:0\n", got);
    }

    #[test]
    fn count_path_without_zero() {
        let matcher = RegexMatcher::new(r"NO MATCH").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .exclude_zero(true)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("", got);
    }

    #[test]
    fn count_path_field_separator() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .separator_field(b"ZZ".to_vec())
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlockZZ2\n", got);
    }

    #[test]
    fn count_path_terminator() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .path_terminator(Some(b'\x00'))
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock\x002\n", got);
    }

    #[test]
    fn count_path_separator() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .separator_path(Some(b'\\'))
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "/home/andrew/sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("\\home\\andrew\\sherlock:2\n", got);
    }

    #[test]
    fn count_max_matches() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::Count)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .max_matches(Some(1))
            .build()
            .search_reader(&matcher, SHERLOCK, printer.sink(&matcher))
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("1\n", got);
    }

    #[test]
    fn count_matches() {
        let matcher = RegexMatcher::new(r"Watson|Sherlock").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::CountMatches)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock:4\n", got);
    }

    #[test]
    fn path_with_match_found() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithMatch)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock\n", got);
    }

    #[test]
    fn path_with_match_not_found() {
        let matcher = RegexMatcher::new(r"ZZZZZZZZ").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithMatch)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("", got);
    }

    #[test]
    fn path_without_match_found() {
        let matcher = RegexMatcher::new(r"ZZZZZZZZZ").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithoutMatch)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("sherlock\n", got);
    }

    #[test]
    fn path_without_match_not_found() {
        let matcher = RegexMatcher::new(r"Watson").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::PathWithoutMatch)
            .build_no_color(vec![]);
        SearcherBuilder::new()
            .build()
            .search_reader(
                &matcher,
                SHERLOCK,
                printer.sink_with_path(&matcher, "sherlock"),
            )
            .unwrap();

        let got = printer_contents(&mut printer);
        assert_eq_printed!("", got);
    }

    #[test]
    fn quiet() {
        let matcher = RegexMatcher::new(r"Watson|Sherlock").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::QuietWithMatch)
            .build_no_color(vec![]);
        let match_count = {
            let mut sink = printer.sink_with_path(&matcher, "sherlock");
            SearcherBuilder::new()
                .build()
                .search_reader(&matcher, SHERLOCK, &mut sink)
                .unwrap();
            sink.match_count
        };

        let got = printer_contents(&mut printer);
        assert_eq_printed!("", got);
        assert_eq!(1, match_count);
    }

    #[test]
    fn quiet_with_stats() {
        let matcher = RegexMatcher::new(r"Watson|Sherlock").unwrap();
        let mut printer = SummaryBuilder::new()
            .kind(SummaryKind::QuietWithMatch)
            .stats(true)
            .build_no_color(vec![]);
        let match_count = {
            let mut sink = printer.sink_with_path(&matcher, "sherlock");
            SearcherBuilder::new()
                .build()
                .search_reader(&matcher, SHERLOCK, &mut sink)
                .unwrap();
            sink.match_count
        };

        let got = printer_contents(&mut printer);
        assert_eq_printed!("", got);
        assert_eq!(3, match_count);
    }
}

#[cfg(test)]
mod bulk_tests {
    use std::fmt::Write as _;

    use super::super::super::matcher::LineTerminator;
    use super::super::super::regex::RegexMatcherBuilder;
    use super::super::super::searcher::{
        BinaryDetection, MmapChoice, Searcher, SearcherBuilder, Sink, SinkContext, SinkCount,
        SinkFinish, SinkMatch,
    };
    use super::super::termcolor::NoColor;
    use super::{SummaryBuilder, SummaryKind};

    struct NoBulk<S>(S);

    impl<S: Sink> Sink for NoBulk<S> {
        type Error = S::Error;

        fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, S::Error> {
            self.0.matched(searcher, mat)
        }

        fn context(
            &mut self,
            searcher: &Searcher,
            ctx: &SinkContext<'_>,
        ) -> Result<bool, S::Error> {
            self.0.context(searcher, ctx)
        }

        fn begin(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
            self.0.begin(searcher)
        }

        fn finish(&mut self, searcher: &Searcher, finish: &SinkFinish) -> Result<(), S::Error> {
            self.0.finish(searcher, finish)
        }
    }

    struct BinaryStop<S>(S, bool);

    impl<S: Sink> Sink for BinaryStop<S> {
        type Error = S::Error;

        fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, S::Error> {
            self.0.matched(searcher, mat)
        }

        fn binary_data(&mut self, _searcher: &Searcher, _offset: u64) -> Result<bool, S::Error> {
            Ok(false)
        }

        fn begin(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
            self.0.begin(searcher)
        }

        fn finish(&mut self, searcher: &Searcher, finish: &SinkFinish) -> Result<(), S::Error> {
            self.0.finish(searcher, finish)
        }

        fn count_mode(&self, searcher: &Searcher) -> SinkCount {
            if self.1 {
                self.0.count_mode(searcher)
            } else {
                SinkCount::None
            }
        }

        fn matched_count(
            &mut self,
            searcher: &Searcher,
            lines: u64,
            matches: u64,
        ) -> Result<bool, S::Error> {
            self.0.matched_count(searcher, lines, matches)
        }
    }

    fn nul_haystack() -> Vec<u8> {
        let mut s = Vec::new();
        for i in 0..40000u32 {
            let tail: &[u8] = match i % 11 {
                0 => b"bar\x00foo",
                3 => b"\x00\x00",
                5 => b"baz\x00",
                _ => b"baz",
            };
            s.extend_from_slice(format!("row {i} foo ").as_bytes());
            s.extend_from_slice(tail);
            s.push(b'\n');
        }
        s.extend_from_slice(b"foo\x00 tail");
        s
    }

    #[test]
    fn converted_bulk_counts_match_reader_counts() {
        let path = std::env::temp_dir().join(format!(
            "greplib-convert-{}-{:?}.txt",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, nul_haystack()).unwrap();
        for pattern in ["foo", "bar", "^$", "a.f", "o*"] {
            for kind in [SummaryKind::Count, SummaryKind::CountMatches] {
                for invert in [false, true] {
                    for parallel in [false, true] {
                        for stop in [false, true] {
                            let mut results = vec![];
                            for bulk in [false, true] {
                                let matcher = RegexMatcherBuilder::new()
                                    .line_terminator(Some(b'\n'))
                                    .build(pattern)
                                    .unwrap();
                                let mut printer = SummaryBuilder::new()
                                    .kind(kind)
                                    .stats(true)
                                    .build(NoColor::new(vec![]));
                                let mut searcher = SearcherBuilder::new()
                                    .invert_match(invert)
                                    .binary_detection(BinaryDetection::convert(0))
                                    .memory_map(MmapChoice::never())
                                    .parallel(parallel)
                                    .parallel_threshold(4096)
                                    .build();
                                let stats = {
                                    let mut sink = printer.sink_with_path(&matcher, "x");
                                    if stop {
                                        searcher
                                            .search_path(
                                                &matcher,
                                                &path,
                                                BinaryStop(&mut sink, bulk),
                                            )
                                            .unwrap();
                                    } else if bulk {
                                        searcher.search_path(&matcher, &path, &mut sink).unwrap();
                                    } else {
                                        searcher
                                            .search_path(&matcher, &path, NoBulk(&mut sink))
                                            .unwrap();
                                    }
                                    let s = sink.stats().unwrap();
                                    (
                                        s.matches(),
                                        s.matched_lines(),
                                        s.bytes_searched(),
                                        sink.binary_byte_offset(),
                                    )
                                };
                                let out =
                                    String::from_utf8(printer.into_inner().into_inner()).unwrap();
                                results.push((out, stats));
                            }
                            assert_eq!(
                                results[0], results[1],
                                "{pattern:?} {kind:?} invert={invert} parallel={parallel} stop={stop}"
                            );
                        }
                    }
                }
            }
        }
        std::fs::remove_file(&path).unwrap();
    }

    fn haystack() -> String {
        let mut s = String::new();
        for i in 0..5000 {
            let tail = if i % 7 == 0 { "bar" } else { "baz" };
            let _ = write!(s, "row {i} foo foo {tail}\r\n");
        }
        s.push_str("tail foo bar no newline");
        s
    }

    #[test]
    fn bulk_counts_match_per_line_counts() {
        let hay = haystack();
        for pattern in ["foo", "bar|z$", r"\b", "o*", "^row 1"] {
            for kind in [SummaryKind::Count, SummaryKind::CountMatches] {
                for invert in [false, true] {
                    for crlf in [false, true] {
                        for parallel in [false, true] {
                            let mut results = vec![];
                            for bulk in [false, true] {
                                let mut mb = RegexMatcherBuilder::new();
                                mb.multi_line(true).line_terminator(Some(b'\n'));
                                if crlf {
                                    mb.crlf(true);
                                }
                                let matcher = mb.build(pattern).unwrap();
                                let mut printer = SummaryBuilder::new()
                                    .kind(kind)
                                    .stats(true)
                                    .build(NoColor::new(vec![]));
                                let mut searcher = SearcherBuilder::new()
                                    .invert_match(invert)
                                    .line_terminator(if crlf {
                                        LineTerminator::crlf()
                                    } else {
                                        LineTerminator::byte(b'\n')
                                    })
                                    .parallel(parallel)
                                    .parallel_threshold(4096)
                                    .build();
                                let stats = {
                                    let mut sink = printer.sink(&matcher);
                                    if bulk {
                                        searcher
                                            .search_slice(&matcher, hay.as_bytes(), &mut sink)
                                            .unwrap();
                                    } else {
                                        searcher
                                            .search_slice(
                                                &matcher,
                                                hay.as_bytes(),
                                                NoBulk(&mut sink),
                                            )
                                            .unwrap();
                                    }
                                    let s = sink.stats().unwrap();
                                    (s.matches(), s.matched_lines(), s.bytes_searched())
                                };
                                let out =
                                    String::from_utf8(printer.into_inner().into_inner()).unwrap();
                                results.push((out, stats));
                            }
                            assert_eq!(
                                results[0], results[1],
                                "{pattern:?} {kind:?} invert={invert} crlf={crlf} parallel={parallel}"
                            );
                        }
                    }
                }
            }
        }
    }
}
