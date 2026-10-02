use std::io::{self, Write};

use super::termcolor::{ColorSpec, HyperlinkSpec, WriteColor};

#[derive(Clone, Debug)]
pub(crate) struct CounterWriter<W> {
    wtr: W,
    count: u64,
    total_count: u64,
}

impl<W: Write> CounterWriter<W> {
    pub(crate) fn new(wtr: W) -> CounterWriter<W> {
        CounterWriter {
            wtr,
            count: 0,
            total_count: 0,
        }
    }
}

impl<W> CounterWriter<W> {
    #[inline]
    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    #[inline]
    pub(crate) fn total_count(&self) -> u64 {
        self.total_count + self.count
    }

    #[inline]
    pub(crate) fn reset_count(&mut self) {
        self.total_count += self.count;
        self.count = 0;
    }

    #[inline]
    pub(crate) fn get_mut(&mut self) -> &mut W {
        &mut self.wtr
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn into_inner(self) -> W {
        self.wtr
    }
}

impl<W: Write> Write for CounterWriter<W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        let n = self.wtr.write(buf)?;
        self.count += n as u64;
        Ok(n)
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> Result<(), io::Error> {
        self.wtr.write_all(buf)?;
        self.count += buf.len() as u64;
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> Result<(), io::Error> {
        self.wtr.flush()
    }
}

impl<W: WriteColor> WriteColor for CounterWriter<W> {
    #[inline]
    fn supports_color(&self) -> bool {
        self.wtr.supports_color()
    }

    #[inline]
    fn supports_hyperlinks(&self) -> bool {
        self.wtr.supports_hyperlinks()
    }

    #[inline]
    fn set_color(&mut self, spec: &ColorSpec) -> io::Result<()> {
        self.wtr.set_color(spec)
    }

    #[inline]
    fn set_hyperlink(&mut self, link: &HyperlinkSpec) -> io::Result<()> {
        self.wtr.set_hyperlink(link)
    }

    #[inline]
    fn reset(&mut self) -> io::Result<()> {
        self.wtr.reset()
    }
}
