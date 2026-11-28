use super::machine::{Machine, Pending, put, write_replacement};

pub(super) struct Utf8 {
    code_point: u32,
    seen: u8,
    needed: u8,
    lower: u8,
    upper: u8,
}

impl Utf8 {
    pub(super) fn new() -> Self {
        Self {
            code_point: 0,
            seen: 0,
            needed: 0,
            lower: 0x80,
            upper: 0xBF,
        }
    }

    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.seen) + usize::from(self.needed != 0)
    }
}

impl Machine for Utf8 {
    fn pending(&self) -> bool {
        self.needed != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        let window = src.len().min(dst.len());
        let valid = match std::str::from_utf8(&src[..window]) {
            Ok(_) => window,
            Err(error) => error.valid_up_to(),
        };
        dst[..valid].copy_from_slice(&src[..valid]);
        (valid, valid)
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        if self.needed == 0 {
            match byte {
                0x00..=0x7F => {
                    dst[0] = byte;
                    return (1, false);
                }
                0xC2..=0xDF => {
                    self.needed = 1;
                    self.code_point = u32::from(byte & 0x1F);
                }
                0xE0..=0xEF => {
                    if byte == 0xE0 {
                        self.lower = 0xA0;
                    } else if byte == 0xED {
                        self.upper = 0x9F;
                    }
                    self.needed = 2;
                    self.code_point = u32::from(byte & 0x0F);
                }
                0xF0..=0xF4 => {
                    if byte == 0xF0 {
                        self.lower = 0x90;
                    } else if byte == 0xF4 {
                        self.upper = 0x8F;
                    }
                    self.needed = 3;
                    self.code_point = u32::from(byte & 0x07);
                }
                _ => return write_replacement(dst),
            }
            return (0, false);
        }
        if byte < self.lower || byte > self.upper {
            *self = Self::new();
            queue.prepend(&[byte]);
            return write_replacement(dst);
        }
        self.lower = 0x80;
        self.upper = 0xBF;
        self.code_point = (self.code_point << 6) | u32::from(byte & 0x3F);
        self.seen += 1;
        if self.seen != self.needed {
            return (0, false);
        }
        let code_point = self.code_point;
        *self = Self::new();
        (put(dst, 0, code_point), false)
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        *self = Self::new();
        write_replacement(dst)
    }
}

pub(super) struct Utf16<const BIG_ENDIAN: bool> {
    lead_byte: Option<u8>,
    lead_surrogate: u16,
}

const ASCII_LANES_LE: u128 = 0xFF80_FF80_FF80_FF80_FF80_FF80_FF80_FF80;
const ASCII_LANES_BE: u128 = 0x80FF_80FF_80FF_80FF_80FF_80FF_80FF_80FF;

fn even_bytes(word: u64) -> u32 {
    let x = word & 0x00FF_00FF_00FF_00FF;
    let x = (x | (x >> 8)) & 0x0000_FFFF_0000_FFFF;
    (x | (x >> 16)) as u32
}

impl<const BIG_ENDIAN: bool> Utf16<BIG_ENDIAN> {
    pub(super) fn new() -> Self {
        Self {
            lead_byte: None,
            lead_surrogate: 0,
        }
    }

    pub(super) fn pending_bytes(&self) -> usize {
        usize::from(self.lead_byte.is_some()) + if self.lead_surrogate == 0 { 0 } else { 2 }
    }

    fn unit(lead: u8, trail: u8) -> u16 {
        if BIG_ENDIAN {
            u16::from_be_bytes([lead, trail])
        } else {
            u16::from_le_bytes([lead, trail])
        }
    }
}

impl<const BIG_ENDIAN: bool> Machine for Utf16<BIG_ENDIAN> {
    fn pending(&self) -> bool {
        self.lead_byte.is_some() || self.lead_surrogate != 0
    }

    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize) {
        let mut read = 0;
        let mut written = 0;
        while read + 2 <= src.len() && dst.len() - written >= 8 {
            if read + 16 <= src.len() {
                let chunk: [u8; 16] = src[read..read + 16].try_into().unwrap();
                let word = u128::from_le_bytes(chunk);
                let mask = if BIG_ENDIAN {
                    ASCII_LANES_BE
                } else {
                    ASCII_LANES_LE
                };
                if word & mask == 0 {
                    let word = if BIG_ENDIAN { word >> 8 } else { word };
                    let low = even_bytes(word as u64);
                    let high = even_bytes((word >> 64) as u64);
                    dst[written..written + 4].copy_from_slice(&low.to_le_bytes());
                    dst[written + 4..written + 8].copy_from_slice(&high.to_le_bytes());
                    read += 16;
                    written += 8;
                    continue;
                }
            }
            let unit = Self::unit(src[read], src[read + 1]);
            if !(0xD800..=0xDFFF).contains(&unit) {
                written += put(dst, written, u32::from(unit));
                read += 2;
                continue;
            }
            if unit >= 0xDC00 || read + 4 > src.len() {
                break;
            }
            let trail = Self::unit(src[read + 2], src[read + 3]);
            if !(0xDC00..=0xDFFF).contains(&trail) {
                break;
            }
            let code_point =
                0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(trail) - 0xDC00);
            written += put(dst, written, code_point);
            read += 4;
        }
        (read, written)
    }

    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool) {
        let Some(lead) = self.lead_byte.take() else {
            self.lead_byte = Some(byte);
            return (0, false);
        };
        let unit = Self::unit(lead, byte);
        if self.lead_surrogate != 0 {
            let lead_surrogate = self.lead_surrogate;
            self.lead_surrogate = 0;
            if (0xDC00..=0xDFFF).contains(&unit) {
                let code_point = 0x1_0000
                    + ((u32::from(lead_surrogate) - 0xD800) << 10)
                    + (u32::from(unit) - 0xDC00);
                return (put(dst, 0, code_point), false);
            }
            queue.prepend(&[lead, byte]);
            return write_replacement(dst);
        }
        if (0xD800..=0xDBFF).contains(&unit) {
            self.lead_surrogate = unit;
            return (0, false);
        }
        if (0xDC00..=0xDFFF).contains(&unit) {
            return write_replacement(dst);
        }
        (put(dst, 0, u32::from(unit)), false)
    }

    fn finish(&mut self, dst: &mut [u8], _queue: &mut Pending) -> (usize, bool) {
        *self = Self::new();
        write_replacement(dst)
    }
}
