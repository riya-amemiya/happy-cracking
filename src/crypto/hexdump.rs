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

    // Optimization: Pre-allocate exact string capacity to avoid reallocations.
    // Each line has up to 16 bytes and takes exactly 74 chars:
    // 8 (offset) + 2 (": ") + 39 (hex pairs) + 2 ("  ") + up to 16 (ascii) + 1 (newline) = 68 to 74 chars
    let num_lines = data.len().div_ceil(16);
    let mut output = String::with_capacity(num_lines * 74);

    // Optimization: Manual byte array lookups instead of expensive `format!` macros
    let hex_chars = b"0123456789abcdef";

    for (line_idx, chunk) in data.chunks(16).enumerate() {
        let offset = line_idx * 16;

        // 8 chars for offset + 2 for ": "
        let mut offset_buf = [b'0'; 10];
        let mut temp_offset = offset;
        for i in 0..8 {
            offset_buf[7 - i] = hex_chars[temp_offset & 0xf];
            temp_offset >>= 4;
        }
        offset_buf[8] = b':';
        offset_buf[9] = b' ';

        // SAFETY: Only contains '0'-'9', 'a'-'f', ':', ' ' which are valid ASCII/UTF-8
        output.push_str(unsafe { std::str::from_utf8_unchecked(&offset_buf) });

        let mut hex_buf = [b' '; 39]; // 8 pairs + 7 spaces

        for pair_idx in 0..8 {
            let byte_offset = pair_idx * 2;
            let hex_idx = pair_idx * 5;

            if byte_offset < chunk.len() {
                hex_buf[hex_idx] = hex_chars[(chunk[byte_offset] >> 4) as usize];
                hex_buf[hex_idx + 1] = hex_chars[(chunk[byte_offset] & 0xf) as usize];

                if byte_offset + 1 < chunk.len() {
                    hex_buf[hex_idx + 2] = hex_chars[(chunk[byte_offset + 1] >> 4) as usize];
                    hex_buf[hex_idx + 3] = hex_chars[(chunk[byte_offset + 1] & 0xf) as usize];
                }
            }
        }

        // SAFETY: Only contains '0'-'9', 'a'-'f', ' ' which are valid ASCII/UTF-8
        output.push_str(unsafe { std::str::from_utf8_unchecked(&hex_buf) });
        output.push_str("  ");

        // Collect ascii part in a buffer to do one push_str instead of pushing chars individually
        let mut ascii_buf = [b'.'; 17];
        let mut i = 0;
        for &byte in chunk {
            if byte.is_ascii_graphic() || byte == b' ' {
                ascii_buf[i] = byte;
            }
            i += 1;
        }
        ascii_buf[i] = b'\n';

        // SAFETY: ASCII graphic chars, spaces, '.', and '\n' are valid ASCII/UTF-8
        output.push_str(unsafe { std::str::from_utf8_unchecked(&ascii_buf[..i + 1]) });
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
