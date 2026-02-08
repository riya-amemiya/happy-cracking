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
    let c = if c == 'J' { 'I' } else { c };
    GRID.iter()
        .position(|&g| g == c)
        .map(|idx| (idx / 5 + 1, idx % 5 + 1))
}

pub fn encrypt(input: &str) -> Result<String> {
    let pairs: Vec<String> = input
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase())
        .filter_map(|c| char_to_position(c).map(|(row, col)| format!("{}{}", row, col)))
        .collect();

    if pairs.is_empty() && input.chars().any(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Failed to encrypt input");
    }

    Ok(pairs.join(" "))
}

pub fn decrypt(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    for token in input.split_whitespace() {
        if token.len() != 2 {
            anyhow::bail!("Invalid Polybius pair: {}", token);
        }

        let digits: Vec<usize> = token
            .chars()
            .map(|c| {
                c.to_digit(10)
                    .map(|d| d as usize)
                    .ok_or_else(|| anyhow::anyhow!("Invalid digit in pair: {}", token))
            })
            .collect::<Result<Vec<_>>>()?;

        let row = digits[0];
        let col = digits[1];

        if !(1..=5).contains(&row) || !(1..=5).contains(&col) {
            anyhow::bail!("Polybius coordinates out of range (1-5): {}", token);
        }

        let idx = (row - 1) * 5 + (col - 1);
        result.push(GRID[idx]);
    }

    Ok(result)
}
