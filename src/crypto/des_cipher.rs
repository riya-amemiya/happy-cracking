use anyhow::{Context, Result};
use clap::Subcommand;

use des::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use des::{Des, TdesEde3};

#[derive(Subcommand)]
pub enum DesCipherAction {
    #[command(about = "DES ECB encrypt (hex key 8 bytes, hex plaintext)")]
    Encrypt {
        #[arg(help = "Plaintext (hex string, must be multiple of 8 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 8 bytes / 16 hex chars)")]
        key: String,
    },
    #[command(about = "DES ECB decrypt (hex key 8 bytes, hex ciphertext)")]
    Decrypt {
        #[arg(help = "Ciphertext (hex string, must be multiple of 8 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 8 bytes / 16 hex chars)")]
        key: String,
    },
    #[command(about = "Triple-DES (EDE3) encrypt (hex key 24 bytes, hex plaintext)")]
    TdesEncrypt {
        #[arg(help = "Plaintext (hex string, must be multiple of 8 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 24 bytes / 48 hex chars)")]
        key: String,
    },
    #[command(about = "Triple-DES (EDE3) decrypt (hex key 24 bytes, hex ciphertext)")]
    TdesDecrypt {
        #[arg(help = "Ciphertext (hex string, must be multiple of 8 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 24 bytes / 48 hex chars)")]
        key: String,
    },
}

pub fn run(action: DesCipherAction) -> Result<()> {
    match action {
        DesCipherAction::Encrypt { input, key } => {
            println!("{}", des_encrypt(&input, &key)?);
        }
        DesCipherAction::Decrypt { input, key } => {
            println!("{}", des_decrypt(&input, &key)?);
        }
        DesCipherAction::TdesEncrypt { input, key } => {
            println!("{}", tdes_encrypt(&input, &key)?);
        }
        DesCipherAction::TdesDecrypt { input, key } => {
            println!("{}", tdes_decrypt(&input, &key)?);
        }
    }
    Ok(())
}

pub const MAX_DES_BYTES: usize = 16 * 1024 * 1024;
const DES_KEY_BYTES: usize = 8;
const TDES_KEY_BYTES: usize = 24;

pub fn decode_des_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_DES_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    hex::decode(hex_str).context("Failed to decode hex input")
}

fn parse_des_blocks(hex_input: &str) -> Result<Vec<u8>> {
    let bytes = decode_des_hex_with_limit(hex_input, MAX_DES_BYTES)?;
    if bytes.is_empty() {
        anyhow::bail!("Input must not be empty");
    }
    if bytes.len() % 8 != 0 {
        anyhow::bail!(
            "Input length must be a multiple of 8 bytes, got {} bytes",
            bytes.len()
        );
    }
    Ok(bytes)
}

pub fn des_encrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key_bytes = decode_des_hex_with_limit(hex_key, DES_KEY_BYTES)?;
    if key_bytes.len() != 8 {
        anyhow::bail!(
            "DES key must be exactly 8 bytes (16 hex chars), got {} bytes",
            key_bytes.len()
        );
    }
    let plaintext = parse_des_blocks(hex_input)?;
    let cipher =
        Des::new(&Array::try_from(key_bytes.as_slice()).context("DES key must be 8 bytes")?);

    let mut output = Vec::with_capacity(plaintext.len());
    for block in plaintext.chunks(8) {
        let mut block_array = Array::try_from(block).context("DES block must be 8 bytes")?;
        cipher.encrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn des_decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key_bytes = decode_des_hex_with_limit(hex_key, DES_KEY_BYTES)?;
    if key_bytes.len() != 8 {
        anyhow::bail!(
            "DES key must be exactly 8 bytes (16 hex chars), got {} bytes",
            key_bytes.len()
        );
    }
    let ciphertext = parse_des_blocks(hex_input)?;
    let cipher =
        Des::new(&Array::try_from(key_bytes.as_slice()).context("DES key must be 8 bytes")?);

    let mut output = Vec::with_capacity(ciphertext.len());
    for block in ciphertext.chunks(8) {
        let mut block_array = Array::try_from(block).context("DES block must be 8 bytes")?;
        cipher.decrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn tdes_encrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key_bytes = decode_des_hex_with_limit(hex_key, TDES_KEY_BYTES)?;
    if key_bytes.len() != 24 {
        anyhow::bail!(
            "Triple-DES key must be exactly 24 bytes (48 hex chars), got {} bytes",
            key_bytes.len()
        );
    }
    let plaintext = parse_des_blocks(hex_input)?;
    let cipher = TdesEde3::new(
        &Array::try_from(key_bytes.as_slice()).context("Triple-DES key must be 24 bytes")?,
    );

    let mut output = Vec::with_capacity(plaintext.len());
    for block in plaintext.chunks(8) {
        let mut block_array = Array::try_from(block).context("DES block must be 8 bytes")?;
        cipher.encrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn tdes_decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key_bytes = decode_des_hex_with_limit(hex_key, TDES_KEY_BYTES)?;
    if key_bytes.len() != 24 {
        anyhow::bail!(
            "Triple-DES key must be exactly 24 bytes (48 hex chars), got {} bytes",
            key_bytes.len()
        );
    }
    let ciphertext = parse_des_blocks(hex_input)?;
    let cipher = TdesEde3::new(
        &Array::try_from(key_bytes.as_slice()).context("Triple-DES key must be 24 bytes")?,
    );

    let mut output = Vec::with_capacity(ciphertext.len());
    for block in ciphertext.chunks(8) {
        let mut block_array = Array::try_from(block).context("DES block must be 8 bytes")?;
        cipher.decrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}
