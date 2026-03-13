use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum PolybiusAction {
    #[command(about = "Encrypt with Polybius square")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decrypt Polybius square")]
    Decrypt {
        #[arg(help = "Polybius encoded text (number pairs, space-separated)")]
        input: String,
    },
}

pub fn run(action: PolybiusAction) -> Result<()> {
    match action {
        PolybiusAction::Encrypt { input } => {
            println!("{}", encrypt(&input)?);
        }
        PolybiusAction::Decrypt { input } => {
            println!("{}", decrypt(&input)?);
        }
    }
    Ok(())
}

// Default 5x5 grid: ABCDEFGHIKLMNOPQRSTUVWXYZ (I/J merged)
const GRID: [char; 25] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    'U', 'V', 'W', 'X', 'Y', 'Z',
];

fn char_to_position(c: char) -> Option<(usize, usize)> {
    if !c.is_ascii_uppercase() {
        return None;
    }
    let b = c as u8;

    // Optimization: Calculate grid index using direct byte arithmetic instead of
    // linear array search over GRID to eliminate lookup overhead.
    let idx = match b {
        b'A'..=b'I' => (b - b'A') as usize,
        b'J' => (b'I' - b'A') as usize, // J merged with I
        b'K'..=b'Z' => (b - b'A' - 1) as usize, // Shifted by 1 due to missing J
        _ => return None,
    };

    Some((idx / 5 + 1, idx % 5 + 1))
}

pub fn encrypt(input: &str) -> Result<String> {
    // Optimization: Pre-allocate String to prevent multiple dynamic allocations.
    // Each character produces "RC " (3 bytes).
    let mut result = String::with_capacity(input.len() * 3);
    let mut encrypted_count = 0;

    // Optimization: Process by bytes to bypass multi-byte unicode decoding overhead.
    // ASCII alphabetic check inherently skips non-ASCII UTF-8 sequences safely.
    for b in input.bytes() {
        if b.is_ascii_alphabetic() {
            let upper_c = b.to_ascii_uppercase() as char;
            if let Some((row, col)) = char_to_position(upper_c) {
                // Optimization: use unsafe and manual push rather than format! inside hot loop.
                result.push((b'0' + row as u8) as char);
                result.push((b'0' + col as u8) as char);
                result.push(' ');
                encrypted_count += 1;
            }
        }
    }

    if encrypted_count == 0 && input.bytes().any(|b| b.is_ascii_alphabetic()) {
        anyhow::bail!("Failed to encrypt input");
    }

    if !result.is_empty() {
        result.pop(); // Remove the trailing space
    }

    Ok(result)
}

pub fn decrypt(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::with_capacity(input.len() / 3 + 1);

    // Optimization: Avoid allocating temporary Vectors for parsed digits.
    // Process input token's bytes directly and do simple ascii byte arithmetic.
    for token in input.split_whitespace() {
        if token.len() != 2 {
            anyhow::bail!("Invalid Polybius pair: {}", token);
        }

        let b_row = token.as_bytes()[0];
        let b_col = token.as_bytes()[1];

        if !b_row.is_ascii_digit() || !b_col.is_ascii_digit() {
             anyhow::bail!("Invalid digit in pair: {}", token);
        }

        let row = (b_row - b'0') as usize;
        let col = (b_col - b'0') as usize;

        if !(1..=5).contains(&row) || !(1..=5).contains(&col) {
            anyhow::bail!("Polybius coordinates out of range (1-5): {}", token);
        }

        let idx = (row - 1) * 5 + (col - 1);
        result.push(GRID[idx]);
    }

    Ok(result)
}
