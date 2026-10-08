use anyhow::{Context, Result};
use clap::Subcommand;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

const DECODE_LUT: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 32 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

#[derive(Subcommand)]
pub enum Base32Action {
    #[command(about = "Encode to Base32")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base32")]
    Decode {
        #[arg(help = "Base32 encoded string")]
        input: String,
    },
}

pub fn run(action: Base32Action) -> Result<()> {
    match action {
        Base32Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base32Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let padded_len = bytes.len().div_ceil(5) * 8;
    let mut out = vec![b'='; padded_len];
    let mut pos = 0;
    let mut buffer: u32 = 0;
    let mut buffer_length: u32 = 0;

    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        buffer_length += 8;
        while buffer_length >= 5 {
            buffer_length -= 5;
            let index = ((buffer >> buffer_length) & 0x1f) as usize;
            out[pos] = ALPHABET[index];
            pos += 1;
        }
    }
    if buffer_length > 0 {
        let index = ((buffer << (5 - buffer_length)) & 0x1f) as usize;
        out[pos] = ALPHABET[index];
    }
    String::from_utf8(out).expect("Base32 alphabet is ASCII")
}

fn decode_bytes(input: &str) -> Result<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut buffer_length: u32 = 0;

    for &b in bytes {
        if b == b'=' {
            continue;
        }
        let value = DECODE_LUT[b as usize];
        if value == 255 {
            anyhow::bail!("Invalid base32 character: {}", b as char);
        }
        buffer = (buffer << 5) | u32::from(value);
        buffer_length += 5;
        if buffer_length >= 8 {
            buffer_length -= 8;
            result.push(((buffer >> buffer_length) & 0xff) as u8);
        }
    }
    Ok(result)
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = decode_bytes(input.trim()).context("Failed to decode Base32")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
