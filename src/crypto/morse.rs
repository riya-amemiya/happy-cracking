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

/// Pack Morse `.`/`-` into a small integer: start at 1, shift left per symbol,
/// set the low bit for `-`. Starting at 1 encodes length, so `.` and `..` land
/// in different slots. International Morse is at most 7 symbols (`$`), so the
/// index always fits in `0..256`.
const fn pack_morse(code: &[u8]) -> usize {
    let mut idx = 1usize;
    let mut i = 0;
    while i < code.len() {
        idx <<= 1;
        if code[i] == b'-' {
            idx |= 1;
        }
        i += 1;
    }
    idx
}

/// Performance: `[Option<&str>; 128]` indexed by ASCII code point replaces a
/// `HashMap<char, &str>` on the encode path. Built at compile time (no
/// `LazyLock` first-call cost).
const ENCODE_LUT: [Option<&str>; 128] = {
    let mut table: [Option<&str>; 128] = [None; 128];
    let mut i = 0;
    while i < MORSE_TABLE.len() {
        let (ch, morse) = MORSE_TABLE[i];
        let idx = ch as usize;
        if idx < 128 {
            table[idx] = Some(morse);
        }
        i += 1;
    }
    table
};

/// Packed Morse token → character.
///
/// Performance: a `[Option<char>; 256]` indexed by `pack_morse` replaces
/// `HashMap<&str, char>` on decode. Each letter is a few bit shifts and an
/// array index instead of hashing, bucket chasing, and string compare. Built
/// at compile time (no `LazyLock`).
const DECODE_LUT: [Option<char>; 256] = {
    let mut table = [None; 256];
    let mut i = 0;
    while i < MORSE_TABLE.len() {
        let (ch, code) = MORSE_TABLE[i];
        table[pack_morse(code.as_bytes())] = Some(ch);
        i += 1;
    }
    table
};

fn lookup_morse(token: &str) -> Option<char> {
    // `$` is the longest table entry (7 symbols); longer tokens cannot hit the LUT.
    if token.is_empty() || token.len() > 7 {
        return None;
    }
    let mut idx = 1usize;
    for b in token.bytes() {
        idx <<= 1;
        match b {
            b'.' => {}
            b'-' => idx |= 1,
            _ => return None,
        }
    }
    DECODE_LUT[idx]
}

pub fn encode(input: &str) -> String {
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
            if let Some(morse) = if idx < 128 { ENCODE_LUT[idx] } else { None } {
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
    // One output buffer instead of a String per word plus Vec+join. Morse
    // letters become one char each, so input.len() is a cheap overestimate.
    let mut result = String::with_capacity(input.len());
    let mut first_word = true;

    for word in input.split(" / ") {
        if !first_word {
            result.push(' ');
        }
        first_word = false;
        for morse in word.split_whitespace() {
            result.push(
                lookup_morse(morse).with_context(|| format!("Unknown Morse code: {}", morse))?,
            );
        }
    }

    Ok(result)
}
