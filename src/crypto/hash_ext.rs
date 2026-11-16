use anyhow::{Context, Result};
use clap::Subcommand;
use sha2::block_api::compress256;

#[derive(Subcommand)]
pub enum HashExtAction {
    #[command(about = "SHA-256 hash length extension attack")]
    Sha256Extend {
        #[arg(help = "Original hash (hex)")]
        original_hash: String,
        #[arg(long, help = "Length of secret + original message in bytes")]
        original_len: u64,
        #[arg(long, help = "Data to append")]
        append: String,
    },
}

pub fn run(action: HashExtAction) -> Result<()> {
    match action {
        HashExtAction::Sha256Extend {
            original_hash,
            original_len,
            append,
        } => {
            let result = sha256_extend(&original_hash, original_len, append.as_bytes())?;
            println!("New hash:       {}", result.new_hash);
            println!("Forged suffix:  {}", hex::encode(&result.forged_suffix));
        }
    }
    Ok(())
}

pub struct ExtensionResult {
    pub new_hash: String,
    pub forged_suffix: Vec<u8>,
}

pub const MAX_APPEND_LEN: usize = 64 * 1024;
pub const MAX_ORIGINAL_HASH_HEX_LEN: usize = 64;

// Given H(secret || message) and the total length of (secret || message),
// computes H(secret || message || padding || append) without knowing the secret.
pub fn sha256_extend(
    original_hash_hex: &str,
    original_len: u64,
    append: &[u8],
) -> Result<ExtensionResult> {
    if append.len() > MAX_APPEND_LEN {
        anyhow::bail!(
            "Append data exceeds maximum length of {MAX_APPEND_LEN} bytes to prevent Denial of Service"
        );
    }

    let original_hash_hex = original_hash_hex.trim();
    if original_hash_hex.len() > MAX_ORIGINAL_HASH_HEX_LEN {
        anyhow::bail!(
            "Original hash exceeds maximum length of {MAX_ORIGINAL_HASH_HEX_LEN} hex characters to prevent Denial of Service"
        );
    }

    let hash_bytes = hex::decode(original_hash_hex).context("Invalid hex in original hash")?;
    if hash_bytes.len() != 32 {
        anyhow::bail!(
            "SHA-256 hash must be 32 bytes (64 hex chars), got {}",
            hash_bytes.len()
        );
    }

    let mut state = [0u32; 8];
    for i in 0..8 {
        state[i] = u32::from_be_bytes([
            hash_bytes[i * 4],
            hash_bytes[i * 4 + 1],
            hash_bytes[i * 4 + 2],
            hash_bytes[i * 4 + 3],
        ]);
    }

    let glue_padding = sha256_padding(original_len)?;

    let total_processed = original_len
        .checked_add(glue_padding.len() as u64)
        .context("Padded original length overflow")?;

    let mut buffer = append.to_vec();
    let total_bytes = total_processed
        .checked_add(append.len() as u64)
        .context("Extended message length overflow")?;
    let final_bit_len = sha256_bit_len(total_bytes)?;
    let append_padding = sha256_finish_padding(append.len() as u64, final_bit_len);
    buffer.extend_from_slice(&append_padding);

    let (blocks, rest) = buffer.as_chunks::<64>();
    compress256(&mut state, blocks);
    if !rest.is_empty() {
        let mut block = [0u8; 64];
        block[..rest.len()].copy_from_slice(rest);
        compress256(&mut state, &[block]);
    }

    let mut new_hash_bytes = Vec::with_capacity(32);
    for &word in &state {
        new_hash_bytes.extend_from_slice(&word.to_be_bytes());
    }

    let mut forged_suffix = glue_padding;
    forged_suffix.extend_from_slice(append);

    Ok(ExtensionResult {
        new_hash: hex::encode(&new_hash_bytes),
        forged_suffix,
    })
}

fn sha256_bit_len(byte_len: u64) -> Result<u64> {
    byte_len
        .checked_mul(8)
        .context("Message length exceeds SHA-256's 64-bit bit-length field")
}

// Padding is: 0x80, then zeros, then 8-byte big-endian bit length,
// such that the total padded length is a multiple of 64 bytes.
fn sha256_padding(message_len: u64) -> Result<Vec<u8>> {
    let bit_len = sha256_bit_len(message_len)?;
    let remainder = (message_len % 64) as usize;
    // We need at least 1 + 8 bytes (0x80 + length), padded to 64
    let padding_len = if remainder < 56 {
        56 - remainder
    } else {
        120 - remainder
    };

    let mut padding = Vec::with_capacity(padding_len + 8);
    padding.push(0x80);
    padding.resize(padding_len, 0x00);
    padding.extend_from_slice(&bit_len.to_be_bytes());
    Ok(padding)
}

fn sha256_finish_padding(append_len: u64, total_bit_len: u64) -> Vec<u8> {
    let remainder = (append_len % 64) as usize;
    let padding_len = if remainder < 56 {
        56 - remainder
    } else {
        120 - remainder
    };

    let mut padding = Vec::with_capacity(padding_len + 8);
    padding.push(0x80);
    padding.resize(padding_len, 0x00);
    padding.extend_from_slice(&total_bit_len.to_be_bytes());
    padding
}
