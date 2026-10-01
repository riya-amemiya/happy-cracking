use std::io::{self, Read};

use memchr::{memchr, memchr_iter, memrchr};

use crate::hc_internal::gnu::output::{
    BinaryFiles, GOOD_READSIZE, LineInfo, LineKind, LineOutcome, OutputConfig,
};
use crate::hc_internal::grep::matcher::Matcher;
use crate::hc_internal::grep::printer::termcolor::WriteColor;
use crate::hc_internal::grep::regex::RegexMatcher;

use super::matchers::{GnuAdapter, PcreMatcher};

pub(crate) type ExecResult<T> = Result<T, String>;

pub(crate) trait GnuExec {
    fn find_line(
        &self,
        buf: &[u8],
        from: usize,
        lim: usize,
        eol: u8,
    ) -> ExecResult<Option<(usize, usize)>>;

    fn spans(&self, buf: &[u8], beg: usize, lim: usize, eol: u8)
    -> ExecResult<Vec<(usize, usize)>>;

    fn fork(&self) -> Self
    where
        Self: Sized;
}

impl GnuExec for GnuAdapter {
    fn fork(&self) -> GnuAdapter {
        GnuAdapter::fork(self)
    }

    fn find_line(
        &self,
        buf: &[u8],
        from: usize,
        lim: usize,
        _eol: u8,
    ) -> ExecResult<Option<(usize, usize)>> {
        Ok(self
            .regex()
            .find_line(&buf[from..lim], 0)
            .map(|(s, e)| (from + s, from + e)))
    }

    fn spans(
        &self,
        buf: &[u8],
        beg: usize,
        lim: usize,
        _eol: u8,
    ) -> ExecResult<Vec<(usize, usize)>> {
        Ok(self.regex().match_spans(&buf[beg..lim - 1]))
    }
}

fn single_byte_invalid(b: u8) -> bool {
    matches!(b, 0x80..=0xc1 | 0xf5..=0xff)
}

impl PcreMatcher {
    fn pexecute(
        &self,
        buf: &[u8],
        beg: usize,
        lim: usize,
        start: Option<usize>,
        eol: u8,
    ) -> ExecResult<Option<(usize, usize)>> {
        let mut p = start.unwrap_or(beg);
        let mut bol = p == 0 || buf[p - 1] == eol;
        let mut line_start = beg;
        let mut subject = beg;
        loop {
            let line_end = memchr(eol, &buf[p..lim]).map_or(lim, |i| p + i);
            if self.utf() {
                while p < line_end && single_byte_invalid(buf[p]) {
                    p += 1;
                    subject = p;
                    bol = false;
                }
            }
            let found = if p == line_end {
                self.empty_match(bol).then_some((p, p))
            } else {
                self.find_in(&buf[subject..line_end], p - subject, !bol)
                    .map_err(super::matchers::gnu_match_message)?
                    .map(|(s, e)| (subject + s, subject + e))
            };
            if let Some((s, e)) = found {
                return Ok(Some(if start.is_some() {
                    (s, e)
                } else {
                    (line_start, (line_end + 1).min(lim))
                }));
            }
            if line_end + 1 >= lim {
                return Ok(None);
            }
            bol = true;
            p = line_end + 1;
            subject = p;
            line_start = p;
        }
    }
}

fn spans_by<F>(buf: &[u8], beg: usize, lim: usize, mut exec: F) -> ExecResult<Vec<(usize, usize)>>
where
    F: FnMut(usize) -> ExecResult<Option<(usize, usize)>>,
{
    let mut spans = Vec::new();
    let mut cur = beg;
    while cur < lim {
        let Some((b, e)) = exec(cur)? else {
            break;
        };
        if b == lim {
            break;
        }
        if e == b {
            cur = b + 1;
        } else {
            spans.push((b - beg, e - beg));
            cur = e;
        }
    }
    let _ = buf;
    Ok(spans)
}

impl GnuExec for PcreMatcher {
    fn fork(&self) -> PcreMatcher {
        self.clone()
    }

    fn find_line(
        &self,
        buf: &[u8],
        from: usize,
        lim: usize,
        eol: u8,
    ) -> ExecResult<Option<(usize, usize)>> {
        self.pexecute(buf, from, lim, None, eol)
    }

    fn spans(
        &self,
        buf: &[u8],
        beg: usize,
        lim: usize,
        eol: u8,
    ) -> ExecResult<Vec<(usize, usize)>> {
        spans_by(buf, beg, lim, |cur| {
            self.pexecute(buf, beg, lim, Some(cur), eol)
        })
    }
}

impl GnuExec for RegexMatcher {
    fn fork(&self) -> RegexMatcher {
        self.clone()
    }

    fn find_line(
        &self,
        buf: &[u8],
        from: usize,
        lim: usize,
        eol: u8,
    ) -> ExecResult<Option<(usize, usize)>> {
        let found = self.find_at(&buf[..lim], from).map_err(|e| e.to_string())?;
        Ok(found.map(|m| {
            let s = memrchr(eol, &buf[from..m.start()]).map_or(from, |i| from + i + 1);
            let e = memchr(eol, &buf[m.start()..lim]).map_or(lim, |i| m.start() + i + 1);
            (s, e)
        }))
    }

    fn spans(
        &self,
        buf: &[u8],
        beg: usize,
        lim: usize,
        _eol: u8,
    ) -> ExecResult<Vec<(usize, usize)>> {
        let line = &buf[beg..lim - 1];
        spans_by(buf, beg, lim, |cur| {
            if cur > line.len() + beg {
                return Ok(None);
            }
            self.find_at(line, cur - beg)
                .map(|m| m.map(|m| (beg + m.start(), beg + m.end())))
                .map_err(|e| e.to_string())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListFiles {
    None,
    Matching,
    NonMatching,
}

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct GnuPrinterConfig {
    pub(crate) output: OutputConfig,
    pub(crate) filename_option: Option<bool>,
    pub(crate) operand_count: usize,
    pub(crate) list_files: ListFiles,
    pub(crate) count_matches: bool,
    pub(crate) out_quiet: bool,
    pub(crate) done_on_match: bool,
    pub(crate) context: bool,
    pub(crate) before: usize,
    pub(crate) after: usize,
    pub(crate) group_separator: Option<Vec<u8>>,
    pub(crate) max_count: u64,
    pub(crate) line_buffered: bool,
    pub(crate) defer_lead: bool,
    pub(crate) file_threads: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct GnuPrinter<W> {
    config: std::sync::Arc<GnuPrinterConfig>,
    wtr: W,
    used: bool,
    spare: Vec<u8>,
    spare_out: Vec<u8>,
}

#[derive(Debug, Default)]
pub(crate) struct GnuOutcome {
    pub(crate) count: u64,
    pub(crate) after_message: Option<String>,
    pub(crate) fatal: Option<String>,
    pub(crate) lead_separator: Option<Vec<u8>>,
    pub(crate) used: bool,
    pub(crate) error: Option<String>,
}

pub(crate) struct InputInfo<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) depth: usize,
    pub(crate) size: Option<u64>,
    pub(crate) stdin: bool,
    pub(crate) regular: bool,
    pub(crate) file: Option<&'a std::fs::File>,
}

impl<W: WriteColor> GnuPrinter<W> {
    pub(crate) fn new(config: GnuPrinterConfig, wtr: W) -> GnuPrinter<W> {
        GnuPrinter {
            config: std::sync::Arc::new(config),
            wtr,
            used: false,
            spare: Vec::new(),
            spare_out: Vec::new(),
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut W {
        &mut self.wtr
    }

    pub(crate) fn config(&self) -> &GnuPrinterConfig {
        &self.config
    }

    pub(crate) fn set_defer_lead(&mut self, yes: bool) {
        std::sync::Arc::make_mut(&mut self.config).defer_lead = yes;
    }

    pub(crate) fn search<X: GnuExec + Sync>(
        &mut self,
        exec: &X,
        reader: &mut dyn Read,
        input: &InputInfo<'_>,
    ) -> io::Result<GnuOutcome> {
        if self.config.defer_lead {
            self.used = false;
        }
        let config = std::sync::Arc::clone(&self.config);
        let mut output = config.output.clone();
        output.prefix.with_filename = config
            .filename_option
            .unwrap_or(config.operand_count > 1 || input.depth > 0);
        if output.layout.initial_tab {
            output.offset_width = crate::hc_internal::gnu::output::offset_width(
                input.size,
                output.prefix.line_number,
            );
        }
        let mut state = Grep {
            exec,
            config: &config,
            output: &output,
            input,
            wtr: &mut self.wtr,
            used: &mut self.used,
            scratch: std::mem::take(&mut self.spare_out),
            buf: Data::Owned(reuse_buffer(std::mem::take(&mut self.spare), output.eol)),
            bufbeg: 1,
            buflim: 1,
            totalcc: 0,
            totalnl: 0,
            lastnl: 1,
            lastout: None,
            outleft: config.max_count,
            pending: 0,
            out_quiet: config.out_quiet,
            done_on_match: config.done_on_match,
            encoding_error_output: false,
            lead: None,
            first_group: true,
            bufoffset: 0,
            after_last_match: 0,
            clean: false,
            pre: Vec::new(),
            pre_at: 0,
            pre_to: 0,
            pre_active: false,
            size: input.size,
            eof: false,
        };
        let result = state.run(reader);
        let lead = state.lead.take();
        let flushed = state.flush_scratch();
        let reclaimed = std::mem::replace(&mut state.buf, Data::Owned(Vec::new()));
        let mut scratch = std::mem::take(&mut state.scratch);
        if let Data::Owned(buf) = reclaimed {
            self.spare = buf;
        }
        scratch.clear();
        self.spare_out = scratch;
        flushed?;
        let mut outcome = match result {
            Ok(outcome) => outcome,
            Err(Failure::Io(err)) => return Err(err),
            Err(Failure::Fatal(msg)) => {
                return Ok(GnuOutcome {
                    fatal: Some(format!("{}: {msg}", String::from_utf8_lossy(input.name))),
                    ..GnuOutcome::default()
                });
            }
        };
        outcome.lead_separator = lead;
        outcome.used = self.used;
        if config.count_matches {
            let mut out = Vec::new();
            output.write_count(
                &mut out,
                output.prefix.with_filename.then_some(input.name),
                outcome.count,
            );
            self.wtr.write_all(&out)?;
        }
        let listed = match config.list_files {
            ListFiles::None => false,
            ListFiles::Matching => outcome.count > 0,
            ListFiles::NonMatching => outcome.count == 0,
        };
        if listed {
            let mut out = Vec::new();
            output.write_file_name_line(&mut out, input.name);
            self.wtr.write_all(&out)?;
        }
        if config.line_buffered && (config.count_matches || listed) {
            self.wtr.flush()?;
        }
        Ok(outcome)
    }
}

enum Failure {
    Io(io::Error),
    Fatal(String),
}

impl From<io::Error> for Failure {
    fn from(err: io::Error) -> Failure {
        Failure::Io(err)
    }
}

#[allow(clippy::struct_excessive_bools)]
struct Grep<'a, X, W> {
    exec: &'a X,
    config: &'a GnuPrinterConfig,
    output: &'a OutputConfig,
    input: &'a InputInfo<'a>,
    wtr: &'a mut W,
    used: &'a mut bool,
    scratch: Vec<u8>,
    buf: Data,
    bufbeg: usize,
    buflim: usize,
    totalcc: u64,
    totalnl: u64,
    lastnl: usize,
    lastout: Option<usize>,
    outleft: u64,
    pending: usize,
    out_quiet: bool,
    done_on_match: bool,
    encoding_error_output: bool,
    lead: Option<Vec<u8>>,
    first_group: bool,
    bufoffset: u64,
    after_last_match: u64,
    clean: bool,
    pre: Vec<(usize, usize)>,
    pre_at: usize,
    pre_to: usize,
    pre_active: bool,
    size: Option<u64>,
    eof: bool,
}

fn read_once(reader: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buf) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            other => return other,
        }
    }
}

impl<X: GnuExec + Sync, W: WriteColor> Grep<'_, X, W> {
    fn eol(&self) -> u8 {
        self.output.eol
    }

    fn flush_scratch(&mut self) -> io::Result<()> {
        if !self.scratch.is_empty() {
            self.wtr.write_all(&self.scratch)?;
            self.scratch.clear();
        }
        Ok(())
    }

    fn fillbuf(&mut self, save: usize, reader: &mut dyn Read) -> io::Result<()> {
        let eol = self.eol();
        let readbuf = 1 + save;
        let need = readbuf + GOOD_READSIZE + 1;
        match &mut self.buf {
            Data::Owned(buf) => {
                buf.copy_within(self.buflim - save..self.buflim, 1);
                buf[0] = eol;
                if buf.len() < need {
                    buf.resize(need.max(buf.len() * 2), 0);
                }
            }
            Data::Mapped(map) => {
                let mut buf = vec![0u8; need.max(1 + GOOD_READSIZE + 1)];
                buf[0] = eol;
                buf[1..readbuf].copy_from_slice(&map[self.buflim - save..self.buflim]);
                self.buf = Data::Owned(buf);
            }
        }
        self.bufbeg = 1;
        let Data::Owned(buf) = &mut self.buf else {
            unreachable!("the buffer was made owned above");
        };
        let result = if self.eof {
            Ok(0)
        } else {
            read_once(reader, &mut buf[readbuf..readbuf + GOOD_READSIZE])
        };
        let n = match result {
            Ok(n) => {
                if self.input.regular && n < GOOD_READSIZE {
                    self.eof = true;
                }
                n
            }
            Err(err) => {
                self.buflim = readbuf;
                return Err(err);
            }
        };
        self.bufoffset += n as u64;
        self.buflim = readbuf + n;
        Ok(())
    }

    fn has_holes(&self, read: u64) -> bool {
        self.input
            .file
            .zip(self.size)
            .is_some_and(|(file, size)| has_hole_after(file, size, read))
    }

    fn first_fill(&mut self, reader: &mut dyn Read) -> io::Result<()> {
        self.fillbuf(0, reader)?;
        if self.size.is_none()
            && self.buflim - self.bufbeg == GOOD_READSIZE
            && let Some(file) = self.input.file
        {
            self.size = file
                .metadata()
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map(|md| md.len());
            self.map_whole();
        }
        Ok(())
    }

    fn map_whole(&mut self) -> bool {
        let (Some(file), Some(size)) = (self.input.file, self.size) else {
            return false;
        };
        if self.config.file_threads <= 1 || size < PARALLEL_MIN as u64 {
            return false;
        }
        let Ok(map) = (unsafe { memmap2::MmapOptions::new().map_copy_read_only(file) }) else {
            return false;
        };
        #[cfg(unix)]
        let _ = map.advise(memmap2::Advice::WillNeed);
        let detect = self.output.eol != 0 && self.output.binary_files != BinaryFiles::Text;
        if detect && has_nul(&map, self.config.file_threads) {
            return false;
        }
        let mut cursor = file;
        if std::io::Seek::seek(&mut cursor, io::SeekFrom::Start(map.len() as u64)).is_err() {
            return false;
        }
        self.bufbeg = 0;
        self.buflim = map.len();
        self.lastnl = 0;
        self.bufoffset = map.len() as u64;
        self.clean = true;
        self.buf = Data::Mapped(map);
        true
    }

    fn prepare(&mut self, beg: usize, lim: usize) {
        self.pre_active = self.config.file_threads > 1
            && lim - beg >= PARALLEL_MIN
            && !self.done_on_match
            && self.config.list_files == ListFiles::None
            && self.outleft == u64::MAX;
        self.pre.clear();
        self.pre_at = 0;
        self.pre_to = beg;
    }

    fn next_match(&mut self, p: usize, lim: usize) -> Result<Option<(usize, usize)>, Failure> {
        if !self.pre_active {
            return self
                .exec
                .find_line(&self.buf, p, lim, self.eol())
                .map_err(Failure::Fatal);
        }
        loop {
            while self.pre.get(self.pre_at).is_some_and(|m| m.0 < p) {
                self.pre_at += 1;
            }
            if let Some(&found) = self.pre.get(self.pre_at) {
                return Ok((found.0 < lim).then_some(found));
            }
            if self.pre_to >= lim {
                return Ok(None);
            }
            let eol = self.eol();
            let start = p.max(self.pre_to);
            let target = start.saturating_add(WINDOW * self.config.file_threads);
            let end = if target >= lim {
                lim
            } else {
                memchr(eol, &self.buf[target..lim]).map_or(lim, |i| target + i + 1)
            };
            self.pre = precompute(
                self.exec,
                &self.buf,
                start,
                end,
                eol,
                self.config.file_threads,
            )
            .map_err(Failure::Fatal)?;
            self.pre_at = 0;
            self.pre_to = end;
        }
    }

    fn nlscan(&mut self, lim: usize) {
        let eol = self.eol();
        if self.lastnl < lim {
            self.totalnl += memchr_iter(eol, &self.buf[self.lastnl..lim]).count() as u64;
        }
        self.lastnl = lim;
    }

    fn line_end(&self, p: usize) -> usize {
        memchr(self.eol(), &self.buf[p..self.buflim]).map_or(self.buflim, |i| p + i) + 1
    }

    fn prline(&mut self, beg: usize, lim: usize, kind: LineKind) -> Result<(), Failure> {
        let line_number = if self.output.prefix.line_number {
            if self.lastnl < lim {
                self.nlscan(beg);
                self.totalnl += 1;
                self.lastnl = lim;
            }
            self.totalnl
        } else {
            0
        };
        let matching = (kind == LineKind::Selected) ^ self.output.invert;
        let spans =
            if matching && (self.output.layout.only_matching || self.output.colors.is_some()) {
                self.exec
                    .spans(&self.buf, beg, lim, self.eol())
                    .map_err(Failure::Fatal)?
            } else {
                Vec::new()
            };
        let info = LineInfo {
            filename: self.input.name,
            line_number,
            byte_offset: self.totalcc + (beg - self.bufbeg) as u64,
        };
        let outcome = self.output.write_line(
            &mut self.scratch,
            kind,
            &self.buf[beg..lim - 1],
            &spans,
            info,
        );
        if outcome == LineOutcome::Suppressed {
            self.encoding_error_output = true;
            return Ok(());
        }
        if self.config.line_buffered {
            self.flush_scratch()?;
            self.wtr.flush()?;
        }
        self.lastout = Some(lim);
        Ok(())
    }

    fn prpending(&mut self, lim: usize) -> Result<(), Failure> {
        if self.lastout.is_none() {
            self.lastout = Some(self.bufbeg);
        }
        while self.pending > 0 {
            let Some(lastout) = self.lastout else {
                break;
            };
            if lastout >= lim {
                break;
            }
            let nl = self.line_end(lastout);
            self.prline(lastout, nl, LineKind::Context)?;
            self.pending -= 1;
        }
        Ok(())
    }

    fn group_separator(&mut self) {
        let Some(sep) = &self.config.group_separator else {
            return;
        };
        if self.config.defer_lead && self.first_group && !*self.used {
            let mut rendered = Vec::new();
            self.output.write_group_separator(&mut rendered, sep);
            self.lead = Some(rendered);
            return;
        }
        if *self.used {
            self.output.write_group_separator(&mut self.scratch, sep);
        }
    }

    fn prtext(&mut self, beg: usize, lim: usize) -> Result<(), Failure> {
        let eol = self.eol();
        if !self.out_quiet && self.pending > 0 {
            self.prpending(beg)?;
        }
        let mut p = beg;
        if !self.out_quiet {
            let bp = self.lastout.unwrap_or(self.bufbeg);
            for _ in 0..self.config.before {
                if p > bp {
                    p -= 1;
                    while p > self.bufbeg && self.buf[p - 1] != eol {
                        p -= 1;
                    }
                }
            }
            if self.config.context && Some(p) != self.lastout {
                self.group_separator();
            }
            while p < beg {
                let nl = self.line_end(p);
                self.prline(p, nl, LineKind::Context)?;
                p = nl;
            }
        }
        let n = if self.output.invert {
            let mut n = 0;
            while p < lim && n < self.outleft {
                let nl = self.line_end(p);
                if !self.out_quiet {
                    self.prline(p, nl, LineKind::Selected)?;
                }
                p = nl;
                n += 1;
            }
            n
        } else {
            if !self.out_quiet {
                self.prline(beg, lim, LineKind::Selected)?;
            }
            p = lim;
            1
        };
        self.after_last_match = self.bufoffset - (self.buflim - p) as u64;
        self.pending = if self.out_quiet { 0 } else { self.config.after };
        *self.used = true;
        self.first_group = false;
        self.outleft -= n;
        Ok(())
    }

    fn print_parallel(&mut self, beg: usize, lim: usize) -> Result<u64, Failure> {
        use rayon::prelude::*;
        let eol = self.eol();
        let threads = self.config.file_threads;
        let numbered = self.output.prefix.line_number;
        let invert = self.output.invert;
        let spans_wanted =
            !invert && (self.output.layout.only_matching || self.output.colors.is_some());
        let mut total = 0u64;
        let mut start = beg;
        let mut line_base = if numbered {
            self.totalnl + memchr_iter(eol, &self.buf[self.bufbeg..beg]).count() as u64
        } else {
            0
        };
        while start < lim {
            let target = start.saturating_add(WINDOW * threads);
            let end = if target >= lim {
                lim
            } else {
                memchr(eol, &self.buf[target..lim]).map_or(lim, |i| target + i + 1)
            };
            let bounds = line_bounds(&self.buf, start, end, eol, threads * 4);
            let buf: &[u8] = &self.buf;
            let counts: Vec<u64> = if numbered {
                bounds
                    .par_windows(2)
                    .map(|w| memchr_iter(eol, &buf[w[0]..w[1]]).count() as u64)
                    .collect()
            } else {
                vec![0; bounds.len() - 1]
            };
            let mut firsts = Vec::with_capacity(counts.len());
            let mut acc = line_base;
            for count in &counts {
                firsts.push(acc);
                acc += count;
            }
            let exec = self.exec;
            let output = self.output;
            let name = self.input.name;
            let totalcc = self.totalcc;
            let bufbeg = self.bufbeg;
            let pieces: Vec<ExecResult<Piece>> = bounds
                .par_windows(2)
                .zip(firsts.par_iter())
                .map_init(
                    || exec.fork(),
                    |exec, (w, &first)| {
                        let (s, e) = (w[0], w[1]);
                        let mut out = Vec::new();
                        let mut selected = 0u64;
                        let mut encoding = false;
                        let mut last_end = None;
                        let mut line_no = first;
                        let mut counted = s;
                        let mut emit = |ls: usize, le: usize| -> ExecResult<()> {
                            if numbered {
                                line_no += memchr_iter(eol, &buf[counted..ls]).count() as u64;
                                counted = ls;
                            }
                            let spans = if spans_wanted {
                                exec.spans(buf, ls, le, eol)?
                            } else {
                                Vec::new()
                            };
                            let info = LineInfo {
                                filename: name,
                                line_number: line_no + 1,
                                byte_offset: totalcc + (ls - bufbeg) as u64,
                            };
                            if output.write_line(
                                &mut out,
                                LineKind::Selected,
                                &buf[ls..le - 1],
                                &spans,
                                info,
                            ) == LineOutcome::Suppressed
                            {
                                encoding = true;
                            } else {
                                last_end = Some(le);
                            }
                            selected += 1;
                            Ok(())
                        };
                        let mut p = s;
                        while p < e {
                            let found = exec.find_line(buf, p, e, eol)?;
                            let (b, m_end) = found.unwrap_or((e, e));
                            if invert {
                                let mut q = p;
                                while q < b {
                                    let qe = memchr(eol, &buf[q..b]).map_or(b, |i| q + i + 1);
                                    emit(q, qe)?;
                                    q = qe;
                                }
                            } else if found.is_some() {
                                emit(b, m_end)?;
                            }
                            if found.is_none() {
                                break;
                            }
                            p = m_end;
                        }
                        Ok((out, selected, encoding, last_end))
                    },
                )
                .collect();
            for piece in pieces {
                let (out, selected, encoding, last_end) = piece.map_err(Failure::Fatal)?;
                self.scratch.extend_from_slice(&out);
                total += selected;
                self.encoding_error_output |= encoding;
                if last_end.is_some() {
                    self.lastout = last_end;
                }
            }
            if numbered {
                line_base = acc;
            }
            self.flush_scratch()?;
            start = end;
        }
        if numbered {
            self.totalnl = line_base;
            self.lastnl = lim;
        }
        if total > 0 {
            *self.used = true;
            self.first_group = false;
        }
        self.outleft -= total;
        Ok(total)
    }

    fn grepbuf(&mut self, beg: usize, lim: usize) -> Result<u64, Failure> {
        if self.out_quiet
            && matches!(self.buf, Data::Mapped(_))
            && self.config.list_files == ListFiles::None
            && !self.done_on_match
            && self.outleft == u64::MAX
            && self.config.file_threads > 1
            && lim - beg >= PARALLEL_MIN
        {
            let n = parallel_count(
                self.exec,
                &self.buf,
                beg,
                lim,
                self.eol(),
                self.config.file_threads,
                self.output.invert,
            )
            .map_err(Failure::Fatal)?;
            if n > 0 {
                *self.used = true;
            }
            self.outleft -= n;
            return Ok(n);
        }
        if !self.out_quiet
            && matches!(self.buf, Data::Mapped(_))
            && !self.config.context
            && self.config.before == 0
            && self.config.after == 0
            && self.config.list_files == ListFiles::None
            && !self.done_on_match
            && self.outleft == u64::MAX
            && self.config.file_threads > 1
            && !self.config.line_buffered
            && lim - beg >= PARALLEL_MIN
        {
            return self.print_parallel(beg, lim);
        }
        let outleft0 = self.outleft;
        let mut p = beg;
        while p < lim {
            let found = self.next_match(p, lim)?;
            let (b, endp) = match found {
                Some(found) => found,
                None if !self.output.invert => break,
                None => (lim, lim),
            };
            if !self.output.invert && b == lim {
                break;
            }
            if !self.output.invert || p < b {
                if self.config.list_files != ListFiles::None {
                    return Ok(1);
                }
                let (prbeg, prend) = if self.output.invert {
                    (p, b)
                } else {
                    (b, endp)
                };
                self.prtext(prbeg, prend)?;
                if self.outleft == 0 || self.done_on_match {
                    break;
                }
            }
            p = endp;
        }
        Ok(outleft0 - self.outleft)
    }

    fn zap_nuls(&mut self, beg: usize, lim: usize) {
        let eol = self.eol();
        let Data::Owned(buf) = &mut self.buf else {
            return;
        };
        for byte in &mut buf[beg..lim] {
            if *byte == 0 {
                *byte = eol;
            }
        }
    }

    fn run(&mut self, reader: &mut dyn Read) -> Result<GnuOutcome, Failure> {
        let eol = self.eol();
        let binary_files = self.output.binary_files;
        let mut nlines: u64 = 0;
        let mut nlines_first_null: Option<u64> = None;
        let mut nul_zapper = false;
        let mut residue = 0usize;
        let mut save = 0usize;
        let mut error = None;
        if !self.map_whole()
            && let Err(err) = self.first_fill(reader)
        {
            return Ok(GnuOutcome {
                error: Some(crate::hc_internal::gnu::output::strerror(&err)),
                ..GnuOutcome::default()
            });
        }
        let mut firsttime = true;
        let mut ineof = false;
        'outer: {
            loop {
                if nlines_first_null.is_none()
                    && !self.clean
                    && eol != 0
                    && binary_files != BinaryFiles::Text
                    && (memchr(0, &self.buf[self.bufbeg..self.buflim]).is_some()
                        || (firsttime && self.has_holes((self.buflim - self.bufbeg) as u64)))
                {
                    if binary_files == BinaryFiles::WithoutMatch {
                        return Ok(GnuOutcome::default());
                    }
                    if !self.config.count_matches {
                        self.out_quiet = true;
                        if self.config.max_count == u64::MAX {
                            self.done_on_match = true;
                        }
                    }
                    nlines_first_null = Some(nlines);
                    nul_zapper = true;
                }
                firsttime = false;
                self.clean = false;
                self.lastnl = self.bufbeg;
                if self.lastout.is_some() {
                    self.lastout = Some(self.bufbeg);
                }
                let mut beg = self.bufbeg + save;
                if beg == self.buflim {
                    ineof = true;
                    break;
                }
                if nul_zapper {
                    self.zap_nuls(beg, self.buflim);
                }
                let last_eol = memrchr(eol, &self.buf[beg..self.buflim]).map(|i| beg + i);
                beg -= residue;
                let lim = last_eol.map_or(beg, |i| i + 1);
                residue = self.buflim - lim;
                if beg < lim {
                    if self.outleft > 0 {
                        self.prepare(beg, lim);
                        nlines += self.grepbuf(beg, lim)?;
                        self.pre_active = false;
                    }
                    if self.pending > 0 {
                        self.prpending(lim)?;
                    }
                    if (self.outleft == 0 && self.pending == 0)
                        || (self.done_on_match && nlines_first_null.unwrap_or(0) < nlines)
                    {
                        break 'outer;
                    }
                }
                let mut b = lim;
                let mut i = 0;
                while i < self.config.before && b > self.bufbeg && Some(b) != self.lastout {
                    i += 1;
                    b -= 1;
                    while b > self.bufbeg && self.buf[b - 1] != eol {
                        b -= 1;
                    }
                }
                if Some(b) != self.lastout {
                    self.lastout = None;
                }
                save = residue + lim - b;
                if self.output.prefix.byte_offset {
                    self.totalcc += (self.buflim - self.bufbeg - save) as u64;
                }
                if self.output.prefix.line_number {
                    self.nlscan(b);
                }
                self.flush_scratch()?;
                if let Err(err) = self.fillbuf(save, reader) {
                    error = Some(crate::hc_internal::gnu::output::strerror(&err));
                    break 'outer;
                }
            }
            if residue > 0 {
                if let Data::Owned(buf) = &mut self.buf {
                    if buf.len() <= self.buflim {
                        buf.push(eol);
                    } else {
                        buf[self.buflim] = eol;
                    }
                }
                self.buflim += 1;
                if self.outleft > 0 {
                    nlines += self.grepbuf(self.bufbeg + save - residue, self.buflim)?;
                }
                if self.pending > 0 {
                    self.prpending(self.buflim)?;
                }
            }
        }
        let out_quiet = self.config.out_quiet;
        let after_message = (binary_files == BinaryFiles::Binary
            && !out_quiet
            && (self.encoding_error_output || nlines_first_null.is_some_and(|n| n < nlines)))
        .then(|| {
            format!(
                "{}: binary file matches",
                String::from_utf8_lossy(self.input.name)
            )
        });
        if self.input.stdin && self.config.list_files == ListFiles::None {
            finalize_stdin(
                self.outleft > 0,
                ineof,
                self.after_last_match,
                self.bufoffset,
            );
        }
        Ok(GnuOutcome {
            count: nlines,
            after_message,
            error,
            ..GnuOutcome::default()
        })
    }
}

#[cfg(unix)]
fn finalize_stdin(outleft: bool, ineof: bool, after_last_match: u64, bufoffset: u64) {
    use std::os::fd::AsFd;
    let stdin = io::stdin();
    let fd = stdin.as_fd();
    let start = rustix::fs::seek(fd, rustix::fs::SeekFrom::Current(0)).ok();
    if outleft {
        if ineof {
            return;
        }
        if start.is_some() && rustix::fs::seek(fd, rustix::fs::SeekFrom::End(0)).is_ok() {
            return;
        }
        let mut sink = vec![0u8; GOOD_READSIZE];
        let mut lock = stdin.lock();
        while let Ok(n) = read_once(&mut lock, &mut sink) {
            if n == 0 {
                break;
            }
        }
    } else if let Some(cur) = start {
        let base = cur.saturating_sub(bufoffset);
        if bufoffset != after_last_match {
            let _ = rustix::fs::seek(fd, rustix::fs::SeekFrom::Start(base + after_last_match));
        }
    }
}

#[cfg(not(unix))]
fn finalize_stdin(outleft: bool, ineof: bool, _after_last_match: u64, _bufoffset: u64) {
    if outleft && !ineof {
        let mut sink = vec![0u8; GOOD_READSIZE];
        let stdin = io::stdin();
        let mut lock = stdin.lock();
        while let Ok(n) = read_once(&mut lock, &mut sink) {
            if n == 0 {
                break;
            }
        }
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd"
))]
pub(crate) fn has_hole_after(file: &std::fs::File, size: u64, read: u64) -> bool {
    if read >= size {
        return false;
    }
    let Ok(hole) = rustix::fs::seek(file, rustix::fs::SeekFrom::Hole(read)) else {
        return false;
    };
    let _ = rustix::fs::seek(file, rustix::fs::SeekFrom::Start(read));
    hole < size
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd"
)))]
pub(crate) fn has_hole_after(_file: &std::fs::File, _size: u64, _read: u64) -> bool {
    false
}

#[cfg(unix)]
#[cfg(unix)]
fn stdout_file_id() -> Option<(u64, u64)> {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;
    static STDOUT_FILE: std::sync::OnceLock<Option<(u64, u64)>> = std::sync::OnceLock::new();
    *STDOUT_FILE.get_or_init(|| {
        let fd = io::stdout().as_fd().try_clone_to_owned().ok()?;
        let out = std::fs::File::from(fd).metadata().ok()?;
        out.is_file().then(|| (out.dev(), out.ino()))
    })
}

#[cfg(unix)]
pub(crate) fn stdout_is_file() -> bool {
    stdout_file_id().is_some()
}

#[cfg(not(unix))]
pub(crate) fn stdout_is_file() -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn same_as_stdout(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    stdout_file_id().is_some_and(|(dev, ino)| md.is_file() && md.dev() == dev && md.ino() == ino)
}

#[cfg(not(unix))]
pub(crate) fn same_as_stdout(_md: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn stdin_metadata() -> Option<std::fs::Metadata> {
    use std::os::fd::AsFd;
    let fd = io::stdin().as_fd().try_clone_to_owned().ok()?;
    std::fs::File::from(fd).metadata().ok()
}

#[cfg(not(unix))]
pub(crate) fn stdin_metadata() -> Option<std::fs::Metadata> {
    None
}

type Piece = (Vec<u8>, u64, bool, Option<usize>);

const PARALLEL_MIN: usize = 4 * 1024 * 1024;
const WINDOW: usize = 8 * 1024 * 1024;

enum Data {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl std::ops::Deref for Data {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Data::Owned(buf) => buf,
            Data::Mapped(map) => map,
        }
    }
}

fn has_nul(data: &[u8], threads: usize) -> bool {
    use rayon::prelude::*;
    if threads <= 1 {
        return memchr(0, data).is_some();
    }
    let part = data.len().div_ceil(threads * 4).max(1);
    data.par_chunks(part)
        .any(|chunk| memchr(0, chunk).is_some())
}

fn precompute<X: GnuExec + Sync>(
    exec: &X,
    buf: &[u8],
    beg: usize,
    lim: usize,
    eol: u8,
    threads: usize,
) -> ExecResult<Vec<(usize, usize)>> {
    use rayon::prelude::*;
    let found: Vec<ExecResult<Vec<(usize, usize)>>> = line_bounds(buf, beg, lim, eol, threads * 4)
        .par_windows(2)
        .map_init(
            || exec.fork(),
            |exec, w| {
                let (start, end) = (w[0], w[1]);
                let mut found = Vec::new();
                let mut p = start;
                while p < end {
                    match exec.find_line(buf, p, end, eol)? {
                        Some((s, e)) => {
                            found.push((s, e));
                            p = e;
                        }
                        None => break,
                    }
                }
                Ok(found)
            },
        )
        .collect();
    let mut all = Vec::new();
    for part in found {
        all.extend(part?);
    }
    Ok(all)
}

fn line_bounds(buf: &[u8], beg: usize, lim: usize, eol: u8, parts: usize) -> Vec<usize> {
    let step = (lim - beg).div_ceil(parts);
    let mut bounds = vec![beg];
    for k in 1..parts {
        let pos = beg + k * step;
        if pos >= lim {
            break;
        }
        let bound = memchr(eol, &buf[pos..lim]).map_or(lim, |i| pos + i + 1);
        if bound < lim && bounds.last().is_some_and(|&last| bound > last) {
            bounds.push(bound);
        }
    }
    bounds.push(lim);
    bounds
}

fn parallel_count<X: GnuExec + Sync>(
    exec: &X,
    buf: &[u8],
    beg: usize,
    lim: usize,
    eol: u8,
    threads: usize,
    invert: bool,
) -> ExecResult<u64> {
    use rayon::prelude::*;
    line_bounds(buf, beg, lim, eol, threads * 4)
        .par_windows(2)
        .map_init(
            || exec.fork(),
            |exec, w| {
                let (start, end) = (w[0], w[1]);
                let mut matched = 0u64;
                let mut p = start;
                while p < end {
                    match exec.find_line(buf, p, end, eol)? {
                        Some((_, e)) => {
                            matched += 1;
                            p = e;
                        }
                        None => break,
                    }
                }
                Ok(if invert {
                    memchr_iter(eol, &buf[start..end]).count() as u64 - matched
                } else {
                    matched
                })
            },
        )
        .try_reduce(|| 0, |a, b| Ok(a + b))
}

fn reuse_buffer(mut buf: Vec<u8>, eol: u8) -> Vec<u8> {
    if buf.len() < 1 + GOOD_READSIZE + 1 {
        buf = vec![eol; 1 + GOOD_READSIZE + 1];
    }
    buf[0] = eol;
    buf
}
