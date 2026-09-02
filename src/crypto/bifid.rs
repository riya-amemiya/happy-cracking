use anyhow::Result;
use clap::Subcommand;

use super::polybius_utils::{build_polybius_square, build_reverse_lookup, find_in_square_fast};

#[derive(Subcommand)]
pub enum BifidAction {
    #[command(about = "Encrypt with Bifid cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Keyword for Polybius square")]
        key: String,
    },
    #[command(about = "Decrypt Bifid cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Keyword for Polybius square")]
        key: String,
    },
}

pub fn run(action: BifidAction) -> Result<()> {
    match action {
        BifidAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        BifidAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

fn normalize_input(input: &str) -> Vec<char> {
    input
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_uppercase)
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect()
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let letters = normalize_input(input);
    if letters.is_empty() {
        return Ok(String::new());
    }

    let square = build_polybius_square(key);
    let reverse = build_reverse_lookup(&square);

    let mut rows = Vec::with_capacity(letters.len());
    let mut cols = Vec::with_capacity(letters.len());
    for &c in &letters {
        let (r, col) = find_in_square_fast(&reverse, c)
            .ok_or_else(|| anyhow::anyhow!("Character {c} not in square"))?;
        rows.push(r);
        cols.push(col);
    }

    // Step 2: concatenate all rows then all cols
    let mut combined = Vec::with_capacity(letters.len() * 2);
    combined.extend_from_slice(&rows);
    combined.extend_from_slice(&cols);

    let result: String = combined
        .chunks(2)
        .map(|pair| square[pair[0] * 5 + pair[1]])
        .collect();

    Ok(result)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let letters = normalize_input(input);
    if letters.is_empty() {
        return Ok(String::new());
    }

    let square = build_polybius_square(key);
    let reverse = build_reverse_lookup(&square);

    let mut coords = Vec::with_capacity(letters.len() * 2);
    for &c in &letters {
        let (r, col) = find_in_square_fast(&reverse, c)
            .ok_or_else(|| anyhow::anyhow!("Character {c} not in square"))?;
        coords.push(r);
        coords.push(col);
    }

    // Step 2: split into rows half and cols half
    let half = letters.len();
    let rows = &coords[..half];
    let cols = &coords[half..];

    let result: String = rows
        .iter()
        .zip(cols.iter())
        .map(|(&r, &c)| square[r * 5 + c])
        .collect();

    Ok(result)
}
