use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

// Performance: direct ASCII-indexed lookup table avoids HashMap hashing overhead
// for the encode hot path. O(1) array index vs O(1) amortized but with hash
// computation, cache-unfriendly bucket chasing, and key comparison.
static ENCODE_LUT: LazyLock<[Option<&'static str>; 128]> = LazyLock::new(|| {
    let mut table: [Option<&'static str>; 128] = [None; 128];
    for &(ch, morse) in MORSE_TABLE {
        let idx = ch as usize;
        if idx < 128 {
            table[idx] = Some(morse);
        }
    }
    table
});

#[derive(Subcommand)]
pub enum MorseAction {
    #[command(about = "Encode text to Morse code")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode Morse code to text")]
    Decode {
        #[arg(help = "Morse code (use . and -, separate letters with space, words with /)")]
        input: String,
    },
}

pub fn run(action: MorseAction) -> Result<()> {
    match action {
        MorseAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        MorseAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

const MORSE_TABLE: &[(char, &str)] = &[
    ('A', ".-"),
    ('B', "-..."),
    ('C', "-.-."),
    ('D', "-.."),
    ('E', "."),
    ('F', "..-."),
    ('G', "--."),
    ('H', "...."),
    ('I', ".."),
    ('J', ".---"),
    ('K', "-.-"),
    ('L', ".-.."),
    ('M', "--"),
    ('N', "-."),
    ('O', "---"),
    ('P', ".--."),
    ('Q', "--.-"),
    ('R', ".-."),
    ('S', "..."),
    ('T', "-"),
    ('U', "..-"),
    ('V', "...-"),
    ('W', ".--"),
    ('X', "-..-"),
    ('Y', "-.--"),
    ('Z', "--.."),
    ('0', "-----"),
    ('1', ".----"),
    ('2', "..---"),
    ('3', "...--"),
    ('4', "....-"),
    ('5', "....."),
    ('6', "-...."),
    ('7', "--..."),
    ('8', "---.."),
    ('9', "----."),
    ('.', ".-.-.-"),
    (',', "--..--"),
    ('?', "..--.."),
    ('!', "-.-.--"),
    ('/', "-..-."),
    ('(', "-.--."),
    (')', "-.--.-"),
    ('&', ".-..."),
    (':', "---..."),
    (';', "-.-.-."),
    ('=', "-...-"),
    ('+', ".-.-."),
    ('-', "-....-"),
    ('_', "..--.-"),
    ('"', ".-..-."),
    ('\'', ".----."),
    ('$', "...-..-"),
    ('@', ".--.-."),
];

static MORSE_TO_CHAR: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| MORSE_TABLE.iter().map(|&(c, m)| (m, c)).collect());

pub fn encode(input: &str) -> String {
    let lut = &*ENCODE_LUT;
    let mut result = String::with_capacity(input.len() * 4);
    let mut first_word = true;

    for word in input.split_whitespace() {
        if !first_word {
            result.push_str(" / ");
        }
        first_word = false;
        let mut first_char = true;
        for c in word.chars() {
            let upper = if c.is_ascii_lowercase() {
                (c as u8 - b'a' + b'A') as char
            } else {
                c
            };
            let idx = upper as usize;
            if let Some(morse) = if idx < 128 { lut[idx] } else { None } {
                if !first_char {
                    result.push(' ');
                }
                first_char = false;
                result.push_str(morse);
            }
        }
    }

    result
}

pub fn decode(input: &str) -> Result<String> {
    input
        .split(" / ")
        .map(|word| {
            word.split_whitespace()
                .map(|morse| {
                    // Performance: use with_context so format! only runs on
                    // lookup failure. context(format!(...)) eagerly allocates
                    // a String for every successful symbol in the hot decode
                    // loop, which dominates the work on long inputs.
                    MORSE_TO_CHAR
                        .get(morse)
                        .copied()
                        .with_context(|| format!("Unknown Morse code: {}", morse))
                })
                .collect::<Result<String>>()
        })
        .collect::<Result<Vec<_>>>()
        .map(|words| words.join(" "))
}
