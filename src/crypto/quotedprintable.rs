use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum QpAction {
    #[command(about = "Encode to Quoted-Printable (RFC 2045)")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Quoted-Printable (RFC 2045)")]
    Decode {
        #[arg(help = "Quoted-Printable encoded string")]
        input: String,
    },
}

pub fn run(action: QpAction) -> Result<()> {
    match action {
        QpAction::Encode { input } => {
            println!("{}", encode(input.as_bytes()));
        }
        QpAction::Decode { input } => {
            let bytes = decode(&input)?;
            match String::from_utf8(bytes.clone()) {
                Ok(s) => println!("{s}"),
                Err(_) => println!("{}", String::from_utf8_lossy(&bytes)),
            }
        }
    }
    Ok(())
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

// Quoted-Printable lines must stay <= 76 chars including the soft-break marker.
const MAX_LINE: usize = 75;

/// Insert a soft line break when the next token would exceed RFC 2045's 76-char
/// line limit. Callers then append the token and bump `line_len` themselves.
fn maybe_soft_break(out: &mut String, line_len: &mut usize, token_len: usize) {
    if *line_len + token_len > MAX_LINE {
        out.push_str("=\n");
        *line_len = 0;
    }
}

fn push_char(out: &mut String, line_len: &mut usize, c: char) {
    maybe_soft_break(out, line_len, 1);
    out.push(c);
    *line_len += 1;
}

/// Append `=XX` without `format!` / heap allocation. HEX is ASCII so these
/// `push` calls write single bytes.
fn push_hex_encoded(out: &mut String, line_len: &mut usize, b: u8) {
    maybe_soft_break(out, line_len, 3);
    out.push('=');
    out.push(HEX[(b >> 4) as usize] as char);
    out.push(HEX[(b & 0xF) as usize] as char);
    *line_len += 3;
}

// Performance: the previous encoder allocated a `String` on every input byte —
// `(b as char).to_string()` for printable ASCII and `format!("={}{}", ...)` for
// escaped bytes — then joined them through `push_str`. On 10k mixed/binary
// inputs that was ~9–12x slower than pushing chars into one pre-sized buffer
// (`release`, 1.83). ASCII-only mail is ~1.1–1.7x because it skips `format!`
// but still avoids the per-byte `to_string()`.
pub fn encode(data: &[u8]) -> String {
    // Worst case is `=XX` (3 chars) per byte, plus occasional `=\n` soft breaks.
    let mut out = String::with_capacity(data.len().saturating_mul(3));
    let mut line_len = 0usize;

    for (i, &b) in data.iter().enumerate() {
        match b {
            b'\n' => {
                out.push('\n');
                line_len = 0;
            }
            b' ' | b'\t' => {
                let at_line_end = match data.get(i + 1) {
                    None => true,
                    Some(&next) => next == b'\n',
                };
                if at_line_end {
                    push_hex_encoded(&mut out, &mut line_len, b);
                } else {
                    push_char(&mut out, &mut line_len, b as char);
                }
            }
            0x21..=0x7E if b != b'=' => {
                push_char(&mut out, &mut line_len, b as char);
            }
            _ => {
                push_hex_encoded(&mut out, &mut line_len, b);
            }
        }
    }

    out
}

fn hex_digit(c: u8) -> Result<u8> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => anyhow::bail!("Invalid hex digit in '=' escape: {:?}", c as char),
    }
}

pub fn decode(s: &str) -> Result<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'=' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if i + 2 < bytes.len() && bytes[i + 1] == b'\r' && bytes[i + 2] == b'\n' {
                i += 3;
                continue;
            }

            if i + 2 >= bytes.len() {
                anyhow::bail!("Truncated '=' escape sequence");
            }
            let hi = hex_digit(bytes[i + 1]).context("Failed to decode Quoted-Printable")?;
            let lo = hex_digit(bytes[i + 2]).context("Failed to decode Quoted-Printable")?;
            result.push((hi << 4) | lo);
            i += 3;
        } else {
            result.push(b);
            i += 1;
        }
    }

    Ok(result)
}
