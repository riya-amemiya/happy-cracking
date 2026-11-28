use super::colors::Colors;
use super::locale;

pub const GOOD_READSIZE: usize = 96 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryFiles {
    Binary,
    Text,
    WithoutMatch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefix {
    pub with_filename: bool,
    pub line_number: bool,
    pub byte_offset: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub only_matching: bool,
    pub initial_tab: bool,
    pub null_after_name: bool,
}

#[derive(Clone, Debug)]
pub struct OutputConfig {
    pub prefix: Prefix,
    pub layout: Layout,
    pub offset_width: usize,
    pub eol: u8,
    pub colors: Option<Colors>,
    pub invert: bool,
    pub binary_files: BinaryFiles,
    pub utf8: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Selected,
    Context,
}

#[derive(Clone, Copy, Debug)]
pub struct LineInfo<'a> {
    pub filename: &'a [u8],
    pub line_number: u64,
    pub byte_offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineOutcome {
    Printed,
    Suppressed,
}

impl OutputConfig {
    #[must_use]
    pub fn new(eol: u8, utf8: bool) -> Self {
        Self {
            prefix: Prefix::default(),
            layout: Layout::default(),
            offset_width: 0,
            eol,
            colors: None,
            invert: false,
            binary_files: BinaryFiles::Binary,
            utf8,
        }
    }

    fn cap<'c>(&'c self, f: impl Fn(&'c Colors) -> &'c str) -> &'c str {
        self.colors.as_ref().map_or("", f)
    }

    fn start(&self, out: &mut Vec<u8>, cap: &str) {
        if let Some(c) = &self.colors {
            c.start(out, cap);
        }
    }

    fn end(&self, out: &mut Vec<u8>, cap: &str) {
        if let Some(c) = &self.colors {
            c.end(out, cap);
        }
    }

    pub fn write_filename(&self, out: &mut Vec<u8>, name: &[u8]) {
        let cap = self.cap(|c| &c.filename);
        self.start(out, cap);
        out.extend_from_slice(name);
        self.end(out, cap);
    }

    fn write_sep(&self, out: &mut Vec<u8>, sep: u8) {
        let cap = self.cap(|c| &c.separator);
        self.start(out, cap);
        out.push(sep);
        self.end(out, cap);
    }

    fn write_offset(&self, out: &mut Vec<u8>, value: u64, cap: &str) {
        self.start(out, cap);
        let s = value.to_string();
        for _ in s.len()..self.offset_width {
            out.push(b' ');
        }
        out.extend_from_slice(s.as_bytes());
        self.end(out, cap);
    }

    fn encoding_blocked(&self, bytes: &[u8]) -> bool {
        self.binary_files != BinaryFiles::Text && locale::has_encoding_error(self.utf8, bytes)
    }

    fn line_head(&self, out: &mut Vec<u8>, info: LineInfo<'_>, offset: u64, len: usize, sep: u8) {
        if self.prefix.with_filename {
            self.write_filename(out, info.filename);
            if self.layout.null_after_name {
                out.push(0);
            } else {
                self.write_sep(out, sep);
            }
        }
        if self.prefix.line_number {
            self.write_offset(out, info.line_number, self.cap(|c| &c.line_number));
            self.write_sep(out, sep);
        }
        if self.prefix.byte_offset {
            self.write_offset(out, offset, self.cap(|c| &c.byte_offset));
            self.write_sep(out, sep);
        }
        if self.layout.initial_tab
            && (self.prefix.with_filename || self.prefix.line_number || self.prefix.byte_offset)
            && len != 0
        {
            out.push(b'\t');
        }
    }

    pub fn write_line(
        &self,
        out: &mut Vec<u8>,
        kind: LineKind,
        line: &[u8],
        spans: &[(usize, usize)],
        info: LineInfo<'_>,
    ) -> LineOutcome {
        let sep = if kind == LineKind::Selected {
            b':'
        } else {
            b'-'
        };
        if !self.layout.only_matching {
            if self.encoding_blocked(line) {
                return LineOutcome::Suppressed;
            }
            self.line_head(out, info, info.byte_offset, line.len(), sep);
        }
        let matching = (kind == LineKind::Selected) ^ self.invert;
        let reverse = self.colors.as_ref().is_some_and(|c| c.reverse);
        let line_color = if (kind == LineKind::Selected) ^ (self.invert && reverse) {
            self.cap(|c| &c.selected_line)
        } else {
            self.cap(|c| &c.context_line)
        };
        let match_color = if kind == LineKind::Selected {
            self.cap(|c| &c.selected_match)
        } else {
            self.cap(|c| &c.context_match)
        };
        let colored = self.colors.is_some();
        let mut cur = 0;
        if (self.layout.only_matching && matching)
            || (colored && (!line_color.is_empty() || !match_color.is_empty()))
        {
            if matching && (self.layout.only_matching || !match_color.is_empty()) {
                let msep = if self.invert { b'-' } else { b':' };
                for &(b, e) in spans {
                    if self.layout.only_matching {
                        if self.encoding_blocked(&line[b..e]) {
                            return LineOutcome::Suppressed;
                        }
                        self.line_head(out, info, info.byte_offset + b as u64, e - b, msep);
                    } else {
                        self.start(out, line_color);
                        out.extend_from_slice(&line[cur..b]);
                    }
                    self.start(out, match_color);
                    out.extend_from_slice(&line[b..e]);
                    self.end(out, match_color);
                    if self.layout.only_matching {
                        out.push(self.eol);
                    }
                    cur = e;
                }
                if self.layout.only_matching {
                    cur = line.len() + 1;
                }
            }
            if !self.layout.only_matching && !line_color.is_empty() {
                let tail_end = if line.last() == Some(&b'\r') {
                    line.len() - 1
                } else {
                    line.len()
                };
                if tail_end > cur {
                    self.start(out, line_color);
                    out.extend_from_slice(&line[cur..tail_end]);
                    self.end(out, line_color);
                    cur = tail_end;
                }
            }
        }
        if !self.layout.only_matching && cur <= line.len() {
            out.extend_from_slice(&line[cur..]);
            out.push(self.eol);
        }
        LineOutcome::Printed
    }

    pub fn write_group_separator(&self, out: &mut Vec<u8>, separator: &[u8]) {
        let cap = self.cap(|c| &c.separator);
        self.start(out, cap);
        out.extend_from_slice(separator);
        self.end(out, cap);
        out.push(b'\n');
    }

    pub fn write_count(&self, out: &mut Vec<u8>, name: Option<&[u8]>, count: u64) {
        if let Some(name) = name {
            self.write_filename(out, name);
            if self.layout.null_after_name {
                out.push(0);
            } else {
                self.write_sep(out, b':');
            }
        }
        out.extend_from_slice(count.to_string().as_bytes());
        out.push(b'\n');
    }

    pub fn write_file_name_line(&self, out: &mut Vec<u8>, name: &[u8]) {
        self.write_filename(out, name);
        out.push(if self.layout.null_after_name {
            0
        } else {
            b'\n'
        });
    }
}

#[must_use]
pub fn offset_width(file_size: Option<u64>, line_number: bool) -> usize {
    let max = i64::MAX as u64;
    let mut num = file_size.filter(|&s| s <= max).unwrap_or(max);
    if line_number && num < max {
        num += 1;
    }
    num.to_string().len()
}

#[must_use]
pub fn strerror(err: &std::io::Error) -> String {
    let s = err.to_string();
    match s.find(" (os error") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(n: u64, off: u64) -> LineInfo<'static> {
        LineInfo {
            filename: b"f1",
            line_number: n,
            byte_offset: off,
        }
    }

    #[test]
    fn plain_and_prefixed_lines() {
        let mut c = OutputConfig::new(b'\n', true);
        c.prefix.with_filename = true;
        c.prefix.line_number = true;
        let mut out = Vec::new();
        c.write_line(&mut out, LineKind::Selected, b"two", &[], info(2, 4));
        c.write_line(&mut out, LineKind::Context, b"three", &[], info(3, 8));
        assert_eq!(out, b"f1:2:two\nf1-3-three\n");
        c.layout.initial_tab = true;
        c.prefix.with_filename = false;
        c.offset_width = offset_width(Some(34), true);
        let mut out = Vec::new();
        c.write_line(&mut out, LineKind::Selected, b"two", &[], info(2, 4));
        assert_eq!(out, b" 2:\ttwo\n");
    }

    #[test]
    fn only_matching_and_color() {
        let mut c = OutputConfig::new(b'\n', true);
        c.layout.only_matching = true;
        c.prefix.byte_offset = true;
        let mut out = Vec::new();
        c.write_line(
            &mut out,
            LineKind::Selected,
            b"xfoo foo",
            &[(1, 4), (5, 8)],
            info(1, 10),
        );
        assert_eq!(out, b"11:foo\n15:foo\n");
        let mut c = OutputConfig::new(b'\n', true);
        c.colors = Some(Colors::default());
        let mut out = Vec::new();
        c.write_line(&mut out, LineKind::Selected, b"ab", &[(1, 2)], info(1, 0));
        assert_eq!(out, b"a\x1b[01;31m\x1b[Kb\x1b[m\x1b[K\n");
        let mut out = Vec::new();
        let st = c.write_line(&mut out, LineKind::Selected, b"caf\xe9", &[], info(1, 0));
        assert_eq!(st, LineOutcome::Suppressed);
        assert_eq!(out, Vec::<u8>::new());
    }

    #[test]
    fn offset_width_without_a_size() {
        assert_eq!(offset_width(None, false), 19);
    }
}
