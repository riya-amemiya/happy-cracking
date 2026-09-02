use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum OtpAction {
    #[command(about = "Encrypt with one-time pad (ASCII input, hex key, hex output)")]
    Encrypt {
        #[arg(help = "Plaintext (ASCII)")]
        input: String,
        #[arg(
            short,
            long,
            help = "Key (hex string, must be >= input length in bytes)"
        )]
        key: String,
    },
    #[command(about = "Decrypt one-time pad (hex input, hex key, ASCII output)")]
    Decrypt {
        #[arg(help = "Ciphertext (hex string)")]
        input: String,
        #[arg(short, long, help = "Key (hex string)")]
        key: String,
    },
    #[command(about = "Show required key length for a given plaintext length")]
    Generate {
        #[arg(help = "Required key length in bytes")]
        length: usize,
    },
}

pub fn run(action: OtpAction) -> Result<()> {
    match action {
        OtpAction::Encrypt { input, key } => {
            println!("{}", encrypt(&input, &key)?);
        }
        OtpAction::Decrypt { input, key } => {
            println!("{}", decrypt(&input, &key)?);
        }
        OtpAction::Generate { length } => {
            println!(
                "Generate a truly random key of {} bytes ({} hex characters).",
                length,
                length * 2
            );
            println!("Use a cryptographically secure random source (e.g., /dev/urandom).");
            println!("Example: head -c {length} /dev/urandom | xxd -p | tr -d '\\n'");
        }
    }
    Ok(())
}

pub fn encrypt(input: &str, hex_key: &str) -> Result<String> {
    let key_bytes = hex::decode(hex_key.trim()).context("Failed to decode hex key")?;
    let input_bytes = input.as_bytes();

    if key_bytes.len() < input_bytes.len() {
        anyhow::bail!(
            "Key length ({} bytes) must be >= input length ({} bytes)",
            key_bytes.len(),
            input_bytes.len()
        );
    }

    let cipher: Vec<u8> = input_bytes
        .iter()
        .zip(key_bytes.iter())
        .map(|(&p, &k)| p ^ k)
        .collect();

    Ok(hex::encode(cipher))
}

pub fn decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let input_bytes = hex::decode(hex_input.trim()).context("Failed to decode hex ciphertext")?;
    let key_bytes = hex::decode(hex_key.trim()).context("Failed to decode hex key")?;

    if key_bytes.len() < input_bytes.len() {
        anyhow::bail!(
            "Key length ({} bytes) must be >= ciphertext length ({} bytes)",
            key_bytes.len(),
            input_bytes.len()
        );
    }

    let plain: Vec<u8> = input_bytes
        .iter()
        .zip(key_bytes.iter())
        .map(|(&c, &k)| c ^ k)
        .collect();

    String::from_utf8(plain).context("Decrypted data is not valid UTF-8")
}
