use std::sync::LazyLock;

use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Crc32ForgeAction {
    #[command(about = "Compute 4 bytes to append to data so the result has a target CRC32")]
    Forge {
        #[arg(help = "Input data (hex-encoded)")]
        input: String,
        #[arg(long, help = "Target CRC32 checksum (hex, e.g. 'deadbeef')")]
        target: String,
    },
    #[command(about = "Verify that appending the forge bytes produces the target CRC32")]
    Verify {
        #[arg(help = "Original data (hex-encoded)")]
        input: String,
        #[arg(long, help = "Forge bytes to append (hex-encoded, 4 bytes)")]
        suffix: String,
        #[arg(long, help = "Expected CRC32 checksum (hex)")]
        target: String,
    },
}

pub const MAX_CRC32_FORGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CRC32_TARGET_HEX_LEN: usize = 8;

fn bounded_crc32_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_CRC32_FORGE_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

fn crc32_hex_error(hex_str: &str) -> anyhow::Error {
    match hex::decode(hex_str) {
        Err(err) => anyhow::Error::new(err).context("Invalid hex input"),
        Ok(_) => anyhow::anyhow!("Invalid hex input"),
    }
}

fn crc32_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn crc32_update(crc: u32, byte: u8, table: &[u32; 256]) -> u32 {
    let index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
    (crc >> 8) ^ table[index]
}

fn crc32_internal_from_hex(hex_str: &str, max_bytes: usize) -> Result<u32> {
    let hex_str = bounded_crc32_hex(hex_str, max_bytes)?;
    let bytes = hex_str.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(crc32_hex_error(hex_str));
    }
    let table = &*CRC32_TABLE;
    let mut crc = 0xFFFF_FFFFu32;
    for &[hi_ch, lo_ch] in bytes.as_chunks::<2>().0 {
        let (Some(hi), Some(lo)) = (crc32_nibble(hi_ch), crc32_nibble(lo_ch)) else {
            return Err(crc32_hex_error(hex_str));
        };
        crc = crc32_update(crc, (hi << 4) | lo, table);
    }
    Ok(crc)
}

pub fn decode_crc32_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_crc32_hex(hex_str, max_bytes)?;
    hex::decode(hex_str).context("Invalid hex input")
}

pub fn parse_crc32_target(target: &str) -> Result<u32> {
    if target.len() > MAX_CRC32_TARGET_HEX_LEN {
        anyhow::bail!("Target exceeds maximum length of {MAX_CRC32_TARGET_HEX_LEN}");
    }
    u32::from_str_radix(target, 16).context("Invalid hex target CRC32")
}

pub fn run(action: Crc32ForgeAction) -> Result<()> {
    match action {
        Crc32ForgeAction::Forge { input, target } => {
            let crc_after_data = crc32_internal_from_hex(&input, MAX_CRC32_FORGE_BYTES)?;
            let target_crc = parse_crc32_target(&target)?;
            let forge_bytes = forge_from_internal_state(crc_after_data, target_crc);
            println!("{}", hex::encode(forge_bytes));
        }
        Crc32ForgeAction::Verify {
            input,
            suffix,
            target,
        } => {
            let mut crc = crc32_internal_from_hex(&input, MAX_CRC32_FORGE_BYTES)?;
            let suffix_bytes = decode_crc32_hex_with_limit(&suffix, MAX_CRC32_FORGE_BYTES)?;
            let target_crc = parse_crc32_target(&target)?;
            let table = &*CRC32_TABLE;
            for &byte in &suffix_bytes {
                crc = crc32_update(crc, byte, table);
            }
            let actual = crc ^ 0xFFFF_FFFF;
            if actual == target_crc {
                println!("Match");
            } else {
                println!("Mismatch (expected {target_crc:08x}, got {actual:08x})");
            }
        }
    }
    Ok(())
}

// CRC32 polynomial (reversed, IEEE 802.3).
const CRC32_POLY: u32 = 0xEDB8_8320;

static CRC32_TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut crc = i;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32_POLY;
            } else {
                crc >>= 1;
            }
        }
        table[i as usize] = crc;
    }
    table
});

static INV_TABLE: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let table = &*CRC32_TABLE;
    let mut inv = [0u8; 256];
    for (i, &entry) in table.iter().enumerate() {
        inv[(entry >> 24) as usize] = i as u8;
    }
    inv
});

#[must_use]
pub fn crc32_compute(data: &[u8]) -> u32 {
    let table = &*CRC32_TABLE;
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc = crc32_update(crc, byte, table);
    }
    crc ^ 0xFFFF_FFFF
}

pub fn forge_crc32_hex(hex_str: &str, target_crc: u32) -> Result<[u8; 4]> {
    let crc_after_data = crc32_internal_from_hex(hex_str, MAX_CRC32_FORGE_BYTES)?;
    Ok(forge_from_internal_state(crc_after_data, target_crc))
}

// Uses reverse CRC32 computation. The CRC32 update is:
//   state_{i+1} = (state_i >> 8) ^ table[(state_i ^ byte_i) & 0xFF]
//
// By reversing 4 steps from the target internal state and matching to the current
// internal state, we can algebraically solve for the 4 unknown bytes.
// The key observation is that the unknown low bytes at each reverse level
// only affect lower bit positions, so they decouple byte-by-byte when
// we set the reversed result equal to the known current state.
fn forge_from_internal_state(crc_after_data: u32, target_crc: u32) -> [u8; 4] {
    let table = &*CRC32_TABLE;
    let target_state = target_crc ^ 0xFFFF_FFFF;
    let inv = &*INV_TABLE;

    // Reverse 4 CRC steps from target_state.
    // At each level, idx = inv[state >> 24], base = (state ^ table[idx]) << 8.
    // The actual previous state is base | L_unknown, where L is the unknown low byte.
    // Since L only occupies bits 0-7, it doesn't affect >> 24, so idx at the next
    // reverse level is independent of the unknowns.
    let i3 = inv[(target_state >> 24) as usize];
    let base3 = (target_state ^ table[i3 as usize]) << 8;

    let i2 = inv[(base3 >> 24) as usize];
    let base2 = (base3 ^ table[i2 as usize]) << 8;

    let i1 = inv[(base2 >> 24) as usize];
    let base1 = (base2 ^ table[i1 as usize]) << 8;

    let i0 = inv[(base1 >> 24) as usize];
    let base0 = (base1 ^ table[i0 as usize]) << 8;

    // After 4 reverses, the result (with all L's = 0) is base0.
    // The actual result with unknowns L0..L3 is:
    //   current_state = base0 ^ (L3 << 24) ^ (L2 << 16) ^ (L1 << 8) ^ L0
    // So the unknowns are simply the bytes of (current_state ^ base0).
    let delta = crc_after_data ^ base0;
    let l3 = ((delta >> 24) & 0xFF) as u8;
    let l2 = ((delta >> 16) & 0xFF) as u8;
    let l1 = ((delta >> 8) & 0xFF) as u8;
    let l0 = (delta & 0xFF) as u8;

    // Recover the actual bytes: byte_i = L_i ^ idx_i
    [l0 ^ i0, l1 ^ i1, l2 ^ i2, l3 ^ i3]
}

#[must_use]
pub fn forge_crc32(data: &[u8], target_crc: u32) -> [u8; 4] {
    let table = &*CRC32_TABLE;
    let mut crc_after_data = 0xFFFF_FFFFu32;
    for &byte in data {
        crc_after_data = crc32_update(crc_after_data, byte, table);
    }
    forge_from_internal_state(crc_after_data, target_crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_compute_hello() {
        let crc = crc32_compute(b"hello");
        assert_eq!(crc, 0x3610a686);
    }

    #[test]
    fn test_forge_and_verify() {
        let data = b"Hello, World!";
        let target_crc = 0xDEADBEEF_u32;
        let forge_bytes = forge_crc32(data, target_crc);
        let mut combined = data.to_vec();
        combined.extend_from_slice(&forge_bytes);
        assert_eq!(crc32_compute(&combined), target_crc);
    }
}
