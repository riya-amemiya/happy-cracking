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
    if data.is_empty() {
        return String::new();
    }

    let num_lines = data.len().div_ceil(16);
    let mut output = Vec::with_capacity(num_lines * 68);
    let hex_chars = b"0123456789abcdef";

    for (line_idx, chunk) in data.chunks(16).enumerate() {
        let offset = line_idx * 16;

        let mut offset_hex = offset;
        let mut buf = [b'0'; 16];
        let mut start_idx = 16;

        loop {
            start_idx -= 1;
            buf[start_idx] = hex_chars[offset_hex & 0xf];
            offset_hex >>= 4;
            if offset_hex == 0 && start_idx <= 8 {
                break;
            }
        }
        while start_idx > 8 {
            start_idx -= 1;
            buf[start_idx] = b'0';
        }

        output.extend_from_slice(&buf[start_idx..]);
        output.extend_from_slice(b": ");

        for pair_idx in 0..8 {
            let byte_offset = pair_idx * 2;
            if byte_offset < chunk.len() {
                let b = chunk[byte_offset];
                output.push(hex_chars[(b >> 4) as usize]);
                output.push(hex_chars[(b & 0xf) as usize]);

                if byte_offset + 1 < chunk.len() {
                    let b2 = chunk[byte_offset + 1];
                    output.push(hex_chars[(b2 >> 4) as usize]);
                    output.push(hex_chars[(b2 & 0xf) as usize]);
                } else {
                    output.extend_from_slice(b"  ");
                }
            } else {
                output.extend_from_slice(b"    ");
            }
            if pair_idx < 7 {
                output.push(b' ');
            }
        }

        output.extend_from_slice(b"  ");

        for &byte in chunk {
            if byte.is_ascii_graphic() || byte == b' ' {
                output.push(byte);
            } else {
                output.push(b'.');
            }
        }

        output.push(b'\n');
    }
    String::from_utf8(output).expect("hexdump output is ASCII")
}

pub fn reverse(hex_dump: &str) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for line in hex_dump.lines() {
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

        let hex_chars: String = hex_part.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let bytes = hex::decode(&hex_chars).context("Failed to decode hex in dump")?;
        result.extend_from_slice(&bytes);
    }
    Ok(result)
}
