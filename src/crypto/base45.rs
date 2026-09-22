use anyhow::{Context, Result};
use clap::Subcommand;

const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

const DECODE_TABLE: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < ALPHABET.len() {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

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

fn decode_digit(byte: u8) -> Result<u32> {
    let v = DECODE_TABLE[byte as usize];
    if v == 255 {
        anyhow::bail!("Invalid Base45 character: {}", byte as char);
    }
    Ok(u32::from(v))
}

pub fn decode(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }

    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() / 3 * 2);
    let (chunks, rest) = bytes.as_chunks::<3>();
    for chunk in chunks {
        let n = decode_digit(chunk[0])?
            + decode_digit(chunk[1])? * 45
            + decode_digit(chunk[2])? * 45 * 45;
        if n > 0xFFFF {
            anyhow::bail!("Invalid Base45 group: value {n} exceeds 0xFFFF");
        }
        result.push((n / 256) as u8);
        result.push((n % 256) as u8);
    }

    match rest {
        [] => {}
        [x, y] => {
            let n = decode_digit(*x)? + decode_digit(*y)? * 45;
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
