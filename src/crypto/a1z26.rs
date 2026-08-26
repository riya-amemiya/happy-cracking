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
    if input.is_empty() {
        return String::new();
    }

    // Max 3 chars per letter (e.g. "26-").
    let mut out = String::with_capacity(input.len() * 3);
    let mut in_number_seq = false;

    for c in input.chars() {
        if c.is_ascii_alphabetic() {
            if in_number_seq {
                out.push('-');
            }
            let num = c.to_ascii_uppercase() as u8 - b'A' + 1;
            if num >= 10 {
                out.push((b'0' + (num / 10)) as char);
                out.push((b'0' + (num % 10)) as char);
            } else {
                out.push((b'0' + num) as char);
            }
            in_number_seq = true;
        } else {
            in_number_seq = false;
            out.push(c);
        }
    }
    out
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
