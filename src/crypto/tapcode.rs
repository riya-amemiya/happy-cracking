use anyhow::Result;
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

const DOTS: [&str; 6] = ["", ".", "..", "...", "....", "....."];

pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 10);
    let mut first_word = true;

    for word in input.split_whitespace() {
        if !first_word {
            out.push_str(" / ");
        }
        first_word = false;

        let mut first_char = true;
        for c in word.bytes() {
            if !c.is_ascii_alphabetic() {
                continue;
            }
            let mut b = c.to_ascii_uppercase();
            if b == b'K' {
                b = b'C';
            }

            let idx = b - b'A';
            let (row, col) = match b {
                b'A'..=b'I' => (idx / 5 + 1, idx % 5 + 1),
                b'K'..=b'Z' => {
                    let shifted = idx - 1;
                    (shifted / 5 + 1, shifted % 5 + 1)
                }
                b'J' => (2, 5),
                _ => continue,
            };

            if !first_char {
                out.push_str("   ");
            }
            first_char = false;

            out.push_str(DOTS[row as usize]);
            out.push(' ');
            out.push_str(DOTS[col as usize]);
        }
    }
    out
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    let mut result = String::with_capacity(input.len() / 3);
    let mut first_word = true;

    for word in input.split(" / ") {
        if !first_word {
            result.push(' ');
        }
        first_word = false;

        for pair in word.split("   ").filter(|s| !s.is_empty()) {
            let mut parts = pair.split(' ').filter(|s| !s.is_empty());
            let row_str = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("Invalid tap code pair: {}", pair))?;
            let col_str = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("Invalid tap code pair: {}", pair))?;

            if parts.next().is_some() {
                anyhow::bail!("Invalid tap code pair: {}", pair);
            }

            let row = row_str.len();
            let col = col_str.len();

            if !(1..=5).contains(&row) || !(1..=5).contains(&col) {
                anyhow::bail!("Tap code values out of range (1-5): {}", pair);
            }
            if !row_str.chars().all(|c| c == '.') || !col_str.chars().all(|c| c == '.') {
                anyhow::bail!("Invalid tap code characters: {}", pair);
            }

            let idx = (row - 1) * 5 + (col - 1);
            result.push(GRID[idx]);
        }
    }

    Ok(result)
}
