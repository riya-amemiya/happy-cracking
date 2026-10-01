use std::io;

use super::super::matcher::LineTerminator;
use super::lines::LineIter;
use super::{ConfigError, Searcher};

pub trait SinkError: Sized {
    fn error_message<T: std::fmt::Display>(message: T) -> Self;

    #[must_use]
    fn error_io(err: io::Error) -> Self {
        Self::error_message(err)
    }

    #[must_use]
    fn error_config(err: ConfigError) -> Self {
        Self::error_message(err)
    }
}

impl SinkError for io::Error {
    fn error_message<T: std::fmt::Display>(message: T) -> io::Error {
        io::Error::other(message.to_string())
    }

    fn error_io(err: io::Error) -> io::Error {
        err
    }
}

impl SinkError for Box<dyn std::error::Error> {
    fn error_message<T: std::fmt::Display>(message: T) -> Box<dyn std::error::Error> {
        Box::<dyn std::error::Error>::from(message.to_string())
    }
}

pub trait Sink {
    type Error: SinkError;

    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error>;

    #[inline]
    fn context(
        &mut self,
        _searcher: &Searcher,
        _context: &SinkContext<'_>,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    #[inline]
    fn context_break(&mut self, _searcher: &Searcher) -> Result<bool, Self::Error> {
        Ok(true)
    }

    #[inline]
    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        _binary_byte_offset: u64,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    #[inline]
    fn begin(&mut self, _searcher: &Searcher) -> Result<bool, Self::Error> {
        Ok(true)
    }

    #[inline]
    fn finish(&mut self, _searcher: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        Ok(())
    }

    #[inline]
    fn count_mode(&self, _searcher: &Searcher) -> SinkCount {
        SinkCount::None
    }

    #[inline]
    fn matched_count(
        &mut self,
        _searcher: &Searcher,
        _lines: u64,
        _matches: u64,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

impl<S: Sink + ?Sized> Sink for &mut S {
    type Error = S::Error;

    #[inline]
    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, S::Error> {
        (**self).matched(searcher, mat)
    }

    #[inline]
    fn context(
        &mut self,
        searcher: &Searcher,
        context: &SinkContext<'_>,
    ) -> Result<bool, S::Error> {
        (**self).context(searcher, context)
    }

    #[inline]
    fn context_break(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        (**self).context_break(searcher)
    }

    #[inline]
    fn binary_data(
        &mut self,
        searcher: &Searcher,
        binary_byte_offset: u64,
    ) -> Result<bool, S::Error> {
        (**self).binary_data(searcher, binary_byte_offset)
    }

    #[inline]
    fn begin(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        (**self).begin(searcher)
    }

    #[inline]
    fn finish(&mut self, searcher: &Searcher, sink_finish: &SinkFinish) -> Result<(), S::Error> {
        (**self).finish(searcher, sink_finish)
    }

    #[inline]
    fn count_mode(&self, searcher: &Searcher) -> SinkCount {
        (**self).count_mode(searcher)
    }

    #[inline]
    fn matched_count(
        &mut self,
        searcher: &Searcher,
        lines: u64,
        matches: u64,
    ) -> Result<bool, S::Error> {
        (**self).matched_count(searcher, lines, matches)
    }
}

impl<S: Sink + ?Sized> Sink for Box<S> {
    type Error = S::Error;

    #[inline]
    fn matched(&mut self, searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, S::Error> {
        (**self).matched(searcher, mat)
    }

    #[inline]
    fn context(
        &mut self,
        searcher: &Searcher,
        context: &SinkContext<'_>,
    ) -> Result<bool, S::Error> {
        (**self).context(searcher, context)
    }

    #[inline]
    fn context_break(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        (**self).context_break(searcher)
    }

    #[inline]
    fn binary_data(
        &mut self,
        searcher: &Searcher,
        binary_byte_offset: u64,
    ) -> Result<bool, S::Error> {
        (**self).binary_data(searcher, binary_byte_offset)
    }

    #[inline]
    fn begin(&mut self, searcher: &Searcher) -> Result<bool, S::Error> {
        (**self).begin(searcher)
    }

    #[inline]
    fn finish(&mut self, searcher: &Searcher, sink_finish: &SinkFinish) -> Result<(), S::Error> {
        (**self).finish(searcher, sink_finish)
    }

    #[inline]
    fn count_mode(&self, searcher: &Searcher) -> SinkCount {
        (**self).count_mode(searcher)
    }

    #[inline]
    fn matched_count(
        &mut self,
        searcher: &Searcher,
        lines: u64,
        matches: u64,
    ) -> Result<bool, S::Error> {
        (**self).matched_count(searcher, lines, matches)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkCount {
    None,
    Lines,
    LinesAndMatches,
}

#[derive(Clone, Debug)]
pub struct SinkFinish {
    pub(crate) byte_count: u64,
    pub(crate) binary_byte_offset: Option<u64>,
}

impl SinkFinish {
    #[inline]
    #[must_use]
    pub fn byte_count(&self) -> u64 {
        self.byte_count
    }

    #[inline]
    #[must_use]
    pub fn binary_byte_offset(&self) -> Option<u64> {
        self.binary_byte_offset
    }
}

#[derive(Clone, Debug)]
pub struct SinkMatch<'b> {
    pub(crate) line_term: LineTerminator,
    pub(crate) bytes: &'b [u8],
    pub(crate) absolute_byte_offset: u64,
    pub(crate) line_number: Option<u64>,
    pub(crate) buffer: &'b [u8],
    pub(crate) bytes_range_in_buffer: std::ops::Range<usize>,
}

impl<'b> SinkMatch<'b> {
    #[inline]
    #[must_use]
    pub fn bytes(&self) -> &'b [u8] {
        self.bytes
    }

    #[inline]
    #[must_use]
    pub fn lines(&self) -> LineIter<'b> {
        LineIter::new(self.line_term.as_byte(), self.bytes)
    }

    #[inline]
    #[must_use]
    pub fn absolute_byte_offset(&self) -> u64 {
        self.absolute_byte_offset
    }

    #[inline]
    #[must_use]
    pub fn line_number(&self) -> Option<u64> {
        self.line_number
    }

    #[inline]
    #[must_use]
    pub fn buffer(&self) -> &'b [u8] {
        self.buffer
    }

    #[inline]
    #[must_use]
    pub fn bytes_range_in_buffer(&self) -> std::ops::Range<usize> {
        self.bytes_range_in_buffer.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SinkContextKind {
    Before,
    After,
    Other,
}

#[derive(Clone, Debug)]
pub struct SinkContext<'b> {
    #[cfg(test)]
    pub(crate) line_term: LineTerminator,
    pub(crate) bytes: &'b [u8],
    pub(crate) kind: SinkContextKind,
    pub(crate) absolute_byte_offset: u64,
    pub(crate) line_number: Option<u64>,
}

impl<'b> SinkContext<'b> {
    #[inline]
    #[must_use]
    pub fn bytes(&self) -> &'b [u8] {
        self.bytes
    }

    #[inline]
    #[must_use]
    pub fn kind(&self) -> &SinkContextKind {
        &self.kind
    }

    #[cfg(test)]
    #[inline]
    #[must_use]
    pub fn lines(&self) -> LineIter<'b> {
        LineIter::new(self.line_term.as_byte(), self.bytes)
    }

    #[inline]
    #[must_use]
    pub fn absolute_byte_offset(&self) -> u64 {
        self.absolute_byte_offset
    }

    #[inline]
    #[must_use]
    pub fn line_number(&self) -> Option<u64> {
        self.line_number
    }
}

#[cfg(test)]
pub mod sinks {
    use std::borrow::Cow;
    use std::io;

    use super::super::Searcher;
    use super::{Sink, SinkError, SinkMatch};

    fn line_number(mat: &SinkMatch<'_>) -> Result<u64, io::Error> {
        mat.line_number()
            .ok_or_else(|| io::Error::error_message("line numbers not enabled"))
    }

    #[derive(Clone, Debug)]
    pub struct UTF8<F>(pub F)
    where
        F: FnMut(u64, &str) -> Result<bool, io::Error>;

    impl<F> Sink for UTF8<F>
    where
        F: FnMut(u64, &str) -> Result<bool, io::Error>,
    {
        type Error = io::Error;

        fn matched(
            &mut self,
            _searcher: &Searcher,
            mat: &SinkMatch<'_>,
        ) -> Result<bool, io::Error> {
            let matched = std::str::from_utf8(mat.bytes()).map_err(io::Error::error_message)?;
            let n = line_number(mat)?;
            (self.0)(n, matched)
        }
    }

    #[derive(Clone, Debug)]
    pub struct Lossy<F>(pub F)
    where
        F: FnMut(u64, &str) -> Result<bool, io::Error>;

    impl<F> Sink for Lossy<F>
    where
        F: FnMut(u64, &str) -> Result<bool, io::Error>,
    {
        type Error = io::Error;

        fn matched(
            &mut self,
            _searcher: &Searcher,
            mat: &SinkMatch<'_>,
        ) -> Result<bool, io::Error> {
            let matched = match std::str::from_utf8(mat.bytes()) {
                Ok(matched) => Cow::Borrowed(matched),
                Err(_) => String::from_utf8_lossy(mat.bytes()),
            };
            let n = line_number(mat)?;
            (self.0)(n, &matched)
        }
    }
}
