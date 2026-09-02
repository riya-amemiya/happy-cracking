use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum UuencodeAction {
    #[command(about = "Encode to uuencode format")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
        #[arg(long, default_value = "data", help = "Filename for the begin header")]
        filename: String,
    },
    #[command(about = "Decode from uuencode format")]
    Decode {
        #[arg(help = "uuencoded text")]
        input: String,
    },
}

pub fn run(action: UuencodeAction) -> Result<()> {
    match action {
        UuencodeAction::Encode { input, filename } => {
            println!("{}", encode(input.as_bytes(), &filename));
        }
        UuencodeAction::Decode { input } => {
            let bytes = decode(&input)?;
            match String::from_utf8(bytes.clone()) {
                Ok(s) => println!("{s}"),
                Err(_) => println!("{}", String::from_utf8_lossy(&bytes)),
            }
        }
    }
    Ok(())
}

fn uu_char(value: u8) -> u8 {
    (value & 0x3F) + 0x20
}

#[must_use]
pub fn encode(data: &[u8], filename: &str) -> String {
    let mut out = format!("begin 644 {filename}\n");

    for chunk in data.chunks(45) {
        out.push(uu_char(chunk.len() as u8) as char);

        for triple in chunk.chunks(3) {
            let b0 = triple[0];
            let b1 = *triple.get(1).unwrap_or(&0);
            let b2 = *triple.get(2).unwrap_or(&0);

            out.push(uu_char(b0 >> 2) as char);
            out.push(uu_char(((b0 << 4) | (b1 >> 4)) & 0x3F) as char);
            out.push(uu_char(((b1 << 2) | (b2 >> 6)) & 0x3F) as char);
            out.push(uu_char(b2 & 0x3F) as char);
        }
        out.push('\n');
    }

    out.push('`');
    out.push('\n');
    out.push_str("end\n");
    out
}

fn uu_value(c: u8) -> Result<u8> {
    match c {
        b'`' | b' ' => Ok(0),
        0x21..=0x60 => Ok(c - 0x20),
        _ => anyhow::bail!("Invalid uuencode character: {:?}", c as char),
    }
}

pub fn decode(s: &str) -> Result<Vec<u8>> {
    let mut lines = s.lines();

    let mut started = false;
    for line in lines.by_ref() {
        if line.starts_with("begin ") {
            started = true;
            break;
        }
    }
    if !started {
        anyhow::bail!("Missing uuencode 'begin' header");
    }

    let mut result = Vec::new();
    for line in lines {
        if line.starts_with("end") {
            return Ok(result);
        }
        let bytes = line.as_bytes();
        if bytes.is_empty() {
            continue;
        }

        let count = uu_value(bytes[0]).context("Failed to read uuencode line length")? as usize;
        if count == 0 {
            continue;
        }

        let mut decoded = Vec::new();
        let body = &bytes[1..];
        for group in body.chunks(4) {
            let v0 = uu_value(group[0]).context("Failed to decode uuencode data")?;
            let v1 = uu_value(*group.get(1).unwrap_or(&b'`'))
                .context("Failed to decode uuencode data")?;
            let v2 = uu_value(*group.get(2).unwrap_or(&b'`'))
                .context("Failed to decode uuencode data")?;
            let v3 = uu_value(*group.get(3).unwrap_or(&b'`'))
                .context("Failed to decode uuencode data")?;

            decoded.push((v0 << 2) | (v1 >> 4));
            decoded.push((v1 << 4) | (v2 >> 2));
            decoded.push((v2 << 6) | v3);
        }

        if count > decoded.len() {
            anyhow::bail!("uuencode line declares more bytes than it contains");
        }
        decoded.truncate(count);
        result.extend_from_slice(&decoded);
    }

    anyhow::bail!("Missing uuencode 'end' footer");
}
