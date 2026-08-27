use anyhow::{Context, Result};
use clap::Subcommand;

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

const ENCODE_LUT: [Option<&str>; 128] = {
    let mut table: [Option<&str>; 128] = [None; 128];
    let mut i = 0;
    while i < NATO_TABLE.len() {
        let (ch, word) = NATO_TABLE[i];
        let idx = ch as usize;
        if idx < 128 {
            table[idx] = Some(word);
        }
        i += 1;
    }
    table
};

/// First-letter buckets for NATO decode.
///
/// Digit words share an initial letter with A–Z words (ZERO/ZULU, FOUR/FIVE/FOXTROT,
/// …), but no letter has more than 3 entries, so a 26×3 table plus
/// `eq_ignore_ascii_case` replaces HashMap hashing and the per-token
/// `to_uppercase()` allocation.
const DECODE_LUT: [[Option<(&str, char)>; 3]; 26] = {
    let mut table = [[None; 3]; 26];
    let mut i = 0;
    while i < NATO_TABLE.len() {
        let (ch, word) = NATO_TABLE[i];
        let bucket = (word.as_bytes()[0] - b'A') as usize;
        let mut slot = 0;
        while slot < 3 {
            if table[bucket][slot].is_none() {
                table[bucket][slot] = Some((word, ch));
                break;
            }
            slot += 1;
        }
        i += 1;
    }
    table
};

const fn decode_lut_len(table: &[[Option<(&str, char)>; 3]; 26]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < 26 {
        let mut slot = 0;
        while slot < 3 {
            if table[i][slot].is_some() {
                n += 1;
            }
            slot += 1;
        }
        i += 1;
    }
    n
}

const _: () = assert!(decode_lut_len(&DECODE_LUT) == NATO_TABLE.len());

fn lookup_nato(word: &str) -> Option<char> {
    let b = word.as_bytes();
    if b.is_empty() {
        return None;
    }
    let first = b[0].to_ascii_uppercase();
    if !first.is_ascii_uppercase() {
        return None;
    }
    let bucket = &DECODE_LUT[(first - b'A') as usize];
    for slot in bucket {
        if let Some((w, ch)) = slot
            && word.eq_ignore_ascii_case(w)
        {
            return Some(*ch);
        }
    }
    None
}

pub fn encode(input: &str) -> String {
    // Average NATO word is ~5 chars + 1 space.
    let mut result = String::with_capacity(input.len() * 6);
    let mut first = true;

    for c in input.chars() {
        let upper = c.to_ascii_uppercase();
        let idx = upper as usize;
        if let Some(word) = if idx < 128 { ENCODE_LUT[idx] } else { None } {
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

    let mut result = String::with_capacity(input.len() / 4);
    for word in input.split_whitespace() {
        let ch = lookup_nato(word).with_context(|| format!("Unknown NATO word: {}", word))?;
        result.push(ch);
    }
    Ok(result)
}
