use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum A1z26Action {
    #[command(about = "Encode text to A1Z26 (A=1, Z=26)")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode A1Z26 numbers to text")]
    Decode {
        #[arg(help = "A1Z26 encoded string (e.g. 8-5-12-12-15)")]
        input: String,
    },
}

pub fn run(action: A1z26Action) -> Result<()> {
    match action {
        A1z26Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        A1z26Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

pub fn encode(input: &str) -> String {
    let mut result = Vec::new();
    let mut pending_numbers: Vec<String> = Vec::new();

    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            let upper = c.to_ascii_uppercase();
            let num = (upper as u8 - b'A' + 1).to_string();
            pending_numbers.push(num);
        } else {
            if !pending_numbers.is_empty() {
                result.push(pending_numbers.join("-"));
                pending_numbers.clear();
            }
            result.push(c.to_string());
        }
    }

    if !pending_numbers.is_empty() {
        result.push(pending_numbers.join("-"));
    }

    result.join("")
}

fn parse_a1z26_number(s: &str) -> Result<char> {
    let n: u8 = s.parse().context("Failed to parse number in A1Z26 input")?;
    if !(1..=26).contains(&n) {
        anyhow::bail!("Number {} is out of range (1-26)", n);
    }
    Ok((b'A' + n - 1) as char)
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let mut result = String::new();
    let mut current_number = String::new();

    for c in input.chars() {
        if c.is_ascii_digit() {
            current_number.push(c);
        } else if c == '-' || c == ' ' || c == ',' {
            if !current_number.is_empty() {
                result.push(parse_a1z26_number(&current_number)?);
                current_number.clear();
            }
        } else {
            if !current_number.is_empty() {
                result.push(parse_a1z26_number(&current_number)?);
                current_number.clear();
            }
            result.push(c);
        }
    }

    if !current_number.is_empty() {
        result.push(parse_a1z26_number(&current_number)?);
    }

    Ok(result)
}
