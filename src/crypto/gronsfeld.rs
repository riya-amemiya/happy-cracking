use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum GronsfeldAction {
    #[command(about = "Encrypt with Gronsfeld cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Numeric key (digits only, e.g., \"31415\")")]
        key: String,
    },
    #[command(about = "Decrypt Gronsfeld cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Numeric key (digits only, e.g., \"31415\")")]
        key: String,
    },
}

pub fn run(action: GronsfeldAction) -> Result<()> {
    match action {
        GronsfeldAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        GronsfeldAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<Vec<u8>> {
    if key.is_empty() {
        anyhow::bail!("Key must be non-empty");
    }
    if !key.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("Key must contain only digits (0-9)");
    }
    Ok(key.bytes().map(|b| b - b'0').collect())
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    let shifts = validate_key(key)?;
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = shifts[key_index % shifts.len()];
                key_index += 1;
                ((c as u8 - base + shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    let shifts = validate_key(key)?;
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = shifts[key_index % shifts.len()];
                key_index += 1;
                ((c as u8 - base + 26 - shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
}
