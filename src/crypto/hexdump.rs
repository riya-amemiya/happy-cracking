use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HexdumpAction {
    #[command(about = "Display hex dump of input")]
    Dump {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Reverse hex dump back to text")]
    Reverse {
        #[arg(help = "Hex dump string")]
        input: String,
    },
}

pub fn run(action: HexdumpAction) -> Result<()> {
    match action {
        HexdumpAction::Dump { input } => {
            print!("{}", dump(&input));
        }
        HexdumpAction::Reverse { input } => {
            let bytes = reverse(&input)?;
            let text = String::from_utf8(bytes).context("Reversed data is not valid UTF-8")?;
            print!("{}", text);
        }
    }
    Ok(())
}

pub fn dump(input: &str) -> String {
    dump_bytes(input.as_bytes())
}

pub fn dump_bytes(data: &[u8]) -> String {
    let mut output = String::new();
    for (line_idx, chunk) in data.chunks(16).enumerate() {
        let offset = line_idx * 16;
        output.push_str(&format!("{:08x}: ", offset));

        // Hex pairs
        for pair_idx in 0..8 {
            let byte_offset = pair_idx * 2;
            if byte_offset < chunk.len() {
                output.push_str(&format!("{:02x}", chunk[byte_offset]));
                if byte_offset + 1 < chunk.len() {
                    output.push_str(&format!("{:02x}", chunk[byte_offset + 1]));
                } else {
                    output.push_str("  ");
                }
            } else {
                output.push_str("    ");
            }
            if pair_idx < 7 {
                output.push(' ');
            }
        }

        // Pad if less than 16 bytes
        output.push_str("  ");

        // ASCII column
        for &byte in chunk {
            if byte.is_ascii_graphic() || byte == b' ' {
                output.push(byte as char);
            } else {
                output.push('.');
            }
        }

        output.push('\n');
    }
    output
}

pub fn reverse(hex_dump: &str) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for line in hex_dump.lines() {
        // Skip empty lines
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Find the colon that separates offset from hex data
        let Some(colon_pos) = line.find(':') else {
            continue;
        };

        let after_colon = &line[colon_pos + 1..];

        // Find where ASCII column starts (two spaces after hex section)
        // The hex section contains hex chars and single spaces between pairs
        let hex_part = if let Some(ascii_start) = after_colon.find("  ") {
            &after_colon[..ascii_start]
        } else {
            after_colon
        };

        // Extract hex characters
        let hex_chars: String = hex_part.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let bytes = hex::decode(&hex_chars).context("Failed to decode hex in dump")?;
        result.extend_from_slice(&bytes);
    }
    Ok(result)
}
