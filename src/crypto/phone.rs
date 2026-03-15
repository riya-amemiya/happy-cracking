use anyhow::Result;
use clap::Subcommand;
use std::sync::LazyLock;

#[derive(Subcommand)]
pub enum PhoneAction {
    #[command(about = "Encode text to phone keypad multi-tap")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode phone keypad multi-tap to text")]
    Decode {
        #[arg(help = "Phone keypad encoded text")]
        input: String,
    },
}

pub fn run(action: PhoneAction) -> Result<()> {
    match action {
        PhoneAction::Encode { input } => {
            println!("{}", encode(&input));
        }
        PhoneAction::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

// (key_digit, letters on that key)
const KEYPAD: &[(char, &[char])] = &[
    ('2', &['A', 'B', 'C']),
    ('3', &['D', 'E', 'F']),
    ('4', &['G', 'H', 'I']),
    ('5', &['J', 'K', 'L']),
    ('6', &['M', 'N', 'O']),
    ('7', &['P', 'Q', 'R', 'S']),
    ('8', &['T', 'U', 'V']),
    ('9', &['W', 'X', 'Y', 'Z']),
];

// Maps char -> (digit, position), e.g. 'A' -> (b'2', 0), 'B' -> (b'2', 1)
// We use a fixed size array mapping from A-Z (0-25) to avoid HashMap overhead
static CHAR_TO_KEY: LazyLock<[(u8, usize); 26]> = LazyLock::new(|| {
    let mut map = [(0, 0); 26];
    for &(digit, letters) in KEYPAD {
        for (pos, &letter) in letters.iter().enumerate() {
            let idx = (letter as u8 - b'A') as usize;
            map[idx] = (digit as u8, pos);
        }
    }
    map
});

fn push_char_presses(c: char, out: &mut String) -> Option<()> {
    if !c.is_ascii_uppercase() {
        return None;
    }
    let idx = (c as u8 - b'A') as usize;
    let (digit, pos) = CHAR_TO_KEY[idx];
    if digit == 0 {
        return None;
    }
    let d = digit as char;
    for _ in 0..=pos {
        out.push(d);
    }
    Some(())
}

fn key_for_char(c: char) -> Option<char> {
    if !c.is_ascii_uppercase() {
        return None;
    }
    let idx = (c as u8 - b'A') as usize;
    let digit = CHAR_TO_KEY[idx].0;
    if digit == 0 {
        return None;
    }
    Some(digit as char)
}

// Maps (digit - '0', press_count) -> char. Max digit is '9' (index 9), max presses 4 (index 4)
// We use an array rather than HashMap to avoid allocation and hashing overhead
static KEY_TO_CHAR: LazyLock<[[char; 5]; 10]> = LazyLock::new(|| {
    let mut map = [['\0'; 5]; 10];
    for &(digit, letters) in KEYPAD {
        let d_idx = (digit as u8 - b'0') as usize;
        for (pos, &letter) in letters.iter().enumerate() {
            map[d_idx][pos + 1] = letter;
        }
    }
    map
});

// Optimization: Pre-allocate the exact string capacity needed to avoid multiple allocations.
pub fn encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 4);
    let mut prev_key: Option<char> = None;

    // Optimization: Filter at the byte level to avoid Unicode boundary decoding overhead
    for &b in input.as_bytes() {
        if b == b' ' {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push('0');
            prev_key = None;
            continue;
        }

        if !b.is_ascii_alphabetic() {
            continue;
        }

        let upper_c = b.to_ascii_uppercase() as char;
        let cur_key = key_for_char(upper_c);
        if cur_key.is_some() {
            if !out.is_empty() && out.ends_with(|ch| ch != ' ') {
                if prev_key.is_some() && prev_key == cur_key {
                    out.push('-');
                } else {
                    out.push(' ');
                }
            }
            push_char_presses(upper_c, &mut out);
            prev_key = cur_key;
        }
    }

    out
}

fn presses_to_char(s: &str) -> Result<char> {
    if s.is_empty() {
        anyhow::bail!("Empty press sequence");
    }

    let bytes = s.as_bytes();
    let digit = bytes[0];
    if !bytes.iter().all(|&b| b == digit) {
        anyhow::bail!("Mixed digits in press sequence: {}", s);
    }

    let count = bytes.len();

    if digit == b'0' {
        return Ok(' ');
    }

    if !digit.is_ascii_digit() || count > 4 {
        anyhow::bail!("Invalid press count {} for key {}", count, digit as char);
    }

    let ch = KEY_TO_CHAR[(digit - b'0') as usize][count];
    if ch == '\0' {
        anyhow::bail!("Invalid press count {} for key {}", count, digit as char);
    }

    Ok(ch)
}

pub fn decode(input: &str) -> Result<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    for group in input.split_whitespace() {
        if group == "0" {
            result.push(' ');
            continue;
        }
        for press_seq in group.split('-') {
            result.push(presses_to_char(press_seq)?);
        }
    }
    Ok(result)
}
