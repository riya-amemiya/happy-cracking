use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Subcommand;

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
    STANDARD.encode(input.as_bytes())
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = STANDARD
        .decode(input.trim())
        .context("Failed to decode Base64")?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
