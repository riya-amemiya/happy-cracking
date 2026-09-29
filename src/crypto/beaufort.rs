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

    let mut bytes = input.as_bytes().to_vec();
    let key_len = key_bytes.len();
    let mut key_index = 0usize;

    for b in &mut bytes {
        if b.is_ascii_alphabetic() {
            let base = if b.is_ascii_uppercase() { b'A' } else { b'a' };
            let k = (key_bytes[key_index % key_len] | 0x20) - b'a';
            let p = *b - base;
            *b = (k + 26 - p) % 26 + base;
            key_index += 1;
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
