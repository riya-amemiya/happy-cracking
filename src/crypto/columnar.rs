use anyhow::Result;
use clap::Subcommand;

use super::shared::column_order;

#[derive(Subcommand)]
pub enum ColumnarAction {
    #[command(about = "Encrypt with Columnar Transposition cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Keyword for column ordering")]
        key: String,
    },
    #[command(about = "Decrypt Columnar Transposition cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Keyword for column ordering")]
        key: String,
    },
}

pub fn run(action: ColumnarAction) -> Result<()> {
    match action {
        ColumnarAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        ColumnarAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

// Safety limit to prevent massive memory allocation (DoS).
// If a user provides a tiny input but a massive key (e.g. 100 million chars),
// the padding logic would allocate 100 million 'X's.
const MAX_KEY_LEN: usize = 1_000_000;

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    if key.len() > MAX_KEY_LEN {
        anyhow::bail!(
            "Key exceeds maximum length of {} to prevent Denial of Service",
            MAX_KEY_LEN
        );
    }

    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    if input.is_empty() {
        return Ok(String::new());
    }

    let key_len = key.len();
    let order = column_order(key);

    // Pad input with 'X' to fill the grid
    let mut padded: Vec<char> = input.chars().collect();
    while !padded.len().is_multiple_of(key_len) {
        padded.push('X');
    }

    // Build grid row by row
    let grid: Vec<Vec<char>> = padded.chunks(key_len).map(|row| row.to_vec()).collect();

    // Read columns in key order
    let mut sorted_cols: Vec<usize> = (0..key_len).collect();
    sorted_cols.sort_by_key(|&col| order[col]);

    let mut result = String::new();
    for &col in &sorted_cols {
        for row in &grid {
            result.push(row[col]);
        }
    }

    Ok(result)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.len() > MAX_KEY_LEN {
        anyhow::bail!(
            "Key exceeds maximum length of {} to prevent Denial of Service",
            MAX_KEY_LEN
        );
    }

    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    if input.is_empty() {
        return Ok(String::new());
    }

    let key_len = key.len();
    let chars: Vec<char> = input.chars().collect();
    let total_len = chars.len();

    if !total_len.is_multiple_of(key_len) {
        anyhow::bail!("Ciphertext length must be a multiple of key length");
    }

    let num_rows = total_len / key_len;
    let order = column_order(key);

    // Determine the order in which columns appear in ciphertext
    let mut sorted_cols: Vec<usize> = (0..key_len).collect();
    sorted_cols.sort_by_key(|&col| order[col]);

    // Fill columns in key order
    let mut columns: Vec<Vec<char>> = vec![Vec::new(); key_len];
    let mut pos = 0;
    for &col in &sorted_cols {
        columns[col] = chars[pos..pos + num_rows].to_vec();
        pos += num_rows;
    }

    // Read row by row
    let mut result = String::new();
    for row in 0..num_rows {
        for column in &columns {
            result.push(column[row]);
        }
    }

    Ok(result)
}
