use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BinaryAction {
    #[command(about = "Encode text to binary")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode binary to text")]
    Decode {
        #[arg(help = "Binary string (space or no space separated)")]
        input: String,
    },
}

pub fn run(action: BinaryAction) -> Result<()> {
    match action {
        BinaryAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        BinaryAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let mut out = vec![0u8; bytes.len() * 9 - 1];
    let mut pos = 0;
    for &b in bytes {
        if pos != 0 {
            out[pos] = b' ';
            pos += 1;
        }
        for j in 0..8 {
            out[pos + j] = b'0' + ((b >> (7 - j)) & 1);
        }
        pos += 8;
    }
    String::from_utf8(out).expect("binary encode produces ASCII digits and spaces")
}

pub fn decode(input: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(input.len() / 8);
    let mut acc = 0u8;
    let mut nbits = 0u8;
    for b in input.bytes() {
        if b == b'0' || b == b'1' {
            acc = (acc << 1) | (b - b'0');
            nbits += 1;
            if nbits == 8 {
                bytes.push(acc);
                acc = 0;
                nbits = 0;
            }
        }
    }
    if nbits != 0 {
        anyhow::bail!("Binary string length must be a multiple of 8");
    }
    String::from_utf8(bytes).context("Decoded data is not valid UTF-8")
}
