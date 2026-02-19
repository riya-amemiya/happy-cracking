use anyhow::Result;
use clap::Subcommand;

use super::polybius_utils::{build_polybius_square, find_in_square};

#[derive(Subcommand)]
pub enum FoursquareAction {
    #[command(about = "Encrypt with Four-square cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(long, help = "First keyword for keyed square")]
        key1: String,
        #[arg(long, help = "Second keyword for keyed square")]
        key2: String,
    },
    #[command(about = "Decrypt Four-square cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(long, help = "First keyword for keyed square")]
        key1: String,
        #[arg(long, help = "Second keyword for keyed square")]
        key2: String,
    },
}

pub fn run(action: FoursquareAction) -> Result<()> {
    match action {
        FoursquareAction::Encrypt { input, key1, key2 } => {
            println!("{}", encrypt(&input, &key1, &key2)?);
        }
        FoursquareAction::Decrypt { input, key1, key2 } => {
            println!("{}", decrypt(&input, &key1, &key2)?);
        }
    }
    Ok(())
}

const STANDARD_SQUARE: [char; 25] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    'U', 'V', 'W', 'X', 'Y', 'Z',
];

fn normalize_char(c: char) -> char {
    if c == 'J' { 'I' } else { c }
}

fn prepare_letters(input: &str) -> Vec<char> {
    input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .map(normalize_char)
        .collect()
}

pub fn encrypt(input: &str, key1: &str, key2: &str) -> Result<String> {
    if key1.is_empty() || !key1.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key1 must be non-empty and contain only alphabetic characters");
    }
    if key2.is_empty() || !key2.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key2 must be non-empty and contain only alphabetic characters");
    }

    let mut letters = prepare_letters(input);
    if letters.is_empty() {
        return Ok(String::new());
    }

    // Pad with X if odd length
    if !letters.len().is_multiple_of(2) {
        letters.push('X');
    }

    let top_right = build_polybius_square(key1);
    let bottom_left = build_polybius_square(key2);

    let mut result = String::new();
    for pair in letters.chunks(2) {
        let (row_a, col_a) = find_in_square(&STANDARD_SQUARE, pair[0])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[0]))?;
        let (row_b, col_b) = find_in_square(&STANDARD_SQUARE, pair[1])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[1]))?;

        // Rectangle swap: top-right square at (row_a, col_b), bottom-left square at (row_b, col_a)
        result.push(top_right[row_a * 5 + col_b]);
        result.push(bottom_left[row_b * 5 + col_a]);
    }

    Ok(result)
}

pub fn decrypt(input: &str, key1: &str, key2: &str) -> Result<String> {
    if key1.is_empty() || !key1.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key1 must be non-empty and contain only alphabetic characters");
    }
    if key2.is_empty() || !key2.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key2 must be non-empty and contain only alphabetic characters");
    }

    let letters = prepare_letters(input);
    if letters.is_empty() {
        return Ok(String::new());
    }

    if !letters.len().is_multiple_of(2) {
        anyhow::bail!("Encrypted text must have even length");
    }

    let top_right = build_polybius_square(key1);
    let bottom_left = build_polybius_square(key2);

    let mut result = String::new();
    for pair in letters.chunks(2) {
        // pair[0] came from top-right, pair[1] from bottom-left
        let (row_a, col_b) = find_in_square(&top_right, pair[0])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[0]))?;
        let (row_b, col_a) = find_in_square(&bottom_left, pair[1])
            .ok_or_else(|| anyhow::anyhow!("Character {} not in square", pair[1]))?;

        // Look up in standard squares
        result.push(STANDARD_SQUARE[row_a * 5 + col_a]);
        result.push(STANDARD_SQUARE[row_b * 5 + col_b]);
    }

    Ok(result)
}
