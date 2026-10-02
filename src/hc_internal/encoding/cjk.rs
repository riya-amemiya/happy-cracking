use std::sync::OnceLock;

use super::data;
use super::machine::{Machine, Pending, copy_ascii, pack_utf8, put, put_packed, write_replacement};

type PairTable = OnceLock<Box<[u32; 0x1_0000]>>;

static SHIFT_JIS_PAIRS: PairTable = OnceLock::new();
static EUC_JP_PAIRS: PairTable = OnceLock::new();
static EUC_KR_PAIRS: PairTable = OnceLock::new();
static GBK_PAIRS: PairTable = OnceLock::new();
static BIG5_PAIRS: PairTable = OnceLock::new();
static ISO_2022_JP_PAIRS: PairTable = OnceLock::new();

fn pair_table(
    cell: &'static PairTable,
    decode: fn(u8, u8) -> Option<u32>,
) -> &'static [u32; 0x1_0000] {
    cell.get_or_init(|| {
        let mut table: Box<[u32; 0x1_0000]> =
            vec![0u32; 0x1_0000].into_boxed_slice().try_into().unwrap();
        for (index, slot) in table.iter_mut().enumerate() {
            let [lead, trail] = (index as u16).to_be_bytes();
            if let Some(c) = decode(lead, trail) {
                *slot = pack_utf8(c);
            }
        }
        table
    })
}

#[inline]
fn fast_pairs(
    src: &[u8],
    dst: &mut [u8],
    pairs: &[u32; 0x1_0000],
    single: impl Fn(u8) -> Option<u32>,
) -> (usize, usize) {
    let mut read = 0;
    let mut written = 0;
    while read + 1 < src.len() && written + 16 <= dst.len() {
        let lead = src[read];
        if lead < 0x80 {
            dst[written] = lead;
            read += 1;
            written += 1;
            if src[read] < 0x80 {
                let n = copy_ascii(&src[read..], &mut dst[written..]);
                read += n;
                written += n;
            }
            continue;
        }
        if let Some(c) = single(lead) {
            written += put(dst, written, c);
            read += 1;
            continue;
        }
        let packed = pairs[usize::from(u16::from_be_bytes([lead, src[read + 1]]))];
        if packed == 0 {
            break;
        }
        written += put_packed(dst, written, packed);
        read += 2;
    }
    if read + 1 == src.len() && written < dst.len() && src[read] < 0x80 {
        dst[written] = src[read];
        read += 1;
        written += 1;
    }
    (read, written)
}

fn lookup(table: &[u16], pointer: usize) -> Option<u32> {
    match table.get(pointer) {
        Some(&c) if c != 0 => Some(u32::from(c)),
        _ => None,
    }
}

fn reject(byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
    if byte < 0x80 {
        queue.prepend(&[byte]);
    }
    write_replacement(dst)
}

fn gb18030_two_byte(lead: u8, byte: u8) -> Option<u32> {
    let offset = if byte < 0x7F { 0x40 } else { 0x41 };
    if !(0x81..=0xFE).contains(&lead)
        || (!(0x40..=0x7E).contains(&byte) && !(0x80..=0xFE).contains(&byte))
    {
        return None;
    }
    Some(u32::from(
        data::GB18030[usize::from(lead - 0x81) * 190 + usize::from(byte - offset)],
    ))
}

fn gb18030_four_byte(first: u8, second: u8, third: u8, fourth: u8) -> Option<u32> {
    let pointer = u32::from(first - 0x81) * 12600
        + u32::from(second - 0x30) * 1260
        + u32::from(third - 0x81) * 10
        + u32::from(fourth - 0x30);
    if (pointer > 39419 && pointer < 189_000) || pointer > 1_237_575 {
        return None;
    }
    if pointer >= 189_000 {
        return Some(0x1_0000 + pointer - 189_000);
    }
    if pointer == 7457 {
        return Some(0xE7C7);
    }
    let index = data::GB18030_RANGES.partition_point(|&(p, _)| u32::from(p) <= pointer) - 1;
    let (offset, code_point) = data::GB18030_RANGES[index];
    Some(u32::from(code_point) + pointer - u32::from(offset))
}

#[derive(Default)]
pub(super) struct Gb18030 {
    first: u8,
    second: u8,
    third: u8,
}

impl Gb18030 {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.first != 0) + usize::from(self.second != 0) + usize::from(self.third != 0)
    }
}

impl Machine for Gb18030 {
    fn pending(&self) -> bool {
        self.first != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        let pairs = pair_table(&GBK_PAIRS, gb18030_two_byte);
        let mut read = 0;
        let mut written = 0;
        loop {
            let (r, w) = fast_pairs(&src[read..], &mut dst[written..], pairs, |byte| {
                (byte == 0x80).then_some(0x20AC)
            });
            read += r;
            written += w;
            if read + 3 >= src.len() || dst.len() - written < 16 {
                break;
            }
            let (first, second, third, fourth) =
                (src[read], src[read + 1], src[read + 2], src[read + 3]);
            if !(0x81..=0xFE).contains(&first)
                || !(0x30..=0x39).contains(&second)
                || !(0x81..=0xFE).contains(&third)
                || !(0x30..=0x39).contains(&fourth)
            {
                break;
            }
            let Some(code_point) = gb18030_four_byte(first, second, third, fourth) else {
                break;
            };
            written += put(dst, written, code_point);
            read += 4;
        }
        (read, written)
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.third != 0 {
            let (first, second, third) = (self.first, self.second, self.third);
            *self = Self::default();
            if !(0x30..=0x39).contains(&byte) {
                queue.prepend(&[second, third, byte]);
                return write_replacement(dst);
            }
            return match gb18030_four_byte(first, second, third, byte) {
                Some(code_point) => (put(dst, 0, code_point), false),
                None => write_replacement(dst),
            };
        }
        if self.second != 0 {
            if (0x81..=0xFE).contains(&byte) {
                self.third = byte;
                return (0, false);
            }
            let second = self.second;
            *self = Self::default();
            queue.prepend(&[second, byte]);
            return write_replacement(dst);
        }
        if self.first != 0 {
            if (0x30..=0x39).contains(&byte) {
                self.second = byte;
                return (0, false);
            }
            let lead = self.first;
            self.first = 0;
            return match gb18030_two_byte(lead, byte) {
                Some(code_point) => (put(dst, 0, code_point), false),
                None => reject(byte, dst, queue),
            };
        }
        match byte {
            0x00..=0x7F => (put(dst, 0, u32::from(byte)), false),
            0x80 => (put(dst, 0, 0x20AC), false),
            0x81..=0xFE => {
                self.first = byte;
                (0, false)
            }
            0xFF => write_replacement(dst),
        }
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        *self = Self::default();
        write_replacement(dst)
    }
}

fn big5_code_points(lead: u8, byte: u8) -> Option<(u32, u32)> {
    let offset = if byte < 0x7F { 0x40 } else { 0x62 };
    if !(0x81..=0xFE).contains(&lead)
        || (!(0x40..=0x7E).contains(&byte) && !(0xA1..=0xFE).contains(&byte))
    {
        return None;
    }
    let pointer = usize::from(lead - 0x81) * 157 + usize::from(byte - offset);
    match pointer {
        1133 => Some((0x00CA, 0x0304)),
        1135 => Some((0x00CA, 0x030C)),
        1164 => Some((0x00EA, 0x0304)),
        1166 => Some((0x00EA, 0x030C)),
        _ => match pointer
            .checked_sub(data::BIG5_OFFSET)
            .and_then(|index| data::BIG5.get(index))
        {
            Some(&c) if c != 0 => Some((c, 0)),
            _ => None,
        },
    }
}

fn big5_single_code_point(lead: u8, byte: u8) -> Option<u32> {
    match big5_code_points(lead, byte) {
        Some((c, 0)) => Some(c),
        _ => None,
    }
}

fn write_big5(dst: &mut [u8], (first, second): (u32, u32)) -> usize {
    let n = put(dst, 0, first);
    if second == 0 {
        n
    } else {
        n + put(dst, n, second)
    }
}

#[derive(Default)]
pub(super) struct Big5 {
    lead: u8,
}

impl Big5 {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead != 0)
    }
}

impl Machine for Big5 {
    fn pending(&self) -> bool {
        self.lead != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        fast_pairs(
            src,
            dst,
            pair_table(&BIG5_PAIRS, big5_single_code_point),
            |_| None,
        )
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.lead != 0 {
            let lead = self.lead;
            self.lead = 0;
            return match big5_code_points(lead, byte) {
                Some(code_points) => (write_big5(dst, code_points), false),
                None => reject(byte, dst, queue),
            };
        }
        match byte {
            0x00..=0x7F => (put(dst, 0, u32::from(byte)), false),
            0x81..=0xFE => {
                self.lead = byte;
                (0, false)
            }
            _ => write_replacement(dst),
        }
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        self.lead = 0;
        write_replacement(dst)
    }
}

fn euc_jp_pair(lead: u8, byte: u8, jis0212: bool) -> Option<u32> {
    if !(0xA1..=0xFE).contains(&lead) || !(0xA1..=0xFE).contains(&byte) {
        return None;
    }
    let pointer = usize::from(lead - 0xA1) * 94 + usize::from(byte - 0xA1);
    if jis0212 {
        lookup(&data::JIS0212, pointer)
    } else {
        lookup(&data::JIS0208, pointer)
    }
}

fn euc_jp_two_byte(lead: u8, byte: u8) -> Option<u32> {
    if lead == 0x8E && (0xA1..=0xDF).contains(&byte) {
        return Some(0xFF61 - 0xA1 + u32::from(byte));
    }
    euc_jp_pair(lead, byte, false)
}

#[derive(Default)]
pub(super) struct EucJp {
    lead: u8,
    jis0212: bool,
}

impl EucJp {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead != 0) + usize::from(self.jis0212)
    }
}

impl Machine for EucJp {
    fn pending(&self) -> bool {
        self.lead != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        fast_pairs(src, dst, pair_table(&EUC_JP_PAIRS, euc_jp_two_byte), |_| {
            None
        })
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.lead == 0x8E && (0xA1..=0xDF).contains(&byte) {
            self.lead = 0;
            return (put(dst, 0, 0xFF61 - 0xA1 + u32::from(byte)), false);
        }
        if self.lead == 0x8F && (0xA1..=0xFE).contains(&byte) {
            self.jis0212 = true;
            self.lead = byte;
            return (0, false);
        }
        if self.lead != 0 {
            let lead = self.lead;
            let jis0212 = self.jis0212;
            *self = Self::default();
            return match euc_jp_pair(lead, byte, jis0212) {
                Some(code_point) => (put(dst, 0, code_point), false),
                None => reject(byte, dst, queue),
            };
        }
        match byte {
            0x00..=0x7F => (put(dst, 0, u32::from(byte)), false),
            0x8E | 0x8F | 0xA1..=0xFE => {
                self.lead = byte;
                (0, false)
            }
            _ => write_replacement(dst),
        }
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        *self = Self::default();
        write_replacement(dst)
    }
}

fn euc_kr_pair(lead: u8, byte: u8) -> Option<u32> {
    if !(0x81..=0xFE).contains(&lead) || !(0x41..=0xFE).contains(&byte) {
        return None;
    }
    lookup(
        &data::EUC_KR,
        usize::from(lead - 0x81) * 190 + usize::from(byte - 0x41),
    )
}

#[derive(Default)]
pub(super) struct EucKr {
    lead: u8,
}

impl EucKr {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead != 0)
    }
}

impl Machine for EucKr {
    fn pending(&self) -> bool {
        self.lead != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        fast_pairs(src, dst, pair_table(&EUC_KR_PAIRS, euc_kr_pair), |_| None)
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.lead != 0 {
            let lead = self.lead;
            self.lead = 0;
            return match euc_kr_pair(lead, byte) {
                Some(code_point) => (put(dst, 0, code_point), false),
                None => reject(byte, dst, queue),
            };
        }
        match byte {
            0x00..=0x7F => (put(dst, 0, u32::from(byte)), false),
            0x81..=0xFE => {
                self.lead = byte;
                (0, false)
            }
            _ => write_replacement(dst),
        }
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        self.lead = 0;
        write_replacement(dst)
    }
}

fn shift_jis_pair(lead: u8, byte: u8) -> Option<u32> {
    let offset = if byte < 0x7F { 0x40 } else { 0x41 };
    let lead_offset = match lead {
        0x81..=0x9F => 0x81,
        0xE0..=0xFC => 0xC1,
        _ => return None,
    };
    if !(0x40..=0x7E).contains(&byte) && !(0x80..=0xFC).contains(&byte) {
        return None;
    }
    let pointer = usize::from(lead - lead_offset) * 188 + usize::from(byte - offset);
    if (8836..=10715).contains(&pointer) {
        return Some(0xE000 - 8836 + pointer as u32);
    }
    lookup(&data::JIS0208, pointer)
}

fn shift_jis_single(byte: u8) -> Option<u32> {
    match byte {
        0x80 => Some(0x80),
        0xA1..=0xDF => Some(0xFF61 - 0xA1 + u32::from(byte)),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct ShiftJis {
    lead: u8,
}

impl ShiftJis {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead != 0)
    }
}

impl Machine for ShiftJis {
    fn pending(&self) -> bool {
        self.lead != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        fast_pairs(
            src,
            dst,
            pair_table(&SHIFT_JIS_PAIRS, shift_jis_pair),
            shift_jis_single,
        )
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.lead != 0 {
            let lead = self.lead;
            self.lead = 0;
            return match shift_jis_pair(lead, byte) {
                Some(code_point) => (put(dst, 0, code_point), false),
                None => reject(byte, dst, queue),
            };
        }
        if let Some(code_point) = shift_jis_single(byte) {
            return (put(dst, 0, code_point), false);
        }
        match byte {
            0x00..=0x7F => (put(dst, 0, u32::from(byte)), false),
            0x81..=0x9F | 0xE0..=0xFC => {
                self.lead = byte;
                (0, false)
            }
            _ => write_replacement(dst),
        }
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        self.lead = 0;
        write_replacement(dst)
    }
}

fn iso_2022_jp_pair(lead: u8, byte: u8) -> Option<u32> {
    if !(0x21..=0x7E).contains(&lead) || !(0x21..=0x7E).contains(&byte) {
        return None;
    }
    lookup(
        &data::JIS0208,
        usize::from(lead - 0x21) * 94 + usize::from(byte - 0x21),
    )
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Iso2022JpState {
    #[default]
    Ascii,
    Roman,
    Katakana,
    LeadByte,
    TrailByte,
    EscapeStart,
    Escape,
}

#[derive(Default)]
pub(super) struct Iso2022Jp {
    state: Iso2022JpState,
    output_state: Iso2022JpState,
    lead: u8,
    output_flag: bool,
}

impl Iso2022Jp {
    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead != 0) + usize::from(self.pending())
    }
}

impl Machine for Iso2022Jp {
    fn pending(&self) -> bool {
        matches!(
            self.state,
            Iso2022JpState::TrailByte | Iso2022JpState::EscapeStart | Iso2022JpState::Escape
        )
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        let mut read = 0;
        let mut written = 0;
        match self.state {
            Iso2022JpState::Ascii => {
                let limit = src.len().min(dst.len());
                while read < limit {
                    let byte = src[read];
                    if byte >= 0x80 || byte == 0x0E || byte == 0x0F || byte == 0x1B {
                        break;
                    }
                    dst[read] = byte;
                    read += 1;
                }
                written = read;
            }
            Iso2022JpState::LeadByte => {
                let pairs = pair_table(&ISO_2022_JP_PAIRS, iso_2022_jp_pair);
                while read + 1 < src.len() && written + 16 <= dst.len() {
                    let packed = pairs[usize::from(u16::from_be_bytes([src[read], src[read + 1]]))];
                    if packed == 0 {
                        break;
                    }
                    written += put_packed(dst, written, packed);
                    read += 2;
                }
            }
            _ => {}
        }
        if read > 0 {
            self.output_flag = false;
        }
        (read, written)
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        match self.state {
            Iso2022JpState::Ascii | Iso2022JpState::Roman => {
                if byte == 0x1B {
                    self.state = Iso2022JpState::EscapeStart;
                    return (0, false);
                }
                self.output_flag = false;
                if byte >= 0x80 || byte == 0x0E || byte == 0x0F {
                    return write_replacement(dst);
                }
                let code_point = match (self.state, byte) {
                    (Iso2022JpState::Roman, 0x5C) => 0x00A5,
                    (Iso2022JpState::Roman, 0x7E) => 0x203E,
                    _ => u32::from(byte),
                };
                (put(dst, 0, code_point), false)
            }
            Iso2022JpState::Katakana => {
                if byte == 0x1B {
                    self.state = Iso2022JpState::EscapeStart;
                    return (0, false);
                }
                self.output_flag = false;
                if (0x21..=0x5F).contains(&byte) {
                    return (put(dst, 0, 0xFF61 - 0x21 + u32::from(byte)), false);
                }
                write_replacement(dst)
            }
            Iso2022JpState::LeadByte => {
                if byte == 0x1B {
                    self.state = Iso2022JpState::EscapeStart;
                    return (0, false);
                }
                self.output_flag = false;
                if (0x21..=0x7E).contains(&byte) {
                    self.lead = byte;
                    self.state = Iso2022JpState::TrailByte;
                    return (0, false);
                }
                write_replacement(dst)
            }
            Iso2022JpState::TrailByte => {
                if byte == 0x1B {
                    self.state = Iso2022JpState::EscapeStart;
                    return write_replacement(dst);
                }
                self.state = Iso2022JpState::LeadByte;
                match iso_2022_jp_pair(self.lead, byte) {
                    Some(code_point) => (put(dst, 0, code_point), false),
                    None => write_replacement(dst),
                }
            }
            Iso2022JpState::EscapeStart => {
                if byte == 0x24 || byte == 0x28 {
                    self.lead = byte;
                    self.state = Iso2022JpState::Escape;
                    return (0, false);
                }
                queue.prepend(&[byte]);
                self.output_flag = false;
                self.state = self.output_state;
                write_replacement(dst)
            }
            Iso2022JpState::Escape => {
                let lead = self.lead;
                self.lead = 0;
                let next = match (lead, byte) {
                    (0x28, 0x42) => Some(Iso2022JpState::Ascii),
                    (0x28, 0x4A) => Some(Iso2022JpState::Roman),
                    (0x28, 0x49) => Some(Iso2022JpState::Katakana),
                    (0x24, 0x40 | 0x42) => Some(Iso2022JpState::LeadByte),
                    _ => None,
                };
                if let Some(next) = next {
                    self.state = next;
                    self.output_state = next;
                    let output = self.output_flag;
                    self.output_flag = true;
                    return if output {
                        write_replacement(dst)
                    } else {
                        (0, false)
                    };
                }
                queue.prepend(&[lead, byte]);
                self.output_flag = false;
                self.state = self.output_state;
                write_replacement(dst)
            }
        }
    }

    fn finish(&mut self, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        match self.state {
            Iso2022JpState::TrailByte => {
                self.state = Iso2022JpState::LeadByte;
            }
            Iso2022JpState::Escape => {
                queue.prepend(&[self.lead]);
                self.lead = 0;
                self.output_flag = false;
                self.state = self.output_state;
            }
            _ => {
                self.output_flag = false;
                self.state = self.output_state;
            }
        }
        write_replacement(dst)
    }
}
