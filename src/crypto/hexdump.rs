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

    // Optimization: Avoid `format!` overhead in hot loop and preallocate
    // Each line has a fixed maximum length of 68 chars
    let num_lines = data.len().div_ceil(16);
    let mut output = String::with_capacity(num_lines * 68);
    let hex_chars = b"0123456789abcdef";

    for (line_idx, chunk) in data.chunks(16).enumerate() {
        let offset = line_idx * 16;

        // Offset: at least 8 hex chars, more if needed (> 4GB)
        let mut offset_hex = offset;
        let mut buf = [b'0'; 16]; // Max 64-bit hex is 16 chars
        let mut start_idx = 16;

        loop {
            start_idx -= 1;
            buf[start_idx] = hex_chars[(offset_hex & 0xf) as usize];
            offset_hex >>= 4;
            if offset_hex == 0 && start_idx <= 8 {
                break;
            }
        }
        // Ensure at least 8 chars width
        while start_idx > 8 {
            start_idx -= 1;
            buf[start_idx] = b'0';
        }

        // Safety: `buf` contains only ASCII hex characters
        output.push_str(unsafe { std::str::from_utf8_unchecked(&buf[start_idx..]) });
        output.push_str(": ");

        // Hex pairs
        for pair_idx in 0..8 {
            let byte_offset = pair_idx * 2;
            if byte_offset < chunk.len() {
                let b = chunk[byte_offset];
                let b_hex = [hex_chars[(b >> 4) as usize], hex_chars[(b & 0xf) as usize]];
                // Safety: `b_hex` contains only ASCII hex characters
                output.push_str(unsafe { std::str::from_utf8_unchecked(&b_hex) });

                if byte_offset + 1 < chunk.len() {
                    let b2 = chunk[byte_offset + 1];
                    let b2_hex = [
                        hex_chars[(b2 >> 4) as usize],
                        hex_chars[(b2 & 0xf) as usize],
                    ];
                    // Safety: `b2_hex` contains only ASCII hex characters
                    output.push_str(unsafe { std::str::from_utf8_unchecked(&b2_hex) });
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
