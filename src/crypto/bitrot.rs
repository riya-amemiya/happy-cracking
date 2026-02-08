use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BitrotAction {
    #[command(about = "Rotate bits left")]
    Rotl {
        #[arg(help = "Input hex string")]
        input: String,
        #[arg(short, long, default_value = "1", help = "Number of bits to rotate")]
        bits: u32,
        #[arg(short, long, default_value = "8", help = "Bit width (8, 16, 32)")]
        width: u32,
    },
    #[command(about = "Rotate bits right")]
    Rotr {
        #[arg(help = "Input hex string")]
        input: String,
        #[arg(short, long, default_value = "1", help = "Number of bits to rotate")]
        bits: u32,
        #[arg(short, long, default_value = "8", help = "Bit width (8, 16, 32)")]
        width: u32,
    },
}

pub fn run(action: BitrotAction) -> Result<()> {
    match action {
        BitrotAction::Rotl { input, bits, width } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let result = rotate_left(&data, bits, width)?;
            println!("{}", hex::encode(&result));
        }
        BitrotAction::Rotr { input, bits, width } => {
            let data = hex::decode(input.trim()).context("Failed to decode input hex")?;
            let result = rotate_right(&data, bits, width)?;
            println!("{}", hex::encode(&result));
        }
    }
    Ok(())
}

pub fn rotate_left(data: &[u8], bits: u32, width: u32) -> Result<Vec<u8>> {
    match width {
        8 => Ok(rotate_left_8(data, bits)),
        16 => rotate_left_16(data, bits),
        32 => rotate_left_32(data, bits),
        _ => anyhow::bail!("Unsupported bit width: {} (must be 8, 16, or 32)", width),
    }
}

pub fn rotate_right(data: &[u8], bits: u32, width: u32) -> Result<Vec<u8>> {
    match width {
        8 => Ok(rotate_right_8(data, bits)),
        16 => rotate_right_16(data, bits),
        32 => rotate_right_32(data, bits),
        _ => anyhow::bail!("Unsupported bit width: {} (must be 8, 16, or 32)", width),
    }
}

fn rotate_left_8(data: &[u8], bits: u32) -> Vec<u8> {
    data.iter().map(|&b| b.rotate_left(bits)).collect()
}

fn rotate_right_8(data: &[u8], bits: u32) -> Vec<u8> {
    data.iter().map(|&b| b.rotate_right(bits)).collect()
}

fn rotate_left_16(data: &[u8], bits: u32) -> Result<Vec<u8>> {
    if !data.len().is_multiple_of(2) {
        anyhow::bail!("Data length must be a multiple of 2 for 16-bit rotation");
    }
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(2) {
        let val = u16::from_be_bytes([chunk[0], chunk[1]]);
        let rotated = val.rotate_left(bits);
        result.extend_from_slice(&rotated.to_be_bytes());
    }
    Ok(result)
}

fn rotate_right_16(data: &[u8], bits: u32) -> Result<Vec<u8>> {
    if !data.len().is_multiple_of(2) {
        anyhow::bail!("Data length must be a multiple of 2 for 16-bit rotation");
    }
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(2) {
        let val = u16::from_be_bytes([chunk[0], chunk[1]]);
        let rotated = val.rotate_right(bits);
        result.extend_from_slice(&rotated.to_be_bytes());
    }
    Ok(result)
}

fn rotate_left_32(data: &[u8], bits: u32) -> Result<Vec<u8>> {
    if !data.len().is_multiple_of(4) {
        anyhow::bail!("Data length must be a multiple of 4 for 32-bit rotation");
    }
    let shift = bits % 32;
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(4) {
        let val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let rotated = val.rotate_left(shift);
        result.extend_from_slice(&rotated.to_be_bytes());
    }
    Ok(result)
}

fn rotate_right_32(data: &[u8], bits: u32) -> Result<Vec<u8>> {
    if !data.len().is_multiple_of(4) {
        anyhow::bail!("Data length must be a multiple of 4 for 32-bit rotation");
    }
    let shift = bits % 32;
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(4) {
        let val = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let rotated = val.rotate_right(shift);
        result.extend_from_slice(&rotated.to_be_bytes());
    }
    Ok(result)
}
