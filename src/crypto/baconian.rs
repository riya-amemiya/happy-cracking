use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BaconianAction {
    #[command(about = "Encode text using Baconian cipher")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode Baconian cipher")]
    Decode {
        #[arg(help = "Baconian encoded text (A/B patterns, space-separated)")]
        input: String,
    },
}

pub fn run(action: BaconianAction) -> Result<()> {
    match action {
        BaconianAction::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        BaconianAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// Bacon's alphabet: I/J share index 8, U/V share index 20
fn char_to_bacon_index(c: char) -> Option<u8> {
    match c.to_ascii_uppercase() {
        'A' => Some(0),
        'B' => Some(1),
        'C' => Some(2),
        'D' => Some(3),
        'E' => Some(4),
        'F' => Some(5),
        'G' => Some(6),
        'H' => Some(7),
        'I' | 'J' => Some(8),
        'K' => Some(9),
        'L' => Some(10),
        'M' => Some(11),
        'N' => Some(12),
        'O' => Some(13),
        'P' => Some(14),
        'Q' => Some(15),
        'R' => Some(16),
        'S' => Some(17),
        'T' => Some(18),
        'U' | 'V' => Some(19),
        'W' => Some(20),
        'X' => Some(21),
        'Y' => Some(22),
        'Z' => Some(23),
        _ => None,
    }
}

fn bacon_index_to_char(idx: u8) -> Option<char> {
    match idx {
        0 => Some('A'),
        1 => Some('B'),
        2 => Some('C'),
        3 => Some('D'),
        4 => Some('E'),
        5 => Some('F'),
        6 => Some('G'),
        7 => Some('H'),
        8 => Some('I'),
        9 => Some('K'),
        10 => Some('L'),
        11 => Some('M'),
        12 => Some('N'),
        13 => Some('O'),
        14 => Some('P'),
        15 => Some('Q'),
        16 => Some('R'),
        17 => Some('S'),
        18 => Some('T'),
        19 => Some('U'),
        20 => Some('W'),
        21 => Some('X'),
        22 => Some('Y'),
        23 => Some('Z'),
        _ => None,
    }
}

const BACON_PATTERNS: [&str; 24] = [
    "AAAAA", // 0  = A
    "AAAAB", // 1  = B
    "AAABA", // 2  = C
    "AAABB", // 3  = D
    "AABAA", // 4  = E
    "AABAB", // 5  = F
    "AABBA", // 6  = G
    "AABBB", // 7  = H
    "ABAAA", // 8  = I/J
    "ABAAB", // 9  = K
    "ABABA", // 10 = L
    "ABABB", // 11 = M
    "ABBAA", // 12 = N
    "ABBAB", // 13 = O
    "ABBBA", // 14 = P
    "ABBBB", // 15 = Q
    "BAAAA", // 16 = R
    "BAAAB", // 17 = S
    "BAABA", // 18 = T
    "BAABB", // 19 = U/V
    "BABAA", // 20 = W
    "BABAB", // 21 = X
    "BABBA", // 22 = Y
    "BABBB", // 23 = Z
];

fn pattern_to_index(pattern: &str) -> Option<u8> {
    if pattern.len() != 5 {
        return None;
    }
    let mut idx: u8 = 0;
    for c in pattern.chars() {
        idx <<= 1;
        match c.to_ascii_uppercase() {
            'A' => {}
            'B' => idx |= 1,
            _ => return None,
        }
    }
    if idx > 23 {
        return None;
    }
    Some(idx)
}

pub fn encode(input: &str) -> Result<String> {
    let mut out = String::with_capacity(input.len() * 6);
    let mut has_content = false;

    for c in input.chars() {
        if let Some(idx) = char_to_bacon_index(c) {
            if has_content {
                out.push(' ');
            }
            out.push_str(BACON_PATTERNS[idx as usize]);
            has_content = true;
        }
    }

    if !has_content && input.chars().any(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Failed to encode input");
    }

    Ok(out)
}

pub fn decode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    for token in input.split_whitespace() {
        let idx = pattern_to_index(token)
            .ok_or_else(|| anyhow::anyhow!("Invalid Baconian pattern: {token}"))?;
        let c = bacon_index_to_char(idx)
            .ok_or_else(|| anyhow::anyhow!("Invalid Baconian index: {idx}"))?;
        result.push(c);
    }

    Ok(result)
}
