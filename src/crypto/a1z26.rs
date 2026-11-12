use anyhow::Result;
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

pub const MAX_A1Z26_DIGITS: usize = 2;

pub fn check_a1z26_digits(digits: &str) -> Result<()> {
    if digits.len() > MAX_A1Z26_DIGITS {
        anyhow::bail!(
            "A1Z26 number exceeds maximum of {MAX_A1Z26_DIGITS} digits to prevent Denial of Service"
        );
    }
    Ok(())
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

#[must_use]
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

fn letter_from_a1z26(n: u8) -> Result<char> {
    if !(1..=26).contains(&n) {
        anyhow::bail!("Number {n} is out of range (1-26)");
    }
    Ok((b'A' + n - 1) as char)
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let bytes = input.as_bytes();
    let mut result = String::with_capacity(bytes.len());
    let mut n = 0u8;
    let mut digits = 0u8;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii() {
            if b.is_ascii_digit() {
                digits += 1;
                if usize::from(digits) > MAX_A1Z26_DIGITS {
                    anyhow::bail!(
                        "A1Z26 number exceeds maximum of {MAX_A1Z26_DIGITS} digits to prevent Denial of Service"
                    );
                }
                n = n * 10 + (b - b'0');
            } else {
                if digits != 0 {
                    result.push(letter_from_a1z26(n)?);
                    n = 0;
                    digits = 0;
                }
                if b != b'-' && b != b' ' && b != b',' {
                    result.push(b as char);
                }
            }
            i += 1;
        } else {
            if digits != 0 {
                result.push(letter_from_a1z26(n)?);
                n = 0;
                digits = 0;
            }
            let Some(ch) = input[i..].chars().next() else {
                break;
            };
            result.push(ch);
            i += ch.len_utf8();
        }
    }

    if digits != 0 {
        result.push(letter_from_a1z26(n)?);
    }

    Ok(result)
}
