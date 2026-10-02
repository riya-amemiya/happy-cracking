use super::CoderResult;

pub(super) const MAX_STEP: usize = 8;
pub(super) const ASCII_MASK: u128 = 0x8080_8080_8080_8080_8080_8080_8080_8080;
pub(super) const REPLACEMENT_UTF8: [u8; 3] = [0xEF, 0xBF, 0xBD];

#[derive(Clone, Copy, Default)]
pub(super) struct Pending {
    bytes: [u8; 4],
    len: u8,
}

impl Pending {
    pub(super) fn is_empty(self) -> bool {
        self.len == 0
    }

    pub(super) fn len(self) -> usize {
        usize::from(self.len)
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[0];
        self.bytes.copy_within(1.., 0);
        self.len -= 1;
        Some(byte)
    }

    pub(super) fn prepend(&mut self, front: &[u8]) {
        let len = self.len();
        self.bytes.copy_within(..len, front.len());
        self.bytes[..front.len()].copy_from_slice(front);
        self.len += front.len() as u8;
    }
}

pub(super) trait Machine {
    fn pending(&self) -> bool;
    fn fast(&mut self, src: &[u8], dst: &mut [u8]) -> (usize, usize);
    fn step(&mut self, byte: u8, dst: &mut [u8], queue: &mut Pending) -> (usize, bool);
    fn finish(&mut self, dst: &mut [u8], queue: &mut Pending) -> (usize, bool);
}

pub(super) fn drive<M: Machine>(
    machine: &mut M,
    queue: &mut Pending,
    src: &[u8],
    dst: &mut [u8],
    last: bool,
) -> (CoderResult, usize, usize, bool) {
    let mut read = 0;
    let mut written = 0;
    let mut errors = false;
    loop {
        if queue.is_empty() && !machine.pending() {
            let (r, w) = machine.fast(&src[read..], &mut dst[written..]);
            read += r;
            written += w;
        }
        if queue.is_empty() && read == src.len() && (!last || !machine.pending()) {
            return (CoderResult::InputEmpty, read, written, errors);
        }
        if dst.len() - written < MAX_STEP {
            return (CoderResult::OutputFull, read, written, errors);
        }
        let out = &mut dst[written..];
        let (w, e) = if let Some(byte) = queue.pop() {
            machine.step(byte, out, queue)
        } else if read < src.len() {
            read += 1;
            machine.step(src[read - 1], out, queue)
        } else {
            machine.finish(out, queue)
        };
        written += w;
        errors |= e;
    }
}

pub(super) const fn pack_utf8(c: u32) -> u32 {
    if c < 0x80 {
        c
    } else if c < 0x800 {
        (0xC0 | (c >> 6)) | ((0x80 | (c & 0x3F)) << 8)
    } else if c < 0x1_0000 {
        (0xE0 | (c >> 12)) | ((0x80 | ((c >> 6) & 0x3F)) << 8) | ((0x80 | (c & 0x3F)) << 16)
    } else {
        (0xF0 | (c >> 18))
            | ((0x80 | ((c >> 12) & 0x3F)) << 8)
            | ((0x80 | ((c >> 6) & 0x3F)) << 16)
            | ((0x80 | (c & 0x3F)) << 24)
    }
}

#[inline]
pub(super) fn packed_len(packed: u32) -> usize {
    ((packed as u8).leading_ones() as usize).max(1)
}

#[inline]
pub(super) fn put_packed(dst: &mut [u8], at: usize, packed: u32) -> usize {
    dst[at..at + 4].copy_from_slice(&packed.to_le_bytes());
    packed_len(packed)
}

#[inline]
pub(super) fn put(dst: &mut [u8], at: usize, c: u32) -> usize {
    put_packed(dst, at, pack_utf8(c))
}

pub(super) fn write_replacement(dst: &mut [u8]) -> (usize, bool) {
    dst[..3].copy_from_slice(&REPLACEMENT_UTF8);
    (3, true)
}

#[inline]
pub(super) fn copy_ascii(src: &[u8], dst: &mut [u8]) -> usize {
    let limit = src.len().min(dst.len());
    let mut i = 0;
    while i + 16 <= limit {
        let chunk: &[u8; 16] = src[i..i + 16].try_into().unwrap();
        dst[i..i + 16].copy_from_slice(chunk);
        let high = u128::from_le_bytes(*chunk) & ASCII_MASK;
        if high != 0 {
            return i + (high.trailing_zeros() / 8) as usize;
        }
        i += 16;
    }
    while i < limit && src[i] < 0x80 {
        dst[i] = src[i];
        i += 1;
    }
    i
}

pub(super) fn ascii_prefix_len(src: &[u8]) -> usize {
    let mut i = 0;
    while i + 16 <= src.len() {
        let chunk: [u8; 16] = src[i..i + 16].try_into().unwrap();
        let high = u128::from_le_bytes(chunk) & ASCII_MASK;
        if high != 0 {
            return i + (high.trailing_zeros() / 8) as usize;
        }
        i += 16;
    }
    while i < src.len() && src[i] < 0x80 {
        i += 1;
    }
    i
}
