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

    let mut padded: Vec<char> = input.chars().collect();
    while !padded.len().is_multiple_of(key_len) {
        padded.push('X');
    }
    let num_rows = padded.len() / key_len;

    // Invert `order` into rank -> original column index.
    let mut col_at_rank = vec![0usize; key_len];
    for (col, &rank) in order.iter().enumerate() {
        col_at_rank[rank] = col;
    }

    let mut result = String::with_capacity(padded.len());
    for &col in &col_at_rank {
        for row in 0..num_rows {
            result.push(padded[row * key_len + col]);
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

    // `order[col]` is the rank at which column `col` appears in the ciphertext,
    // so its data occupies `chars[order[col] * num_rows .. (order[col]+1) * num_rows]`.
    let mut result = String::with_capacity(total_len);
    for row in 0..num_rows {
        for col in 0..key_len {
            result.push(chars[order[col] * num_rows + row]);
        }
    }

    Ok(result)
}
