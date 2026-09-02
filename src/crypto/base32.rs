use anyhow::{Context, Result};
use clap::Subcommand;
use umt_rust::crypto::{umt_decode_base32, umt_encode_base32};

#[derive(Subcommand)]
pub enum Base32Action {
    #[command(about = "Encode to Base32")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base32")]
    Decode {
        #[arg(help = "Base32 encoded string")]
        input: String,
    },
}

pub fn run(action: Base32Action) -> Result<()> {
    match action {
        Base32Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base32Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    umt_encode_base32(input.as_bytes())
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = umt_decode_base32(input.trim()).context("Failed to decode Base32")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
