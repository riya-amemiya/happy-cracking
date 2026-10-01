use super::super::matcher::Matcher;
use super::core::{Core, LineFeed};
use super::line_buffer::{DEFAULT_BUFFER_CAPACITY, LineBufferReader};
use super::lines::{self, LineStep};
use super::sink::{Sink, SinkError};
use super::{Config, Range, Searcher};

pub(crate) struct ReadByLine<'s, M, R, S> {
    config: &'s Config,
    core: Core<'s, M, S>,
    rdr: LineBufferReader<'s, R>,
}

impl<'s, M, R, S> ReadByLine<'s, M, R, S>
where
    M: Matcher,
    R: std::io::Read,
    S: Sink,
{
    pub(crate) fn new(
        searcher: &'s Searcher,
        matcher: M,
        read_from: LineBufferReader<'s, R>,
        write_to: S,
    ) -> ReadByLine<'s, M, R, S> {
        ReadByLine {
            config: &searcher.config,
            core: Core::new(searcher, matcher, write_to, false),
            rdr: read_from,
        }
    }

    pub(crate) fn run(mut self) -> Result<(), S::Error> {
        if self.core.begin()? {
            while self.fill()? {
                if !self.core.match_by_line(self.rdr.buffer())? {
                    let consumed = self.core.pos();
                    self.rdr.consume(consumed);
                    break;
                }
            }
        }
        self.core.finish(
            self.rdr.absolute_byte_offset(),
            self.rdr.binary_byte_offset(),
        )
    }

    fn fill(&mut self) -> Result<bool, S::Error> {
        let already_binary = self.rdr.binary_byte_offset().is_some();
        let old_buf_len = self.rdr.buffer().len();
        let consumed = self.core.roll(self.rdr.buffer());
        self.rdr.consume(consumed);
        let didread = self.rdr.fill().map_err(S::Error::error_io)?;
        if !already_binary
            && let Some(offset) = self.rdr.binary_byte_offset()
            && !self.core.binary_data(offset)?
        {
            return Ok(false);
        }
        if !didread || self.should_binary_quit() {
            return Ok(false);
        }
        if consumed == 0 && old_buf_len == self.rdr.buffer().len() {
            self.rdr.consume(old_buf_len);
            return Ok(false);
        }
        Ok(true)
    }

    fn should_binary_quit(&self) -> bool {
        self.rdr.binary_byte_offset().is_some() && self.config.binary.quit_byte().is_some()
    }
}

pub(crate) struct SliceByLine<'s, M, S> {
    core: Core<'s, M, S>,
    slice: &'s [u8],
}

impl<'s, M: Matcher, S: Sink> SliceByLine<'s, M, S> {
    pub(crate) fn new(
        searcher: &'s Searcher,
        matcher: M,
        slice: &'s [u8],
        write_to: S,
        binary: bool,
    ) -> SliceByLine<'s, M, S> {
        SliceByLine {
            core: Core::new(searcher, matcher, write_to, binary),
            slice,
        }
    }

    pub(crate) fn with_feed(mut self, feed: &'s mut dyn LineFeed) -> SliceByLine<'s, M, S> {
        self.core.set_feed(feed);
        self
    }

    pub(crate) fn run(mut self) -> Result<(), S::Error> {
        if self.core.begin()? {
            let binary_upto = std::cmp::min(self.slice.len(), DEFAULT_BUFFER_CAPACITY);
            let binary_range = Range::new(0, binary_upto);
            if !self.core.detect_binary(self.slice, &binary_range)? {
                while !self.slice[self.core.pos()..].is_empty()
                    && self.core.match_by_line(self.slice)?
                {}
            }
        }
        let byte_count = self.byte_count();
        let binary_byte_offset = self.core.binary_byte_offset();
        self.core.finish(byte_count, binary_byte_offset)
    }

    fn byte_count(&mut self) -> u64 {
        match self.core.binary_byte_offset() {
            Some(offset) if offset < self.core.pos() as u64 => offset,
            _ => self.core.pos() as u64,
        }
    }
}

pub(crate) struct MultiLine<'s, M, S> {
    config: &'s Config,
    core: Core<'s, M, S>,
    slice: &'s [u8],
    last_match: Option<Range>,
}

impl<'s, M: Matcher, S: Sink> MultiLine<'s, M, S> {
    pub(crate) fn new(
        searcher: &'s Searcher,
        matcher: M,
        slice: &'s [u8],
        write_to: S,
    ) -> MultiLine<'s, M, S> {
        MultiLine {
            config: &searcher.config,
            core: Core::new(searcher, matcher, write_to, true),
            slice,
            last_match: None,
        }
    }

    pub(crate) fn run(mut self) -> Result<(), S::Error> {
        if self.core.begin()? {
            let binary_upto = std::cmp::min(self.slice.len(), DEFAULT_BUFFER_CAPACITY);
            let binary_range = Range::new(0, binary_upto);
            if !self.core.detect_binary(self.slice, &binary_range)? {
                let mut keepgoing = true;
                while !self.slice[self.core.pos()..].is_empty() && keepgoing {
                    keepgoing = self.sink()?;
                }
                if keepgoing {
                    keepgoing = match self.last_match.take() {
                        None => true,
                        Some(last_match) => {
                            if self.sink_context(&last_match)? {
                                self.sink_matched(&last_match)?;
                            }
                            true
                        }
                    };
                }
                if keepgoing {
                    if self.config.passthru() {
                        self.core
                            .other_context_by_line(self.slice, self.slice.len())?;
                    } else {
                        self.core
                            .after_context_by_line(self.slice, self.slice.len())?;
                    }
                }
            }
        }
        let byte_count = self.byte_count();
        let binary_byte_offset = self.core.binary_byte_offset();
        self.core.finish(byte_count, binary_byte_offset)
    }

    fn sink(&mut self) -> Result<bool, S::Error> {
        if self.config.invert_match() {
            return self.sink_matched_inverted();
        }
        let Some(mat) = self.find()? else {
            self.core.set_pos(self.slice.len());
            return Ok(true);
        };
        self.advance(&mat);
        let line = lines::locate(self.slice, self.config.line_term.as_byte(), mat);
        match self.last_match.take() {
            None => {
                self.last_match = Some(line);
                Ok(true)
            }
            Some(last_match) => {
                if last_match.end() >= line.start() {
                    self.last_match = Some(last_match.with_end(line.end()));
                    Ok(true)
                } else {
                    self.last_match = Some(line);
                    if !self.sink_context(&last_match)? {
                        return Ok(false);
                    }
                    self.sink_matched(&last_match)
                }
            }
        }
    }

    fn sink_matched_inverted(&mut self) -> Result<bool, S::Error> {
        let invert_match = match self.find()? {
            None => {
                let range = Range::new(self.core.pos(), self.slice.len());
                self.core.set_pos(range.end());
                range
            }
            Some(mat) => {
                let line = lines::locate(self.slice, self.config.line_term.as_byte(), mat);
                let range = Range::new(self.core.pos(), line.start());
                self.advance(&line);
                range
            }
        };
        if invert_match.is_empty() {
            return Ok(true);
        }
        if !self.sink_context(&invert_match)? {
            return Ok(false);
        }
        let mut stepper = LineStep::new(
            self.config.line_term.as_byte(),
            invert_match.start(),
            invert_match.end(),
        );
        while let Some(line) = stepper.next_match(self.slice) {
            if !self.sink_matched(&line)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn sink_matched(&mut self, range: &Range) -> Result<bool, S::Error> {
        if range.is_empty() {
            return Ok(false);
        }
        self.core.matched(self.slice, range)
    }

    fn sink_context(&mut self, range: &Range) -> Result<bool, S::Error> {
        if self.config.passthru() {
            if !self.core.other_context_by_line(self.slice, range.start())? {
                return Ok(false);
            }
        } else {
            if !self.core.after_context_by_line(self.slice, range.start())? {
                return Ok(false);
            }
            if !self
                .core
                .before_context_by_line(self.slice, range.start())?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn find(&mut self) -> Result<Option<Range>, S::Error> {
        let pos = self.core.pos();
        self.core
            .find(&self.slice[pos..])
            .map(|m| m.map(|m| m.offset(pos)))
    }

    fn advance(&mut self, range: &Range) {
        self.core.set_pos(range.end());
        if range.is_empty() && self.core.pos() < self.slice.len() {
            let newpos = self.core.pos() + 1;
            self.core.set_pos(newpos);
        }
    }

    fn byte_count(&mut self) -> u64 {
        match self.core.binary_byte_offset() {
            Some(offset) if offset < self.core.pos() as u64 => offset,
            _ => self.core.pos() as u64,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::testutil::{KitchenSink, RegexMatcher, SearcherTester};
    use super::super::{BinaryDetection, SearcherBuilder};

    use super::*;

    const SHERLOCK: &str = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.\
";

    const CODE: &str = "\
extern crate snap;

use std::io;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    // Wrap the stdin reader in a Snappy reader.
    let mut rdr = snap::Reader::new(stdin.lock());
    let mut wtr = stdout.lock();
    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");
}
";

    #[test]
    fn basic1() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn basic2() {
        let exp = "\nbyte count:366\n";
        SearcherTester::new(SHERLOCK, "NADA")
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn basic3() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "a")
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn basic4() {
        let haystack = "\
a
b

c


d
";
        let byte_count = haystack.len();
        let exp = format!("0:a\n\nbyte count:{byte_count}\n");
        SearcherTester::new(haystack, "a")
            .line_number(false)
            .expected_no_line_number(&exp)
            .test();
    }

    #[test]
    fn invert1() {
        let exp = "\
65:Holmeses, success in the province of detective work must always
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn line_number1() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        let exp_line = "\
1:0:For the Doctor Watsons of this world, as opposed to the Sherlock
3:129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_line)
            .test();
    }

    #[test]
    fn line_number_invert1() {
        let exp = "\
65:Holmeses, success in the province of detective work must always
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        let exp_line = "\
2:65:Holmeses, success in the province of detective work must always
4:193:can extract a clew from a wisp of straw or a flake of cigar ash;
5:258:but Doctor Watson has to have it taken out for him and dusted,
6:321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .invert_match(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_line)
            .test();
    }

    #[test]
    fn multi_line_overlap1() {
        let haystack = "xxx\nabc\ndefxxxabc\ndefxxx\nxxx";
        let byte_count = haystack.len();
        let exp = format!("4:abc\n8:defxxxabc\n18:defxxx\n\nbyte count:{byte_count}\n");

        SearcherTester::new(haystack, "abc\ndef")
            .by_line(false)
            .line_number(false)
            .expected_no_line_number(&exp)
            .test();
    }

    #[test]
    fn multi_line_overlap2() {
        let haystack = "xxx\nabc\ndefabc\ndefxxx\nxxx";
        let byte_count = haystack.len();
        let exp = format!("4:abc\n8:defabc\n15:defxxx\n\nbyte count:{byte_count}\n");

        SearcherTester::new(haystack, "abc\ndef")
            .by_line(false)
            .line_number(false)
            .expected_no_line_number(&exp)
            .test();
    }

    #[test]
    fn empty_line1() {
        let exp = "\nbyte count:0\n";
        SearcherTester::new("", r"^$")
            .expected_no_line_number(exp)
            .expected_with_line_number(exp)
            .test();
    }

    #[test]
    fn empty_line2() {
        let exp = "0:\n\nbyte count:1\n";
        let exp_line = "1:0:\n\nbyte count:1\n";

        SearcherTester::new("\n", r"^$")
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_line)
            .test();
    }

    #[test]
    fn empty_line3() {
        let exp = "0:\n1:\n\nbyte count:2\n";
        let exp_line = "1:0:\n2:1:\n\nbyte count:2\n";

        SearcherTester::new("\n\n", r"^$")
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_line)
            .test();
    }

    #[test]
    fn empty_line4() {
        let haystack = "\
a
b

c


d
";
        let byte_count = haystack.len();
        let exp = format!("4:\n7:\n8:\n\nbyte count:{byte_count}\n");
        let exp_line = format!("3:4:\n5:7:\n6:8:\n\nbyte count:{byte_count}\n");

        SearcherTester::new(haystack, r"^$")
            .expected_no_line_number(&exp)
            .expected_with_line_number(&exp_line)
            .test();
    }

    #[test]
    fn empty_line5() {
        let haystack = "\
a
b

c


d";
        let byte_count = haystack.len();
        let exp = format!("4:\n7:\n8:\n\nbyte count:{byte_count}\n");
        let exp_line = format!("3:4:\n5:7:\n6:8:\n\nbyte count:{byte_count}\n");

        SearcherTester::new(haystack, r"^$")
            .expected_no_line_number(&exp)
            .expected_with_line_number(&exp_line)
            .test();
    }

    #[test]
    fn empty_line6() {
        let haystack = "\
a
b

c


d

";
        let byte_count = haystack.len();
        let exp = format!("4:\n7:\n8:\n11:\n\nbyte count:{byte_count}\n");
        let exp_line = format!("3:4:\n5:7:\n6:8:\n8:11:\n\nbyte count:{byte_count}\n");

        SearcherTester::new(haystack, r"^$")
            .expected_no_line_number(&exp)
            .expected_with_line_number(&exp_line)
            .test();
    }

    #[test]
    fn big1() {
        let mut haystack = String::new();
        haystack.push_str("a\n");
        for _ in 0..(4 * (DEFAULT_BUFFER_CAPACITY + 7)) {
            haystack.push_str("zzz\n");
        }
        haystack.push_str("a\n");

        let byte_count = haystack.len();
        let exp = format!("0:a\n1048690:a\n\nbyte count:{byte_count}\n");

        SearcherTester::new(&haystack, "a")
            .line_number(false)
            .expected_no_line_number(&exp)
            .test();
    }

    #[test]
    fn big_error_one_line() {
        let mut haystack = String::new();
        haystack.push_str("a\n");
        for _ in 0..(4 * (DEFAULT_BUFFER_CAPACITY + 7)) {
            haystack.push_str("zzz\n");
        }
        haystack.push_str("a\n");

        let matcher = RegexMatcher::new("a");
        let mut sink = KitchenSink::new();
        let mut searcher = SearcherBuilder::new().heap_limit(Some(3)).build();
        let result = searcher.search_reader(&matcher, haystack.as_bytes(), &mut sink);
        assert!(result.is_err());
    }

    #[test]
    fn big_error_multi_line() {
        let mut haystack = String::new();
        haystack.push_str("a\n");
        for _ in 0..(4 * (DEFAULT_BUFFER_CAPACITY + 7)) {
            haystack.push_str("zzz\n");
        }
        haystack.push_str("a\n");

        let matcher = RegexMatcher::new("a");
        let mut sink = KitchenSink::new();
        let mut searcher = SearcherBuilder::new()
            .multi_line(true)
            .heap_limit(Some(haystack.len()))
            .build();
        let result = searcher.search_reader(&matcher, haystack.as_bytes(), &mut sink);
        assert!(result.is_err());
    }

    #[test]
    fn binary1() {
        let haystack = "\x00a";
        let exp = "\nbyte count:0\nbinary offset:0\n";

        SearcherTester::new(haystack, "a")
            .binary_detection(BinaryDetection::quit(0))
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn binary2() {
        let haystack = "a\x00";
        let exp = "\nbyte count:0\nbinary offset:1\n";

        SearcherTester::new(haystack, "a")
            .binary_detection(BinaryDetection::quit(0))
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn binary3() {
        let mut haystack = String::new();
        haystack.push_str("a\n");
        for _ in 0..DEFAULT_BUFFER_CAPACITY {
            haystack.push_str("zzz\n");
        }
        haystack.push_str("a\n");
        haystack.push_str("zzz\n");
        haystack.push_str("a\x00a\n");
        haystack.push_str("zzz\n");
        haystack.push_str("a\n");

        let exp = "0:a\n\nbyte count:262146\nbinary offset:262153\n";
        let exp_slice = "0:a\n262146:a\n\nbyte count:262153\nbinary offset:262153\n";

        SearcherTester::new(&haystack, "a")
            .binary_detection(BinaryDetection::quit(0))
            .line_number(false)
            .auto_heap_limit(false)
            .expected_no_line_number(exp)
            .expected_slice_no_line_number(exp_slice)
            .test();
    }

    #[test]
    fn binary4() {
        let mut haystack = String::new();
        haystack.push_str("a\n");
        for _ in 0..DEFAULT_BUFFER_CAPACITY {
            haystack.push_str("zzz\n");
        }
        haystack.push_str("a\n");
        haystack.push_str("b\x00b\n");
        haystack.push_str("a\x00a\n");
        haystack.push_str("a\n");

        let exp = "0:a\n\nbyte count:262146\nbinary offset:262149\n";
        let exp_slice = "0:a\n262146:a\n\nbyte count:262153\nbinary offset:262153\n";

        SearcherTester::new(&haystack, "a")
            .binary_detection(BinaryDetection::quit(0))
            .line_number(false)
            .auto_heap_limit(false)
            .expected_no_line_number(exp)
            .expected_slice_no_line_number(exp_slice)
            .test();
    }

    #[test]
    fn passthru_sherlock1() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .passthru(true)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn passthru_sherlock_invert1() {
        let exp = "\
0-For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .passthru(true)
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock1() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;

byte count:366
";
        let exp_lines = "\
1:0:For the Doctor Watsons of this world, as opposed to the Sherlock
2-65-Holmeses, success in the province of detective work must always
3:129:be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(1)
            .before_context(1)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(1)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .before_context(1)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock_invert1() {
        let exp = "\
0-For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
1-0-For the Doctor Watsons of this world, as opposed to the Sherlock
2:65:Holmeses, success in the province of detective work must always
3-129-be, to a very large extent, the result of luck. Sherlock Holmes
4:193:can extract a clew from a wisp of straw or a flake of cigar ash;
5:258:but Doctor Watson has to have it taken out for him and dusted,
6:321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(1)
            .before_context(1)
            .line_number(true)
            .invert_match(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        SearcherTester::new(SHERLOCK, "Sherlock")
            .before_context(1)
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(1)
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock2() {
        let exp = "\
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
2-65-Holmeses, success in the province of detective work must always
3:129:be, to a very large extent, the result of luck. Sherlock Holmes
4:193:can extract a clew from a wisp of straw or a flake of cigar ash;
5-258-but Doctor Watson has to have it taken out for him and dusted,
6:321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, " a ")
            .after_context(1)
            .before_context(1)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        SearcherTester::new(SHERLOCK, " a ")
            .before_context(1)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
129:be, to a very large extent, the result of luck. Sherlock Holmes
193:can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, " a ")
            .after_context(1)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock_invert2() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
1:0:For the Doctor Watsons of this world, as opposed to the Sherlock
2:65:Holmeses, success in the province of detective work must always
3-129-be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;
5:258:but Doctor Watson has to have it taken out for him and dusted,
6-321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, " a ")
            .after_context(1)
            .before_context(1)
            .line_number(true)
            .invert_match(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
--
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,

byte count:366
";
        SearcherTester::new(SHERLOCK, " a ")
            .before_context(1)
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
--
258:but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, " a ")
            .after_context(1)
            .line_number(false)
            .invert_match(true)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock3() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,

byte count:366
";
        let exp_lines = "\
1:0:For the Doctor Watsons of this world, as opposed to the Sherlock
2-65-Holmeses, success in the province of detective work must always
3:129:be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;
5-258-but Doctor Watson has to have it taken out for him and dusted,

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock4() {
        let exp = "\
129-be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
3-129-be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;
5:258:but Doctor Watson has to have it taken out for him and dusted,
6-321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "dusted")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
258:but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "dusted")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
129-be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258:but Doctor Watson has to have it taken out for him and dusted,

byte count:366
";
        SearcherTester::new(SHERLOCK, "dusted")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock5() {
        let exp = "\
0-For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
1-0-For the Doctor Watsons of this world, as opposed to the Sherlock
2:65:Holmeses, success in the province of detective work must always
3-129-be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;
5-258-but Doctor Watson has to have it taken out for him and dusted,
6:321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "success|attached")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
65:Holmeses, success in the province of detective work must always
129-be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
--
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "success|attached")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
0-For the Doctor Watsons of this world, as opposed to the Sherlock
65:Holmeses, success in the province of detective work must always
--
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321:and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "success|attached")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_sherlock6() {
        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        let exp_lines = "\
1:0:For the Doctor Watsons of this world, as opposed to the Sherlock
2-65-Holmeses, success in the province of detective work must always
3:129:be, to a very large extent, the result of luck. Sherlock Holmes
4-193-can extract a clew from a wisp of straw or a flake of cigar ash;
5-258-but Doctor Watson has to have it taken out for him and dusted,
6-321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(3)
            .before_context(3)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes
193-can extract a clew from a wisp of straw or a flake of cigar ash;
258-but Doctor Watson has to have it taken out for him and dusted,
321-and exhibited clearly, with a label attached.
byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .after_context(3)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
0:For the Doctor Watsons of this world, as opposed to the Sherlock
65-Holmeses, success in the province of detective work must always
129:be, to a very large extent, the result of luck. Sherlock Holmes

byte count:366
";
        SearcherTester::new(SHERLOCK, "Sherlock")
            .before_context(3)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_code1() {
        let exp = "\
33-
34-fn main() {
46:    let stdin = io::stdin();
75-    let stdout = io::stdout();
106-
107:    // Wrap the stdin reader in a Snappy reader.
156:    let mut rdr = snap::Reader::new(stdin.lock());
207-    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        let exp_lines = "\
4-33-
5-34-fn main() {
6:46:    let stdin = io::stdin();
7-75-    let stdout = io::stdout();
8-106-
9:107:    // Wrap the stdin reader in a Snappy reader.
10:156:    let mut rdr = snap::Reader::new(stdin.lock());
11-207-    let mut wtr = stdout.lock();
12-240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        SearcherTester::new(CODE, "stdin")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
46:    let stdin = io::stdin();
75-    let stdout = io::stdout();
106-
107:    // Wrap the stdin reader in a Snappy reader.
156:    let mut rdr = snap::Reader::new(stdin.lock());
207-    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        SearcherTester::new(CODE, "stdin")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
33-
34-fn main() {
46:    let stdin = io::stdin();
75-    let stdout = io::stdout();
106-
107:    // Wrap the stdin reader in a Snappy reader.
156:    let mut rdr = snap::Reader::new(stdin.lock());

byte count:307
";
        SearcherTester::new(CODE, "stdin")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_code2() {
        let exp = "\
34-fn main() {
46-    let stdin = io::stdin();
75:    let stdout = io::stdout();
106-
107-    // Wrap the stdin reader in a Snappy reader.
156-    let mut rdr = snap::Reader::new(stdin.lock());
207:    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");
305-}

byte count:307
";
        let exp_lines = "\
5-34-fn main() {
6-46-    let stdin = io::stdin();
7:75:    let stdout = io::stdout();
8-106-
9-107-    // Wrap the stdin reader in a Snappy reader.
10-156-    let mut rdr = snap::Reader::new(stdin.lock());
11:207:    let mut wtr = stdout.lock();
12-240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");
13-305-}

byte count:307
";
        SearcherTester::new(CODE, "stdout")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
75:    let stdout = io::stdout();
106-
107-    // Wrap the stdin reader in a Snappy reader.
--
207:    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");
305-}

byte count:307
";
        SearcherTester::new(CODE, "stdout")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
34-fn main() {
46-    let stdin = io::stdin();
75:    let stdout = io::stdout();
--
107-    // Wrap the stdin reader in a Snappy reader.
156-    let mut rdr = snap::Reader::new(stdin.lock());
207:    let mut wtr = stdout.lock();

byte count:307
";
        SearcherTester::new(CODE, "stdout")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn context_code3() {
        let exp = "\
20-use std::io;
33-
34:fn main() {
46-    let stdin = io::stdin();
75-    let stdout = io::stdout();
106-
107-    // Wrap the stdin reader in a Snappy reader.
156:    let mut rdr = snap::Reader::new(stdin.lock());
207-    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        let exp_lines = "\
3-20-use std::io;
4-33-
5:34:fn main() {
6-46-    let stdin = io::stdin();
7-75-    let stdout = io::stdout();
8-106-
9-107-    // Wrap the stdin reader in a Snappy reader.
10:156:    let mut rdr = snap::Reader::new(stdin.lock());
11-207-    let mut wtr = stdout.lock();
12-240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        SearcherTester::new(CODE, "fn main|let mut rdr")
            .after_context(2)
            .before_context(2)
            .line_number(true)
            .expected_no_line_number(exp)
            .expected_with_line_number(exp_lines)
            .test();

        let exp = "\
34:fn main() {
46-    let stdin = io::stdin();
75-    let stdout = io::stdout();
--
156:    let mut rdr = snap::Reader::new(stdin.lock());
207-    let mut wtr = stdout.lock();
240-    io::copy(&mut rdr, &mut wtr).expect(\"I/O operation failed\");

byte count:307
";
        SearcherTester::new(CODE, "fn main|let mut rdr")
            .after_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();

        let exp = "\
20-use std::io;
33-
34:fn main() {
--
106-
107-    // Wrap the stdin reader in a Snappy reader.
156:    let mut rdr = snap::Reader::new(stdin.lock());

byte count:307
";
        SearcherTester::new(CODE, "fn main|let mut rdr")
            .before_context(2)
            .line_number(false)
            .expected_no_line_number(exp)
            .test();
    }

    #[test]
    fn scratch() {
        use super::super::sink::sinks;
        use super::super::testutil::RegexMatcher;

        const SHERLOCK: &[u8] = b"\
For the Doctor Wat\xFFsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.\
    ";

        let haystack = SHERLOCK;
        let matcher = RegexMatcher::new("Sherlock");
        let mut searcher = SearcherBuilder::new().line_number(true).build();
        searcher
            .search_reader(
                &matcher,
                haystack,
                sinks::Lossy(|n, line| {
                    print!("{n}:{line}");
                    Ok(true)
                }),
            )
            .unwrap();
    }

    #[test]
    fn regression_2260() {
        use crate::hc_internal::grep::regex::RegexMatcherBuilder;

        use super::super::SearcherBuilder;

        let matcher = RegexMatcherBuilder::new()
            .line_terminator(Some(b'\n'))
            .build(r"^\w+$")
            .unwrap();
        let mut searcher = SearcherBuilder::new().line_number(true).build();

        let mut seen = false;
        searcher
            .search_slice(
                &matcher,
                b"GATC\n",
                super::super::sink::sinks::UTF8(|_, _| {
                    seen = true;
                    Ok(true)
                }),
            )
            .unwrap();
        assert!(seen);
    }
}
