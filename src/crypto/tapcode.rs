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
    // Optimization: Avoid `format!`, multiple iterations, and `Vec<String>::join`
    let mut result = String::with_capacity(input.len() * 10);
    let mut first_word = true;

    for word in input.split_whitespace() {
        if !first_word {
            result.push_str(" / ");
        }
        first_word = false;

        let mut first_char = true;

        for b in word.bytes() {
            if !b.is_ascii_alphabetic() {
                continue;
            }

            let mut b_adj = b.to_ascii_uppercase();
            if b_adj == b'K' {
                b_adj = b'C';
            }

            let idx = b_adj - b'A';
            let (row, col) = match b_adj {
                b'A'..=b'J' => (idx / 5 + 1, idx % 5 + 1),
                b'L'..=b'Z' => {
                    let shifted_idx = idx - 1; // Since K is skipped
                    (shifted_idx / 5 + 1, shifted_idx % 5 + 1)
                }
                _ => continue,
            };

            if !first_char {
                result.push_str("   ");
            }
            first_char = false;

            // Precomputed dot sequences avoid runtime string allocation and format overhead
            static DOTS: [[&str; 5]; 5] = [
                [". .", ". ..", ". ...", ". ....", ". ....."],
                [".. .", ".. ..", ".. ...", ".. ....", ".. ....."],
                ["... .", "... ..", "... ...", "... ....", "... ....."],
                [".... .", ".... ..", ".... ...", ".... ....", ".... ....."],
                ["..... .", "..... ..", "..... ...", "..... ....", "..... ....."],
            ];

            result.push_str(DOTS[(row - 1) as usize][(col - 1) as usize]);
        }
    }

    result
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    // Optimization: Pre-allocate result to avoid multiple `Vec` and `String` allocations,
    // and manually track dots instead of `split`, `filter`, and `.collect::<Vec<_>>().join(" ")`.
    let mut result = String::with_capacity(input.len() / 3);
    let mut first_word = true;

    for word in input.split(" / ") {
        if !first_word {
            result.push(' ');
        }
        first_word = false;

        for pair in word.split("   ") {
            if pair.is_empty() {
                continue;
            }

            let mut row = 0;
            let mut col = 0;
            let mut parts = 0;
            let mut last_was_dot = false;

            for &b in pair.as_bytes() {
                if b == b'.' {
                    if !last_was_dot {
                        parts += 1;
                    }
                    if parts == 1 {
                        row += 1;
                    } else if parts == 2 {
                        col += 1;
                    } else {
                        anyhow::bail!("Invalid tap code pair: {}", pair);
                    }
                    last_was_dot = true;
                } else if b == b' ' {
                    last_was_dot = false;
                } else {
                    anyhow::bail!("Invalid tap code characters: {}", pair);
                }
            }

            if parts != 2 {
                anyhow::bail!("Invalid tap code pair: {}", pair);
            }

            if row == 0 || col == 0 || row > 5 || col > 5 {
                anyhow::bail!("Tap code values out of range (1-5): {}", pair);
            }

            let idx = (row - 1) * 5 + (col - 1);
            result.push(GRID[idx]);
        }
    }

    Ok(result)
}
