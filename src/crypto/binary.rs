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
    input
        .bytes()
        .map(|b| format!("{:08b}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn decode(input: &str) -> Result<String> {
    let cleaned: String = input.chars().filter(|c| *c == '0' || *c == '1').collect();

    if !cleaned.len().is_multiple_of(8) {
        anyhow::bail!("Binary string length must be a multiple of 8");
    }

    let bytes: Result<Vec<u8>, _> = cleaned
        .as_bytes()
        .chunks(8)
        .map(|chunk| {
            let s = std::str::from_utf8(chunk).unwrap();
            u8::from_str_radix(s, 2)
        })
        .collect();

    let bytes = bytes.context("Failed to parse binary")?;
    String::from_utf8(bytes).context("Decoded data is not valid UTF-8")
}
