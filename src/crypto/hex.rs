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

pub const MAX_HEX_BYTES: usize = 16 * 1024 * 1024;

pub fn decode_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_HEX_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    hex::decode(hex_str).context("Failed to decode hex")
}

#[must_use]
pub fn encode(input: &str) -> String {
    hex::encode(input.as_bytes())
}

pub fn decode(input: &str) -> Result<String> {
    let decoded = decode_hex_with_limit(input, MAX_HEX_BYTES)?;
    String::from_utf8(decoded).context("Decoded data is not valid UTF-8")
}
