use std::io;

use memchr::{memchr, memrchr};

pub(crate) const DEFAULT_BUFFER_CAPACITY: usize = 64 * (1 << 10);

#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum BufferAllocation {
    #[default]
    Eager,
    Error(usize),
}

pub(crate) fn alloc_error(limit: usize) -> io::Error {
    io::Error::other(format!("configured allocation limit ({limit}) exceeded"))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BinaryDetection {
    #[default]
    None,
    Quit(u8),
    Convert(u8),
}

impl BinaryDetection {
    fn is_quit(self) -> bool {
        matches!(self, BinaryDetection::Quit(_))
    }
}

#[derive(Clone, Copy, Debug)]
struct Config {
    capacity: usize,
    lineterm: u8,
    buffer_alloc: BufferAllocation,
    binary: BinaryDetection,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            capacity: DEFAULT_BUFFER_CAPACITY,
            lineterm: b'\n',
            buffer_alloc: BufferAllocation::default(),
            binary: BinaryDetection::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LineBufferBuilder {
    config: Config,
}

impl LineBufferBuilder {
    pub(crate) fn new() -> LineBufferBuilder {
        LineBufferBuilder::default()
    }

    pub(crate) fn build(&self) -> LineBuffer {
        LineBuffer {
            config: self.config,
            buf: vec![0; self.config.capacity],
            pos: 0,
            last_lineterm: 0,
            end: 0,
            absolute_byte_offset: 0,
            binary_byte_offset: None,
        }
    }

    pub(crate) fn capacity(&mut self, capacity: usize) -> &mut LineBufferBuilder {
        self.config.capacity = capacity;
        self
    }

    pub(crate) fn line_terminator(&mut self, lineterm: u8) -> &mut LineBufferBuilder {
        self.config.lineterm = lineterm;
        self
    }

    pub(crate) fn buffer_alloc(&mut self, behavior: BufferAllocation) -> &mut LineBufferBuilder {
        self.config.buffer_alloc = behavior;
        self
    }

    pub(crate) fn binary_detection(
        &mut self,
        detection: BinaryDetection,
    ) -> &mut LineBufferBuilder {
        self.config.binary = detection;
        self
    }
}

#[derive(Debug)]
pub(crate) struct LineBufferReader<'b, R> {
    rdr: R,
    line_buffer: &'b mut LineBuffer,
}

impl<'b, R: io::Read> LineBufferReader<'b, R> {
    pub(crate) fn new(rdr: R, line_buffer: &'b mut LineBuffer) -> LineBufferReader<'b, R> {
        line_buffer.clear();
        LineBufferReader { rdr, line_buffer }
    }

    pub(crate) fn absolute_byte_offset(&self) -> u64 {
        self.line_buffer.absolute_byte_offset
    }

    pub(crate) fn binary_byte_offset(&self) -> Option<u64> {
        self.line_buffer.binary_byte_offset
    }

    pub(crate) fn fill(&mut self) -> Result<bool, io::Error> {
        self.line_buffer.fill(&mut self.rdr)
    }

    pub(crate) fn buffer(&self) -> &[u8] {
        self.line_buffer.buffer()
    }

    pub(crate) fn consume(&mut self, amt: usize) {
        self.line_buffer.consume(amt);
    }

    #[cfg(test)]
    fn consume_all(&mut self) {
        let amt = self.buffer().len();
        self.consume(amt);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LineBuffer {
    config: Config,
    buf: Vec<u8>,
    pos: usize,
    last_lineterm: usize,
    end: usize,
    absolute_byte_offset: u64,
    binary_byte_offset: Option<u64>,
}

impl LineBuffer {
    pub(crate) fn set_binary_detection(&mut self, binary: BinaryDetection) {
        self.config.binary = binary;
    }

    fn clear(&mut self) {
        self.pos = 0;
        self.last_lineterm = 0;
        self.end = 0;
        self.absolute_byte_offset = 0;
        self.binary_byte_offset = None;
    }

    fn buffer(&self) -> &[u8] {
        &self.buf[self.pos..self.last_lineterm]
    }

    fn consume(&mut self, amt: usize) {
        assert!(amt <= self.buffer().len());
        self.pos += amt;
        self.absolute_byte_offset += amt as u64;
    }

    fn fill<R: io::Read>(&mut self, mut rdr: R) -> Result<bool, io::Error> {
        if self.config.binary.is_quit() && self.binary_byte_offset.is_some() {
            return Ok(!self.buffer().is_empty());
        }
        self.roll();
        loop {
            self.ensure_capacity()?;
            let readlen = rdr.read(&mut self.buf[self.end..])?;
            if readlen == 0 {
                self.last_lineterm = self.end;
                return Ok(!self.buffer().is_empty());
            }
            let oldend = self.end;
            self.end += readlen;
            match self.config.binary {
                BinaryDetection::None => {}
                BinaryDetection::Quit(byte) => {
                    if let Some(i) = memchr(byte, &self.buf[oldend..self.end]) {
                        self.end = oldend + i;
                        self.last_lineterm = self.end;
                        self.binary_byte_offset = Some(self.absolute_byte_offset + self.end as u64);
                        return Ok(self.pos < self.end);
                    }
                }
                BinaryDetection::Convert(byte) => {
                    if let Some(i) =
                        replace_bytes(&mut self.buf[oldend..self.end], byte, self.config.lineterm)
                        && self.binary_byte_offset.is_none()
                    {
                        self.binary_byte_offset =
                            Some(self.absolute_byte_offset + (oldend + i) as u64);
                    }
                }
            }
            if let Some(i) = memrchr(self.config.lineterm, &self.buf[oldend..self.end]) {
                self.last_lineterm = oldend + i + 1;
                return Ok(true);
            }
        }
    }

    fn roll(&mut self) {
        if self.pos == self.end {
            self.pos = 0;
            self.last_lineterm = 0;
            self.end = 0;
            return;
        }
        let roll_len = self.end - self.pos;
        self.buf.copy_within(self.pos..self.end, 0);
        self.pos = 0;
        self.last_lineterm = roll_len;
        self.end = roll_len;
    }

    fn ensure_capacity(&mut self) -> Result<(), io::Error> {
        if self.end < self.buf.len() {
            return Ok(());
        }
        let len = self.buf.len().max(1);
        let additional = match self.config.buffer_alloc {
            BufferAllocation::Eager => len * 2,
            BufferAllocation::Error(limit) => {
                let used = self.buf.len() - self.config.capacity;
                let n = std::cmp::min(len * 2, limit - used);
                if n == 0 {
                    return Err(alloc_error(self.config.capacity + limit));
                }
                n
            }
        };
        let newlen = self.buf.len() + additional;
        self.buf.resize(newlen, 0);
        Ok(())
    }
}

pub(crate) fn replace_bytes(mut bytes: &mut [u8], src: u8, replacement: u8) -> Option<usize> {
    if src == replacement {
        return None;
    }
    let first_pos = memchr(src, bytes)?;
    bytes[first_pos] = replacement;
    bytes = &mut bytes[first_pos + 1..];
    while let Some(i) = memchr(src, bytes) {
        bytes[i] = replacement;
        bytes = &mut bytes[i + 1..];
        while bytes.first() == Some(&src) {
            bytes[0] = replacement;
            bytes = &mut bytes[1..];
        }
    }
    Some(first_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHERLOCK: &str = "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, success in the province of detective work must always
be, to a very large extent, the result of luck. Sherlock Holmes
can extract a clew from a wisp of straw or a flake of cigar ash;
but Doctor Watson has to have it taken out for him and dusted,
and exhibited clearly, with a label attached.\
";

    fn s(slice: &str) -> String {
        slice.to_string()
    }

    fn replace_str(slice: &str, src: u8, replacement: u8) -> (String, Option<usize>) {
        let mut dst = Vec::from(slice);
        let result = replace_bytes(&mut dst, src, replacement);
        (String::from_utf8(dst).unwrap(), result)
    }

    fn bstr<R: io::Read>(rdr: &LineBufferReader<'_, R>) -> String {
        String::from_utf8_lossy(rdr.buffer()).into_owned()
    }

    #[test]
    fn replace() {
        assert_eq!(replace_str("", b'b', b'z'), (s(""), None));
        assert_eq!(replace_str("a", b'a', b'a'), (s("a"), None));
        assert_eq!(replace_str("a", b'b', b'z'), (s("a"), None));
        assert_eq!(replace_str("abc", b'b', b'z'), (s("azc"), Some(1)));
        assert_eq!(replace_str("abb", b'b', b'z'), (s("azz"), Some(1)));
        assert_eq!(replace_str("aba", b'a', b'z'), (s("zbz"), Some(0)));
        assert_eq!(replace_str("bbb", b'b', b'z'), (s("zzz"), Some(0)));
        assert_eq!(replace_str("bac", b'b', b'z'), (s("zac"), Some(0)));
    }

    #[test]
    fn buffer_basics1() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\n");
        assert_eq!(rdr.absolute_byte_offset(), 0);
        rdr.consume(5);
        assert_eq!(rdr.absolute_byte_offset(), 5);
        rdr.consume_all();
        assert_eq!(rdr.absolute_byte_offset(), 11);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "maggie");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_basics2() {
        let bytes = "homer\nlisa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\nmaggie\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_basics3() {
        let bytes = "\n";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_basics4() {
        let bytes = "\n\n";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "\n\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_empty() {
        let bytes = "";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_zero_capacity() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new().capacity(0).build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        while rdr.fill().unwrap() {
            rdr.consume_all();
        }
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_small_capacity() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new().capacity(1).build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        let mut got = vec![];
        while rdr.fill().unwrap() {
            got.extend_from_slice(rdr.buffer());
            rdr.consume_all();
        }
        assert_eq!(bytes.as_bytes(), &got[..]);
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_limited_capacity1() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new()
            .capacity(1)
            .buffer_alloc(BufferAllocation::Error(5))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\n");
        rdr.consume_all();
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "lisa\n");
        rdr.consume_all();
        assert!(rdr.fill().is_err());
        assert_eq!(bstr(&rdr), "m");
        rdr.consume_all();
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "aggie");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
    }

    #[test]
    fn buffer_limited_capacity2() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new()
            .capacity(1)
            .buffer_alloc(BufferAllocation::Error(6))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\n");
        rdr.consume_all();
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "lisa\n");
        rdr.consume_all();
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "maggie");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
    }

    #[test]
    fn buffer_limited_capacity3() {
        let bytes = "homer\nlisa\nmaggie";
        let mut linebuf = LineBufferBuilder::new()
            .capacity(1)
            .buffer_alloc(BufferAllocation::Error(0))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(rdr.fill().is_err());
        assert_eq!(bstr(&rdr), "");
    }

    #[test]
    fn buffer_binary_none() {
        let bytes = "homer\nli\x00sa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new().build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nli\x00sa\nmaggie\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), None);
    }

    #[test]
    fn buffer_binary_quit1() {
        let bytes = "homer\nli\x00sa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Quit(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nli");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), 8);
        assert_eq!(rdr.binary_byte_offset(), Some(8));
    }

    #[test]
    fn buffer_binary_quit2() {
        let bytes = "\x00homer\nlisa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Quit(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert!(!rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "");
        assert_eq!(rdr.absolute_byte_offset(), 0);
        assert_eq!(rdr.binary_byte_offset(), Some(0));
    }

    #[test]
    fn buffer_binary_quit3() {
        let bytes = "homer\nlisa\nmaggie\n\x00";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Quit(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\nmaggie\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64 - 1);
        assert_eq!(rdr.binary_byte_offset(), Some(bytes.len() as u64 - 1));
    }

    #[test]
    fn buffer_binary_quit4() {
        let bytes = "homer\nlisa\nmaggie\x00\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Quit(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\nmaggie");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64 - 2);
        assert_eq!(rdr.binary_byte_offset(), Some(bytes.len() as u64 - 2));
    }

    #[test]
    fn buffer_binary_quit5() {
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Quit(b'u'))
            .build();
        let mut rdr = LineBufferReader::new(SHERLOCK.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(
            bstr(&rdr),
            "\
For the Doctor Watsons of this world, as opposed to the Sherlock
Holmeses, s\
"
        );
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), 76);
        assert_eq!(rdr.binary_byte_offset(), Some(76));
        assert_eq!(SHERLOCK.as_bytes()[76], b'u');
    }

    #[test]
    fn buffer_binary_convert1() {
        let bytes = "homer\nli\x00sa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Convert(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nli\nsa\nmaggie\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), Some(8));
    }

    #[test]
    fn buffer_binary_convert2() {
        let bytes = "\x00homer\nlisa\nmaggie\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Convert(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "\nhomer\nlisa\nmaggie\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), Some(0));
    }

    #[test]
    fn buffer_binary_convert3() {
        let bytes = "homer\nlisa\nmaggie\n\x00";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Convert(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\nmaggie\n\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), Some(bytes.len() as u64 - 1));
    }

    #[test]
    fn buffer_binary_convert4() {
        let bytes = "homer\nlisa\nmaggie\x00\n";
        let mut linebuf = LineBufferBuilder::new()
            .binary_detection(BinaryDetection::Convert(b'\x00'))
            .build();
        let mut rdr = LineBufferReader::new(bytes.as_bytes(), &mut linebuf);
        assert_eq!(rdr.buffer().len(), 0);
        assert!(rdr.fill().unwrap());
        assert_eq!(bstr(&rdr), "homer\nlisa\nmaggie\n\n");
        rdr.consume_all();
        assert!(!rdr.fill().unwrap());
        assert_eq!(rdr.absolute_byte_offset(), bytes.len() as u64);
        assert_eq!(rdr.binary_byte_offset(), Some(bytes.len() as u64 - 2));
    }
}
