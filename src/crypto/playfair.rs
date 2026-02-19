use anyhow::Result;
use clap::Subcommand;

use super::polybius_utils::{build_polybius_square, find_in_square};

#[derive(Subcommand)]
pub enum PlayfairAction {
    #[command(about = "Encrypt with Playfair cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Keyword for generating the 5x5 matrix")]
        key: String,
    },
    #[command(about = "Decrypt Playfair cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Keyword for generating the 5x5 matrix")]
        key: String,
    },
}

pub fn run(action: PlayfairAction) -> Result<()> {
    match action {
        PlayfairAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        PlayfairAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

fn prepare_digraphs(input: &str) -> Vec<(char, char)> {
    let letters: Vec<char> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect();

    let mut digraphs = Vec::new();
    let mut i = 0;
    while i < letters.len() {
        let a = letters[i];
        if i + 1 < letters.len() {
            let b = letters[i + 1];
            if a == b {
                digraphs.push((a, 'X'));
                i += 1;
            } else {
                digraphs.push((a, b));
                i += 2;
            }
        } else {
            digraphs.push((a, 'X'));
            i += 1;
        }
    }

    digraphs
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let filtered: String = input.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if filtered.is_empty() {
        return Ok(String::new());
    }

    let matrix = build_polybius_square(key);
    let digraphs = prepare_digraphs(input);

    let mut result = String::new();
    for (a, b) in digraphs {
        let (ra, ca) = find_in_square(&matrix, a)
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", a))?;
        let (rb, cb) = find_in_square(&matrix, b)
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", b))?;

        if ra == rb {
            // Same row: shift right
            result.push(matrix[ra * 5 + (ca + 1) % 5]);
            result.push(matrix[rb * 5 + (cb + 1) % 5]);
        } else if ca == cb {
            // Same column: shift down
            result.push(matrix[((ra + 1) % 5) * 5 + ca]);
            result.push(matrix[((rb + 1) % 5) * 5 + cb]);
        } else {
            // Rectangle: swap columns
            result.push(matrix[ra * 5 + cb]);
            result.push(matrix[rb * 5 + ca]);
        }
    }

    Ok(result)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let letters: Vec<char> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .map(|c| if c == 'J' { 'I' } else { c })
        .collect();

    if letters.is_empty() {
        return Ok(String::new());
    }

    if !letters.len().is_multiple_of(2) {
        anyhow::bail!("Encrypted text must have even length");
    }

    let matrix = build_polybius_square(key);

    let mut result = String::new();
    for pair in letters.chunks(2) {
        let (ra, ca) = find_in_square(&matrix, pair[0])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[0]))?;
        let (rb, cb) = find_in_square(&matrix, pair[1])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[1]))?;

        if ra == rb {
            // Same row: shift left
            result.push(matrix[ra * 5 + (ca + 4) % 5]);
            result.push(matrix[rb * 5 + (cb + 4) % 5]);
        } else if ca == cb {
            // Same column: shift up
            result.push(matrix[((ra + 4) % 5) * 5 + ca]);
            result.push(matrix[((rb + 4) % 5) * 5 + cb]);
        } else {
            // Rectangle: swap columns
            result.push(matrix[ra * 5 + cb]);
            result.push(matrix[rb * 5 + ca]);
        }
    }

    Ok(result)
}
