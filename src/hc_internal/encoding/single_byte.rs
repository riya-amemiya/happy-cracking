use super::CoderResult;
use super::machine::{copy_ascii, pack_utf8, packed_len};

const PACKED_REPLACEMENT: u32 = pack_utf8(0xFFFD);

pub(super) const fn pack(code_points: &[u16; 128]) -> [u32; 128] {
    let mut out = [0u32; 128];
    let mut i = 0;
    while i < 128 {
        out[i] = match code_points[i] {
            0 => PACKED_REPLACEMENT,
            c => pack_utf8(c as u32),
        };
        i += 1;
    }
    out
}

const fn user_defined() -> [u32; 128] {
    let mut out = [0u32; 128];
    let mut i = 0;
    while i < 128 {
        out[i] = pack_utf8(0xF780 + i as u32);
        i += 1;
    }
    out
}

pub(super) static X_USER_DEFINED: [u32; 128] = user_defined();

pub(super) fn decode(
    table: &[u32; 128],
    src: &[u8],
    dst: &mut [u8],
) -> (CoderResult, usize, usize, bool) {
    let mut read = 0;
    let mut written = 0;
    let mut errors = false;
    while read < src.len() {
        let byte = src[read];
        if byte < 0x80 {
            let n = copy_ascii(&src[read..], &mut dst[written..]);
            if n == 0 {
                return (CoderResult::OutputFull, read, written, errors);
            }
            read += n;
            written += n;
            continue;
        }
        let packed = table[usize::from(byte - 0x80)];
        let bytes = packed.to_le_bytes();
        let len = packed_len(packed);
        let room = dst.len() - written;
        if room >= 4 {
            dst[written..written + 4].copy_from_slice(&bytes);
        } else if room >= len {
            dst[written..written + len].copy_from_slice(&bytes[..len]);
        } else {
            return (CoderResult::OutputFull, read, written, errors);
        }
        errors |= packed == PACKED_REPLACEMENT;
        read += 1;
        written += len;
    }
    (CoderResult::InputEmpty, read, written, errors)
}
