use anyhow::{Context, Result};
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
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() || !key_bytes.iter().all(u8::is_ascii_alphabetic) {
        anyhow::bail!("Key must be non-empty and contain only alphabetic characters");
    }

    let shifts: Vec<u8> = key_bytes.iter().map(|&kb| (kb | 0x20) - b'a').collect();
    let mut bytes = input.as_bytes().to_vec();
    let key_len = shifts.len();
    let mut key_index = 0usize;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let x = shifts[key_index] + 26 - (*b - base);
            *b = if x >= 26 { x - 26 } else { x } + base;
            key_index += 1;
            if key_index == key_len {
                key_index = 0;
            }
        }
    }

    String::from_utf8(bytes).context("beaufort cipher: produced invalid UTF-8")
}

pub fn encrypt(input: &str, key: &str) -> Result<String> {
    cipher(input, key)
}

pub fn decrypt(input: &str, key: &str) -> Result<String> {
    cipher(input, key)
}
