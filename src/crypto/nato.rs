use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

// Performance: direct ASCII-indexed lookup table avoids HashMap hashing overhead
// for the encode hot path. O(1) array index vs O(1) amortized but with hash
// computation, cache-unfriendly bucket chasing, and key comparison.
static ENCODE_LUT: LazyLock<[Option<&'static str>; 128]> = LazyLock::new(|| {
    let mut table: [Option<&'static str>; 128] = [None; 128];
    for &(ch, word) in NATO_TABLE {
        let idx = ch as usize;
        if idx < 128 {
            table[idx] = Some(word);
        }
    }
    table
});

#[derive(Subcommand)]
pub enum NatoAction {
    #[command(about = "Encode text to NATO phonetic alphabet")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode NATO phonetic alphabet to text")]
    Decode {
        #[arg(help = "NATO phonetic words (space-separated)")]
        input: String,
    },
}

pub fn run(action: NatoAction) -> Result<()> {
    match action {
        NatoAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        NatoAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

const NATO_TABLE: &[(char, &str)] = &[
    ('A', "ALFA"),
    ('B', "BRAVO"),
    ('C', "CHARLIE"),
    ('D', "DELTA"),
    ('E', "ECHO"),
    ('F', "FOXTROT"),
    ('G', "GOLF"),
    ('H', "HOTEL"),
    ('I', "INDIA"),
    ('J', "JULIET"),
    ('K', "KILO"),
    ('L', "LIMA"),
    ('M', "MIKE"),
    ('N', "NOVEMBER"),
    ('O', "OSCAR"),
    ('P', "PAPA"),
    ('Q', "QUEBEC"),
    ('R', "ROMEO"),
    ('S', "SIERRA"),
    ('T', "TANGO"),
    ('U', "UNIFORM"),
    ('V', "VICTOR"),
    ('W', "WHISKEY"),
    ('X', "XRAY"),
    ('Y', "YANKEE"),
    ('Z', "ZULU"),
    ('0', "ZERO"),
    ('1', "ONE"),
    ('2', "TWO"),
    ('3', "THREE"),
    ('4', "FOUR"),
    ('5', "FIVE"),
    ('6', "SIX"),
    ('7', "SEVEN"),
    ('8', "EIGHT"),
    ('9', "NINE"),
];

static NATO_TO_CHAR: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| NATO_TABLE.iter().map(|&(c, word)| (word, c)).collect());

pub fn encode(input: &str) -> String {
    let lut = &*ENCODE_LUT;
    // Pre-allocate: average NATO word is ~5 chars + 1 space separator
    let mut result = String::with_capacity(input.len() * 6);
    let mut first = true;

    for c in input.chars() {
        let upper = c.to_ascii_uppercase();
        let idx = upper as usize;
        if let Some(word) = if idx < 128 { lut[idx] } else { None } {
            if !first {
                result.push(' ');
            }
            first = false;
            result.push_str(word);
        }
    }

    result
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    input
        .split_whitespace()
        .map(|word| {
            let upper = word.to_uppercase();
            NATO_TO_CHAR
                .get(upper.as_str())
                .copied()
                .context(format!("Unknown NATO word: {}", word))
        })
        .collect::<Result<String>>()
}
