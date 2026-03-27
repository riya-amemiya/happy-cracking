use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum HillAction {
    #[command(about = "Encrypt with Hill cipher")]
    Encrypt {
        #[arg(help = "Input text (uppercase alphabetic)")]
        input: String,
        #[arg(
            short,
            long,
            help = "Key matrix as space-separated digits (e.g., \"6 24 1 13 16 10 20 17 15\" for 3x3)"
        )]
        key: String,
    },
    #[command(about = "Decrypt Hill cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(
            short,
            long,
            help = "Key matrix as space-separated digits (e.g., \"6 24 1 13 16 10 20 17 15\" for 3x3)"
        )]
        key: String,
    },
}

pub fn run(action: HillAction) -> Result<()> {
    match action {
        HillAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        HillAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

fn parse_key_matrix(key: &str) -> Result<(Vec<i64>, usize)> {
    let values: Vec<i64> = key
        .split_whitespace()
        .map(|s| s.parse::<i64>().context("Invalid number in key matrix"))
        .collect::<Result<Vec<_>>>()?;

    let n = (values.len() as f64).sqrt() as usize;
    if n * n != values.len() || !(2..=3).contains(&n) {
        anyhow::bail!("Key matrix must be 2x2 (4 values) or 3x3 (9 values)");
    }

    Ok((values, n))
}

fn mod26(x: i64) -> i64 {
    x.rem_euclid(26)
}

fn mod_inverse_26(a: i64) -> Option<i64> {
    let a = mod26(a);
    (1..26).find(|&x| mod26(a * x) == 1)
}

fn det_2x2(m: &[i64]) -> i64 {
    m[0] * m[3] - m[1] * m[2]
}

fn det_3x3(m: &[i64]) -> i64 {
    m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6])
}

fn inverse_matrix_2x2(m: &[i64]) -> Result<Vec<i64>> {
    let det = mod26(det_2x2(m));
    let det_inv = mod_inverse_26(det)
        .ok_or_else(|| anyhow::anyhow!("Key matrix is not invertible mod 26"))?;

    Ok(vec![
        mod26(det_inv * m[3]),
        mod26(det_inv * (-m[1])),
        mod26(det_inv * (-m[2])),
        mod26(det_inv * m[0]),
    ])
}

fn cofactor_3x3(m: &[i64], row: usize, col: usize) -> i64 {
    // Use fixed-size array to avoid heap allocation
    let mut sub = [0i64; 4];
    let mut idx = 0;
    for r in 0..3 {
        for c in 0..3 {
            if r != row && c != col {
                sub[idx] = m[r * 3 + c];
                idx += 1;
            }
        }
    }
    let minor = sub[0] * sub[3] - sub[1] * sub[2];
    if (row + col).is_multiple_of(2) {
        minor
    } else {
        -minor
    }
}

fn inverse_matrix_3x3(m: &[i64]) -> Result<Vec<i64>> {
    let det = mod26(det_3x3(m));
    let det_inv = mod_inverse_26(det)
        .ok_or_else(|| anyhow::anyhow!("Key matrix is not invertible mod 26"))?;

    // Adjugate matrix (transpose of cofactor matrix)
    let mut inv = vec![0i64; 9];
    for r in 0..3 {
        for c in 0..3 {
            // Transpose: inv[c][r] = cofactor(r, c)
            inv[c * 3 + r] = mod26(det_inv * cofactor_3x3(m, r, c));
        }
    }

    Ok(inv)
}

fn inverse_matrix(m: &[i64], n: usize) -> Result<Vec<i64>> {
    match n {
        2 => inverse_matrix_2x2(m),
        3 => inverse_matrix_3x3(m),
        _ => anyhow::bail!("Only 2x2 and 3x3 matrices are supported"),
    }
}

fn multiply_matrix_vector(m: &[i64], v: &[i64], n: usize) -> Vec<i64> {
    (0..n)
        .map(|r| mod26((0..n).map(|c| m[r * n + c] * v[c]).sum::<i64>()))
        .collect()
}

fn prepare_input(input: &str, n: usize) -> Vec<i64> {
    let mut values: Vec<i64> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .map(|c| (c as u8 - b'A') as i64)
        .collect();

    // Pad with 'X' (23) if needed
    while !values.len().is_multiple_of(n) {
        values.push(23); // 'X'
    }

    values
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    let (matrix, n) = parse_key_matrix(key)?;

    if !input.chars().any(|c| c.is_ascii_alphabetic()) {
        return Ok(String::new());
    }

    let values = prepare_input(input, n);

    let result: String = values
        .chunks(n)
        .flat_map(|chunk| {
            multiply_matrix_vector(&matrix, chunk, n)
                .into_iter()
                .map(|v| (v as u8 + b'A') as char)
        })
        .collect();

    Ok(result)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    let (matrix, n) = parse_key_matrix(key)?;
    let inv = inverse_matrix(&matrix, n)?;

    if !input.chars().any(|c| c.is_ascii_alphabetic()) {
        return Ok(String::new());
    }

    let values: Vec<i64> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .map(|c| (c as u8 - b'A') as i64)
        .collect();

    if !values.len().is_multiple_of(n) {
        anyhow::bail!(
            "Ciphertext length must be a multiple of matrix dimension ({})",
            n
        );
    }

    let result: String = values
        .chunks(n)
        .flat_map(|chunk| {
            multiply_matrix_vector(&inv, chunk, n)
                .into_iter()
                .map(|v| (v as u8 + b'A') as char)
        })
        .collect();

    Ok(result)
}
