mod ast;
mod charset;
mod check;
mod compile;
mod error;
mod lits;
mod parse;
mod possess;
mod prefilter;
mod prog;
mod start;
mod ucd;
mod ucd_tables;
mod utf8check;
mod vm;

use std::cell::RefCell;
use std::sync::Arc;

pub use error::Error;
#[cfg(test)]
pub use error::ErrorKind;

use compile::LookInfo;
use prefilter::Prefilter;
use prog::Prog;
use vm::{Code, Outcome, Scratch, UNSET, Vm};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    pub caseless: bool,
    pub multi_line: bool,
    pub dotall: bool,
    pub extended: bool,
    pub utf: bool,
    pub ucp: bool,
    pub crlf: bool,
    pub dollar_endonly: bool,
    pub ascii_bsd: bool,
    pub match_line: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchOptions {
    pub not_bol: bool,
    pub not_eol: bool,
}

#[derive(Debug)]
struct Inner {
    prog: Prog,
    looks: Vec<LookInfo>,
    names: Vec<Option<String>>,
    pre: Prefilter,
}

#[derive(Clone, Debug)]
pub struct Regex {
    inner: Arc<Inner>,
}

#[derive(Clone, Debug)]
pub struct CaptureLocations {
    locs: Vec<usize>,
}

impl CaptureLocations {
    pub fn get(&self, i: usize) -> Option<(usize, usize)> {
        let s = *self.locs.get(2 * i)?;
        let e = *self.locs.get(2 * i + 1)?;
        if s == UNSET || e == UNSET {
            None
        } else {
            Some((s, e))
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.locs.len() / 2
    }
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::new());
}

impl Regex {
    pub fn new(pattern: &str, config: Config) -> Result<Regex, Error> {
        let pc = parse::ParseConfig {
            caseless: config.caseless,
            multi_line: config.multi_line,
            dotall: config.dotall,
            extended: config.extended,
            utf: config.utf,
            ucp: config.ucp,
            crlf: config.crlf,
            dollar_endonly: config.dollar_endonly,
            ascii_bsd: config.ascii_bsd,
            match_line: config.match_line,
        };
        let ast = parse::parse(pattern, &pc)?;
        let mut names: Vec<Option<String>> = vec![None; ast.capture_count as usize + 1];
        for ng in &ast.names {
            let slot = &mut names[ng.number as usize];
            if slot.is_none() {
                *slot = Some(String::from_utf8_lossy(&ng.name).into_owned());
            }
        }
        let invalid_ok = config.ucp;
        let compiled = compile::compile(&ast, invalid_ok)?;
        let pre = Prefilter::new(&ast, &compiled.prog);
        Ok(Regex {
            inner: Arc::new(Inner {
                prog: compiled.prog,
                looks: compiled.looks,
                names,
                pre,
            }),
        })
    }

    pub fn find_at(&self, haystack: &[u8], start: usize) -> Result<Option<(usize, usize)>, Error> {
        self.search(haystack, start, None, MatchOptions::default())
    }

    pub fn find_at_with(
        &self,
        haystack: &[u8],
        start: usize,
        opts: MatchOptions,
    ) -> Result<Option<(usize, usize)>, Error> {
        self.search(haystack, start, None, opts)
    }

    #[cfg(test)]
    pub fn is_match_at(&self, haystack: &[u8], start: usize) -> Result<bool, Error> {
        Ok(self
            .search(haystack, start, None, MatchOptions::default())?
            .is_some())
    }

    #[cfg(test)]
    pub fn is_match_at_with(
        &self,
        haystack: &[u8],
        start: usize,
        opts: MatchOptions,
    ) -> Result<bool, Error> {
        Ok(self.search(haystack, start, None, opts)?.is_some())
    }

    pub fn capture_locations(&self) -> CaptureLocations {
        CaptureLocations {
            locs: vec![UNSET; self.inner.prog.ncap * 2],
        }
    }

    pub fn captures_read_at(
        &self,
        locs: &mut CaptureLocations,
        haystack: &[u8],
        start: usize,
    ) -> Result<Option<(usize, usize)>, Error> {
        self.captures_read_at_with(locs, haystack, start, MatchOptions::default())
    }

    pub fn captures_read_at_with(
        &self,
        locs: &mut CaptureLocations,
        haystack: &[u8],
        start: usize,
        opts: MatchOptions,
    ) -> Result<Option<(usize, usize)>, Error> {
        let n = self.inner.prog.ncap * 2;
        locs.locs.clear();
        locs.locs.resize(n, UNSET);
        self.search(haystack, start, Some(&mut locs.locs), opts)
    }

    #[cfg(test)]
    pub fn captures_len(&self) -> usize {
        self.inner.prog.ncap
    }

    pub fn capture_names(&self) -> &[Option<String>] {
        &self.inner.names
    }

    fn search(
        &self,
        hay: &[u8],
        start: usize,
        caps: Option<&mut Vec<usize>>,
        opts: MatchOptions,
    ) -> Result<Option<(usize, usize)>, Error> {
        assert!(
            start <= hay.len(),
            "start ({start}) must be <= haystack length ({})",
            hay.len()
        );
        let inner = &*self.inner;
        let prog = &inner.prog;
        if prog.utf
            && !prog.invalid_ok
            && let Some(code) = utf8check::check(hay, start, prog.max_lookbehind)
        {
            return Err(Error::matching(code));
        }
        if inner.pre.rejects(hay, start) {
            return Ok(None);
        }
        SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            let mut vm = Vm::new(prog, &inner.looks, hay, start, &mut scratch);
            vm.notbol = opts.not_bol;
            if !prog.interp {
                vm.set_bounds(0, hay.len(), opts.not_eol);
                return self.scan(&mut vm, hay, start, hay.len(), caps);
            }
            let mut caps = caps;
            let true_end = hay.len();
            let mut first = start;
            let mut skipped = false;
            while first < true_end && hay[first] & 0xc0 == 0x80 {
                first += 1;
                skipped = true;
            }
            let mut floor = first;
            if !skipped {
                for _ in 0..prog.max_lookbehind {
                    if floor == 0 {
                        break;
                    }
                    floor -= 1;
                    while floor > 0 && hay[floor] & 0xc0 == 0x80 {
                        floor -= 1;
                    }
                }
            }
            let mut end = loop {
                match first_invalid(hay, floor) {
                    None => break true_end,
                    Some(e) if e < first => {
                        floor = e + 1;
                        while floor < first && hay[floor] & 0xc0 == 0x80 {
                            floor += 1;
                        }
                    }
                    Some(e) => break e,
                }
            };
            loop {
                vm.set_bounds(floor, end, end != true_end || opts.not_eol);
                if let Some(m) = self.scan(&mut vm, hay, first, end, caps.as_deref_mut())? {
                    return Ok(Some(m));
                }
                if end == true_end {
                    return Ok(None);
                }
                end = loop {
                    first = end + 1;
                    while first < true_end && hay[first] & 0xc0 == 0x80 {
                        first += 1;
                    }
                    if first >= true_end {
                        return Ok(None);
                    }
                    floor = first;
                    match first_invalid(hay, first) {
                        None => break true_end,
                        Some(e) if e > first => break e,
                        Some(e) => end = e,
                    }
                };
            }
        })
    }

    fn scan(
        &self,
        vm: &mut Vm<'_>,
        hay: &[u8],
        start: usize,
        limit: usize,
        caps: Option<&mut Vec<usize>>,
    ) -> Result<Option<(usize, usize)>, Error> {
        let inner = &*self.inner;
        let prog = &inner.prog;
        let utf = prog.utf;
        let mut p = start;
        let mut req_at = usize::MAX;
        loop {
            let Some(cand) = inner.pre.next(prog, hay, p, start) else {
                return Ok(None);
            };
            if cand > limit {
                return Ok(None);
            }
            p = cand;
            if !inner.pre.viable(hay, p, limit, &mut req_at) {
                return Ok(None);
            }
            let outcome = vm.run(p);
            let next = match outcome {
                Outcome::Match(s, e) => {
                    if let Some(c) = caps {
                        let regs = vm.regs();
                        for (i, slot) in c.iter_mut().enumerate().skip(2) {
                            *slot = regs[i];
                        }
                        c[0] = s;
                        c[1] = e;
                    }
                    return Ok(Some((s, e)));
                }
                Outcome::Error(code) => return Err(Error::matching(code)),
                Outcome::Fail(code) => match code {
                    Code::Commit(_) => return Ok(None),
                    Code::Skip(q, _) if q > p => q,
                    Code::SkipArg(..) => {
                        vm.ignore_skip_arg = vm.skip_arg_count;
                        continue;
                    }
                    _ if prog.interp => {
                        let mut q = p + 1;
                        while q < limit && hay[q] & 0xc0 == 0x80 {
                            q += 1;
                        }
                        q
                    }
                    _ => match inner.pre.lead_rep() {
                        Some(item) => vm::next_boundary(hay, p, utf).max(vm.run_end(item, p)),
                        None => vm::next_boundary(hay, p, utf),
                    },
                },
            };
            vm.ignore_skip_arg = 0;
            if inner.pre.anchored() || next > limit {
                return Ok(None);
            }
            let mut next = next;
            if next > start
                && next < limit
                && hay[next - 1] == b'\r'
                && hay[next] == b'\n'
                && !prog.has_crorlf
                && matches!(
                    prog.newline,
                    ast::Newline::Any | ast::Newline::AnyCrLf | ast::Newline::CrLf
                )
            {
                next += 1;
            }
            p = next;
        }
    }
}

fn first_invalid(hay: &[u8], from: usize) -> Option<usize> {
    std::str::from_utf8(&hay[from..])
        .err()
        .map(|e| from + e.valid_up_to())
}

pub fn escape(pattern: &str) -> String {
    let mut quoted = String::with_capacity(pattern.len());
    for c in pattern.chars() {
        if matches!(
            c,
            '\\' | '.'
                | '+'
                | '*'
                | '?'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '{'
                | '}'
                | '^'
                | '$'
                | '#'
                | '-'
        ) {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted
}

#[cfg(test)]
mod tests;
