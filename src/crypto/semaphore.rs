use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum SemaphoreAction {
    #[command(about = "Encode text to flag semaphore positions")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode flag semaphore positions to text")]
    Decode {
        #[arg(help = "Semaphore encoded string (e.g. 1-2 1-3 1-4)")]
        input: String,
    },
}

pub fn run(action: SemaphoreAction) -> Result<()> {
    match action {
        SemaphoreAction::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        SemaphoreAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// Positions: 1=S, 2=SW, 3=W, 4=NW, 5=N, 6=NE, 7=E, 8=SE
const SEMAPHORE_MAP: &[(char, &str)] = &[
    ('A', "1-2"),
    ('B', "1-3"),
    ('C', "1-4"),
    ('D', "1-5"),
    ('E', "1-6"),
    ('F', "1-7"),
    ('G', "1-8"),
    ('H', "2-3"),
    ('I', "2-4"),
    ('J', "5-6"),
    ('K', "3-1"),
    ('L', "3-2"),
    ('M', "3-4"),
    ('N', "3-5"),
    ('O', "3-6"),
    ('P', "3-7"),
    ('Q', "3-8"),
    ('R', "4-3"),
    ('S', "4-5"),
    ('T', "4-6"),
    ('U', "4-7"),
    ('V', "5-1"),
    ('W', "6-2"),
    ('X', "6-3"),
    ('Y', "6-4"),
    ('Z', "6-5"),
];

static CHAR_TO_SEMAPHORE: LazyLock<HashMap<char, &'static str>> =
    LazyLock::new(|| SEMAPHORE_MAP.iter().copied().collect());

static SEMAPHORE_TO_CHAR: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| SEMAPHORE_MAP.iter().map(|&(c, code)| (code, c)).collect());

pub fn encode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let codes: Result<Vec<&str>> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| {
            CHAR_TO_SEMAPHORE
                .get(&c)
                .copied()
                .context(format!("No semaphore code for character: {}", c))
        })
        .collect();

    Ok(codes?.join(" "))
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|code| {
            SEMAPHORE_TO_CHAR
                .get(code)
                .copied()
                .context(format!("Unknown semaphore code: {}", code))
        })
        .collect()
}
