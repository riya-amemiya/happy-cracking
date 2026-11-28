const BLOCK: usize = 64;
const MAX_NEEDLE: usize = 4;

pub(crate) fn literal_line_counts(hay: &[u8], needle: &[u8], term: u8) -> Option<(u64, u64)> {
    if !cfg!(any(target_arch = "aarch64", target_arch = "x86_64"))
        || needle.is_empty()
        || needle.len() > MAX_NEEDLE
        || needle.contains(&term)
    {
        return None;
    }
    let pad = (1..=u8::MAX).find(|b| *b != term && !needle.contains(b))?;
    let mut keys = [term; MAX_NEEDLE + 1];
    keys[1..=needle.len()].copy_from_slice(needle);
    let keys = &keys[..=needle.len()];
    let full = hay.len() / BLOCK;
    let rem = hay.len() % BLOCK;
    let mut tail = [pad; BLOCK];
    tail[..rem].copy_from_slice(&hay[full * BLOCK..]);
    let blocks = full + usize::from(rem > 0);
    let block = |i: usize| -> &[u8; BLOCK] {
        if i < full {
            hay[i * BLOCK..(i + 1) * BLOCK].try_into().unwrap()
        } else {
            &tail
        }
    };
    let mut lines = 0u64;
    let mut clean = 0u64;
    let mut carry = 1u64;
    let mut cur = [0u64; MAX_NEEDLE + 1];
    if blocks > 0 {
        masks(block(0), keys, &mut cur);
    }
    for b in 0..blocks {
        let mut next = [0u64; MAX_NEEDLE + 1];
        if b + 1 < blocks {
            masks(block(b + 1), keys, &mut next);
        }
        let terms = cur[0];
        let mut starts = cur[1];
        for i in 1..needle.len() {
            starts &= (cur[i + 1] >> i) | (next[i + 1] << (BLOCK - i));
        }
        let (sum, overflow) = (!(terms | starts)).overflowing_add((terms << 1) | carry);
        clean += u64::from((terms & sum).count_ones());
        lines += u64::from(terms.count_ones());
        carry = (terms >> 63) | u64::from(overflow);
        cur = next;
    }
    if hay.last().is_some_and(|&b| b != term) {
        lines += 1;
        clean += carry;
    }
    Some((lines, lines - clean))
}

#[cfg(target_arch = "aarch64")]
fn masks(block: &[u8; BLOCK], keys: &[u8], out: &mut [u64]) {
    use std::arch::aarch64::{
        vandq_u8, vceqq_u8, vdupq_n_u8, vgetq_lane_u64, vld1q_u8, vpaddq_u8, vreinterpretq_u64_u8,
    };
    const BITS: [u8; 16] = [1, 2, 4, 8, 16, 32, 64, 128, 1, 2, 4, 8, 16, 32, 64, 128];
    unsafe {
        let bits = vld1q_u8(BITS.as_ptr());
        let p = block.as_ptr();
        let v0 = vld1q_u8(p);
        let v1 = vld1q_u8(p.add(16));
        let v2 = vld1q_u8(p.add(32));
        let v3 = vld1q_u8(p.add(48));
        for (slot, &key) in out.iter_mut().zip(keys) {
            let k = vdupq_n_u8(key);
            let m0 = vandq_u8(vceqq_u8(v0, k), bits);
            let m1 = vandq_u8(vceqq_u8(v1, k), bits);
            let m2 = vandq_u8(vceqq_u8(v2, k), bits);
            let m3 = vandq_u8(vceqq_u8(v3, k), bits);
            let s = vpaddq_u8(vpaddq_u8(m0, m1), vpaddq_u8(m2, m3));
            *slot = vgetq_lane_u64::<0>(vreinterpretq_u64_u8(vpaddq_u8(s, s)));
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn masks(block: &[u8; BLOCK], keys: &[u8], out: &mut [u64]) {
    use std::arch::x86_64::__m128i;
    unsafe {
        masks_sse2(
            &std::mem::transmute::<[u8; BLOCK], [__m128i; 4]>(*block),
            keys,
            out,
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
fn masks_sse2(lanes: &[std::arch::x86_64::__m128i; 4], keys: &[u8], out: &mut [u64]) {
    use std::arch::x86_64::{_mm_cmpeq_epi8, _mm_movemask_epi8, _mm_set1_epi8};
    for (slot, &key) in out.iter_mut().zip(keys) {
        let k = _mm_set1_epi8(i8::from_ne_bytes([key]));
        let mut mask = 0u64;
        for (i, lane) in lanes.iter().enumerate() {
            let bits = _mm_movemask_epi8(_mm_cmpeq_epi8(*lane, k));
            mask |= u64::from(u32::from_ne_bytes(bits.to_ne_bytes()) & 0xFFFF) << (i * 16);
        }
        *slot = mask;
    }
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn masks(_block: &[u8; BLOCK], _keys: &[u8], _out: &mut [u64]) {}

const GROUP: usize = BLOCK * 255;

pub(crate) fn count_byte(hay: &[u8], byte: u8) -> u64 {
    let (groups, rest) = hay.as_chunks::<GROUP>();
    let (blocks, tail) = rest.as_chunks::<BLOCK>();
    groups
        .iter()
        .map(|group| count_blocks(group.as_chunks::<BLOCK>().0, byte))
        .sum::<u64>()
        + count_blocks(blocks, byte)
        + tail.iter().map(|&b| u64::from(b == byte)).sum::<u64>()
}

#[inline]
fn count_blocks(blocks: &[[u8; BLOCK]], byte: u8) -> u64 {
    let mut acc = [0u8; BLOCK];
    for block in blocks {
        for (slot, &b) in acc.iter_mut().zip(block) {
            *slot = slot.wrapping_add(u8::from(b == byte));
        }
    }
    acc.iter().map(|&n| u64::from(n)).sum()
}
#[cfg(test)]
mod tests {
    use super::{count_byte, literal_line_counts};

    fn naive(hay: &[u8], needle: &[u8], term: u8) -> (u64, u64) {
        let mut lines = 0;
        let mut matched = 0;
        for line in hay.split_inclusive(|&b| b == term) {
            lines += 1;
            if line.windows(needle.len()).any(|w| w == needle) {
                matched += 1;
            }
        }
        (lines, matched)
    }

    #[test]
    fn matches_naive_counts() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let alphabet = b"ab\ncde\n\nf";
        for len in [0usize, 1, 2, 63, 64, 65, 127, 128, 129, 200, 1000, 4099] {
            for round in 0..20 {
                let hay: Vec<u8> = (0..len)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        alphabet[usize::try_from(state % alphabet.len() as u64).unwrap()]
                    })
                    .collect();
                for needle in [&b"a"[..], b"ab", b"cde", b"fab", b"e", b"ba"] {
                    let got = literal_line_counts(&hay, needle, b'\n');
                    if let Some(got) = got {
                        assert_eq!(got, naive(&hay, needle, b'\n'), "len {len} round {round}");
                    }
                }
            }
        }
    }

    #[test]
    fn count_byte_matches_naive() {
        let hay: Vec<u8> = (0..70_000u32)
            .map(|i| u8::try_from(i * 7 % 13).unwrap())
            .collect();
        for len in [0usize, 1, 63, 64, 65, 1000, 16_320, 16_321, 70_000] {
            for start in [0usize, 1, 7] {
                let slice = &hay[start.min(len)..len];
                let want = slice.iter().map(|&b| u64::from(b == 5)).sum::<u64>();
                assert_eq!(count_byte(slice, 5), want, "{len} {start}");
            }
        }
    }

    #[test]
    fn rejects_unsupported_needles() {
        assert_eq!(literal_line_counts(b"abc", b"", b'\n'), None);
        assert_eq!(literal_line_counts(b"abc", b"abcde", b'\n'), None);
        assert_eq!(literal_line_counts(b"abc", b"a\n", b'\n'), None);
    }
}
