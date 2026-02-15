use anyhow::Result;
use clap::Subcommand;

use super::shared::column_order;

#[derive(Subcommand)]
pub enum AdfgvxAction {
    #[command(about = "Encrypt with ADFGVX cipher")]
    Encrypt {
        #[arg(help = "Input text (letters and digits)")]
        input: String,
        #[arg(
            short,
            long,
            help = "36-character key for 6x6 grid (permutation of A-Z0-9), or \"default\""
        )]
        key: String,
        #[arg(short, long, help = "Keyword for columnar transposition")]
        transposition_key: String,
    },
    #[command(about = "Decrypt ADFGVX cipher")]
    Decrypt {
        #[arg(help = "Encrypted text (ADFGVX characters)")]
        input: String,
        #[arg(
            short,
            long,
            help = "36-character key for 6x6 grid (permutation of A-Z0-9), or \"default\""
        )]
        key: String,
        #[arg(short, long, help = "Keyword for columnar transposition")]
        transposition_key: String,
    },
}

pub fn run(action: AdfgvxAction) -> Result<()> {
    match action {
        AdfgvxAction::Encrypt {
            input,
            key,
            transposition_key,
        } => {
            println!("{}", encrypt(&input, &key, &transposition_key)?);
        }
        AdfgvxAction::Decrypt {
            input,
            key,
            transposition_key,
        } => {
            println!("{}", decrypt(&input, &key, &transposition_key)?);
        }
    }
    Ok(())
}

const ADFGVX: [char; 6] = ['A', 'D', 'F', 'G', 'V', 'X'];
const DEFAULT_GRID: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

fn build_grid(key: &str) -> Result<Vec<char>> {
    let grid_str = if key.eq_ignore_ascii_case("default") {
        DEFAULT_GRID.to_string()
    } else {
        key.to_uppercase()
    };

    let grid: Vec<char> = grid_str.chars().collect();
    if grid.len() != 36 {
        anyhow::bail!("Grid key must be exactly 36 characters (A-Z and 0-9)");
    }

    // Verify all characters are alphanumeric
    for &c in &grid {
        if !c.is_ascii_alphanumeric() {
            anyhow::bail!("Grid key must contain only letters A-Z and digits 0-9");
        }
    }

    Ok(grid)
}

fn find_in_grid(grid: &[char], c: char) -> Option<(usize, usize)> {
    grid.iter()
        .position(|&g| g == c)
        .map(|idx| (idx / 6, idx % 6))
}

pub fn encrypt(input: &str, key: &str, transposition_key: &str) -> Result<String> {
    if transposition_key.is_empty() || !transposition_key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Transposition key must be non-empty and contain only alphabetic characters");
    }

    let grid = build_grid(key)?;

    // Step 1: substitute each character with ADFGVX pair
    let mut fractionated = String::new();
    for c in input.to_uppercase().chars() {
        if !c.is_ascii_alphanumeric() {
            continue;
        }
        let (row, col) = find_in_grid(&grid, c)
            .ok_or_else(|| anyhow::anyhow!("Character '{}' not in grid", c))?;
        fractionated.push(ADFGVX[row]);
        fractionated.push(ADFGVX[col]);
    }

    if fractionated.is_empty() {
        return Ok(String::new());
    }

    // Step 2: columnar transposition
    let tk_len = transposition_key.len();
    let order = column_order(transposition_key);
    let chars: Vec<char> = fractionated.chars().collect();

    // Fill grid row by row into columns
    let num_rows = chars.len().div_ceil(tk_len);
    let mut columns: Vec<Vec<char>> = vec![Vec::with_capacity(num_rows); tk_len];
    for row in 0..num_rows {
        for (col, column) in columns.iter_mut().enumerate() {
            let idx = row * tk_len + col;
            if idx < chars.len() {
                column.push(chars[idx]);
            }
        }
    }

    // Read columns in key order
    let mut sorted_cols: Vec<usize> = (0..tk_len).collect();
    sorted_cols.sort_by_key(|&col| order[col]);

    let mut result = String::new();
    for &col in &sorted_cols {
        for &c in &columns[col] {
            result.push(c);
        }
    }

    Ok(result)
}

pub fn decrypt(input: &str, key: &str, transposition_key: &str) -> Result<String> {
    if transposition_key.is_empty() || !transposition_key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Transposition key must be non-empty and contain only alphabetic characters");
    }

    let grid = build_grid(key)?;
    let adfgvx_chars: Vec<char> = input
        .to_uppercase()
        .chars()
        .filter(|c| ADFGVX.contains(c))
        .collect();

    if adfgvx_chars.is_empty() {
        return Ok(String::new());
    }

    // Step 1: reverse columnar transposition
    let tk_len = transposition_key.len();
    let total = adfgvx_chars.len();
    let num_rows = total.div_ceil(tk_len);
    let full_cols = total % tk_len;
    // If total is exactly divisible, all columns are full
    let full_cols = if full_cols == 0 { tk_len } else { full_cols };

    let order = column_order(transposition_key);

    // Determine the length of each column in sorted order
    let mut sorted_cols: Vec<usize> = (0..tk_len).collect();
    sorted_cols.sort_by_key(|&col| order[col]);

    let mut columns: Vec<Vec<char>> = vec![Vec::new(); tk_len];
    let mut pos = 0;
    for &col in &sorted_cols {
        let col_len = if col < full_cols {
            num_rows
        } else {
            num_rows - 1
        };
        columns[col] = adfgvx_chars[pos..pos + col_len].to_vec();
        pos += col_len;
    }

    // Read row by row to get the fractionated string
    let mut fractionated = String::new();
    for row in 0..num_rows {
        for column in &columns {
            if row < column.len() {
                fractionated.push(column[row]);
            }
        }
    }

    // Step 2: convert ADFGVX pairs back to characters
    let frac_chars: Vec<char> = fractionated.chars().collect();
    if !frac_chars.len().is_multiple_of(2) {
        anyhow::bail!("Invalid ciphertext: fractionated text has odd length");
    }

    let mut result = String::new();
    for pair in frac_chars.chunks(2) {
        let row = ADFGVX
            .iter()
            .position(|&c| c == pair[0])
            .ok_or_else(|| anyhow::anyhow!("Invalid ADFGVX character: {}", pair[0]))?;
        let col = ADFGVX
            .iter()
            .position(|&c| c == pair[1])
            .ok_or_else(|| anyhow::anyhow!("Invalid ADFGVX character: {}", pair[1]))?;
        result.push(grid[row * 6 + col]);
    }

    Ok(result)
}
