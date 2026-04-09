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

pub fn run(action: PaddingAction) -> Result<()> {
    match action {
        PaddingAction::Pkcs7Pad { input, block_size } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let padded = pkcs7_pad(&data, block_size)?;
            println!("{}", hex::encode(&padded));
        }
        PaddingAction::Pkcs7Unpad { input, block_size } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let unpadded = pkcs7_unpad(&data, block_size)?;
            println!("{}", hex::encode(&unpadded));
        }
        PaddingAction::ZeroPad { input, block_size } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let padded = zero_pad(&data, block_size)?;
            println!("{}", hex::encode(&padded));
        }
        PaddingAction::ZeroUnpad { input } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let unpadded = zero_unpad(&data);
            println!("{}", hex::encode(&unpadded));
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

    // Defense-in-depth: use ok_or_else instead of unwrap to avoid a panic
    // if the empty-check guard above is ever refactored away.
    let pad_byte = *data
        .last()
        .ok_or_else(|| anyhow::anyhow!("Input data is empty"))?;
    let pad_len = pad_byte as usize;

    if pad_len == 0 || pad_len > block_size {
        anyhow::bail!("Invalid PKCS7 padding value: {}", pad_byte);
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
            "Block size {} exceeds maximum of {} to prevent excessive memory allocation",
            block_size,
            MAX_ZERO_PAD_BLOCK_SIZE
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

pub fn zero_unpad(data: &[u8]) -> Vec<u8> {
    let end = data.iter().rposition(|&b| b != 0).map_or(0, |pos| pos + 1);
    data[..end].to_vec()
}
