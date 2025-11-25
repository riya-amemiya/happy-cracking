use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HexAction {
    #[command(about = "Encode to hex")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from hex")]
    Decode {
        #[arg(help = "Hex string")]
        input: String,
    },
}

pub fn run(action: HexAction) -> Result<()> {
    match action {
        HexAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        HexAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> String {
    hex::encode(input.as_bytes())
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = hex::decode(input.trim()).context("Failed to decode hex")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
