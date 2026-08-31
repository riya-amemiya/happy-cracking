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

pub fn encode(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    // 8 bits + 1 space per byte, minus the trailing space.
    let mut out = String::with_capacity(input.len() * 9 - 1);
    for (i, b) in input.bytes().enumerate() {
        if i > 0 {
            out.push(' ');
        }

        let mut bits = [0u8; 8];
        for j in 0..8 {
            bits[7 - j] = b'0' + ((b >> j) & 1);
        }
        out.push_str(std::str::from_utf8(&bits).expect("bits are ASCII digits"));
    }
    out
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
