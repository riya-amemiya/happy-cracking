use anyhow::{Context, Result};
use clap::Subcommand;

const ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&()*+,./:;<=>?@[]^_`{|}~\"";

#[derive(Subcommand)]
pub enum Base91Action {
    #[command(about = "Encode to Base91")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base91")]
    Decode {
        #[arg(help = "Base91 encoded string")]
        input: String,
    },
}

pub fn run(action: Base91Action) -> Result<()> {
    match action {
        Base91Action::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        Base91Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> Result<String> {
    let data = input.as_bytes();
    if data.is_empty() {
        return Ok(String::new());
    }

    let mut result = Vec::new();
    let mut b: u32 = 0;
    let mut n: u32 = 0;

    for &byte in data {
        b |= (byte as u32) << n;
        n += 8;

        if n > 13 {
            let mut v = b & 8191; // 13 bits
            if v > 88 {
                b >>= 13;
                n -= 13;
            } else {
                v = b & 16383; // 14 bits
                b >>= 14;
                n -= 14;
            }
            result.push(ALPHABET[(v % 91) as usize]);
            result.push(ALPHABET[(v / 91) as usize]);
        }
    }

    if n > 0 {
        result.push(ALPHABET[(b % 91) as usize]);
        if n > 7 || b > 90 {
            result.push(ALPHABET[(b / 91) as usize]);
        }
    }

    String::from_utf8(result).context("Encoded data is not valid UTF-8")
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut decode_table = [255u8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        decode_table[c as usize] = i as u8;
    }

    let mut result = Vec::new();
    let mut b: u32 = 0;
    let mut n: u32 = 0;
    let mut v: i32 = -1;

    for &byte in input.as_bytes() {
        let d = decode_table[byte as usize];
        if d == 255 {
            anyhow::bail!("Invalid Base91 character: {}", byte as char);
        }
        if v < 0 {
            v = d as i32;
        } else {
            v += (d as i32) * 91;
            b |= (v as u32) << n;
            n += if (v & 8191) > 88 { 13 } else { 14 };
            loop {
                result.push((b & 255) as u8);
                b >>= 8;
                n -= 8;
                if n <= 7 {
                    break;
                }
            }
            v = -1;
        }
    }

    if v >= 0 {
        result.push((b | (v as u32) << n) as u8);
    }

    String::from_utf8(result).context("Decoded data is not valid UTF-8")
}
