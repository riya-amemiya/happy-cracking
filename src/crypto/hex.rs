use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HexAction {
    #[command(about = "Encode to hex")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from hex")]
    Decode {
        #[arg(help = "Hex string")]
        input: String,
    },
}

pub fn run(action: HexAction) -> Result<()> {
    match action {
        HexAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        HexAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub const MAX_HEX_BYTES: usize = 16 * 1024 * 1024;

const HEX_LUT: [u8; 16] = *b"0123456789abcdef";

pub fn decode_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_HEX_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    hex::decode(hex_str).context("Failed to decode hex")
}

#[must_use]
pub fn encode(input: &str) -> String {
    encode_bytes(input.as_bytes())
}

fn encode_bytes(src: &[u8]) -> String {
    let mut out = vec![0u8; src.len() * 2];
    encode_into(src, &mut out);
    String::from_utf8(out).expect("hex output is ASCII")
}

fn encode_into(src: &[u8], dst: &mut [u8]) {
    debug_assert!(dst.len() >= src.len() * 2);
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("ssse3") {
            unsafe {
                encode_ssse3(src, dst);
            }
            return;
        }
    }
    encode_scalar(src, dst);
}

fn encode_scalar(src: &[u8], dst: &mut [u8]) {
    for (i, &b) in src.iter().enumerate() {
        dst[i * 2] = HEX_LUT[(b >> 4) as usize];
        dst[i * 2 + 1] = HEX_LUT[(b & 0x0f) as usize];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
fn encode_ssse3(src: &[u8], dst: &mut [u8]) {
    use std::arch::x86_64::{
        __m128i, _mm_and_si128, _mm_set1_epi8, _mm_setr_epi8, _mm_shuffle_epi8, _mm_srli_epi16,
        _mm_unpackhi_epi8, _mm_unpacklo_epi8,
    };
    unsafe {
        let lut = _mm_setr_epi8(
            b'0' as i8, b'1' as i8, b'2' as i8, b'3' as i8, b'4' as i8, b'5' as i8, b'6' as i8,
            b'7' as i8, b'8' as i8, b'9' as i8, b'a' as i8, b'b' as i8, b'c' as i8, b'd' as i8,
            b'e' as i8, b'f' as i8,
        );
        let mask = _mm_set1_epi8(0x0f);
        let mut i = 0;
        while i + 16 <= src.len() {
            let mut chunk = [0u8; 16];
            chunk.copy_from_slice(&src[i..i + 16]);
            let v = std::mem::transmute::<[u8; 16], __m128i>(chunk);
            let hi = _mm_and_si128(_mm_srli_epi16::<4>(v), mask);
            let lo = _mm_and_si128(v, mask);
            let hi_h = _mm_shuffle_epi8(lut, hi);
            let lo_h = _mm_shuffle_epi8(lut, lo);
            let t0 = std::mem::transmute::<__m128i, [u8; 16]>(_mm_unpacklo_epi8(hi_h, lo_h));
            let t1 = std::mem::transmute::<__m128i, [u8; 16]>(_mm_unpackhi_epi8(hi_h, lo_h));
            dst[i * 2..i * 2 + 16].copy_from_slice(&t0);
            dst[i * 2 + 16..i * 2 + 32].copy_from_slice(&t1);
            i += 16;
        }
        encode_scalar(&src[i..], &mut dst[i * 2..]);
    }
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = decode_hex_with_limit(input, MAX_HEX_BYTES)?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
