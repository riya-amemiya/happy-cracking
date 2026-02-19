use anyhow::Result;
use clap::Subcommand;

use super::polybius_utils::{build_polybius_square, find_in_square};

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
        .filter(|c| c.is_ascii_uppercase())
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

    // Step 1: get row and column coordinates
    let mut rows = Vec::with_capacity(letters.len());
    let mut cols = Vec::with_capacity(letters.len());
    for &c in &letters {
        let (r, col) = find_in_square(&square, c)
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", c))?;
        rows.push(r);
        cols.push(col);
    }

    // Step 2: concatenate all rows then all cols
    let mut combined = Vec::with_capacity(letters.len() * 2);
    combined.extend_from_slice(&rows);
    combined.extend_from_slice(&cols);

    // Step 3: read pairs and convert back
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

    // Step 1: convert each ciphertext letter to row/col pair
    let mut coords = Vec::with_capacity(letters.len() * 2);
    for &c in &letters {
        let (r, col) = find_in_square(&square, c)
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", c))?;
        coords.push(r);
        coords.push(col);
    }

    // Step 2: split into rows half and cols half
    let half = letters.len();
    let rows = &coords[..half];
    let cols = &coords[half..];

    // Step 3: pair up row[i] and col[i] to get original letters
    let result: String = rows
        .iter()
        .zip(cols.iter())
        .map(|(&r, &c)| square[r * 5 + c])
        .collect();

    Ok(result)
}
