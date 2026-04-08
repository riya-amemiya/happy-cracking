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

// Performance: [char; 26] array indexed by (letter - 'A') replaces HashMap<char, char>
// for encoding. Eliminates hashing overhead, bucket chasing, and key comparison —
// a direct O(1) array index vs amortized O(1) HashMap lookup with allocation.
const ENCODE_LUT: [char; 26] = {
    let mut table = ['\0'; 26];
    let mut i = 0;
    while i < LETTER_TABLE.len() {
        let (ch, br) = LETTER_TABLE[i];
        table[ch as usize - 'A' as usize] = br;
        i += 1;
    }
    table
};

// Performance: [char; 10] array indexed by digit position replaces the linear scan
// in braille_to_digit that iterated LETTER_TABLE with .enumerate().find().
// Digits 1-9,0 map to Braille patterns of letters A-J respectively.
const DIGIT_BRAILLE_LUT: [char; 10] = {
    let mut table = ['\0'; 10];
    // '1' maps to A (index 0), ..., '9' maps to I (index 8), '0' maps to J (index 9)
    let mut i = 0;
    while i < 10 {
        table[i] = LETTER_TABLE[i].1;
        i += 1;
    }
    table
};

// Performance: reverse lookup from Braille codepoint to letter using a compact array
// indexed by (braille_char - 0x2800). Braille block is U+2800..U+283F (64 codepoints),
// so a [Option<char>; 64] array replaces HashMap<char, char> for decoding.
// Eliminates hashing overhead and heap allocation from LazyLock<HashMap>.
const DECODE_LUT: [Option<char>; 64] = {
    let mut table: [Option<char>; 64] = [None; 64];
    let mut i = 0;
    while i < LETTER_TABLE.len() {
        let (ch, br) = LETTER_TABLE[i];
        let idx = br as usize - 0x2800;
        table[idx] = Some(ch);
        i += 1;
    }
    table
};

fn digit_to_braille(d: char) -> Option<char> {
    let idx = match d {
        '1'..='9' => (d as u8 - b'1') as usize,
        '0' => 9,
        _ => return None,
    };
    Some(DIGIT_BRAILLE_LUT[idx])
}

const DIGIT_MAP: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];

// Performance: O(1) array lookup replaces O(N) linear scan through LETTER_TABLE
fn braille_to_digit(b: char) -> Option<char> {
    let idx = b as usize;
    if !(0x2800..0x2840).contains(&idx) {
        return None;
    }
    // Check if this braille char matches one of the first 10 letters (A-J = digits)
    let offset = idx - 0x2800;
    DECODE_LUT[offset].and_then(|ch| {
        let letter_idx = (ch as u8).wrapping_sub(b'A') as usize;
        if letter_idx < 10 {
            Some(DIGIT_MAP[letter_idx])
        } else {
            None
        }
    })
}

pub fn encode(input: &str) -> String {
    // Performance: avoid input.to_uppercase() allocation by converting case inline.
    // Use ENCODE_LUT array instead of HashMap for O(1) direct-index lookup.
    let mut result = String::with_capacity(input.len() * 3);
    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            let idx = (c.to_ascii_uppercase() as u8 - b'A') as usize;
            result.push(ENCODE_LUT[idx]);
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

    let mut result = String::with_capacity(input.len());
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
            let idx = c as usize;
            let letter = if (0x2800..0x2840).contains(&idx) {
                DECODE_LUT[idx - 0x2800]
            } else {
                None
            };
            let letter = letter.context(format!("Unknown Braille character: {:?}", c))?;
            result.push(letter);
        }
    }
    Ok(result)
}
