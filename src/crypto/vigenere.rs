use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum VigenereAction {
    #[command(about = "Encrypt with Vigenère cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Encryption key (alphabetic)")]
        key: String,
    },
    #[command(about = "Decrypt Vigenère cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Decryption key (alphabetic)")]
        key: String,
    },
}

pub fn run(action: VigenereAction) -> Result<()> {
    match action {
        VigenereAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        VigenereAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
    }
    Ok(())
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let key_upper: Vec<u8> = key.to_uppercase().bytes().collect();
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = key_upper[key_index % key_upper.len()] - b'A';
                key_index += 1;
                (((c as u8 - base) + shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let key_upper: Vec<u8> = key.to_uppercase().bytes().collect();
    let mut key_index = 0;

    Ok(input
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shift = key_upper[key_index % key_upper.len()] - b'A';
                key_index += 1;
                (((c as u8 - base) + 26 - shift) % 26 + base) as char
            } else {
                c
            }
        })
        .collect())
}
