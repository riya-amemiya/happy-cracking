use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum SemaphoreAction {
    #[command(about = "Encode text to flag semaphore positions")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode flag semaphore positions to text")]
    Decode {
        #[arg(help = "Semaphore encoded string (e.g. 1-2 1-3 1-4)")]
        input: String,
    },
}

pub fn run(action: SemaphoreAction) -> Result<()> {
    match action {
        SemaphoreAction::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        SemaphoreAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// Positions: 1=S, 2=SW, 3=W, 4=NW, 5=N, 6=NE, 7=E, 8=SE
const SEMAPHORE_MAP: &[(char, &str)] = &[
    ('A', "1-2"),
    ('B', "1-3"),
    ('C', "1-4"),
    ('D', "1-5"),
    ('E', "1-6"),
    ('F', "1-7"),
    ('G', "1-8"),
    ('H', "2-3"),
    ('I', "2-4"),
    ('J', "5-6"),
    ('K', "3-1"),
    ('L', "3-2"),
    ('M', "3-4"),
    ('N', "3-5"),
    ('O', "3-6"),
    ('P', "3-7"),
    ('Q', "3-8"),
    ('R', "4-3"),
    ('S', "4-5"),
    ('T', "4-6"),
    ('U', "4-7"),
    ('V', "5-1"),
    ('W', "6-2"),
    ('X', "6-3"),
    ('Y', "6-4"),
    ('Z', "6-5"),
];

// Performance: [Option<&str>; 26] array indexed by (letter - 'A') replaces
// HashMap<char, &str> for encoding. Eliminates hashing overhead, bucket chasing,
// and key comparison — a direct O(1) array index vs amortized O(1) HashMap lookup.
const ENCODE_LUT: [Option<&str>; 26] = {
    let mut table: [Option<&str>; 26] = [None; 26];
    let mut i = 0;
    while i < SEMAPHORE_MAP.len() {
        let (ch, code) = SEMAPHORE_MAP[i];
        table[ch as usize - 'A' as usize] = Some(code);
        i += 1;
    }
    table
};

static SEMAPHORE_TO_CHAR: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| SEMAPHORE_MAP.iter().map(|&(c, code)| (code, c)).collect());

// Performance: build output directly into a pre-allocated String instead of
// collecting into Vec<&str> and joining. This avoids the intermediate Vec
// allocation and the second pass that join() performs to concatenate.
pub fn encode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    // Each semaphore code is 3 chars ("X-Y") + 1 space separator
    let mut result = String::with_capacity(input.len() * 4);
    let mut has_content = false;

    for b in input.bytes() {
        let upper = match b {
            b'a'..=b'z' => b - b'a',
            b'A'..=b'Z' => b - b'A',
            _ => continue,
        };
        if has_content {
            result.push(' ');
        }
        result.push_str(ENCODE_LUT[upper as usize].ok_or_else(|| {
            anyhow::anyhow!("No semaphore code for character: {}", upper as char)
        })?);
        has_content = true;
    }

    Ok(result)
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|code| {
            SEMAPHORE_TO_CHAR
                .get(code)
                .copied()
                .with_context(|| format!("Unknown semaphore code: {}", code))
        })
        .collect()
}
