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
// A=0, B=1, C=2, D=3, E=4, F=5, G=6, H=7, I/J=8, K=9, L=10,
// M=11, N=12, O=13, P=14, Q=15, R=16, S=17, T=18, U/V=19, W=20,
// X=21, Y=22, Z=23
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

// Pre-computed Baconian patterns for indices 0-23.
// Avoids allocating a new String per character via iterator .collect().
const BACON_PATTERNS: [&str; 24] = [
    "AAAAA", "AAAAB", "AAABA", "AAABB", "AABAA", "AABAB", "AABBA", "AABBB",
    "ABAAA", "ABAAB", "ABABA", "ABABB", "ABBAA", "ABBAB", "ABBBA", "ABBBB",
    "BAAAA", "BAAAB", "BAABA", "BAABB", "BABAA", "BABAB", "BABBA", "BABBB",
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
    let alpha_count = input.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if alpha_count == 0 {
        if input.chars().any(|c| c.is_ascii_alphabetic()) {
            anyhow::bail!("Failed to encode input");
        }
        return Ok(String::new());
    }

    // Pre-allocate: each letter becomes 5 chars + 1 space separator (except last)
    let mut result = String::with_capacity(alpha_count * 6);
    let mut first = true;

    for c in input.chars() {
        if let Some(idx) = char_to_bacon_index(c) {
            if !first {
                result.push(' ');
            }
            // Use pre-computed lookup table instead of allocating a String per character
            result.push_str(BACON_PATTERNS[idx as usize]);
            first = false;
        }
    }

    Ok(result)
}

pub fn decode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    for token in input.split_whitespace() {
        let idx = pattern_to_index(token)
            .ok_or_else(|| anyhow::anyhow!("Invalid Baconian pattern: {}", token))?;
        let c = bacon_index_to_char(idx)
            .ok_or_else(|| anyhow::anyhow!("Invalid Baconian index: {}", idx))?;
        result.push(c);
    }

    Ok(result)
}
