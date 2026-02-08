use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum StrToolsAction {
    #[command(about = "Reverse a string")]
    Reverse {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Show ASCII/Unicode code points for each character")]
    Ord {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Convert space-separated code points to characters")]
    Chr {
        #[arg(help = "Space-separated code points (e.g. \"72 101 108 108 111\")")]
        input: String,
    },
}

pub fn run(action: StrToolsAction) -> Result<()> {
    match action {
        StrToolsAction::Reverse { input } => {
            println!("{}", reverse(&input));
        }
        StrToolsAction::Ord { input } => {
            println!("{}", ord(&input));
        }
        StrToolsAction::Chr { input } => {
            println!("{}", chr(&input)?);
        }
    }
    Ok(())
}

// Reverse a string by Unicode grapheme.
pub fn reverse(input: &str) -> String {
    input.chars().rev().collect()
}

// Return each character with its code point in "A=65 B=66" format.
pub fn ord(input: &str) -> String {
    input
        .chars()
        .map(|c| format!("{}={}", c, c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

// Convert space-separated numeric values to a string.
pub fn chr(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|s| {
            let n: u32 = s
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid code point: {}", s))?;
            char::from_u32(n).ok_or_else(|| anyhow::anyhow!("Invalid Unicode code point: {}", n))
        })
        .collect()
}
