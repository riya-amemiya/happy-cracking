use anyhow::{Context, Result};
use clap::Subcommand;

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

pub fn encode(input: &str) -> String {
    input
        .to_uppercase()
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter_map(|c| {
                    MORSE_TABLE
                        .iter()
                        .find(|(ch, _)| *ch == c)
                        .map(|(_, morse)| *morse)
                })
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
                    MORSE_TABLE
                        .iter()
                        .find(|(_, m)| *m == morse)
                        .map(|(c, _)| *c)
                        .context(format!("Unknown Morse code: {}", morse))
                })
                .collect::<Result<String>>()
        })
        .collect::<Result<Vec<_>>>()
        .map(|words| words.join(" "))
}
