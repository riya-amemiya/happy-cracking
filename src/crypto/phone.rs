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

fn char_to_presses(c: char) -> Option<String> {
    for &(digit, letters) in KEYPAD {
        if let Some(pos) = letters.iter().position(|&l| l == c) {
            return Some(digit.to_string().repeat(pos + 1));
        }
    }
    None
}

fn key_for_char(c: char) -> Option<char> {
    for &(digit, letters) in KEYPAD {
        if letters.contains(&c) {
            return Some(digit);
        }
    }
    None
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
    for &(key, letters) in KEYPAD {
        if key == digit {
            if count == 0 || count > letters.len() {
                anyhow::bail!("Invalid press count {} for key {}", count, digit);
            }
            return Ok(letters[count - 1]);
        }
    }

    if digit == '0' {
        return Ok(' ');
    }

    anyhow::bail!("Unknown key digit: {}", digit)
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
