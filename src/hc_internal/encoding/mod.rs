mod cjk;
mod data;
mod machine;
mod single_byte;
#[cfg(test)]
mod tests;
mod unicode;

use std::borrow::Cow;
use std::fmt;
use std::io::{self, Read};

use cjk::{Big5, EucJp, EucKr, Gb18030, Iso2022Jp, ShiftJis};
use machine::{MAX_STEP, Pending, ascii_prefix_len, drive};
use unicode::{Utf8, Utf16};

#[derive(Clone, Copy)]
enum Kind {
    SingleByte(&'static [u32; 128]),
    Utf8,
    Utf16Le,
    Utf16Be,
    Gb18030,
    Big5,
    EucJp,
    Iso2022Jp,
    ShiftJis,
    EucKr,
    Replacement,
}

pub struct Encoding {
    name: &'static str,
    kind: Kind,
}

macro_rules! encodings {
    ($($public:ident $init:ident $name:literal $kind:expr;)*) => {
        $(
            static $init: Encoding = Encoding { name: $name, kind: $kind };
            pub static $public: &Encoding = &$init;
        )*
    };
}

encodings! {
    BIG5 BIG5_INIT "Big5" Kind::Big5;
    EUC_JP EUC_JP_INIT "EUC-JP" Kind::EucJp;
    EUC_KR EUC_KR_INIT "EUC-KR" Kind::EucKr;
    GB18030 GB18030_INIT "gb18030" Kind::Gb18030;
    GBK GBK_INIT "GBK" Kind::Gb18030;
    IBM866 IBM866_INIT "IBM866" Kind::SingleByte(&data::IBM866);
    ISO_2022_JP ISO_2022_JP_INIT "ISO-2022-JP" Kind::Iso2022Jp;
    ISO_8859_10 ISO_8859_10_INIT "ISO-8859-10" Kind::SingleByte(&data::ISO_8859_10);
    ISO_8859_13 ISO_8859_13_INIT "ISO-8859-13" Kind::SingleByte(&data::ISO_8859_13);
    ISO_8859_14 ISO_8859_14_INIT "ISO-8859-14" Kind::SingleByte(&data::ISO_8859_14);
    ISO_8859_15 ISO_8859_15_INIT "ISO-8859-15" Kind::SingleByte(&data::ISO_8859_15);
    ISO_8859_16 ISO_8859_16_INIT "ISO-8859-16" Kind::SingleByte(&data::ISO_8859_16);
    ISO_8859_2 ISO_8859_2_INIT "ISO-8859-2" Kind::SingleByte(&data::ISO_8859_2);
    ISO_8859_3 ISO_8859_3_INIT "ISO-8859-3" Kind::SingleByte(&data::ISO_8859_3);
    ISO_8859_4 ISO_8859_4_INIT "ISO-8859-4" Kind::SingleByte(&data::ISO_8859_4);
    ISO_8859_5 ISO_8859_5_INIT "ISO-8859-5" Kind::SingleByte(&data::ISO_8859_5);
    ISO_8859_6 ISO_8859_6_INIT "ISO-8859-6" Kind::SingleByte(&data::ISO_8859_6);
    ISO_8859_7 ISO_8859_7_INIT "ISO-8859-7" Kind::SingleByte(&data::ISO_8859_7);
    ISO_8859_8 ISO_8859_8_INIT "ISO-8859-8" Kind::SingleByte(&data::ISO_8859_8);
    ISO_8859_8_I ISO_8859_8_I_INIT "ISO-8859-8-I" Kind::SingleByte(&data::ISO_8859_8);
    KOI8_R KOI8_R_INIT "KOI8-R" Kind::SingleByte(&data::KOI8_R);
    KOI8_U KOI8_U_INIT "KOI8-U" Kind::SingleByte(&data::KOI8_U);
    MACINTOSH MACINTOSH_INIT "macintosh" Kind::SingleByte(&data::MACINTOSH);
    REPLACEMENT REPLACEMENT_INIT "replacement" Kind::Replacement;
    SHIFT_JIS SHIFT_JIS_INIT "Shift_JIS" Kind::ShiftJis;
    UTF_16BE UTF_16BE_INIT "UTF-16BE" Kind::Utf16Be;
    UTF_16LE UTF_16LE_INIT "UTF-16LE" Kind::Utf16Le;
    UTF_8 UTF_8_INIT "UTF-8" Kind::Utf8;
    WINDOWS_1250 WINDOWS_1250_INIT "windows-1250" Kind::SingleByte(&data::WINDOWS_1250);
    WINDOWS_1251 WINDOWS_1251_INIT "windows-1251" Kind::SingleByte(&data::WINDOWS_1251);
    WINDOWS_1252 WINDOWS_1252_INIT "windows-1252" Kind::SingleByte(&data::WINDOWS_1252);
    WINDOWS_1253 WINDOWS_1253_INIT "windows-1253" Kind::SingleByte(&data::WINDOWS_1253);
    WINDOWS_1254 WINDOWS_1254_INIT "windows-1254" Kind::SingleByte(&data::WINDOWS_1254);
    WINDOWS_1255 WINDOWS_1255_INIT "windows-1255" Kind::SingleByte(&data::WINDOWS_1255);
    WINDOWS_1256 WINDOWS_1256_INIT "windows-1256" Kind::SingleByte(&data::WINDOWS_1256);
    WINDOWS_1257 WINDOWS_1257_INIT "windows-1257" Kind::SingleByte(&data::WINDOWS_1257);
    WINDOWS_1258 WINDOWS_1258_INIT "windows-1258" Kind::SingleByte(&data::WINDOWS_1258);
    WINDOWS_874 WINDOWS_874_INIT "windows-874" Kind::SingleByte(&data::WINDOWS_874);
    X_MAC_CYRILLIC X_MAC_CYRILLIC_INIT "x-mac-cyrillic" Kind::SingleByte(&data::X_MAC_CYRILLIC);
    X_USER_DEFINED X_USER_DEFINED_INIT "x-user-defined" Kind::SingleByte(&single_byte::X_USER_DEFINED);
}

impl Encoding {
    #[must_use]
    pub fn for_label(label: &[u8]) -> Option<&'static Encoding> {
        let trimmed = label.trim_ascii();
        if trimmed.len() > data::LONGEST_LABEL {
            return None;
        }
        let mut lower = [0u8; data::LONGEST_LABEL];
        let lower = &mut lower[..trimmed.len()];
        lower.copy_from_slice(trimmed);
        lower.make_ascii_lowercase();
        data::LABELS
            .binary_search_by(|(candidate, _)| (*candidate).cmp(&*lower))
            .ok()
            .map(|index| data::LABELS[index].1)
    }

    #[must_use]
    pub fn for_label_no_replacement(label: &[u8]) -> Option<&'static Encoding> {
        Self::for_label(label).filter(|encoding| *encoding != REPLACEMENT)
    }

    #[must_use]
    pub fn for_bom(buffer: &[u8]) -> Option<(&'static Encoding, usize)> {
        if buffer.starts_with(b"\xEF\xBB\xBF") {
            Some((UTF_8, 3))
        } else if buffer.starts_with(b"\xFF\xFE") {
            Some((UTF_16LE, 2))
        } else if buffer.starts_with(b"\xFE\xFF") {
            Some((UTF_16BE, 2))
        } else {
            None
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub fn new_decoder_without_bom_handling(&'static self) -> Decoder {
        let variant = match self.kind {
            Kind::SingleByte(table) => Variant::SingleByte(table),
            Kind::Utf8 => Variant::Utf8(Utf8::new()),
            Kind::Utf16Le => Variant::Utf16Le(Utf16::new()),
            Kind::Utf16Be => Variant::Utf16Be(Utf16::new()),
            Kind::Gb18030 => Variant::Gb18030(Gb18030::default()),
            Kind::Big5 => Variant::Big5(Big5::default()),
            Kind::EucJp => Variant::EucJp(EucJp::default()),
            Kind::Iso2022Jp => Variant::Iso2022Jp(Iso2022Jp::default()),
            Kind::ShiftJis => Variant::ShiftJis(ShiftJis::default()),
            Kind::EucKr => Variant::EucKr(EucKr::default()),
            Kind::Replacement => Variant::Replacement(false),
        };
        Decoder {
            queue: Pending::default(),
            variant,
        }
    }

    fn bom(&self) -> &'static [u8] {
        match self.kind {
            Kind::Utf8 => b"\xEF\xBB\xBF",
            Kind::Utf16Le => b"\xFF\xFE",
            Kind::Utf16Be => b"\xFE\xFF",
            _ => b"",
        }
    }

    fn identity_prefix_len(&self, input: &[u8]) -> usize {
        match self.kind {
            Kind::Utf8 => match std::str::from_utf8(input) {
                Ok(_) => input.len(),
                Err(error) => error.valid_up_to(),
            },
            Kind::SingleByte(_)
            | Kind::Gb18030
            | Kind::Big5
            | Kind::EucJp
            | Kind::ShiftJis
            | Kind::EucKr => ascii_prefix_len(input),
            Kind::Utf16Le | Kind::Utf16Be | Kind::Iso2022Jp | Kind::Replacement => 0,
        }
    }
}

impl PartialEq for Encoding {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Encoding {}

impl fmt::Debug for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Encoding {{ {} }}", self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoderResult {
    InputEmpty,
    OutputFull,
}

enum Variant {
    SingleByte(&'static [u32; 128]),
    Utf8(Utf8),
    Utf16Le(Utf16<false>),
    Utf16Be(Utf16<true>),
    Gb18030(Gb18030),
    Big5(Big5),
    EucJp(EucJp),
    Iso2022Jp(Iso2022Jp),
    ShiftJis(ShiftJis),
    EucKr(EucKr),
    Replacement(bool),
}

pub struct Decoder {
    queue: Pending,
    variant: Variant,
}

impl Decoder {
    pub fn decode_to_utf8(
        &mut self,
        src: &[u8],
        dst: &mut [u8],
        last: bool,
    ) -> (CoderResult, usize, usize, bool) {
        let queue = &mut self.queue;
        match &mut self.variant {
            Variant::SingleByte(table) => single_byte::decode(table, src, dst),
            Variant::Utf8(machine) => drive(machine, queue, src, dst, last),
            Variant::Utf16Le(machine) => drive(machine, queue, src, dst, last),
            Variant::Utf16Be(machine) => drive(machine, queue, src, dst, last),
            Variant::Gb18030(machine) => drive(machine, queue, src, dst, last),
            Variant::Big5(machine) => drive(machine, queue, src, dst, last),
            Variant::EucJp(machine) => drive(machine, queue, src, dst, last),
            Variant::Iso2022Jp(machine) => drive(machine, queue, src, dst, last),
            Variant::ShiftJis(machine) => drive(machine, queue, src, dst, last),
            Variant::EucKr(machine) => drive(machine, queue, src, dst, last),
            Variant::Replacement(emitted) => {
                if *emitted || src.is_empty() {
                    return (CoderResult::InputEmpty, src.len(), 0, false);
                }
                if dst.len() < 3 {
                    return (CoderResult::OutputFull, 0, 0, false);
                }
                *emitted = true;
                dst[..3].copy_from_slice(&machine::REPLACEMENT_UTF8);
                (CoderResult::InputEmpty, src.len(), 3, true)
            }
        }
    }

    #[must_use]
    pub fn max_utf8_buffer_length(&self, byte_length: usize) -> Option<usize> {
        let pending = self.queue.len()
            + match &self.variant {
                Variant::SingleByte(_) | Variant::Replacement(_) => 0,
                Variant::Utf8(machine) => machine.pending_bytes(),
                Variant::Utf16Le(machine) => machine.pending_bytes(),
                Variant::Utf16Be(machine) => machine.pending_bytes(),
                Variant::Gb18030(machine) => machine.pending_bytes(),
                Variant::Big5(machine) => machine.pending_bytes(),
                Variant::EucJp(machine) => machine.pending_bytes(),
                Variant::Iso2022Jp(machine) => machine.pending_bytes(),
                Variant::ShiftJis(machine) => machine.pending_bytes(),
                Variant::EucKr(machine) => machine.pending_bytes(),
            };
        let units = byte_length.checked_add(pending)?;
        let units = match self.variant {
            Variant::Utf16Le(_) | Variant::Utf16Be(_) => units / 2 + 1,
            Variant::Replacement(_) => 1,
            _ => units,
        };
        units.checked_mul(3)?.checked_add(MAX_STEP)
    }

    fn drops_pending_lead(&self) -> bool {
        use machine::Machine;
        match &self.variant {
            Variant::ShiftJis(machine) => machine.pending(),
            Variant::EucKr(machine) => machine.pending(),
            Variant::Big5(machine) => machine.pending(),
            _ => false,
        }
    }
}

const READ_CHUNK: usize = 8 * 1024;

fn final_chunk_len(total: usize, skip: usize) -> usize {
    let peek = total.min(3);
    match total - peek {
        0 => peek - skip,
        rest => (rest - 1) % READ_CHUNK + 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeOptions {
    pub encoding: Option<&'static Encoding>,
    pub bom_sniffing: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            encoding: None,
            bom_sniffing: true,
        }
    }
}

fn sniffed_bom_len(head: &[u8]) -> usize {
    if head.len() >= 2 && (head.starts_with(b"\xFF\xFE") || head.starts_with(b"\xFE\xFF")) {
        2
    } else if head == b"\xEF\xBB\xBF" {
        3
    } else {
        0
    }
}

fn select(head: &[u8], opts: &DecodeOptions) -> (usize, Option<&'static Encoding>) {
    if !opts.bom_sniffing {
        return (0, opts.encoding);
    }
    let head = &head[..head.len().min(3)];
    let detected = if head.len() == 3 {
        Encoding::for_bom(head).map(|(encoding, _)| encoding)
    } else {
        None
    };
    let encoding = match detected {
        Some(encoding) if encoding != UTF_8 => Some(encoding),
        _ => opts.encoding,
    };
    (sniffed_bom_len(head), encoding)
}

#[cfg(test)]
#[must_use]
pub fn needs_transcoding(prefix: &[u8], opts: &DecodeOptions) -> bool {
    opts.encoding.is_some() || (opts.bom_sniffing && Encoding::for_bom(prefix).is_some())
}

#[must_use]
pub fn decode_all<'a>(input: &'a [u8], opts: &DecodeOptions) -> Cow<'a, [u8]> {
    let (skip, encoding) = select(input, opts);
    let final_chunk = final_chunk_len(input.len(), skip);
    let input = &input[skip..];
    let Some(encoding) = encoding else {
        return Cow::Borrowed(input);
    };
    let input = input.strip_prefix(encoding.bom()).unwrap_or(input);
    let identity = encoding.identity_prefix_len(input);
    if identity == input.len() {
        return Cow::Borrowed(input);
    }
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let rest = &input[identity..];
    let capacity = decoder
        .max_utf8_buffer_length(rest.len())
        .and_then(|n| n.checked_add(identity))
        .unwrap_or(input.len());
    let mut out = vec![0u8; capacity];
    out[..identity].copy_from_slice(&input[..identity]);
    let mut read = 0;
    let mut written = identity;
    let mut last = false;
    loop {
        let (result, r, w, _) = decoder.decode_to_utf8(&rest[read..], &mut out[written..], last);
        read += r;
        written += w;
        if result == CoderResult::OutputFull {
            out.resize(out.len() + rest.len() - read + 64, 0);
        } else if last || (decoder.drops_pending_lead() && final_chunk != 1) {
            break;
        } else {
            last = true;
        }
    }
    out.truncate(written);
    Cow::Owned(out)
}

pub struct DecodeReader<R> {
    inner: R,
    opts: DecodeOptions,
    started: bool,
    decoder: Option<Decoder>,
    input: Box<[u8]>,
    pos: usize,
    len: usize,
    last_chunk: usize,
    eof: bool,
    done: bool,
    spill: [u8; 2 * MAX_STEP],
    spill_pos: usize,
    spill_len: usize,
}

impl<R: Read> DecodeReader<R> {
    pub fn new(inner: R, opts: DecodeOptions) -> Self {
        Self {
            inner,
            opts,
            started: false,
            decoder: None,
            input: vec![0; READ_CHUNK].into_boxed_slice(),
            pos: 0,
            len: 0,
            last_chunk: 0,
            eof: false,
            done: false,
            spill: [0; 2 * MAX_STEP],
            spill_pos: 0,
            spill_len: 0,
        }
    }

    fn read_once(&mut self, end: usize) -> io::Result<usize> {
        loop {
            match self.inner.read(&mut self.input[self.len..end]) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(0);
                }
                Ok(n) => {
                    self.len += n;
                    return Ok(n);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        self.input.copy_within(self.pos..self.len, 0);
        self.len -= self.pos;
        self.pos = 0;
        let n = self.read_once(self.input.len())?;
        if n > 0 {
            self.last_chunk = n;
        }
        Ok(())
    }

    fn start(&mut self) -> io::Result<()> {
        while self.len < 3 && !self.eof {
            self.read_once(3)?;
        }
        let (skip, encoding) = select(&self.input[..self.len], &self.opts);
        self.pos = skip;
        self.last_chunk = self.len - skip;
        if let Some(encoding) = encoding {
            let bom = encoding.bom();
            while self.len - self.pos < bom.len() && !self.eof {
                self.fill()?;
            }
            if self.input[self.pos..self.len].starts_with(bom) {
                self.pos += bom.len();
            }
            self.decoder = Some(encoding.new_decoder_without_bom_handling());
        }
        self.started = true;
        Ok(())
    }

    fn passthrough(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos < self.len {
            let n = buf.len().min(self.len - self.pos);
            buf[..n].copy_from_slice(&self.input[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        if self.eof {
            return Ok(0);
        }
        self.inner.read(buf)
    }

    fn transcode(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.spill_pos < self.spill_len {
            let n = buf.len().min(self.spill_len - self.spill_pos);
            buf[..n].copy_from_slice(&self.spill[self.spill_pos..self.spill_pos + n]);
            self.spill_pos += n;
            return Ok(n);
        }
        loop {
            if self.done {
                return Ok(0);
            }
            if self.pos == self.len && !self.eof {
                self.fill()?;
            }
            let last = self.eof && self.pos == self.len;
            let Some(decoder) = self.decoder.as_mut() else {
                return Ok(0);
            };
            if last && decoder.drops_pending_lead() && self.last_chunk != 1 {
                self.done = true;
                return Ok(0);
            }
            let src = &self.input[self.pos..self.len];
            let direct = buf.len() >= MAX_STEP;
            let (result, read, written, _) = if direct {
                decoder.decode_to_utf8(src, buf, last)
            } else {
                decoder.decode_to_utf8(src, &mut self.spill, last)
            };
            self.pos += read;
            if last && result == CoderResult::InputEmpty {
                self.done = true;
            }
            if written == 0 {
                continue;
            }
            if direct {
                return Ok(written);
            }
            let n = buf.len().min(written);
            buf[..n].copy_from_slice(&self.spill[..n]);
            self.spill_pos = n;
            self.spill_len = written;
            return Ok(n);
        }
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if !self.started {
            self.start()?;
        }
        if self.decoder.is_none() {
            return self.passthrough(buf);
        }
        self.transcode(buf)
    }
}
