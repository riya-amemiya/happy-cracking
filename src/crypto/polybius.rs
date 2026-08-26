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

pub fn encrypt(input: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(input.len() * 3);

    for b in input.bytes() {
        if !b.is_ascii_alphabetic() {
            continue;
        }

        let mut b = b.to_ascii_uppercase();
        if b == b'J' {
            b = b'I';
        }

        let idx = b - b'A';
        let (row, col) = match b {
            b'A'..=b'I' => (idx / 5 + 1, idx % 5 + 1),
            b'K'..=b'Z' => {
                let shifted_idx = idx - 1; // Since J is skipped
                (shifted_idx / 5 + 1, shifted_idx % 5 + 1)
            }
            _ => continue,
        };

        if !bytes.is_empty() {
            bytes.push(b' ');
        }
        bytes.push(b'0' + row);
        bytes.push(b'0' + col);
    }

    if bytes.is_empty() && input.chars().any(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Failed to encrypt input");
    }

    Ok(String::from_utf8(bytes).expect("polybius encrypt produces ASCII digits and spaces"))
}

pub fn decrypt(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::with_capacity(input.len() / 3);
    for token in input.split_whitespace() {
        let bytes = token.as_bytes();
        if bytes.len() != 2 {
            anyhow::bail!("Invalid Polybius pair: {}", token);
        }

        let r_byte = bytes[0];
        let c_byte = bytes[1];

        if !(b'1'..=b'5').contains(&r_byte) || !(b'1'..=b'5').contains(&c_byte) {
            if !r_byte.is_ascii_digit() || !c_byte.is_ascii_digit() {
                anyhow::bail!("Invalid digit in pair: {}", token);
            }
            anyhow::bail!("Polybius coordinates out of range (1-5): {}", token);
        }

        let row = (r_byte - b'0') as usize;
        let col = (c_byte - b'0') as usize;

        let idx = (row - 1) * 5 + (col - 1);
        result.push(GRID[idx]);
    }

    Ok(result)
}
