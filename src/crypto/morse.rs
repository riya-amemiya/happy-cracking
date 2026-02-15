use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

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

static CHAR_TO_MORSE: LazyLock<HashMap<char, &'static str>> =
    LazyLock::new(|| MORSE_TABLE.iter().copied().collect());

static MORSE_TO_CHAR: LazyLock<HashMap<&'static str, char>> =
    LazyLock::new(|| MORSE_TABLE.iter().map(|&(c, m)| (m, c)).collect());

pub fn encode(input: &str) -> String {
    input
        .to_uppercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter_map(|c| CHAR_TO_MORSE.get(&c).copied())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

pub fn decode(input: &str) -> Result<String> {
    input
        .split(" / ")
        .map(|word| {
            word.split_whitespace()
                .map(|morse| {
                    MORSE_TO_CHAR
                        .get(morse)
                        .copied()
                        .context(format!("Unknown Morse code: {}", morse))
                })
                .collect::<Result<String>>()
        })
        .collect::<Result<Vec<_>>>()
        .map(|words| words.join(" "))
}
