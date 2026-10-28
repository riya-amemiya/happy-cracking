use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PaddingAction {
    #[command(about = "Apply PKCS7 padding")]
    Pkcs7Pad {
        #[arg(help = "Input hex string")]
        input: String,
        #[arg(long, default_value = "16", help = "Block size (1-255)")]
        block_size: usize,
    },
    #[command(about = "Remove PKCS7 padding")]
    Pkcs7Unpad {
        #[arg(help = "Input hex string")]
        input: String,
        #[arg(long, default_value = "16", help = "Block size (1-255)")]
        block_size: usize,
    },
    #[command(about = "Apply zero padding")]
    ZeroPad {
        #[arg(help = "Input hex string")]
        input: String,
        #[arg(long, default_value = "16", help = "Block size")]
        block_size: usize,
    },
    #[command(about = "Remove zero padding")]
    ZeroUnpad {
        #[arg(help = "Input hex string")]
        input: String,
    },
}

pub const MAX_PADDING_BYTES: usize = 16 * 1024 * 1024;

const HEX_OUT: &[u8; 16] = b"0123456789abcdef";

fn padding_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn padding_hex_error(hex_str: &str) -> anyhow::Error {
    match hex::decode(hex_str) {
        Err(err) => anyhow::Error::new(err).context("Failed to decode input hex"),
        Ok(_) => anyhow::anyhow!("Failed to decode input hex"),
    }
}

fn bounded_padding_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_PADDING_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

fn require_padding_hex(hex_str: &str) -> Result<&[u8]> {
    let bytes = hex_str.as_bytes();
    if bytes.len().is_multiple_of(2) && bytes.iter().all(|&b| padding_hex_nibble(b).is_some()) {
        Ok(bytes)
    } else {
        Err(padding_hex_error(hex_str))
    }
}

fn hex_pair_byte(bytes: &[u8], pair_index: usize) -> u8 {
    let i = pair_index * 2;
    let hi = padding_hex_nibble(bytes[i]).unwrap_or(0);
    let lo = padding_hex_nibble(bytes[i + 1]).unwrap_or(0);
    (hi << 4) | lo
}

fn lowercase_hex(src: &[u8], dst: &mut [u8]) {
    for (out, &b) in dst.iter_mut().zip(src) {
        *out = b.to_ascii_lowercase();
    }
}

fn hex_ascii_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("padding hex output is ASCII")
}

pub fn decode_padding_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_padding_hex(hex_str, max_bytes)?;
    hex::decode(hex_str).context("Failed to decode input hex")
}

pub fn pkcs7_pad_hex(input: &str, block_size: usize) -> Result<String> {
    let hex_str = bounded_padding_hex(input, MAX_PADDING_BYTES)?;
    let bytes = require_padding_hex(hex_str)?;
    if block_size == 0 || block_size > 255 {
        anyhow::bail!("Block size must be between 1 and 255");
    }
    let n = bytes.len() / 2;
    let pad_len = block_size - (n % block_size);
    let mut out = vec![0u8; bytes.len() + pad_len * 2];
    lowercase_hex(bytes, &mut out[..bytes.len()]);
    let pad_byte = pad_len as u8;
    let hi = HEX_OUT[(pad_byte >> 4) as usize];
    let lo = HEX_OUT[(pad_byte & 0x0f) as usize];
    let mut pos = bytes.len();
    for _ in 0..pad_len {
        out[pos] = hi;
        out[pos + 1] = lo;
        pos += 2;
    }
    Ok(hex_ascii_string(out))
}

pub fn pkcs7_unpad_hex(input: &str, block_size: usize) -> Result<String> {
    let hex_str = bounded_padding_hex(input, MAX_PADDING_BYTES)?;
    let bytes = require_padding_hex(hex_str)?;
    if block_size == 0 || block_size > 255 {
        anyhow::bail!("Block size must be between 1 and 255");
    }
    let n = bytes.len() / 2;
    if n == 0 {
        anyhow::bail!("Input data is empty");
    }
    if !n.is_multiple_of(block_size) {
        anyhow::bail!("Input length is not a multiple of block size");
    }

    let pad_byte = hex_pair_byte(bytes, n - 1);
    let pad_len = pad_byte as usize;
    if pad_len == 0 || pad_len > block_size {
        anyhow::bail!("Invalid PKCS7 padding value: {pad_byte}");
    }
    if n < pad_len {
        anyhow::bail!("Padding length exceeds data length");
    }
    let pad_start = n - pad_len;
    for i in pad_start..n {
        if hex_pair_byte(bytes, i) != pad_byte {
            anyhow::bail!("Invalid PKCS7 padding bytes");
        }
    }

    let keep = pad_start * 2;
    let mut out = vec![0u8; keep];
    lowercase_hex(&bytes[..keep], &mut out);
    Ok(hex_ascii_string(out))
}

pub fn zero_pad_hex(input: &str, block_size: usize) -> Result<String> {
    let hex_str = bounded_padding_hex(input, MAX_PADDING_BYTES)?;
    let bytes = require_padding_hex(hex_str)?;
    if block_size == 0 || bytes.is_empty() {
        let mut out = vec![0u8; bytes.len()];
        lowercase_hex(bytes, &mut out);
        return Ok(hex_ascii_string(out));
    }
    if block_size > MAX_ZERO_PAD_BLOCK_SIZE {
        anyhow::bail!(
            "Block size {block_size} exceeds maximum of {MAX_ZERO_PAD_BLOCK_SIZE} to prevent excessive memory allocation"
        );
    }
    let n = bytes.len() / 2;
    let remainder = n % block_size;
    if remainder == 0 {
        let mut out = vec![0u8; bytes.len()];
        lowercase_hex(bytes, &mut out);
        return Ok(hex_ascii_string(out));
    }
    let pad_len = block_size - remainder;
    let mut out = vec![0u8; bytes.len() + pad_len * 2];
    lowercase_hex(bytes, &mut out[..bytes.len()]);
    for slot in &mut out[bytes.len()..] {
        *slot = b'0';
    }
    Ok(hex_ascii_string(out))
}

pub fn zero_unpad_hex(input: &str) -> Result<String> {
    let hex_str = bounded_padding_hex(input, MAX_PADDING_BYTES)?;
    let bytes = require_padding_hex(hex_str)?;
    let mut pairs = bytes.len() / 2;
    while pairs > 0 && hex_pair_byte(bytes, pairs - 1) == 0 {
        pairs -= 1;
    }
    let keep = pairs * 2;
    let mut out = vec![0u8; keep];
    lowercase_hex(&bytes[..keep], &mut out);
    Ok(hex_ascii_string(out))
}

pub fn run(action: PaddingAction) -> Result<()> {
    match action {
        PaddingAction::Pkcs7Pad { input, block_size } => {
            println!("{}", pkcs7_pad_hex(&input, block_size)?);
        }
        PaddingAction::Pkcs7Unpad { input, block_size } => {
            println!("{}", pkcs7_unpad_hex(&input, block_size)?);
        }
        PaddingAction::ZeroPad { input, block_size } => {
            println!("{}", zero_pad_hex(&input, block_size)?);
        }
        PaddingAction::ZeroUnpad { input } => {
            println!("{}", zero_unpad_hex(&input)?);
        }
    }
    Ok(())
}

pub fn pkcs7_pad(data: &[u8], block_size: usize) -> Result<Vec<u8>> {
    if block_size == 0 || block_size > 255 {
        anyhow::bail!("Block size must be between 1 and 255");
    }
    let pad_len = block_size - (data.len() % block_size);
    let mut result = data.to_vec();
    result.extend(std::iter::repeat_n(pad_len as u8, pad_len));
    Ok(result)
}

pub fn pkcs7_unpad(data: &[u8], block_size: usize) -> Result<Vec<u8>> {
    if block_size == 0 || block_size > 255 {
        anyhow::bail!("Block size must be between 1 and 255");
    }
    if data.is_empty() {
        anyhow::bail!("Input data is empty");
    }
    if !data.len().is_multiple_of(block_size) {
        anyhow::bail!("Input length is not a multiple of block size");
    }

    let pad_byte = *data
        .last()
        .ok_or_else(|| anyhow::anyhow!("Input data is empty"))?;
    let pad_len = pad_byte as usize;

    if pad_len == 0 || pad_len > block_size {
        anyhow::bail!("Invalid PKCS7 padding value: {pad_byte}");
    }
    if data.len() < pad_len {
        anyhow::bail!("Padding length exceeds data length");
    }
    if !data[data.len() - pad_len..].iter().all(|&b| b == pad_byte) {
        anyhow::bail!("Invalid PKCS7 padding bytes");
    }

    Ok(data[..data.len() - pad_len].to_vec())
}

// Safety limit for zero-pad block size to prevent Denial of Service.
// Without a limit, a user could pass e.g. block_size=4294967295 with a tiny
// input, causing allocation of ~4 GB of zero bytes (OOM / memory exhaustion).
// 16 MB is generous for any realistic block-cipher block size.
const MAX_ZERO_PAD_BLOCK_SIZE: usize = 16 * 1024 * 1024;

pub fn zero_pad(data: &[u8], block_size: usize) -> Result<Vec<u8>> {
    if block_size == 0 || data.is_empty() {
        return Ok(data.to_vec());
    }
    if block_size > MAX_ZERO_PAD_BLOCK_SIZE {
        anyhow::bail!(
            "Block size {block_size} exceeds maximum of {MAX_ZERO_PAD_BLOCK_SIZE} to prevent excessive memory allocation"
        );
    }
    let remainder = data.len() % block_size;
    if remainder == 0 {
        return Ok(data.to_vec());
    }
    let pad_len = block_size - remainder;
    let mut result = data.to_vec();
    result.extend(std::iter::repeat_n(0u8, pad_len));
    Ok(result)
}

#[must_use]
pub fn zero_unpad(data: &[u8]) -> Vec<u8> {
    let end = data.iter().rposition(|&b| b != 0).map_or(0, |pos| pos + 1);
    data[..end].to_vec()
}
