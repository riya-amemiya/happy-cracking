use anyhow::Result;
use clap::Subcommand;

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

fn column_order(key: &str) -> Vec<usize> {
    let key_upper: Vec<char> = key.to_uppercase().chars().collect();
    let mut indices: Vec<usize> = (0..key_upper.len()).collect();
    indices.sort_by(|&a, &b| key_upper[a].cmp(&key_upper[b]).then(a.cmp(&b)));

    let mut order = vec![0; key_upper.len()];
    for (rank, &idx) in indices.iter().enumerate() {
        order[idx] = rank;
    }
    order
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
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
