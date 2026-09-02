use anyhow::{Context, Result};
use clap::Subcommand;
use std::sync::LazyLock;

const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

static DECODE_TABLE: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [255u8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    table
});

#[derive(Subcommand)]
pub enum Base45Action {
    #[command(about = "Encode to Base45 (RFC 9285)")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base45 (RFC 9285)")]
    Decode {
        #[arg(help = "Base45 encoded string")]
        input: String,
    },
}

pub fn run(action: Base45Action) -> Result<()> {
    match action {
        Base45Action::Encode { input } => {
            println!("{}", encode(input.as_bytes()));
        }
        Base45Action::Decode { input } => {
            let bytes = decode(&input)?;

            match String::from_utf8(bytes.clone()).context("Decoded data is not valid UTF-8") {
                Ok(s) => println!("{s}"),
                Err(_) => println!("(lossy) {}", String::from_utf8_lossy(&bytes)),
            }
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(data: &[u8]) -> String {
    let mut result = String::with_capacity(data.len().div_ceil(2) * 3);

    let (chunks, rest) = data.as_chunks::<2>();
    for chunk in chunks {
        let n = u16::from(chunk[0]) * 256 + u16::from(chunk[1]);
        let c = (n % 45) as usize;
        let d = ((n / 45) % 45) as usize;
        let e = ((n / 45 / 45) % 45) as usize;
        result.push(ALPHABET[c] as char);
        result.push(ALPHABET[d] as char);
        result.push(ALPHABET[e] as char);
    }

    if let [b] = rest {
        let n = u16::from(*b);
        let c = (n % 45) as usize;
        let d = ((n / 45) % 45) as usize;
        result.push(ALPHABET[c] as char);
        result.push(ALPHABET[d] as char);
    }

    result
}

pub fn decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }

    let mut values = Vec::with_capacity(s.len());
    for ch in s.chars() {
        if !ch.is_ascii() {
            anyhow::bail!("Invalid Base45 character: {ch}");
        }
        let v = DECODE_TABLE[ch as usize];
        if v == 255 {
            anyhow::bail!("Invalid Base45 character: {ch}");
        }
        values.push(u32::from(v));
    }

    let mut result = Vec::with_capacity(values.len() / 3 * 2);
    let (chunks, rest) = values.as_chunks::<3>();
    for chunk in chunks {
        let n = chunk[0] + chunk[1] * 45 + chunk[2] * 45 * 45;
        if n > 0xFFFF {
            anyhow::bail!("Invalid Base45 group: value {n} exceeds 0xFFFF");
        }
        result.push((n / 256) as u8);
        result.push((n % 256) as u8);
    }

    match rest.len() {
        0 => {}
        2 => {
            let n = rest[0] + rest[1] * 45;
            if n > 0xFF {
                anyhow::bail!("Invalid Base45 group: value {n} exceeds 0xFF");
            }
            result.push(n as u8);
        }
        _ => {
            anyhow::bail!("Invalid Base45 length: trailing group must be 2 characters");
        }
    }

    Ok(result)
}
