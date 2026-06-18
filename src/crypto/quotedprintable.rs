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

fn push_encoded(out: &mut String, line_len: &mut usize, token: &str) {
    if *line_len + token.len() > 75 {
        out.push_str("=\n");
        *line_len = 0;
    }
    out.push_str(token);
    *line_len += token.len();
}

pub fn encode(data: &[u8]) -> String {
    let mut out = String::new();
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
                    let token = format!(
                        "={}{}",
                        HEX[(b >> 4) as usize] as char,
                        HEX[(b & 0xF) as usize] as char
                    );
                    push_encoded(&mut out, &mut line_len, &token);
                } else {
                    push_encoded(&mut out, &mut line_len, &(b as char).to_string());
                }
            }
            0x21..=0x7E if b != b'=' => {
                push_encoded(&mut out, &mut line_len, &(b as char).to_string());
            }
            _ => {
                let token = format!(
                    "={}{}",
                    HEX[(b >> 4) as usize] as char,
                    HEX[(b & 0xF) as usize] as char
                );
                push_encoded(&mut out, &mut line_len, &token);
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
