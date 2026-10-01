use std::io::{self, BufWriter, LineWriter, Stdout, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::hc_internal::grep::printer::termcolor::{Ansi, ColorSpec, NoColor, WriteColor};

pub(crate) enum Sink {
    Line(LineWriter<Stdout>),
    Block(BufWriter<Stdout>),
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Sink::Line(w) => w.write(buf),
            Sink::Block(w) => w.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            Sink::Line(w) => w.write_all(buf),
            Sink::Block(w) => w.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Sink::Line(w) => w.flush(),
            Sink::Block(w) => w.flush(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Colored<W: Write> {
    Ansi(Ansi<W>),
    Plain(NoColor<W>),
}

impl<W: Write> Colored<W> {
    pub(crate) fn new(wtr: W, color: bool) -> Colored<W> {
        if color {
            Colored::Ansi(Ansi::new(wtr))
        } else {
            Colored::Plain(NoColor::new(wtr))
        }
    }

    pub(crate) fn get_ref(&self) -> &W {
        match self {
            Colored::Ansi(w) => w.get_ref(),
            Colored::Plain(w) => w.get_ref(),
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut W {
        match self {
            Colored::Ansi(w) => w.get_mut(),
            Colored::Plain(w) => w.get_mut(),
        }
    }
}

impl<W: Write> Write for Colored<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Colored::Ansi(w) => w.write(buf),
            Colored::Plain(w) => w.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            Colored::Ansi(w) => w.write_all(buf),
            Colored::Plain(w) => w.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Colored::Ansi(w) => w.flush(),
            Colored::Plain(w) => w.flush(),
        }
    }
}

impl<W: Write> WriteColor for Colored<W> {
    fn supports_color(&self) -> bool {
        match self {
            Colored::Ansi(w) => w.supports_color(),
            Colored::Plain(w) => w.supports_color(),
        }
    }

    fn supports_hyperlinks(&self) -> bool {
        match self {
            Colored::Ansi(w) => w.supports_hyperlinks(),
            Colored::Plain(w) => w.supports_hyperlinks(),
        }
    }

    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        match self {
            Colored::Ansi(w) => w.set_color(spec),
            Colored::Plain(w) => w.set_color(spec),
        }
    }

    fn set_hyperlink(
        &mut self,
        link: &crate::hc_internal::grep::printer::termcolor::HyperlinkSpec<'_>,
    ) -> io::Result<()> {
        match self {
            Colored::Ansi(w) => w.set_hyperlink(link),
            Colored::Plain(w) => w.set_hyperlink(link),
        }
    }

    fn reset(&mut self) -> io::Result<()> {
        match self {
            Colored::Ansi(w) => w.reset(),
            Colored::Plain(w) => w.reset(),
        }
    }
}

pub(crate) type StdoutStream = Colored<Sink>;

pub(crate) fn stdout(color: bool, line_buffered: bool) -> StdoutStream {
    let sink = if line_buffered {
        Sink::Line(LineWriter::new(io::stdout()))
    } else {
        Sink::Block(BufWriter::with_capacity(64 * 1024, io::stdout()))
    };
    Colored::new(sink, color)
}

pub(crate) type Buffer = Colored<Vec<u8>>;

impl Buffer {
    pub(crate) fn clear(&mut self) {
        self.get_mut().clear();
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        self.get_ref()
    }
}

pub(crate) struct BufferWriter {
    stdout: Mutex<Sink>,
    color: bool,
    separator: Option<Vec<u8>>,
    printed: AtomicBool,
    gnu_used: AtomicBool,
}

impl BufferWriter {
    pub(crate) fn new(
        color: bool,
        line_buffered: bool,
        separator: Option<Vec<u8>>,
    ) -> BufferWriter {
        let sink = if line_buffered {
            Sink::Line(LineWriter::new(io::stdout()))
        } else {
            Sink::Block(BufWriter::with_capacity(256 * 1024, io::stdout()))
        };
        BufferWriter {
            stdout: Mutex::new(sink),
            color,
            separator,
            printed: AtomicBool::new(false),
            gnu_used: AtomicBool::new(false),
        }
    }

    pub(crate) fn buffer(&self) -> Buffer {
        Colored::new(Vec::new(), self.color)
    }

    pub(crate) fn print(&self, buf: &Buffer) -> io::Result<()> {
        self.print_with(buf, None, false)
    }

    pub(crate) fn print_with(
        &self,
        buf: &Buffer,
        lead: Option<&[u8]>,
        used: bool,
    ) -> io::Result<()> {
        let mut stdout = self
            .stdout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !buf.as_slice().is_empty() {
            if let Some(sep) = &self.separator
                && self.printed.load(Ordering::Relaxed)
            {
                stdout.write_all(sep)?;
                stdout.write_all(b"\n")?;
            }
            if let Some(lead) = lead
                && self.gnu_used.load(Ordering::Relaxed)
            {
                stdout.write_all(lead)?;
            }
            stdout.write_all(buf.as_slice())?;
            self.printed.store(true, Ordering::Relaxed);
        }
        if used {
            self.gnu_used.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub(crate) fn flush(&self) -> io::Result<()> {
        self.stdout
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flush()
    }
}
