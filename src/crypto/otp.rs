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

const HEX_OUT: &[u8; 16] = b"0123456789abcdef";

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

fn otp_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn invalid_otp_hex(hex_str: &str) -> Result<String> {
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
    let key_hex = hex_key.as_bytes();
    if !key_hex.len().is_multiple_of(2) {
        return invalid_otp_hex(hex_key);
    }

    let input_bytes = input.as_bytes();
    let key_len = key_hex.len() / 2;
    if key_len < input_bytes.len() {
        if key_hex.iter().any(|&b| otp_hex_nibble(b).is_none()) {
            return invalid_otp_hex(hex_key);
        }
        anyhow::bail!(
            "Key length ({} bytes) must be >= input length ({} bytes)",
            key_len,
            input_bytes.len()
        );
    }

    let mut out = vec![0u8; input_bytes.len().saturating_mul(2)];
    for (i, &plain) in input_bytes.iter().enumerate() {
        let Some(hi) = otp_hex_nibble(key_hex[i * 2]) else {
            return invalid_otp_hex(hex_key);
        };
        let Some(lo) = otp_hex_nibble(key_hex[i * 2 + 1]) else {
            return invalid_otp_hex(hex_key);
        };
        let xor = plain ^ ((hi << 4) | lo);
        out[i * 2] = HEX_OUT[(xor >> 4) as usize];
        out[i * 2 + 1] = HEX_OUT[(xor & 0x0f) as usize];
    }
    if key_hex[input_bytes.len() * 2..]
        .iter()
        .any(|&b| otp_hex_nibble(b).is_none())
    {
        return invalid_otp_hex(hex_key);
    }
    Ok(String::from_utf8(out).expect("OTP hex output is ASCII"))
}

pub fn decrypt(hex_input: &str, hex_key: &str) -> Result<String> {
    let input_bytes = decode_otp_hex_with_limit(hex_input, MAX_OTP_BYTES)?;
    let key_bytes = decode_otp_hex_with_limit(hex_key, MAX_OTP_BYTES)?;

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
