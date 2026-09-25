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
const ADFGVX_BYTES: [u8; 6] = *b"ADFGVX";
const ADFGVX_INDEX: [u8; 256] = {
    let mut table = [0xFFu8; 256];
    table[b'A' as usize] = 0;
    table[b'D' as usize] = 1;
    table[b'F' as usize] = 2;
    table[b'G' as usize] = 3;
    table[b'V' as usize] = 4;
    table[b'X' as usize] = 5;
    table
};
const DEFAULT_GRID: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Maximum ADFGVX transposition-key length.
///
/// SECURITY: `--transposition-key` is a user-controlled allocation bound.
/// Encrypt/decrypt build `tk_len` columns plus rank vectors via `column_order`.
/// A tiny plaintext with a multi-million-character key would exhaust memory —
/// the same class of Denial of Service already capped in the columnar cipher.
const MAX_TRANSPOSITION_KEY_LEN: usize = 1_000_000;

fn check_transposition_key(transposition_key: &str) -> Result<()> {
    // Bound length before scanning characters so a huge invalid key cannot
    // burn CPU in `chars().all` before being rejected.
    if transposition_key.len() > MAX_TRANSPOSITION_KEY_LEN {
        anyhow::bail!(
            "Transposition key exceeds maximum length of {MAX_TRANSPOSITION_KEY_LEN} to prevent Denial of Service"
        );
    }
    if transposition_key.is_empty() || !transposition_key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Transposition key must be non-empty and contain only alphabetic characters");
    }
    Ok(())
}

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

    for &c in &grid {
        if !c.is_ascii_alphanumeric() {
            anyhow::bail!("Grid key must contain only letters A-Z and digits 0-9");
        }
    }

    Ok(grid)
}

fn columns_in_rank_order(order: &[usize]) -> Vec<usize> {
    let mut col_at_rank = vec![0usize; order.len()];
    for (col, &rank) in order.iter().enumerate() {
        col_at_rank[rank] = col;
    }
    col_at_rank
}

pub fn encrypt(input: &str, key: &str, transposition_key: &str) -> Result<String> {
    check_transposition_key(transposition_key)?;

    let grid = build_grid(key)?;
    let mut lut = [0xFFu8; 256];
    for (i, &c) in grid.iter().enumerate() {
        let b = c as u32;
        if b < 256 {
            lut[b as usize] = i as u8;
        }
    }

    let mut fractionated = Vec::with_capacity(input.len().saturating_mul(2));
    for c in input.to_uppercase().chars() {
        if !c.is_ascii_alphanumeric() {
            continue;
        }
        let idx = lut[c as usize];
        if idx == 0xFF {
            anyhow::bail!("Character '{c}' not in grid");
        }
        let idx = idx as usize;
        fractionated.push(ADFGVX_BYTES[idx / 6]);
        fractionated.push(ADFGVX_BYTES[idx % 6]);
    }

    if fractionated.is_empty() {
        return Ok(String::new());
    }

    let tk_len = transposition_key.len();
    let ranked = columns_in_rank_order(&column_order(transposition_key));
    let n = fractionated.len();
    let num_rows = n.div_ceil(tk_len);
    let mut result = Vec::with_capacity(n);
    for &col in &ranked {
        for row in 0..num_rows {
            let idx = row * tk_len + col;
            if idx < n {
                result.push(fractionated[idx]);
            }
        }
    }

    Ok(String::from_utf8(result).expect("ADFGVX ciphertext is ASCII"))
}

pub fn decrypt(input: &str, key: &str, transposition_key: &str) -> Result<String> {
    check_transposition_key(transposition_key)?;

    let grid = build_grid(key)?;
    let symbols: Vec<u8> = input
        .to_uppercase()
        .chars()
        .filter(|c| ADFGVX.contains(c))
        .map(|c| c as u8)
        .collect();

    if symbols.is_empty() {
        return Ok(String::new());
    }

    let tk_len = transposition_key.len();
    let total = symbols.len();
    if !total.is_multiple_of(2) {
        anyhow::bail!("Invalid ciphertext: fractionated text has odd length");
    }

    let num_rows = total.div_ceil(tk_len);
    let rem = total % tk_len;
    let full_cols = if rem == 0 { tk_len } else { rem };

    let ranked = columns_in_rank_order(&column_order(transposition_key));
    let mut starts = vec![0usize; tk_len];
    let mut lens = vec![0usize; tk_len];
    let mut pos = 0;
    for &col in &ranked {
        let col_len = if col < full_cols {
            num_rows
        } else {
            num_rows - 1
        };
        starts[col] = pos;
        lens[col] = col_len;
        pos += col_len;
    }

    let mut result = String::with_capacity(total / 2);
    let mut pending: Option<u8> = None;
    for row in 0..num_rows {
        for col in 0..tk_len {
            if row >= lens[col] {
                continue;
            }
            let symbol = symbols[starts[col] + row];
            match pending.take() {
                None => pending = Some(symbol),
                Some(first) => {
                    let row_i = ADFGVX_INDEX[first as usize];
                    let col_i = ADFGVX_INDEX[symbol as usize];
                    if row_i == 0xFF {
                        anyhow::bail!("Invalid ADFGVX character: {}", first as char);
                    }
                    if col_i == 0xFF {
                        anyhow::bail!("Invalid ADFGVX character: {}", symbol as char);
                    }
                    result.push(grid[row_i as usize * 6 + col_i as usize]);
                }
            }
        }
    }

    Ok(result)
}
