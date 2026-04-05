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

// Direct byte mutation avoids UTF-8 char boundary decoding and per-character
// allocation overhead from chars().map().collect(). Safe because Gronsfeld only
// transforms ASCII alphabetic bytes to other ASCII alphabetic bytes.
pub fn encrypt(input: &str, key: &str) -> Result<String> {
    let shifts = validate_key(key)?;
    let mut bytes = input.as_bytes().to_vec();
    let mut key_index = 0;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let shift = shifts[key_index % shifts.len()];
            key_index += 1;
            *b = (*b - base + shift) % 26 + base;
        }
    }

    Ok(unsafe { String::from_utf8_unchecked(bytes) })
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    let shifts = validate_key(key)?;
    let mut bytes = input.as_bytes().to_vec();
    let mut key_index = 0;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let shift = shifts[key_index % shifts.len()];
            key_index += 1;
            *b = (*b - base + 26 - shift) % 26 + base;
        }
    }

    Ok(unsafe { String::from_utf8_unchecked(bytes) })
}
