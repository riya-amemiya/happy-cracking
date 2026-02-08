use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Base85Action {
    #[command(about = "Encode to Base85 (Ascii85)")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base85 (Ascii85)")]
    Decode {
        #[arg(help = "Base85 encoded string")]
        input: String,
    },
}

pub fn run(action: Base85Action) -> Result<()> {
    match action {
        Base85Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base85Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let chunks = bytes.chunks(4);

    for chunk in chunks {
        if chunk.len() == 4 {
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if value == 0 {
                result.push('z');
            } else {
                let mut chars = [0u8; 5];
                let mut v = value;
                for i in (0..5).rev() {
                    chars[i] = (v % 85) as u8 + b'!';
                    v /= 85;
                }
                for c in &chars {
                    result.push(*c as char);
                }
            }
        } else {
            // Pad with zeros to 4 bytes
            let mut padded = [0u8; 4];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = u32::from_be_bytes(padded);
            let mut chars = [0u8; 5];
            let mut v = value;
            for i in (0..5).rev() {
                chars[i] = (v % 85) as u8 + b'!';
                v /= 85;
            }
            // Output ceil(chunk.len() + 1) characters for partial block
            let output_len = chunk.len() + 1;
            for c in &chars[..output_len] {
                result.push(*c as char);
            }
        }
    }

    result
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut bytes = Vec::new();
    let chars: Vec<u8> = input.bytes().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == b'z' {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
            i += 1;
        } else {
            let remaining = chars.len() - i;
            let block_len = remaining.min(5);

            if block_len < 2 {
                anyhow::bail!("Invalid Base85 input: incomplete block");
            }

            // Pad short blocks with 'u' (ASCII 117, the highest value char)
            let mut block = [b'u'; 5];
            for j in 0..block_len {
                let c = chars[i + j];
                if !(b'!'..=b'u').contains(&c) {
                    anyhow::bail!("Invalid Base85 character: {}", c as char);
                }
                block[j] = c;
            }

            let mut value: u64 = 0;
            for &b in &block {
                value = value * 85 + (b - b'!') as u64;
            }

            if value > u32::MAX as u64 {
                anyhow::bail!("Invalid Base85 input: block value overflow");
            }

            let decoded = (value as u32).to_be_bytes();

            if block_len == 5 {
                bytes.extend_from_slice(&decoded);
            } else {
                // Partial block: output (block_len - 1) bytes
                let output_len = block_len - 1;
                bytes.extend_from_slice(&decoded[..output_len]);
            }

            i += block_len;
        }
    }

    String::from_utf8(bytes).context("Decoded data is not valid UTF-8")
}
