use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BrailleAction {
    #[command(about = "Encode text to Braille")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode Braille to text")]
    Decode {
        #[arg(help = "Braille encoded text")]
        input: String,
    },
}

pub fn run(action: BrailleAction) -> Result<()> {
    match action {
        BrailleAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        BrailleAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

const LETTER_TABLE: &[(char, char)] = &[
    ('A', '\u{2801}'),
    ('B', '\u{2803}'),
    ('C', '\u{2809}'),
    ('D', '\u{2819}'),
    ('E', '\u{2811}'),
    ('F', '\u{280B}'),
    ('G', '\u{281B}'),
    ('H', '\u{2813}'),
    ('I', '\u{280A}'),
    ('J', '\u{281A}'),
    ('K', '\u{2805}'),
    ('L', '\u{2807}'),
    ('M', '\u{280D}'),
    ('N', '\u{281D}'),
    ('O', '\u{2815}'),
    ('P', '\u{280F}'),
    ('Q', '\u{281F}'),
    ('R', '\u{2817}'),
    ('S', '\u{280E}'),
    ('T', '\u{281E}'),
    ('U', '\u{2825}'),
    ('V', '\u{2827}'),
    ('W', '\u{283A}'),
    ('X', '\u{282D}'),
    ('Y', '\u{283D}'),
    ('Z', '\u{2835}'),
];

// Numbers use same patterns as A-J with a number prefix
const NUMBER_PREFIX: char = '\u{283C}';
const BRAILLE_SPACE: char = '\u{2800}';

fn digit_to_braille(d: char) -> Option<char> {
    let idx = match d {
        '1' => 0, // same as A
        '2' => 1, // same as B
        '3' => 2, // same as C
        '4' => 3, // same as D
        '5' => 4, // same as E
        '6' => 5, // same as F
        '7' => 6, // same as G
        '8' => 7, // same as H
        '9' => 8, // same as I
        '0' => 9, // same as J
        _ => return None,
    };
    Some(LETTER_TABLE[idx].1)
}

fn braille_to_digit(b: char) -> Option<char> {
    LETTER_TABLE.iter().enumerate().find_map(|(idx, &(_, br))| {
        if br == b {
            match idx {
                0 => Some('1'),
                1 => Some('2'),
                2 => Some('3'),
                3 => Some('4'),
                4 => Some('5'),
                5 => Some('6'),
                6 => Some('7'),
                7 => Some('8'),
                8 => Some('9'),
                9 => Some('0'),
                _ => None,
            }
        } else {
            None
        }
    })
}

pub fn encode(input: &str) -> String {
    let mut result = String::new();
    for c in input.to_uppercase().chars() {
        if c.is_ascii_alphabetic() {
            if let Some(&(_, braille)) = LETTER_TABLE.iter().find(|&&(ch, _)| ch == c) {
                result.push(braille);
            }
        } else if c.is_ascii_digit() {
            result.push(NUMBER_PREFIX);
            if let Some(braille) = digit_to_braille(c) {
                result.push(braille);
            }
        } else if c == ' ' {
            result.push(BRAILLE_SPACE);
        }
    }
    result
}

pub fn decode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    let mut in_number_mode = false;
    for c in input.chars() {
        if c == NUMBER_PREFIX {
            in_number_mode = true;
            continue;
        }
        if c == BRAILLE_SPACE {
            result.push(' ');
            in_number_mode = false;
            continue;
        }
        if in_number_mode {
            let digit = braille_to_digit(c)
                .context(format!("Invalid Braille number character: {:?}", c))?;
            result.push(digit);
            in_number_mode = false;
        } else {
            let letter = LETTER_TABLE
                .iter()
                .find(|&&(_, br)| br == c)
                .map(|&(ch, _)| ch)
                .context(format!("Unknown Braille character: {:?}", c))?;
            result.push(letter);
        }
    }
    Ok(result)
}
