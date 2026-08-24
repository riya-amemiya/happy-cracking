use anyhow::Result;
use clap::Subcommand;

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

/// `DECODE_LUT[digit][press_count - 1]` → letter for T9 keys 2–9.
///
/// A `[Option<char>; 4]` per digit 0–9 replaces `HashMap<(char, usize), char>`.
/// T9 is ASCII digits 2–9 with 1–4 presses, so decode is two array indexes
/// instead of hashing a tuple. Built at compile time (no `LazyLock`).
const fn build_decode_lut() -> [[Option<char>; 4]; 10] {
    let mut lut = [[None; 4]; 10];
    let mut i = 0;
    while i < KEYPAD.len() {
        let (digit, letters) = KEYPAD[i];
        let d = (digit as u8 - b'0') as usize;
        let mut j = 0;
        while j < letters.len() {
            lut[d][j] = Some(letters[j]);
            j += 1;
        }
        i += 1;
    }
    lut
}

const DECODE_LUT: [[Option<char>; 4]; 10] = build_decode_lut();

// Pre-computed lookup table mapping each uppercase letter (A-Z) to its phone
// keypad multi-tap string. This eliminates runtime string allocation from
// `digit.to_string().repeat(pos + 1)` which previously created intermediate
// String objects on every call. The first byte of each entry is the keypad
// digit, so encode can group same-key letters without a HashMap.
const PHONE_PRESS_TABLE: [&str; 26] = [
    "2",    // A
    "22",   // B
    "222",  // C
    "3",    // D
    "33",   // E
    "333",  // F
    "4",    // G
    "44",   // H
    "444",  // I
    "5",    // J
    "55",   // K
    "555",  // L
    "6",    // M
    "66",   // N
    "666",  // O
    "7",    // P
    "77",   // Q
    "777",  // R
    "7777", // S
    "8",    // T
    "88",   // U
    "888",  // V
    "9",    // W
    "99",   // X
    "999",  // Y
    "9999", // Z
];

pub fn encode(input: &str) -> String {
    let upper = input.to_uppercase();
    // Pre-allocate output assuming ~3 chars per input char (press digits + separators)
    let mut result = String::with_capacity(upper.len() * 4);
    let mut prev_key: Option<u8> = None;
    let mut group_has_content = false;

    for c in upper.chars() {
        if c == ' ' {
            if group_has_content {
                result.push(' ');
            }
            result.push('0');
            // Mark that we have content so the next letter group gets a space separator
            group_has_content = true;
            prev_key = None;
            continue;
        }

        if !c.is_ascii_alphabetic() {
            continue;
        }

        let idx = (c as u8 - b'A') as usize;
        let presses = PHONE_PRESS_TABLE[idx];
        // First byte of the press string IS the keypad digit — no HashMap.
        let cur_key = Some(presses.as_bytes()[0]);

        if prev_key == cur_key {
            // Same key group, separate with dash
            result.push('-');
        } else {
            // Different key group, separate with space
            if group_has_content {
                result.push(' ');
            }
            group_has_content = true;
        }

        result.push_str(presses);
        prev_key = cur_key;
    }

    result
}

fn presses_to_char(s: &str) -> Result<char> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        anyhow::bail!("Empty press sequence");
    }

    let digit = bytes[0];
    if !bytes.iter().all(|&b| b == digit) {
        anyhow::bail!("Mixed digits in press sequence: {}", s);
    }

    let count = bytes.len();

    if digit == b'0' {
        return Ok(' ');
    }

    let d = digit.wrapping_sub(b'0') as usize;
    if d < 10
        && (1..=4).contains(&count)
        && let Some(ch) = DECODE_LUT[d][count - 1]
    {
        return Ok(ch);
    }

    anyhow::bail!("Invalid press count {} for key {}", count, digit as char)
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
