use anyhow::{Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum BaudotAction {
    #[command(about = "Encode text to Baudot/ITA2 code")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode Baudot/ITA2 code to text")]
    Decode {
        #[arg(help = "Baudot encoded string (space-separated 5-bit binary)")]
        input: String,
    },
}

pub fn run(action: BaudotAction) -> Result<()> {
    match action {
        BaudotAction::Encode { input } => {
            println!("{}", encode(&input)?);
        }
        BaudotAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// ITA2 letter codes (MSB first): index -> (letter_char, figure_char)
// Code 0x00 = NUL, 0x1B = FIGS shift, 0x1F = LTRS shift
const ITA2_TABLE: &[(char, char)] = &[
    ('\0', '\0'),  // 00000 - NUL
    ('E', '3'),    // 00001
    ('\n', '\n'),  // 00010 - LF
    ('A', '-'),    // 00011
    (' ', ' '),    // 00100 - SPACE
    ('S', '\''),   // 00101
    ('I', '8'),    // 00110
    ('U', '7'),    // 00111
    ('\r', '\r'),  // 01000 - CR
    ('D', '$'),    // 01001
    ('R', '4'),    // 01010
    ('J', '\x07'), // 01011 - BELL in figures
    ('N', ','),    // 01100
    ('F', '!'),    // 01101
    ('C', ':'),    // 01110
    ('K', '('),    // 01111
    ('T', '5'),    // 10000
    ('Z', '"'),    // 10001
    ('L', ')'),    // 10010
    ('W', '2'),    // 10011
    ('H', '#'),    // 10100
    ('Y', '6'),    // 10101
    ('P', '0'),    // 10110
    ('Q', '1'),    // 10111
    ('O', '9'),    // 11000
    ('B', '?'),    // 11001
    ('G', '&'),    // 11010
    ('\0', '\0'),  // 11011 - FIGS shift
    ('M', '.'),    // 11100
    ('X', '/'),    // 11101
    ('V', ';'),    // 11110
    ('\0', '\0'),  // 11111 - LTRS shift
];

const FIGS_SHIFT: u8 = 0b11011;
const LTRS_SHIFT: u8 = 0b11111;

// Pre-computed 5-bit binary string representations of ITA2 codes
const ITA2_BIN_TABLE: &[&str; 32] = &[
    "00000", "00001", "00010", "00011", "00100", "00101", "00110", "00111", "01000", "01001",
    "01010", "01011", "01100", "01101", "01110", "01111", "10000", "10001", "10010", "10011",
    "10100", "10101", "10110", "10111", "11000", "11001", "11010", "11011", "11100", "11101",
    "11110", "11111",
];

// Maps a character to (code, is_figure) for O(1) encode lookups
static CHAR_TO_CODE: LazyLock<HashMap<char, (u8, bool)>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (i, &(letter, figure)) in ITA2_TABLE.iter().enumerate() {
        let code = i as u8;
        if code == FIGS_SHIFT || code == LTRS_SHIFT {
            continue;
        }
        if letter != '\0' && !map.contains_key(&letter) {
            map.insert(letter, (code, false));
        }
        if figure != '\0' && figure != letter && !map.contains_key(&figure) {
            map.insert(figure, (code, true));
        }
    }
    map
});

pub fn encode(input: &str) -> Result<String> {
    if input.is_empty() {
        return Ok(String::new());
    }

    let upper = input.to_uppercase();
    let mut out = String::with_capacity(input.len() * 6);
    let mut in_figures = false;

    for c in upper.chars() {
        if let Some(&(code, is_figure)) = CHAR_TO_CODE.get(&c) {
            if is_figure && !in_figures {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(ITA2_BIN_TABLE[FIGS_SHIFT as usize]);
                in_figures = true;
            } else if !is_figure && in_figures && c != ' ' {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(ITA2_BIN_TABLE[LTRS_SHIFT as usize]);
                in_figures = false;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(ITA2_BIN_TABLE[code as usize]);
        }
    }

    Ok(out)
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    let mut in_figures = false;

    for code_str in input.split_whitespace() {
        if code_str.len() != 5 || !code_str.chars().all(|c| c == '0' || c == '1') {
            anyhow::bail!("Invalid Baudot code: {}", code_str);
        }

        let code = u8::from_str_radix(code_str, 2)
            .with_context(|| format!("Failed to parse Baudot code: {}", code_str))?;

        if code == FIGS_SHIFT {
            in_figures = true;
            continue;
        }
        if code == LTRS_SHIFT {
            in_figures = false;
            continue;
        }

        if let Some((letter, figure)) = ITA2_TABLE.get(code as usize) {
            let ch = if in_figures { *figure } else { *letter };
            if ch != '\0' {
                result.push(ch);
            }
        } else {
            anyhow::bail!("Baudot code out of range: {}", code);
        }
    }

    Ok(result)
}
