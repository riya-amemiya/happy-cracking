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

pub fn encode(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    // Optimization: Pre-allocate output buffer, manually construct dots
    // and avoid intermediate String/Vec allocations + format! + join
    let mut out = String::with_capacity(input.len() * 15);
    let mut first_word = true;

    for word in input.split_whitespace() {
        let mut word_str = String::new();
        let mut first_char = true;

        for c in word.chars() {
            if c.is_ascii_alphabetic() {
                let mut b = c.to_ascii_uppercase() as u8;
                if b == b'K' {
                    b = b'C';
                }

                let idx = if b >= b'L' { b - b'A' - 1 } else { b - b'A' };

                let row = (idx / 5 + 1) as usize;
                let col = (idx % 5 + 1) as usize;

                if !first_char {
                    word_str.push_str("   ");
                }
                first_char = false;

                for _ in 0..row {
                    word_str.push('.');
                }
                word_str.push(' ');
                for _ in 0..col {
                    word_str.push('.');
                }
            }
        }

        if !first_word {
            out.push_str(" / ");
        }
        first_word = false;
        out.push_str(&word_str);
    }
    out
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
