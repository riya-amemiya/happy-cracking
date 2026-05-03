use anyhow::{Context, Result};
use clap::Subcommand;
use umt_rust::crypto::{umt_decode_base58, umt_encode_base58};

#[derive(Subcommand)]
pub enum Base58Action {
    #[command(about = "Encode to Base58")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base58")]
    Decode {
        #[arg(help = "Base58 encoded string")]
        input: String,
    },
}

pub fn run(action: Base58Action) -> Result<()> {
    match action {
        Base58Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base58Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> String {
    umt_encode_base58(input.as_bytes())
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = umt_decode_base58(input.trim()).context("Failed to decode Base58")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
