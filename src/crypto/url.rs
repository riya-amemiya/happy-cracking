use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum UrlAction {
    #[command(about = "URL encode")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "URL decode")]
    Decode {
        #[arg(help = "URL encoded string")]
        input: String,
    },
}

pub fn run(action: UrlAction) -> Result<()> {
    match action {
        UrlAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        UrlAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    urlencoding::encode(input).into_owned()
}

pub fn decode(input: &str) -> Result<String> {
    urlencoding::decode(input.trim())
        .map(std::borrow::Cow::into_owned)
        .map_err(|e| anyhow::anyhow!("Failed to decode URL: {e}"))
}
