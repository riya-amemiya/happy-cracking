use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TapcodeAction {
    #[command(about = "Encode text to tap code")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode tap code to text")]
    Decode {
        #[arg(help = "Tap code (dots separated by spaces, words separated by /)")]
        input: String,
    },
}

pub fn run(action: TapcodeAction) -> Result<()> {
    match action {
        TapcodeAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        TapcodeAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// 5x5 grid: A B C/K D E / F G H I J / L M N O P / Q R S T U / V W X Y Z
const GRID: [char; 25] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T',
    'U', 'V', 'W', 'X', 'Y', 'Z',
];

fn char_to_taps(c: char) -> Option<(usize, usize)> {
    let c = if c == 'K' { 'C' } else { c };
    GRID.iter()
        .position(|&g| g == c)
        .map(|idx| (idx / 5 + 1, idx % 5 + 1))
}

fn taps_to_dots(row: usize, col: usize) -> String {
    format!("{} {}", ".".repeat(row), ".".repeat(col))
}

pub fn encode(input: &str) -> String {
    input
        .to_uppercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_ascii_alphabetic())
                .filter_map(|c| char_to_taps(c).map(|(row, col)| taps_to_dots(row, col)))
                .collect::<Vec<_>>()
                .join("   ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    input
        .split(" / ")
        .map(|word| {
            word.split("   ")
                .filter(|s| !s.is_empty())
                .map(|pair| {
                    let parts: Vec<&str> = pair.split(' ').filter(|s| !s.is_empty()).collect();
                    if parts.len() != 2 {
                        anyhow::bail!("Invalid tap code pair: {}", pair);
                    }
                    let row = parts[0].len();
                    let col = parts[1].len();
                    if !(1..=5).contains(&row) || !(1..=5).contains(&col) {
                        anyhow::bail!("Tap code values out of range (1-5): {}", pair);
                    }
                    if !parts[0].chars().all(|c| c == '.') || !parts[1].chars().all(|c| c == '.') {
                        anyhow::bail!("Invalid tap code characters: {}", pair);
                    }
                    let idx = (row - 1) * 5 + (col - 1);
                    Ok(GRID[idx])
                })
                .collect::<Result<String>>()
        })
        .collect::<Result<Vec<_>>>()
        .map(|words| words.join(" "))
        .context("Failed to decode tap code")
}
