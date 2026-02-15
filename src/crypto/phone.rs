use anyhow::Result;
use clap::Subcommand;
use std::collections::HashMap;
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

// Maps char -> (digit, position), e.g. 'A' -> ('2', 0), 'B' -> ('2', 1)
static CHAR_TO_KEY: LazyLock<HashMap<char, (char, usize)>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for &(digit, letters) in KEYPAD {
        for (pos, &letter) in letters.iter().enumerate() {
            map.insert(letter, (digit, pos));
        }
    }
    map
});

// Maps (digit, press_count) -> char, e.g. ('2', 1) -> 'A', ('2', 2) -> 'B'
static KEY_TO_CHAR: LazyLock<HashMap<(char, usize), char>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for &(digit, letters) in KEYPAD {
        for (pos, &letter) in letters.iter().enumerate() {
            map.insert((digit, pos + 1), letter);
        }
    }
    map
});

fn char_to_presses(c: char) -> Option<String> {
    CHAR_TO_KEY
        .get(&c)
        .map(|&(digit, pos)| digit.to_string().repeat(pos + 1))
}

fn key_for_char(c: char) -> Option<char> {
    CHAR_TO_KEY.get(&c).map(|&(digit, _)| digit)
}

pub fn encode(input: &str) -> String {
    let upper = input.to_uppercase();
    let mut groups: Vec<String> = Vec::new();
    let mut prev_key: Option<char> = None;
    let mut current_group: Vec<String> = Vec::new();

    for c in upper.chars() {
        if c == ' ' {
            if !current_group.is_empty() {
                groups.push(current_group.join("-"));
                current_group.clear();
            }
            groups.push("0".to_string());
            prev_key = None;
            continue;
        }

        if !c.is_ascii_alphabetic() {
            continue;
        }

        let cur_key = key_for_char(c);
        if let Some(presses) = char_to_presses(c) {
            if prev_key.is_some() && prev_key == cur_key {
                current_group.push(presses);
            } else {
                if !current_group.is_empty() {
                    groups.push(current_group.join("-"));
                    current_group.clear();
                }
                current_group.push(presses);
            }
            prev_key = cur_key;
        }
    }

    if !current_group.is_empty() {
        groups.push(current_group.join("-"));
    }

    groups.join(" ")
}

fn presses_to_char(s: &str) -> Result<char> {
    if s.is_empty() {
        anyhow::bail!("Empty press sequence");
    }

    let digit = s.chars().next().unwrap();
    if !s.chars().all(|c| c == digit) {
        anyhow::bail!("Mixed digits in press sequence: {}", s);
    }

    let count = s.len();

    if digit == '0' {
        return Ok(' ');
    }

    KEY_TO_CHAR
        .get(&(digit, count))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Invalid press count {} for key {}", count, digit))
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
