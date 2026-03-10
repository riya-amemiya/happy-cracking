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

pub fn run(action: Crc32ForgeAction) -> Result<()> {
    match action {
        Crc32ForgeAction::Forge { input, target } => {
            let data = hex::decode(&input).context("Invalid hex input")?;
            let target_crc =
                u32::from_str_radix(&target, 16).context("Invalid hex target CRC32")?;
            let forge_bytes = forge_crc32(&data, target_crc);
            println!("{}", hex::encode(forge_bytes));
        }
        Crc32ForgeAction::Verify {
            input,
            suffix,
            target,
        } => {
            let data = hex::decode(&input).context("Invalid hex input")?;
            let suffix_bytes = hex::decode(&suffix).context("Invalid hex suffix")?;
            let target_crc =
                u32::from_str_radix(&target, 16).context("Invalid hex target CRC32")?;
            let mut combined = data;
            combined.extend_from_slice(&suffix_bytes);
            let actual = crc32_compute(&combined);
            if actual == target_crc {
                println!("Match");
            } else {
                println!("Mismatch (expected {:08x}, got {:08x})", target_crc, actual);
            }
        }
    }
    Ok(())
}

// CRC32 polynomial (reversed, IEEE 802.3).
const CRC32_POLY: u32 = 0xEDB88320;

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

// Compute CRC32 checksum of a byte slice (IEEE 802.3 / ISO 3309).
pub fn crc32_compute(data: &[u8]) -> u32 {
    let table = &*CRC32_TABLE;
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        let index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[index];
    }
    crc ^ 0xFFFF_FFFF
}

// Forge 4 bytes to append to data so that crc32(data || forge_bytes) == target_crc.
//
// Uses reverse CRC32 computation. The CRC32 update is:
//   state_{i+1} = (state_i >> 8) ^ table[(state_i ^ byte_i) & 0xFF]
//
// By reversing 4 steps from the target internal state and matching to the current
// internal state, we can algebraically solve for the 4 unknown bytes.
// The key observation is that the unknown low bytes at each reverse level
// only affect lower bit positions, so they decouple byte-by-byte when
// we set the reversed result equal to the known current state.
pub fn forge_crc32(data: &[u8], target_crc: u32) -> [u8; 4] {
    let table = &*CRC32_TABLE;

    // Compute CRC32 internal state after processing data (before final XOR).
    let mut crc_after_data = 0xFFFF_FFFFu32;
    for &byte in data {
        let index = ((crc_after_data ^ u32::from(byte)) & 0xFF) as usize;
        crc_after_data = (crc_after_data >> 8) ^ table[index];
    }

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
