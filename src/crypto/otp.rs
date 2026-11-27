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
                hex_char_count(length)?
            );
            println!("Use a cryptographically secure random source (e.g., /dev/urandom).");
            println!("Example: head -c {length} /dev/urandom | xxd -p | tr -d '\\n'");
        }
    }
    Ok(())
}

pub const MAX_OTP_BYTES: usize = 16 * 1024 * 1024;

fn bounded_otp_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_OTP_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

fn invalid_otp_hex<T>(hex_str: &str) -> Result<T> {
    match hex::decode(hex_str) {
        Err(err) => Err(anyhow::Error::new(err).context("Failed to decode hex input")),
        Ok(_) => Err(anyhow::anyhow!("Failed to decode hex input")),
    }
}

pub fn decode_otp_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_otp_hex(hex_str, max_bytes)?;
    hex::decode(hex_str).context("Failed to decode hex input")
}

pub fn hex_char_count(byte_len: usize) -> Result<usize> {
    byte_len
        .checked_mul(2)
        .context("OTP key length overflows hex character count")
}

pub fn encrypt(input: &str, hex_key: &str) -> Result<String> {
    let hex_key = bounded_otp_hex(hex_key, MAX_OTP_BYTES)?;
    let mut key = super::hex::decode_hex_with_limit(hex_key, MAX_OTP_BYTES)
        .or_else(|_| invalid_otp_hex(hex_key))?;
    let input_bytes = input.as_bytes();
    if key.len() < input_bytes.len() {
        anyhow::bail!(
            "Key length ({} bytes) must be >= input length ({} bytes)",
            key.len(),
            input_bytes.len()
        );
    }
    let used = &mut key[..input_bytes.len()];
    for (b, p) in used.iter_mut().zip(input_bytes) {
        *b ^= p;
    }
    Ok(super::hex::encode_bytes(used))
}

pub fn decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let hex_input = bounded_otp_hex(hex_input, MAX_OTP_BYTES)?;
    let hex_key = bounded_otp_hex(hex_key, MAX_OTP_BYTES)?;
    let mut plain = super::hex::decode_hex_with_limit(hex_input, MAX_OTP_BYTES)
        .or_else(|_| invalid_otp_hex(hex_input))?;
    let key = super::hex::decode_hex_with_limit(hex_key, MAX_OTP_BYTES)
        .or_else(|_| invalid_otp_hex(hex_key))?;
    if key.len() < plain.len() {
        anyhow::bail!(
            "Key length ({} bytes) must be >= ciphertext length ({} bytes)",
            key.len(),
            plain.len()
        );
    }
    for (b, k) in plain.iter_mut().zip(&key) {
        *b ^= k;
    }
    String::from_utf8(plain).context("Decrypted data is not valid UTF-8")
}
