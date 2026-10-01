#[cfg(test)]
use std::env;
use std::fmt;
use std::io::{self, Write};
use std::str::FromStr;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::Flags;

pub trait WriteColor: io::Write {
    fn supports_color(&self) -> bool;

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()>;

    fn reset(&mut self) -> io::Result<()>;

    fn set_hyperlink(&mut self, _link: &HyperlinkSpec) -> io::Result<()> {
        Ok(())
    }

    fn supports_hyperlinks(&self) -> bool {
        false
    }
}

impl<T: ?Sized + WriteColor> WriteColor for &mut T {
    fn supports_color(&self) -> bool {
        (**self).supports_color()
    }

    fn supports_hyperlinks(&self) -> bool {
        (**self).supports_hyperlinks()
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        (**self).set_color(spec)
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        (**self).set_hyperlink(link)
    }

    fn reset(&mut self) -> io::Result<()> {
        (**self).reset()
    }
}

impl<T: ?Sized + WriteColor> WriteColor for Box<T> {
    fn supports_color(&self) -> bool {
        (**self).supports_color()
    }

    fn supports_hyperlinks(&self) -> bool {
        (**self).supports_hyperlinks()
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        (**self).set_color(spec)
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        (**self).set_hyperlink(link)
    }

    fn reset(&mut self) -> io::Result<()> {
        (**self).reset()
    }
}

impl WriteColor for io::Sink {
    fn supports_color(&self) -> bool {
        false
    }

    fn set_color(&mut self, _: &ColorSpec) -> io::Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ColorChoice {
    #[default]
    Auto,
    Never,
}

#[cfg(test)]
impl ColorChoice {
    #[must_use]
    pub fn should_attempt_color(self) -> bool {
        match self {
            ColorChoice::Never => false,
            ColorChoice::Auto => env_allows_color(),
        }
    }
}

#[cfg(test)]
#[cfg(not(windows))]
fn env_allows_color() -> bool {
    match env::var_os("TERM") {
        None => return false,
        Some(k) => {
            if k == "dumb" {
                return false;
            }
        }
    }
    env::var_os("NO_COLOR").is_none()
}

#[cfg(test)]
#[cfg(windows)]
fn env_allows_color() -> bool {
    if let Some(k) = env::var_os("TERM")
        && k == "dumb"
    {
        return false;
    }
    env::var_os("NO_COLOR").is_none()
}

#[derive(Clone, Debug)]
pub struct NoColor<W>(W);

impl<W: Write> NoColor<W> {
    pub fn new(wtr: W) -> NoColor<W> {
        NoColor(wtr)
    }

    #[cfg(test)]
    pub fn into_inner(self) -> W {
        self.0
    }

    pub fn get_ref(&self) -> &W {
        &self.0
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.0
    }
}

impl<W: Write> Write for NoColor<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> WriteColor for NoColor<W> {
    #[inline]
    fn supports_color(&self) -> bool {
        false
    }

    #[inline]
    fn set_color(&mut self, _: &ColorSpec) -> io::Result<()> {
        Ok(())
    }

    #[inline]
    fn reset(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Ansi<W>(W);

impl<W: Write> Ansi<W> {
    pub fn new(wtr: W) -> Ansi<W> {
        Ansi(wtr)
    }

    pub fn get_ref(&self) -> &W {
        &self.0
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.0
    }
}

impl<W: Write> Write for Ansi<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.write_all(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<W: Write> WriteColor for Ansi<W> {
    #[inline]
    fn supports_color(&self) -> bool {
        true
    }

    #[inline]
    fn supports_hyperlinks(&self) -> bool {
        true
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        let mut buf = Vec::with_capacity(32);
        spec.write_ansi(&mut buf);
        self.0.write_all(&buf)
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        self.0.write_all(b"\x1B]8;;")?;
        if let Some(uri) = link.uri() {
            self.0.write_all(uri)?;
        }
        self.0.write_all(b"\x1B\\")
    }

    #[inline]
    fn reset(&mut self) -> io::Result<()> {
        self.0.write_all(b"\x1B[0m")
    }
}

fn write_ansi_code(out: &mut Vec<u8>, prefix: &[u8], codes: &[u8]) {
    out.extend_from_slice(prefix);
    for (i, &code) in codes.iter().enumerate() {
        if i > 0 {
            out.push(b';');
        }
        let c1 = (code / 100) % 10;
        let c2 = (code / 10) % 10;
        let c3 = code % 10;
        if c1 != 0 {
            out.push(b'0' + c1);
        }
        if c2 != 0 || c1 != 0 {
            out.push(b'0' + c2);
        }
        out.push(b'0' + c3);
    }
    out.push(b'm');
}

fn write_color(out: &mut Vec<u8>, fg: bool, color: Color, intense: bool) {
    let basic = |n: u8| -> u8 {
        match color {
            Color::Black => n,
            Color::Red => n + 1,
            Color::Green => n + 2,
            Color::Yellow => n + 3,
            Color::Blue => n + 4,
            Color::Magenta => n + 5,
            Color::Cyan => n + 6,
            _ => n + 7,
        }
    };
    match color {
        Color::Ansi256(c) => {
            write_ansi_code(out, if fg { b"\x1B[38;5;" } else { b"\x1B[48;5;" }, &[c]);
        }
        Color::Rgb(r, g, b) => {
            write_ansi_code(
                out,
                if fg { b"\x1B[38;2;" } else { b"\x1B[48;2;" },
                &[r, g, b],
            );
        }
        _ if intense => {
            write_ansi_code(
                out,
                if fg { b"\x1B[38;5;" } else { b"\x1B[48;5;" },
                &[basic(8)],
            );
        }
        _ => {
            out.extend_from_slice(if fg { b"\x1B[3" } else { b"\x1B[4" });
            out.push(b'0' + basic(0));
            out.push(b'm');
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct BufferWriter {
    printed: AtomicBool,
    separator: Option<Vec<u8>>,
    color_choice: ColorChoice,
}

#[cfg(test)]
impl BufferWriter {
    #[must_use]
    pub fn stdout(choice: ColorChoice) -> BufferWriter {
        BufferWriter {
            printed: AtomicBool::new(false),
            separator: None,
            color_choice: choice,
        }
    }

    pub fn separator(&mut self, sep: Option<Vec<u8>>) {
        self.separator = sep;
    }

    #[must_use]
    pub fn buffer(&self) -> Buffer {
        Buffer::new(self.color_choice)
    }

    pub fn print_to<W: Write>(&self, wtr: &mut W, buf: &Buffer) -> io::Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        if let Some(ref sep) = self.separator
            && self.printed.load(Ordering::Relaxed)
        {
            wtr.write_all(sep)?;
            wtr.write_all(b"\n")?;
        }
        wtr.write_all(buf.as_slice())?;
        self.printed.store(true, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct Buffer(BufferInner);

#[cfg(test)]
#[derive(Clone, Debug)]
enum BufferInner {
    NoColor(NoColor<Vec<u8>>),
    Ansi(Ansi<Vec<u8>>),
}

#[cfg(test)]
impl Buffer {
    #[must_use]
    pub fn new(choice: ColorChoice) -> Buffer {
        if choice.should_attempt_color() {
            Buffer::ansi()
        } else {
            Buffer::no_color()
        }
    }

    #[must_use]
    pub fn no_color() -> Buffer {
        Buffer(BufferInner::NoColor(NoColor(vec![])))
    }

    #[must_use]
    pub fn ansi() -> Buffer {
        Buffer(BufferInner::Ansi(Ansi(vec![])))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        match self.0 {
            BufferInner::NoColor(ref b) => &b.0,
            BufferInner::Ansi(ref b) => &b.0,
        }
    }
}

#[cfg(test)]
impl Write for Buffer {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.0 {
            BufferInner::NoColor(ref mut w) => w.write(buf),
            BufferInner::Ansi(ref mut w) => w.write(buf),
        }
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self.0 {
            BufferInner::NoColor(ref mut w) => w.write_all(buf),
            BufferInner::Ansi(ref mut w) => w.write_all(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
impl WriteColor for Buffer {
    #[inline]
    fn supports_color(&self) -> bool {
        matches!(self.0, BufferInner::Ansi(_))
    }

    #[inline]
    fn supports_hyperlinks(&self) -> bool {
        matches!(self.0, BufferInner::Ansi(_))
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        match self.0 {
            BufferInner::NoColor(ref mut w) => w.set_color(spec),
            BufferInner::Ansi(ref mut w) => w.set_color(spec),
        }
    }

    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        match self.0 {
            BufferInner::NoColor(ref mut w) => w.set_hyperlink(link),
            BufferInner::Ansi(ref mut w) => w.set_hyperlink(link),
        }
    }

    fn reset(&mut self) -> io::Result<()> {
        match self.0 {
            BufferInner::NoColor(ref mut w) => w.reset(),
            BufferInner::Ansi(ref mut w) => w.reset(),
        }
    }
}

const BOLD: u32 = 1;
const INTENSE: u32 = 1 << 1;
const UNDERLINE: u32 = 1 << 2;
const DIMMED: u32 = 1 << 3;
const ITALIC: u32 = 1 << 4;
const RESET: u32 = 1 << 5;
const STRIKETHROUGH: u32 = 1 << 6;
const STYLES: u32 = BOLD | INTENSE | UNDERLINE | DIMMED | ITALIC | STRIKETHROUGH;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorSpec {
    fg_color: Option<Color>,
    bg_color: Option<Color>,
    flags: Flags,
}

impl Default for ColorSpec {
    fn default() -> ColorSpec {
        ColorSpec {
            fg_color: None,
            bg_color: None,
            flags: Flags(RESET),
        }
    }
}

impl ColorSpec {
    #[cfg(test)]
    #[must_use]
    pub fn new() -> ColorSpec {
        ColorSpec::default()
    }

    pub fn set_fg(&mut self, color: Option<Color>) -> &mut ColorSpec {
        self.fg_color = color;
        self
    }

    pub fn set_bg(&mut self, color: Option<Color>) -> &mut ColorSpec {
        self.bg_color = color;
        self
    }

    pub fn set_bold(&mut self, yes: bool) -> &mut ColorSpec {
        self.flags.set(BOLD, yes);
        self
    }

    pub fn set_italic(&mut self, yes: bool) -> &mut ColorSpec {
        self.flags.set(ITALIC, yes);
        self
    }

    pub fn set_underline(&mut self, yes: bool) -> &mut ColorSpec {
        self.flags.set(UNDERLINE, yes);
        self
    }

    pub fn set_intense(&mut self, yes: bool) -> &mut ColorSpec {
        self.flags.set(INTENSE, yes);
        self
    }

    #[must_use]
    pub fn is_none(&self) -> bool {
        self.fg_color.is_none() && self.bg_color.is_none() && !self.flags.contains(STYLES)
    }

    pub fn clear(&mut self) {
        self.fg_color = None;
        self.bg_color = None;
        self.flags.set(STYLES, false);
    }

    pub fn write_ansi(&self, out: &mut Vec<u8>) {
        if self.flags.contains(RESET) {
            out.extend_from_slice(b"\x1B[0m");
        }
        if self.flags.contains(BOLD) {
            out.extend_from_slice(b"\x1B[1m");
        }
        if self.flags.contains(DIMMED) {
            out.extend_from_slice(b"\x1B[2m");
        }
        if self.flags.contains(ITALIC) {
            out.extend_from_slice(b"\x1B[3m");
        }
        if self.flags.contains(UNDERLINE) {
            out.extend_from_slice(b"\x1B[4m");
        }
        if self.flags.contains(STRIKETHROUGH) {
            out.extend_from_slice(b"\x1B[9m");
        }
        if let Some(c) = self.fg_color {
            write_color(out, true, c, self.flags.contains(INTENSE));
        }
        if let Some(c) = self.bg_color {
            write_color(out, false, c, self.flags.contains(INTENSE));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    Black,
    Blue,
    Green,
    Red,
    Cyan,
    Magenta,
    Yellow,
    White,
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    fn from_str_numeric(s: &str) -> Result<Color, ParseColorError> {
        fn parse_number(s: &str) -> Option<u8> {
            match s.strip_prefix("0x") {
                Some(hex) => u8::from_str_radix(hex, 16).ok(),
                None => s.parse::<u8>().ok(),
            }
        }
        let err = |kind| ParseColorError {
            kind,
            given: s.to_string(),
        };
        let codes: Vec<&str> = s.split(',').collect();
        if codes.len() == 1 {
            if let Some(n) = parse_number(codes[0]) {
                Ok(Color::Ansi256(n))
            } else if s.chars().all(|c| c.is_ascii_hexdigit()) {
                Err(err(ParseColorErrorKind::InvalidAnsi256))
            } else {
                Err(err(ParseColorErrorKind::InvalidName))
            }
        } else if codes.len() == 3 {
            let mut v = vec![];
            for code in codes {
                v.push(parse_number(code).ok_or_else(|| err(ParseColorErrorKind::InvalidRgb))?);
            }
            Ok(Color::Rgb(v[0], v[1], v[2]))
        } else if s.contains(',') {
            Err(err(ParseColorErrorKind::InvalidRgb))
        } else {
            Err(err(ParseColorErrorKind::InvalidName))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseColorError {
    kind: ParseColorErrorKind,
    given: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParseColorErrorKind {
    InvalidName,
    InvalidAnsi256,
    InvalidRgb,
}

impl ParseColorError {
    #[must_use]
    pub fn invalid(&self) -> &str {
        &self.given
    }
}

impl std::error::Error for ParseColorError {}

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ParseColorErrorKind::InvalidName => write!(
                f,
                "unrecognized color name '{}'. Choose from: black, blue, green, red, cyan, magenta, yellow, white",
                self.given
            ),
            ParseColorErrorKind::InvalidAnsi256 => write!(
                f,
                "unrecognized ansi256 color number, should be '[0-255]' (or a hex number), but is '{}'",
                self.given
            ),
            ParseColorErrorKind::InvalidRgb => write!(
                f,
                "unrecognized RGB color triple, should be '[0-255],[0-255],[0-255]' (or a hex triple), but is '{}'",
                self.given
            ),
        }
    }
}

impl FromStr for Color {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Color, ParseColorError> {
        match &*s.to_lowercase() {
            "black" => Ok(Color::Black),
            "blue" => Ok(Color::Blue),
            "green" => Ok(Color::Green),
            "red" => Ok(Color::Red),
            "cyan" => Ok(Color::Cyan),
            "magenta" => Ok(Color::Magenta),
            "yellow" => Ok(Color::Yellow),
            "white" => Ok(Color::White),
            _ => Color::from_str_numeric(s),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HyperlinkSpec<'a> {
    uri: Option<&'a [u8]>,
}

impl<'a> HyperlinkSpec<'a> {
    #[must_use]
    pub fn open(uri: &'a [u8]) -> HyperlinkSpec<'a> {
        HyperlinkSpec { uri: Some(uri) }
    }

    #[must_use]
    pub fn close() -> HyperlinkSpec<'a> {
        HyperlinkSpec { uri: None }
    }

    #[must_use]
    pub fn uri(&self) -> Option<&'a [u8]> {
        self.uri
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ansi(spec: &ColorSpec) -> String {
        let mut out = vec![];
        spec.write_ansi(&mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn buffer_writer_separator() {
        let mut bw = BufferWriter::stdout(ColorChoice::Never);
        bw.separator(Some(b"--".to_vec()));
        let mut out = vec![];
        let mut a = bw.buffer();
        a.write_all(b"a\n").unwrap();
        let empty = bw.buffer();
        let mut b = bw.buffer();
        b.write_all(b"b\n").unwrap();
        bw.print_to(&mut out, &a).unwrap();
        bw.print_to(&mut out, &empty).unwrap();
        bw.print_to(&mut out, &b).unwrap();
        assert_eq!(out, b"a\n--\nb\n");
    }

    #[test]
    fn color_parse_and_ansi() {
        let mut spec = ColorSpec::new();
        spec.set_fg(Some("red".parse().unwrap())).set_bold(true);
        assert_eq!(ansi(&spec), "\x1B[0m\x1B[1m\x1B[31m");
        spec.set_intense(true);
        assert_eq!(ansi(&spec), "\x1B[0m\x1B[1m\x1B[38;5;9m");
        let mut spec = ColorSpec::new();
        spec.set_bg(Some("0x10".parse().unwrap()));
        assert_eq!(ansi(&spec), "\x1B[0m\x1B[48;5;16m");
        let mut spec = ColorSpec::new();
        spec.set_fg(Some("255,0,10".parse().unwrap()));
        assert_eq!(ansi(&spec), "\x1B[0m\x1B[38;2;255;0;10m");
        assert_eq!(
            "nope".parse::<Color>().unwrap_err().to_string(),
            "unrecognized color name 'nope'. Choose from: black, blue, green, red, cyan, magenta, yellow, white"
        );
        assert_eq!(
            "256".parse::<Color>().unwrap_err().to_string(),
            "unrecognized ansi256 color number, should be '[0-255]' (or a hex number), but is '256'"
        );
        assert!("1,2".parse::<Color>().is_err());
    }
}
