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
        anyhow::bail!("Key exceeds maximum length of {MAX_KEY_LEN} to prevent Denial of Service");
    }

    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    if input.is_empty() {
        return Ok(String::new());
    }

    let key_len = key.len();
    let order = column_order(key);
    if input.is_ascii() {
        Ok(encrypt_ascii(input.as_bytes(), key_len, &order))
    } else {
        Ok(encrypt_chars(input, key_len, &order))
    }
}

fn col_at_rank(order: &[usize]) -> Vec<usize> {
    let mut rank_to_col = vec![0usize; order.len()];
    for (col, &rank) in order.iter().enumerate() {
        rank_to_col[rank] = col;
    }
    rank_to_col
}

fn encrypt_ascii(input: &[u8], key_len: usize, order: &[usize]) -> String {
    let mut padded = Vec::with_capacity(input.len() + key_len);
    padded.extend_from_slice(input);
    while !padded.len().is_multiple_of(key_len) {
        padded.push(b'X');
    }
    let num_rows = padded.len() / key_len;
    let rank_to_col = col_at_rank(order);

    let mut result = vec![0u8; padded.len()];
    for (rank, &col) in rank_to_col.iter().enumerate() {
        let dest = rank * num_rows;
        for row in 0..num_rows {
            result[dest + row] = padded[row * key_len + col];
        }
    }
    String::from_utf8(result).expect("columnar encrypt of ASCII input stays ASCII")
}

fn encrypt_chars(input: &str, key_len: usize, order: &[usize]) -> String {
    let mut padded: Vec<char> = input.chars().collect();
    while !padded.len().is_multiple_of(key_len) {
        padded.push('X');
    }
    let num_rows = padded.len() / key_len;
    let rank_to_col = col_at_rank(order);

    let mut result = String::with_capacity(padded.len());
    for &col in &rank_to_col {
        for row in 0..num_rows {
            result.push(padded[row * key_len + col]);
        }
    }
    result
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.len() > MAX_KEY_LEN {
        anyhow::bail!("Key exceeds maximum length of {MAX_KEY_LEN} to prevent Denial of Service");
    }

    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    if input.is_empty() {
        return Ok(String::new());
    }

    let key_len = key.len();
    let order = column_order(key);
    if input.is_ascii() {
        decrypt_ascii(input.as_bytes(), key_len, &order)
    } else {
        decrypt_chars(input, key_len, &order)
    }
}

fn decrypt_ascii(input: &[u8], key_len: usize, order: &[usize]) -> Result<String> {
    let total_len = input.len();
    if !total_len.is_multiple_of(key_len) {
        anyhow::bail!("Ciphertext length must be a multiple of key length");
    }

    let num_rows = total_len / key_len;
    let mut result = vec![0u8; total_len];
    for row in 0..num_rows {
        let dest = row * key_len;
        for col in 0..key_len {
            result[dest + col] = input[order[col] * num_rows + row];
        }
    }
    Ok(String::from_utf8(result).expect("columnar decrypt of ASCII input stays ASCII"))
}

fn decrypt_chars(input: &str, key_len: usize, order: &[usize]) -> Result<String> {
    let chars: Vec<char> = input.chars().collect();
    let total_len = chars.len();

    if !total_len.is_multiple_of(key_len) {
        anyhow::bail!("Ciphertext length must be a multiple of key length");
    }

    let num_rows = total_len / key_len;
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
