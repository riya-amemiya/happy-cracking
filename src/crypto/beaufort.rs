use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum BeaufortAction {
    #[command(about = "Encrypt with Beaufort cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Encryption key (alphabetic)")]
        key: String,
    },
    #[command(about = "Decrypt Beaufort cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Decryption key (alphabetic)")]
        key: String,
    },
}

pub fn run(action: BeaufortAction) -> Result<()> {
    match action {
        BeaufortAction::Encrypt { input, key } | BeaufortAction::Decrypt { input, key } => {
            println!("{}", cipher(&input, &key)?);
        }
    }
    Ok(())
}

// Beaufort cipher is self-reciprocal: encrypt and decrypt use the same operation
// E(x) = (key - plaintext) mod 26
pub fn cipher(input: &str, key: &str) -> Result<String> {
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
                let k = key_upper[key_index % key_upper.len()] - b'A';
                let p = c.to_ascii_uppercase() as u8 - b'A';
                key_index += 1;
                let encrypted = (i32::from(k) - i32::from(p)).rem_euclid(26) as u8;
                (encrypted + base) as char
            } else {
                c
            }
        })
        .collect())
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    cipher(input, key)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    cipher(input, key)
}
