use anyhow::{Context, Result};
use clap::Subcommand;
use umt_rust::string::{umt_from_base64, umt_to_base64};

#[derive(Subcommand)]
pub enum Base64Action {
    #[command(about = "Encode to Base64")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base64")]
    Decode {
        #[arg(help = "Base64 encoded string")]
        input: String,
    },
}

pub fn run(action: Base64Action) -> Result<()> {
    match action {
        Base64Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base64Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> String {
    umt_to_base64(input)
}

pub fn decode(input: &str) -> Result<String> {
    umt_from_base64(input.trim())
        .map_err(|e| anyhow::anyhow!(e))
        .context("Failed to decode Base64")
}
