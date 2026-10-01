use memchr::memchr;

use super::super::matcher::{LineMatchKind, Matcher};
use super::line_buffer::BinaryDetection;
use super::lines::{self, LineStep};
use super::sink::{Sink, SinkContext, SinkContextKind, SinkError, SinkFinish, SinkMatch};
use super::{Config, Range, Searcher};

enum FastMatchResult {
    Continue,
    Stop,
    SwitchToSlow,
}

pub(crate) trait LineFeed {
    fn peek(&mut self, pos: usize, end: usize) -> Result<Option<(Range, Option<u64>)>, String>;

    fn consume(&mut self);
}

pub(crate) struct Core<'s, M, S> {
    config: &'s Config,
    matcher: M,
    searcher: &'s Searcher,
    sink: S,
    binary: bool,
    pos: usize,
    absolute_byte_offset: u64,
    binary_byte_offset: Option<usize>,
    line_number: Option<u64>,
    last_line_counted: usize,
    last_line_visited: usize,
    after_context_left: usize,
    has_sunk: bool,
    has_matched: bool,
    count: u64,
    feed: Option<&'s mut dyn LineFeed>,
    line_hint: Option<(usize, u64)>,
}

impl<'s, M: Matcher, S: Sink> Core<'s, M, S> {
    pub(crate) fn new(searcher: &'s Searcher, matcher: M, sink: S, binary: bool) -> Core<'s, M, S> {
        Core {
            config: &searcher.config,
            matcher,
            searcher,
            sink,
            binary,
            pos: 0,
            absolute_byte_offset: 0,
            binary_byte_offset: None,
            line_number: if searcher.config.line_number() {
                Some(1)
            } else {
                None
            },
            last_line_counted: 0,
            last_line_visited: 0,
            after_context_left: 0,
            has_sunk: false,
            has_matched: false,
            count: 0,
            feed: None,
            line_hint: None,
        }
    }

    pub(crate) fn set_feed(&mut self, feed: &'s mut dyn LineFeed) {
        self.feed = Some(feed);
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn count(&self) -> u64 {
        self.count
    }

    fn increment_count(&mut self) {
        self.count += 1;
    }

    pub(crate) fn binary_byte_offset(&self) -> Option<u64> {
        self.binary_byte_offset.map(|offset| offset as u64)
    }

    pub(crate) fn matcher(&self) -> &M {
        &self.matcher
    }

    pub(crate) fn matched(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        self.sink_matched(buf, range)
    }

    pub(crate) fn binary_data(&mut self, binary_byte_offset: u64) -> Result<bool, S::Error> {
        self.sink.binary_data(self.searcher, binary_byte_offset)
    }

    fn is_match(&self, line: &[u8]) -> Result<bool, S::Error> {
        let line = lines::without_terminator(line, self.config.line_term);
        self.matcher.is_match(line).map_err(S::Error::error_message)
    }

    pub(crate) fn find(&mut self, slice: &[u8]) -> Result<Option<Range>, S::Error> {
        if self.has_exceeded_match_limit() {
            return Ok(None);
        }
        match self.matcher().find(slice) {
            Err(err) => Err(S::Error::error_message(err)),
            Ok(None) => Ok(None),
            Ok(Some(m)) => {
                self.increment_count();
                Ok(Some(m))
            }
        }
    }

    fn shortest_match(&mut self, slice: &[u8]) -> Result<Option<usize>, S::Error> {
        if self.has_exceeded_match_limit() {
            return Ok(None);
        }
        self.matcher
            .shortest_match(slice)
            .map_err(S::Error::error_message)
    }

    pub(crate) fn begin(&mut self) -> Result<bool, S::Error> {
        self.sink.begin(self.searcher)
    }

    pub(crate) fn finish(
        &mut self,
        byte_count: u64,
        binary_byte_offset: Option<u64>,
    ) -> Result<(), S::Error> {
        self.sink.finish(
            self.searcher,
            &SinkFinish {
                byte_count,
                binary_byte_offset,
            },
        )
    }

    pub(crate) fn match_by_line(&mut self, buf: &[u8]) -> Result<bool, S::Error> {
        if self.is_line_by_line_fast() {
            match self.match_by_line_fast(buf)? {
                FastMatchResult::SwitchToSlow => self.match_by_line_slow(buf),
                FastMatchResult::Continue => Ok(true),
                FastMatchResult::Stop => Ok(false),
            }
        } else {
            self.match_by_line_slow(buf)
        }
    }

    pub(crate) fn roll(&mut self, buf: &[u8]) -> usize {
        let consumed = if self.config.max_context() == 0 {
            buf.len()
        } else {
            let context_start = lines::preceding(
                buf,
                self.config.line_term.as_byte(),
                self.config.before_context,
            );
            std::cmp::max(context_start, self.last_line_visited)
        };
        self.count_lines(buf, consumed);
        self.absolute_byte_offset += consumed as u64;
        self.last_line_counted = 0;
        self.last_line_visited = 0;
        self.set_pos(buf.len() - consumed);
        consumed
    }

    pub(crate) fn detect_binary(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        if self.binary_byte_offset.is_some() {
            return Ok(self.config.binary.quit_byte().is_some());
        }
        let binary_byte = match self.config.binary.0 {
            BinaryDetection::Quit(b) | BinaryDetection::Convert(b) => b,
            BinaryDetection::None => return Ok(false),
        };
        if let Some(i) = memchr(binary_byte, &buf[*range]) {
            let offset = range.start() + i;
            self.binary_byte_offset = Some(offset);
            if !self.binary_data(offset as u64)? {
                return Ok(true);
            }
            Ok(self.config.binary.quit_byte().is_some())
        } else {
            Ok(false)
        }
    }

    pub(crate) fn before_context_by_line(
        &mut self,
        buf: &[u8],
        upto: usize,
    ) -> Result<bool, S::Error> {
        if self.config.before_context == 0 {
            return Ok(true);
        }
        let range = Range::new(self.last_line_visited, upto);
        if range.is_empty() {
            return Ok(true);
        }
        let before_context_start = range.start()
            + lines::preceding(
                &buf[range],
                self.config.line_term.as_byte(),
                self.config.before_context - 1,
            );
        let mut stepper = LineStep::new(
            self.config.line_term.as_byte(),
            before_context_start,
            range.end(),
        );
        while let Some(line) = stepper.next_match(buf) {
            if !self.sink_break_context(line.start())? {
                return Ok(false);
            }
            if !self.sink_before_context(buf, &line)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn after_context_by_line(
        &mut self,
        buf: &[u8],
        upto: usize,
    ) -> Result<bool, S::Error> {
        if self.after_context_left == 0 {
            return Ok(true);
        }
        let exceeded_match_limit = self.has_exceeded_match_limit();
        let mut stepper = LineStep::new(
            self.config.line_term.as_byte(),
            self.last_line_visited,
            upto,
        );
        while let Some(line) = stepper.next_match(buf) {
            if exceeded_match_limit && self.is_match(&buf[line])? != self.config.invert_match() {
                let after_context_left = self.after_context_left;
                self.set_pos(line.end());
                if !self.sink_matched(buf, &line)? {
                    return Ok(false);
                }
                self.after_context_left = after_context_left - 1;
            } else if !self.sink_after_context(buf, &line)? {
                return Ok(false);
            }
            if self.after_context_left == 0 {
                break;
            }
        }
        Ok(true)
    }

    pub(crate) fn other_context_by_line(
        &mut self,
        buf: &[u8],
        upto: usize,
    ) -> Result<bool, S::Error> {
        let mut stepper = LineStep::new(
            self.config.line_term.as_byte(),
            self.last_line_visited,
            upto,
        );
        while let Some(line) = stepper.next_match(buf) {
            if !self.sink_other_context(buf, &line)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn match_by_line_slow(&mut self, buf: &[u8]) -> Result<bool, S::Error> {
        let mut stepper = LineStep::new(self.config.line_term.as_byte(), self.pos(), buf.len());
        while let Some(line) = stepper.next_match(buf) {
            if self.has_exceeded_match_limit()
                && !self.config.passthru()
                && self.after_context_left == 0
            {
                return Ok(false);
            }
            let matched = if self.feed.is_some() {
                self.fed_match(&line, buf.len())?
            } else {
                let slice = lines::without_terminator(&buf[line], self.config.line_term);
                self.shortest_match(slice)?.is_some()
            };
            self.set_pos(line.end());
            let success = matched != self.config.invert_match();
            if success {
                self.has_matched = true;
                self.increment_count();
                if !self.before_context_by_line(buf, line.start())? {
                    return Ok(false);
                }
                if !self.sink_matched(buf, &line)? {
                    return Ok(false);
                }
            } else if self.after_context_left >= 1 {
                if !self.sink_after_context(buf, &line)? {
                    return Ok(false);
                }
            } else if self.config.passthru() && !self.sink_other_context(buf, &line)? {
                return Ok(false);
            }
            if self.config.stop_on_nonmatch() && !success && self.has_matched {
                return Ok(false);
            }
            if !success
                && !self.config.invert_match()
                && let Some(next) = self.skip_to(&line, buf.len())?
            {
                self.set_pos(next);
                stepper = LineStep::new(self.config.line_term.as_byte(), next, buf.len());
            }
        }
        Ok(true)
    }

    fn fed_match(&mut self, line: &Range, end: usize) -> Result<bool, S::Error> {
        if self.has_exceeded_match_limit() {
            return Ok(false);
        }
        let Some(feed) = self.feed.as_mut() else {
            return Ok(false);
        };
        let next = feed
            .peek(line.start(), end)
            .map_err(S::Error::error_message)?;
        if next.is_some_and(|(m, _)| m.start() == line.start()) {
            feed.consume();
            return Ok(true);
        }
        Ok(false)
    }

    fn skip_to(&mut self, line: &Range, end: usize) -> Result<Option<usize>, S::Error> {
        if self.after_context_left > 0
            || self.config.passthru()
            || self.config.stop_on_nonmatch()
            || self.has_exceeded_match_limit()
        {
            return Ok(None);
        }
        let Some(feed) = self.feed.as_mut() else {
            return Ok(None);
        };
        let next = feed
            .peek(line.end(), end)
            .map_err(S::Error::error_message)?
            .map_or(end, |(m, _)| m.start());
        Ok(Some(next).filter(|&next| next > line.end()))
    }

    fn match_by_line_fast(&mut self, buf: &[u8]) -> Result<FastMatchResult, S::Error> {
        while !buf[self.pos()..].is_empty() {
            if self.config.stop_on_nonmatch() && self.has_matched {
                return Ok(FastMatchResult::SwitchToSlow);
            }
            if self.config.invert_match() {
                if !self.match_by_line_fast_invert(buf)? {
                    break;
                }
            } else if let Some(line) = self.find_by_line_fast(buf)? {
                self.has_matched = true;
                self.increment_count();
                if self.config.max_context() > 0 {
                    if !self.after_context_by_line(buf, line.start())? {
                        return Ok(FastMatchResult::Stop);
                    }
                    if !self.before_context_by_line(buf, line.start())? {
                        return Ok(FastMatchResult::Stop);
                    }
                }
                self.set_pos(line.end());
                if !self.sink_matched(buf, &line)? {
                    return Ok(FastMatchResult::Stop);
                }
            } else {
                break;
            }
        }
        if !self.after_context_by_line(buf, buf.len())? {
            return Ok(FastMatchResult::Stop);
        }
        if self.has_exceeded_match_limit() && self.after_context_left == 0 {
            return Ok(FastMatchResult::Stop);
        }
        self.set_pos(buf.len());
        Ok(FastMatchResult::Continue)
    }

    #[inline]
    fn match_by_line_fast_invert(&mut self, buf: &[u8]) -> Result<bool, S::Error> {
        let invert_match = match self.find_by_line_fast(buf)? {
            None => {
                let range = Range::new(self.pos(), buf.len());
                self.set_pos(range.end());
                range
            }
            Some(line) => {
                let range = Range::new(self.pos(), line.start());
                self.set_pos(line.end());
                range
            }
        };
        if invert_match.is_empty() {
            return Ok(true);
        }
        self.has_matched = true;
        if !self.after_context_by_line(buf, invert_match.start())? {
            return Ok(false);
        }
        if !self.before_context_by_line(buf, invert_match.start())? {
            return Ok(false);
        }
        let mut stepper = LineStep::new(
            self.config.line_term.as_byte(),
            invert_match.start(),
            invert_match.end(),
        );
        while let Some(line) = stepper.next_match(buf) {
            self.increment_count();
            if !self.sink_matched(buf, &line)? {
                return Ok(false);
            }
            if self.has_exceeded_match_limit() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[inline]
    fn find_by_line_fast(&mut self, buf: &[u8]) -> Result<Option<Range>, S::Error> {
        if let Some(feed) = self.feed.as_mut() {
            if self.count >= self.config.max_matches.unwrap_or(u64::MAX) {
                return Ok(None);
            }
            return match feed
                .peek(self.pos, buf.len())
                .map_err(S::Error::error_message)?
            {
                None => Ok(None),
                Some((line, number)) => {
                    feed.consume();
                    if let Some(n) = number {
                        self.line_hint = Some((line.start(), n));
                    }
                    Ok(Some(line))
                }
            };
        }
        let mut pos = self.pos();
        while !buf[pos..].is_empty() {
            if self.has_exceeded_match_limit() {
                return Ok(None);
            }
            match self.matcher.find_candidate_line(&buf[pos..]) {
                Err(err) => return Err(S::Error::error_message(err)),
                Ok(None) => return Ok(None),
                Ok(Some(LineMatchKind::Confirmed(i))) => {
                    let line = lines::locate(
                        buf,
                        self.config.line_term.as_byte(),
                        Range::zero(i).offset(pos),
                    );
                    if line.start() == buf.len() {
                        pos = buf.len();
                        continue;
                    }
                    return Ok(Some(line));
                }
                #[cfg(test)]
                Ok(Some(LineMatchKind::Candidate(i))) => {
                    let line = lines::locate(
                        buf,
                        self.config.line_term.as_byte(),
                        Range::zero(i).offset(pos),
                    );
                    if self.is_match(&buf[line])? {
                        return Ok(Some(line));
                    }
                    pos = line.end();
                }
            }
        }
        Ok(None)
    }

    #[inline]
    fn sink_matched(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        if self.binary && self.detect_binary(buf, range)? {
            return Ok(false);
        }
        if !self.sink_break_context(range.start())? {
            return Ok(false);
        }
        self.count_lines(buf, range.start());
        let offset = self.absolute_byte_offset + range.start() as u64;
        let keepgoing = self.sink.matched(
            self.searcher,
            &SinkMatch {
                line_term: self.config.line_term,
                bytes: &buf[*range],
                absolute_byte_offset: offset,
                line_number: self.line_number,
                buffer: buf,
                bytes_range_in_buffer: range.start()..range.end(),
            },
        )?;
        if !keepgoing {
            return Ok(false);
        }
        self.last_line_visited = range.end();
        self.after_context_left = self.config.after_context;
        self.has_sunk = true;
        Ok(true)
    }

    fn sink_context_kind(
        &mut self,
        buf: &[u8],
        range: &Range,
        kind: SinkContextKind,
    ) -> Result<bool, S::Error> {
        if self.binary && self.detect_binary(buf, range)? {
            return Ok(false);
        }
        self.count_lines(buf, range.start());
        let offset = self.absolute_byte_offset + range.start() as u64;
        let keepgoing = self.sink.context(
            self.searcher,
            &SinkContext {
                #[cfg(test)]
                line_term: self.config.line_term,
                bytes: &buf[*range],
                kind,
                absolute_byte_offset: offset,
                line_number: self.line_number,
            },
        )?;
        if !keepgoing {
            return Ok(false);
        }
        self.last_line_visited = range.end();
        self.has_sunk = true;
        Ok(true)
    }

    fn sink_before_context(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        self.sink_context_kind(buf, range, SinkContextKind::Before)
    }

    fn sink_after_context(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        assert!(self.after_context_left >= 1);
        if !self.sink_context_kind(buf, range, SinkContextKind::After)? {
            return Ok(false);
        }
        self.after_context_left -= 1;
        Ok(true)
    }

    fn sink_other_context(&mut self, buf: &[u8], range: &Range) -> Result<bool, S::Error> {
        self.sink_context_kind(buf, range, SinkContextKind::Other)
    }

    fn sink_break_context(&mut self, start_of_line: usize) -> Result<bool, S::Error> {
        let is_gap = self.last_line_visited < start_of_line;
        let any_context = self.config.before_context > 0 || self.config.after_context > 0;
        if !any_context || !self.has_sunk || !is_gap {
            Ok(true)
        } else {
            self.sink.context_break(self.searcher)
        }
    }

    fn count_lines(&mut self, buf: &[u8], upto: usize) {
        let Some(line_number) = self.line_number.as_mut() else {
            return;
        };
        if self.last_line_counted >= upto {
            return;
        }
        let term = self.config.line_term.as_byte();
        if let Some((hint_pos, hint_number)) = self.line_hint
            && self.last_line_counted <= hint_pos
            && (upto >= hint_pos || hint_pos - upto < upto - self.last_line_counted)
        {
            if upto >= hint_pos {
                *line_number = hint_number + lines::count(&buf[hint_pos..upto], term);
            } else {
                *line_number = hint_number - lines::count(&buf[upto..hint_pos], term);
            }
            self.last_line_counted = upto;
            return;
        }
        *line_number += lines::count(&buf[self.last_line_counted..upto], term);
        self.last_line_counted = upto;
    }

    pub(crate) fn is_line_by_line_fast(&self) -> bool {
        if self.config.passthru() {
            return false;
        }
        if self.config.stop_on_nonmatch() && self.has_matched {
            return false;
        }
        if let Some(line_term) = self.matcher.line_terminator() {
            if line_term.as_byte() == b'\x00' {
                return false;
            }
            if line_term == self.config.line_term {
                return true;
            }
        }
        if let Some(non_matching) = self.matcher.non_matching_bytes()
            && non_matching.contains(self.config.line_term.as_byte())
        {
            return true;
        }
        false
    }

    fn has_exceeded_match_limit(&self) -> bool {
        self.config
            .max_matches
            .is_some_and(|limit| self.count() >= limit)
    }
}
