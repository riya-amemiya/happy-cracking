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
            print!("{text}");
        }
    }
    Ok(())
}

#[must_use]
pub fn dump(input: &str) -> String {
    dump_bytes(input.as_bytes())
}

#[must_use]
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

pub const MAX_HEXDUMP_BYTES: usize = 16 * 1024 * 1024;

pub fn reverse(hex_dump: &str) -> Result<Vec<u8>> {
    reverse_with_limit(hex_dump, MAX_HEXDUMP_BYTES)
}

pub fn reverse_with_limit(hex_dump: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let max_bytes = max_bytes.min(MAX_HEXDUMP_BYTES);
    let max_chars = max_bytes.saturating_mul(8);
    if hex_dump.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    let mut result = Vec::with_capacity((hex_dump.len() / 4).min(max_bytes));
    for line in hex_dump.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some(colon_pos) = line.find(':') else {
            continue;
        };

        let after_colon = &line[colon_pos + 1..];

        let hex_part = if let Some(ascii_start) = after_colon.find("  ") {
            &after_colon[..ascii_start]
        } else {
            after_colon
        };

        let additional = hex_part.bytes().filter(u8::is_ascii_hexdigit).count() / 2;
        if result.len().saturating_add(additional) > max_bytes {
            anyhow::bail!(
                "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
            );
        }

        append_hex_pairs(hex_part.as_bytes(), &mut result)
            .context("Failed to decode hex in dump")?;
    }
    Ok(result)
}

fn append_hex_pairs(hex_part: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let mut hi: Option<u8> = None;
    for &b in hex_part {
        let nibble = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => continue,
        };
        if let Some(h) = hi.take() {
            out.push((h << 4) | nibble);
        } else {
            hi = Some(nibble);
        }
    }
    if hi.is_some() {
        anyhow::bail!("odd number of hex digits");
    }
    Ok(())
}
