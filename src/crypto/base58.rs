use anyhow::{Context, Result};
use clap::Subcommand;

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
    bs58::encode(input.as_bytes()).into_string()
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = bs58::decode(input.trim())
        .into_vec()
        .context("Failed to decode Base58")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
