use std::collections::HashSet;

use anyhow::{Context, Result};
use clap::Subcommand;

use aes::Aes128;
use aes::cipher::{Array, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit, KeySizeUser};

#[derive(Subcommand)]
pub enum AesCipherAction {
    #[command(about = "AES-128 ECB encrypt (hex key, hex plaintext)")]
    EcbEncrypt {
        #[arg(help = "Plaintext (hex string, must be multiple of 16 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 16 bytes / 32 hex chars)")]
        key: String,
    },
    #[command(about = "AES-128 ECB decrypt (hex key, hex ciphertext)")]
    EcbDecrypt {
        #[arg(help = "Ciphertext (hex string, must be multiple of 16 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 16 bytes / 32 hex chars)")]
        key: String,
    },
    #[command(about = "AES-128 CBC encrypt (hex key, hex IV, hex plaintext)")]
    CbcEncrypt {
        #[arg(help = "Plaintext (hex string, must be multiple of 16 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 16 bytes)")]
        key: String,
        #[arg(long, help = "Initialization vector (hex string, 16 bytes)")]
        iv: String,
    },
    #[command(about = "AES-128 CBC decrypt (hex key, hex IV, hex ciphertext)")]
    CbcDecrypt {
        #[arg(help = "Ciphertext (hex string, must be multiple of 16 bytes)")]
        input: String,
        #[arg(short, long, help = "Key (hex string, 16 bytes)")]
        key: String,
        #[arg(long, help = "Initialization vector (hex string, 16 bytes)")]
        iv: String,
    },
    #[command(about = "Detect ECB mode by finding duplicate 16-byte blocks")]
    EcbDetect {
        #[arg(help = "Ciphertext (hex string)")]
        input: String,
    },
}

pub fn run(action: AesCipherAction) -> Result<()> {
    match action {
        AesCipherAction::EcbEncrypt { input, key } => {
            println!("{}", ecb_encrypt(&input, &key)?);
        }
        AesCipherAction::EcbDecrypt { input, key } => {
            println!("{}", ecb_decrypt(&input, &key)?);
        }
        AesCipherAction::CbcEncrypt { input, key, iv } => {
            println!("{}", cbc_encrypt(&input, &key, &iv)?);
        }
        AesCipherAction::CbcDecrypt { input, key, iv } => {
            println!("{}", cbc_decrypt(&input, &key, &iv)?);
        }
        AesCipherAction::EcbDetect { input } => {
            let (detected, duplicates) = ecb_detect(&input)?;
            if detected {
                println!("ECB mode detected! Found {duplicates} duplicate block(s):");
                println!("The ciphertext contains repeated 16-byte blocks, suggesting ECB mode.");
            } else {
                println!("No duplicate blocks found. Likely not ECB mode.");
            }
        }
    }
    Ok(())
}

fn parse_key(hex_key: &str) -> Result<Array<u8, <Aes128 as KeySizeUser>::KeySize>> {
    let key_bytes = hex::decode(hex_key.trim()).context("Failed to decode hex key")?;
    Array::try_from(key_bytes.as_slice()).map_err(|_| {
        anyhow::anyhow!(
            "AES-128 key must be exactly 16 bytes (32 hex chars), got {} bytes",
            key_bytes.len()
        )
    })
}

fn parse_blocks(hex_input: &str) -> Result<Vec<u8>> {
    let bytes = hex::decode(hex_input.trim()).context("Failed to decode hex input")?;
    if bytes.is_empty() {
        anyhow::bail!("Input must not be empty");
    }
    if bytes.len() % 16 != 0 {
        anyhow::bail!(
            "Input length must be a multiple of 16 bytes, got {} bytes",
            bytes.len()
        );
    }
    Ok(bytes)
}

fn parse_iv(hex_iv: &str) -> Result<[u8; 16]> {
    let iv_bytes = hex::decode(hex_iv.trim()).context("Failed to decode hex IV")?;
    if iv_bytes.len() != 16 {
        anyhow::bail!(
            "IV must be exactly 16 bytes (32 hex chars), got {} bytes",
            iv_bytes.len()
        );
    }
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&iv_bytes);
    Ok(iv)
}

pub fn ecb_encrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let plaintext = parse_blocks(hex_input)?;
    let cipher = Aes128::new(&key);

    let mut output = Vec::with_capacity(plaintext.len());
    for block in plaintext.chunks(16) {
        let mut block_array = Array::try_from(block).context("AES block must be 16 bytes")?;
        cipher.encrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn ecb_decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let ciphertext = parse_blocks(hex_input)?;
    let cipher = Aes128::new(&key);

    let mut output = Vec::with_capacity(ciphertext.len());
    for block in ciphertext.chunks(16) {
        let mut block_array = Array::try_from(block).context("AES block must be 16 bytes")?;
        cipher.decrypt_block(&mut block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn cbc_encrypt(hex_input: &str, hex_key: &str, hex_iv: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let plaintext = parse_blocks(hex_input)?;
    let mut prev = parse_iv(hex_iv)?;
    let cipher = Aes128::new(&key);

    let mut output = Vec::with_capacity(plaintext.len());
    for block in plaintext.chunks(16) {
        let mut xored = [0u8; 16];
        for i in 0..16 {
            xored[i] = block[i] ^ prev[i];
        }
        let mut block_array = Array::from(xored);
        cipher.encrypt_block(&mut block_array);
        prev.copy_from_slice(&block_array);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn cbc_decrypt(hex_input: &str, hex_key: &str, hex_iv: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let ciphertext = parse_blocks(hex_input)?;
    let mut prev = parse_iv(hex_iv)?;
    let cipher = Aes128::new(&key);

    let mut output = Vec::with_capacity(ciphertext.len());
    for block in ciphertext.chunks(16) {
        let mut block_array = Array::try_from(block).context("AES block must be 16 bytes")?;
        cipher.decrypt_block(&mut block_array);
        for i in 0..16 {
            block_array[i] ^= prev[i];
        }
        prev.copy_from_slice(block);
        output.extend_from_slice(&block_array);
    }
    Ok(hex::encode(output))
}

pub fn ecb_detect(hex_input: &str) -> Result<(bool, usize)> {
    let bytes = hex::decode(hex_input.trim()).context("Failed to decode hex input")?;
    if bytes.len() < 16 {
        return Ok((false, 0));
    }

    let blocks: Vec<&[u8]> = bytes.chunks(16).collect();
    let unique: HashSet<&[u8]> = blocks.iter().copied().collect();

    let duplicates = blocks.len() - unique.len();
    Ok((duplicates > 0, duplicates))
}
