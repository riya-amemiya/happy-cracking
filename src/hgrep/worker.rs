use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use crate::hc_internal::encoding::{DecodeOptions, DecodeReader, Encoding, decode_all};
use crate::hc_internal::grep::matcher::Matcher;
use crate::hc_internal::grep::printer::termcolor::WriteColor;
use crate::hc_internal::grep::printer::{JSON, Standard, Stats, Summary};
use crate::hc_internal::grep::searcher::{BinaryDetection, Searcher};
use crate::hc_internal::walker::overrides::Override;

use super::gnu_search::{GnuPrinter, InputInfo, same_as_stdout, stdout_is_file};
use super::haystack::Haystack;
use super::matchers::PatternMatcher;
use super::process::{CommandReader, DecompressionMatcher, DecompressionReader, resolve_binary};

pub(crate) type Transcoder = Arc<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

pub(crate) fn transcoder(encoding: Option<&'static Encoding>, bom_sniffing: bool) -> Transcoder {
    Arc::new(move |bytes: &[u8]| {
        let opts = DecodeOptions {
            encoding,
            bom_sniffing,
        };
        match decode_all(bytes, &opts) {
            Cow::Borrowed(_) => None,
            Cow::Owned(decoded) => Some(decoded),
        }
    })
}

#[derive(Clone)]
struct Config {
    preprocessor: Option<PathBuf>,
    preprocessor_globs: Override,
    search_zip: bool,
    binary_implicit: BinaryDetection,
    binary_explicit: BinaryDetection,
    gnu: bool,
    label: Option<Vec<u8>>,
    dev_null: Option<bool>,
    gnu_decode: Option<DecodeOptions>,
}

#[derive(Clone)]
pub(crate) struct SearchWorkerBuilder {
    config: Config,
}

impl SearchWorkerBuilder {
    pub(crate) fn new() -> SearchWorkerBuilder {
        SearchWorkerBuilder {
            config: Config {
                preprocessor: None,
                preprocessor_globs: Override::empty(),
                search_zip: false,
                binary_implicit: BinaryDetection::none(),
                binary_explicit: BinaryDetection::none(),
                gnu: false,
                label: None,
                dev_null: None,
                gnu_decode: None,
            },
        }
    }

    pub(crate) fn dev_null(&mut self, inverted_status: Option<bool>) -> &mut SearchWorkerBuilder {
        self.config.dev_null = inverted_status;
        self
    }

    pub(crate) fn build<W: WriteColor>(
        &self,
        matcher: PatternMatcher,
        searcher: Searcher,
        printer: Printer<W>,
    ) -> SearchWorker<W> {
        SearchWorker {
            decomp: self.config.search_zip.then(DecompressionMatcher::new),
            config: self.config.clone(),
            matcher,
            searcher,
            printer,
        }
    }

    pub(crate) fn preprocessor(
        &mut self,
        cmd: Option<PathBuf>,
    ) -> Result<&mut SearchWorkerBuilder, super::process::CommandError> {
        self.config.preprocessor = match cmd {
            Some(prog) => Some(resolve_binary(&prog)?),
            None => None,
        };
        Ok(self)
    }

    pub(crate) fn preprocessor_globs(&mut self, globs: Override) -> &mut SearchWorkerBuilder {
        self.config.preprocessor_globs = globs;
        self
    }

    pub(crate) fn search_zip(&mut self, yes: bool) -> &mut SearchWorkerBuilder {
        self.config.search_zip = yes;
        self
    }

    pub(crate) fn binary_detection_implicit(
        &mut self,
        detection: BinaryDetection,
    ) -> &mut SearchWorkerBuilder {
        self.config.binary_implicit = detection;
        self
    }

    pub(crate) fn binary_detection_explicit(
        &mut self,
        detection: BinaryDetection,
    ) -> &mut SearchWorkerBuilder {
        self.config.binary_explicit = detection;
        self
    }

    pub(crate) fn gnu(&mut self, yes: bool, label: Option<Vec<u8>>) -> &mut SearchWorkerBuilder {
        self.config.gnu = yes;
        self.config.label = label;
        self
    }

    pub(crate) fn gnu_decode(&mut self, decode: Option<DecodeOptions>) -> &mut SearchWorkerBuilder {
        self.config.gnu_decode = decode;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchResult {
    pub(crate) has_match: bool,
    pub(crate) stats: Option<Stats>,
    pub(crate) after_message: Option<String>,
    pub(crate) fatal: Option<String>,
    pub(crate) lead_separator: Option<Vec<u8>>,
    pub(crate) gnu_used: bool,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum Printer<W> {
    Standard(Standard<W>),
    Summary(Summary<W>),
    Json(JSON<W>),
    Gnu(GnuPrinter<W>),
}

impl<W: WriteColor> Printer<W> {
    pub(crate) fn set_defer_lead(&mut self, yes: bool) {
        if let Printer::Gnu(p) = self {
            p.set_defer_lead(yes);
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut W {
        match self {
            Printer::Standard(p) => p.get_mut(),
            Printer::Summary(p) => p.get_mut(),
            Printer::Json(p) => p.get_mut(),
            Printer::Gnu(p) => p.get_mut(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct SearchWorker<W> {
    config: Config,
    decomp: Option<DecompressionMatcher>,
    matcher: PatternMatcher,
    searcher: Searcher,
    printer: Printer<W>,
}

enum Source<'a> {
    Path(&'a Path),
    Reader(&'a mut dyn Read),
}

struct Target<'a> {
    display: Cow<'a, Path>,
    depth: usize,
    stdin: bool,
    regular: bool,
}

impl<W: WriteColor> SearchWorker<W> {
    pub(crate) fn printer(&mut self) -> &mut Printer<W> {
        &mut self.printer
    }

    fn stdin_display(&self, haystack: &Haystack) -> PathBuf {
        if let Some(label) = &self.config.label {
            return PathBuf::from(String::from_utf8_lossy(label).into_owned());
        }
        if self.config.gnu {
            PathBuf::from("(standard input)")
        } else {
            haystack.path().to_path_buf()
        }
    }

    pub(crate) fn search(&mut self, haystack: &Haystack) -> io::Result<SearchResult> {
        let detection = if haystack.is_explicit() {
            self.config.binary_explicit.clone()
        } else {
            self.config.binary_implicit.clone()
        };
        self.searcher.set_binary_detection(detection);
        let path = haystack.path();
        if haystack.is_stdin() {
            let display = self.stdin_display(haystack);
            let stdin = io::stdin();
            let mut lock = stdin.lock();
            return self.run(
                Source::Reader(&mut lock),
                &Target {
                    display: Cow::Owned(display),
                    depth: 0,
                    stdin: true,
                    regular: false,
                },
            );
        }
        let target = Target {
            display: Cow::Borrowed(path),
            depth: haystack.depth(),
            stdin: false,
            regular: haystack.is_regular(),
        };
        if self.should_preprocess(path) {
            return self.search_preprocessor(path, &target);
        }
        if self.decomp.as_ref().is_some_and(|d| d.has_command(path)) {
            return self.search_decompress(path, &target);
        }
        self.run(Source::Path(path), &target)
    }

    fn should_preprocess(&self, path: &Path) -> bool {
        if self.config.preprocessor.is_none() {
            return false;
        }
        self.config.preprocessor_globs.is_empty()
            || !self
                .config
                .preprocessor_globs
                .matched(path, false)
                .is_ignore()
    }

    fn search_preprocessor(
        &mut self,
        path: &Path,
        target: &Target<'_>,
    ) -> io::Result<SearchResult> {
        let bin = self
            .config
            .preprocessor
            .clone()
            .expect("preprocessor configured");
        let mut cmd = Command::new(&bin);
        cmd.arg(path).stdin(Stdio::from(File::open(path)?));
        let mut reader = CommandReader::new(&mut cmd).map_err(|err| {
            io::Error::other(format!(
                "preprocessor command could not start: '{cmd:?}': {err}"
            ))
        })?;
        let result = self
            .run(Source::Reader(&mut reader), target)
            .map_err(|err| {
                io::Error::other(format!("preprocessor command failed: '{cmd:?}': {err}"))
            });
        let closed = reader.close();
        let result = result?;
        closed?;
        Ok(result)
    }

    fn search_decompress(&mut self, path: &Path, target: &Target<'_>) -> io::Result<SearchResult> {
        let matcher = self.decomp.clone().expect("decompression enabled");
        let mut reader = DecompressionReader::new(&matcher, path)?;
        let result = self.run(Source::Reader(&mut reader), target);
        let closed = reader.close();
        let result = result?;
        closed?;
        Ok(result)
    }

    fn run(&mut self, source: Source<'_>, target: &Target<'_>) -> io::Result<SearchResult> {
        if let Printer::Gnu(printer) = &mut self.printer {
            return gnu_run(
                &self.matcher,
                printer,
                source,
                target,
                self.config.gnu_decode,
            );
        }
        let dev_null = self.config.dev_null;
        match &self.matcher {
            PatternMatcher::Rust(m) => search_with(
                m,
                &mut self.searcher,
                &mut self.printer,
                source,
                target,
                dev_null,
            ),
            PatternMatcher::Pcre(m) => search_with(
                m,
                &mut self.searcher,
                &mut self.printer,
                source,
                target,
                dev_null,
            ),
            PatternMatcher::Gnu(m) => search_with(
                m,
                &mut self.searcher,
                &mut self.printer,
                source,
                target,
                dev_null,
            ),
        }
    }
}

fn drive<M, S>(matcher: &M, searcher: &mut Searcher, source: Source<'_>, sink: S) -> io::Result<()>
where
    M: Matcher,
    S: crate::hc_internal::grep::searcher::Sink,
    S::Error: Into<io::Error>,
{
    match source {
        Source::Path(path) => searcher
            .search_path(matcher, path, sink)
            .map_err(Into::into),
        Source::Reader(reader) => searcher
            .search_reader(matcher, reader, sink)
            .map_err(Into::into),
    }
}

struct DevNullSink {
    matched: bool,
}

impl crate::hc_internal::grep::searcher::Sink for DevNullSink {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        _mat: &crate::hc_internal::grep::searcher::SinkMatch<'_>,
    ) -> Result<bool, io::Error> {
        self.matched = true;
        Ok(false)
    }
}

fn search_with<M: Matcher, W: WriteColor>(
    matcher: &M,
    searcher: &mut Searcher,
    printer: &mut Printer<W>,
    source: Source<'_>,
    target: &Target<'_>,
    dev_null: Option<bool>,
) -> io::Result<SearchResult> {
    if let Some(inverted_status) = dev_null {
        let mut sink = DevNullSink { matched: false };
        drive(matcher, searcher, source, &mut sink)?;
        return Ok(SearchResult {
            has_match: sink.matched != inverted_status,
            ..SearchResult::default()
        });
    }
    let path: &Path = &target.display;
    match printer {
        Printer::Standard(p) => {
            let mut sink = p.sink_with_path(matcher, path);
            drive(matcher, searcher, source, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().cloned(),
                ..SearchResult::default()
            })
        }
        Printer::Summary(p) => {
            let mut sink = p.sink_with_path(matcher, path);
            drive(matcher, searcher, source, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: sink.stats().cloned(),
                ..SearchResult::default()
            })
        }
        Printer::Json(p) => {
            let mut sink = p.sink_with_path(matcher, path);
            drive(matcher, searcher, source, &mut sink)?;
            Ok(SearchResult {
                has_match: sink.has_match(),
                stats: Some(sink.stats().clone()),
                ..SearchResult::default()
            })
        }
        Printer::Gnu(_) => unreachable!("GNU printers search through gnu_run"),
    }
}
fn gnu_run<W: WriteColor>(
    matcher: &PatternMatcher,
    printer: &mut GnuPrinter<W>,
    source: Source<'_>,
    target: &Target<'_>,
    decode: Option<DecodeOptions>,
) -> io::Result<SearchResult> {
    let config = printer.config();
    let guard_output = !config.out_quiet
        && config.list_files == super::gnu_search::ListFiles::None
        && config.max_count > 1;
    let tab = config.output.layout.initial_tab;
    let mut opened: Option<File> = None;
    let mut size = None;
    let mut regular = false;
    if let Source::Path(path) = &source {
        let file = File::open(path)?;
        regular = target.regular;
        if (!regular || tab || (guard_output && stdout_is_file()))
            && let Ok(md) = file.metadata()
        {
            if guard_output && same_as_stdout(&md) {
                return Err(io::Error::other("input file is also the output"));
            }
            regular = md.is_file();
            size = regular.then_some(md.len());
        }
        opened = Some(file);
    }
    let mut file_reader = opened.as_ref();
    let reader: &mut dyn Read = match (source, file_reader.as_mut()) {
        (Source::Path(_), Some(file)) => file,
        (Source::Reader(reader), _) => {
            if target.stdin
                && let Some(md) = super::gnu_search::stdin_metadata()
            {
                if guard_output && same_as_stdout(&md) {
                    return Err(io::Error::other("input file is also the output"));
                }
                size = md.is_file().then_some(md.len());
            }
            reader
        }
        (Source::Path(_), None) => unreachable!("a path source always opens a file"),
    };
    let mut decoded = None;
    let reader: &mut dyn Read = match decode {
        None => reader,
        Some(opts) => decoded.insert(DecodeReader::new(reader, opts)),
    };
    let name = super::flags::os_bytes(target.display.as_os_str()).to_vec();
    let info = InputInfo {
        name: &name,
        depth: target.depth,
        size,
        stdin: target.stdin,
        regular: regular && decode.is_none(),
        file: opened.as_ref().filter(|_| decode.is_none()),
    };
    let outcome = match matcher {
        PatternMatcher::Gnu(m) => printer.search(m, reader, &info)?,
        PatternMatcher::Pcre(m) => printer.search(m, reader, &info)?,
        PatternMatcher::Rust(m) => printer.search(m, reader, &info)?,
    };
    Ok(SearchResult {
        has_match: outcome.count > 0,
        stats: None,
        after_message: outcome.after_message,
        fatal: outcome.fatal,
        lead_separator: outcome.lead_separator,
        gnu_used: outcome.used,
        error: outcome
            .error
            .map(|err| format!("{}: {err}", target.display.display())),
    })
}
