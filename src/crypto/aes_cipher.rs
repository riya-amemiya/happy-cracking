use std::collections::HashSet;

use anyhow::Result;
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

pub const MAX_AES_BYTES: usize = 16 * 1024 * 1024;
const AES128_KEY_BYTES: usize = 16;
const AES_BLOCK_BYTES: usize = 16;

fn bounded_aes_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_AES_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

fn invalid_aes_hex<T>(hex_str: &str) -> Result<T> {
    match hex::decode(hex_str) {
        Err(err) => Err(anyhow::Error::new(err).context("Failed to decode hex input")),
        Ok(_) => Err(anyhow::anyhow!("Failed to decode hex input")),
    }
}

pub fn decode_aes_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_aes_hex(hex_str, max_bytes)?;
    super::hex::decode_hex_with_limit(hex_str, max_bytes).or_else(|_| invalid_aes_hex(hex_str))
}

fn require_aes_block_hex(hex_input: &str) -> Result<&str> {
    let hex_str = bounded_aes_hex(hex_input, MAX_AES_BYTES)?;
    if hex_str.is_empty() {
        anyhow::bail!("Input must not be empty");
    }
    let bytes = hex_str.as_bytes();
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return invalid_aes_hex(hex_str);
    }
    if !pairs.len().is_multiple_of(AES_BLOCK_BYTES) {
        anyhow::bail!(
            "Input length must be a multiple of 16 bytes, got {} bytes",
            pairs.len()
        );
    }
    Ok(hex_str)
}

fn array_bytes(block: &Array<u8, <Aes128 as KeySizeUser>::KeySize>) -> [u8; AES_BLOCK_BYTES] {
    let mut out = [0u8; AES_BLOCK_BYTES];
    out.copy_from_slice(block);
    out
}

fn crypt_aes_blocks(
    hex_input: &str,
    mut transform: impl FnMut([u8; AES_BLOCK_BYTES]) -> [u8; AES_BLOCK_BYTES],
) -> Result<String> {
    let hex_str = require_aes_block_hex(hex_input)?;
    let mut bytes = super::hex::decode_hex_with_limit(hex_str, MAX_AES_BYTES)
        .or_else(|_| invalid_aes_hex(hex_str))?;
    for chunk in bytes.as_chunks_mut::<AES_BLOCK_BYTES>().0 {
        *chunk = transform(*chunk);
    }
    Ok(super::hex::encode_bytes(&bytes))
}

fn parse_key(hex_key: &str) -> Result<Array<u8, <Aes128 as KeySizeUser>::KeySize>> {
    let key_bytes = decode_aes_hex_with_limit(hex_key, AES128_KEY_BYTES)?;
    Array::try_from(key_bytes.as_slice()).map_err(|_| {
        anyhow::anyhow!(
            "AES-128 key must be exactly 16 bytes (32 hex chars), got {} bytes",
            key_bytes.len()
        )
    })
}

fn parse_iv(hex_iv: &str) -> Result<[u8; AES_BLOCK_BYTES]> {
    let iv_bytes = decode_aes_hex_with_limit(hex_iv, AES128_KEY_BYTES)?;
    if iv_bytes.len() != AES_BLOCK_BYTES {
        anyhow::bail!(
            "IV must be exactly 16 bytes (32 hex chars), got {} bytes",
            iv_bytes.len()
        );
    }
    let mut iv = [0u8; AES_BLOCK_BYTES];
    iv.copy_from_slice(&iv_bytes);
    Ok(iv)
}

pub fn ecb_encrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let cipher = Aes128::new(&key);
    crypt_aes_blocks(hex_input, |block| {
        let mut block_array = Array::from(block);
        cipher.encrypt_block(&mut block_array);
        array_bytes(&block_array)
    })
}

pub fn ecb_decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let cipher = Aes128::new(&key);
    crypt_aes_blocks(hex_input, |block| {
        let mut block_array = Array::from(block);
        cipher.decrypt_block(&mut block_array);
        array_bytes(&block_array)
    })
}

pub fn cbc_encrypt(hex_input: &str, hex_key: &str, hex_iv: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let mut prev = parse_iv(hex_iv)?;
    let cipher = Aes128::new(&key);
    crypt_aes_blocks(hex_input, |block| {
        let mut xored = [0u8; AES_BLOCK_BYTES];
        for (slot, (b, p)) in xored.iter_mut().zip(block.iter().zip(prev.iter())) {
            *slot = b ^ p;
        }
        let mut block_array = Array::from(xored);
        cipher.encrypt_block(&mut block_array);
        prev.copy_from_slice(&block_array);
        array_bytes(&block_array)
    })
}

pub fn cbc_decrypt(hex_input: &str, hex_key: &str, hex_iv: &str) -> Result<String> {
    let key = parse_key(hex_key)?;
    let mut prev = parse_iv(hex_iv)?;
    let cipher = Aes128::new(&key);
    crypt_aes_blocks(hex_input, |block| {
        let mut block_array = Array::from(block);
        cipher.decrypt_block(&mut block_array);
        for (slot, p) in block_array.iter_mut().zip(prev.iter()) {
            *slot ^= *p;
        }
        prev = block;
        array_bytes(&block_array)
    })
}

pub fn ecb_detect(hex_input: &str) -> Result<(bool, usize)> {
    let bytes = decode_aes_hex_with_limit(hex_input, MAX_AES_BYTES)?;
    if bytes.len() < 16 {
        return Ok((false, 0));
    }

    let blocks: Vec<&[u8]> = bytes.chunks(16).collect();
    let unique: HashSet<&[u8]> = blocks.iter().copied().collect();

    let duplicates = blocks.len() - unique.len();
    Ok((duplicates > 0, duplicates))
}
