use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum Rc4Action {
    #[command(about = "RC4 encrypt/decrypt (symmetric, hex key, hex input/output)")]
    Cipher {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(short, long, help = "Key (hex string)")]
        key: String,
    },
    #[command(about = "RC4 encrypt/decrypt with ASCII key")]
    CipherAscii {
        #[arg(help = "Input (ASCII string)")]
        input: String,
        #[arg(short, long, help = "Key (ASCII string)")]
        key: String,
    },
    #[command(about = "Show S-box state after Key Scheduling Algorithm")]
    Sbox {
        #[arg(help = "Key (hex string)")]
        key: String,
    },
}

pub const MAX_RC4_BYTES: usize = 16 * 1024 * 1024;

const HEX_OUT: &[u8; 16] = b"0123456789abcdef";

fn rc4_hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn bounded_rc4_hex(hex_str: &str, max_bytes: usize) -> Result<&str> {
    let hex_str = hex_str.trim();
    let max_bytes = max_bytes.min(MAX_RC4_BYTES);
    let max_chars = max_bytes.saturating_mul(2);
    if hex_str.len() > max_chars {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(hex_str)
}

fn invalid_rc4_hex<T>(hex_str: &str) -> Result<T> {
    match hex::decode(hex_str) {
        Err(err) => Err(anyhow::Error::new(err).context("Failed to decode hex input")),
        Ok(_) => Err(anyhow::anyhow!("Failed to decode hex input")),
    }
}

pub fn decode_rc4_hex_with_limit(hex_str: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let hex_str = bounded_rc4_hex(hex_str, max_bytes)?;
    hex::decode(hex_str).context("Failed to decode hex input")
}

pub fn rc4_hex(input_hex: &str, key: &[u8]) -> Result<(String, Vec<u8>)> {
    rc4_hex_with_limit(input_hex, key, MAX_RC4_BYTES)
}

pub fn rc4_hex_with_limit(
    input_hex: &str,
    key: &[u8],
    max_bytes: usize,
) -> Result<(String, Vec<u8>)> {
    if key.is_empty() {
        anyhow::bail!("RC4 key must not be empty");
    }

    let hex_str = bounded_rc4_hex(input_hex, max_bytes)?;
    let hex_bytes = hex_str.as_bytes();
    let (pairs, rest) = hex_bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return invalid_rc4_hex(hex_str);
    }

    let n = pairs.len();
    let mut hex_out = vec![0u8; hex_bytes.len()];
    let mut plain = vec![0u8; n];
    let mut s = ksa(key);
    let mut i: u8 = 0;
    let mut j: u8 = 0;

    for (idx, pair) in pairs.iter().enumerate() {
        let Some(hi) = rc4_hex_nibble(pair[0]) else {
            return invalid_rc4_hex(hex_str);
        };
        let Some(lo) = rc4_hex_nibble(pair[1]) else {
            return invalid_rc4_hex(hex_str);
        };
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        let out = ((hi << 4) | lo) ^ k;
        plain[idx] = out;
        hex_out[idx * 2] = HEX_OUT[(out >> 4) as usize];
        hex_out[idx * 2 + 1] = HEX_OUT[(out & 0x0f) as usize];
    }

    Ok((
        String::from_utf8(hex_out).expect("RC4 hex output is ASCII"),
        plain,
    ))
}

pub fn cipher_ascii_with_limit(input: &str, key: &str, max_bytes: usize) -> Result<Vec<u8>> {
    let max_bytes = max_bytes.min(MAX_RC4_BYTES);
    if input.len() > max_bytes || key.len() > max_bytes {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    rc4(input.as_bytes(), key.as_bytes())
}

pub fn run(action: Rc4Action) -> Result<()> {
    match action {
        Rc4Action::Cipher { input, key } => {
            let key_bytes = decode_rc4_hex_with_limit(&key, MAX_RC4_BYTES)?;
            let (hex_out, plain) = rc4_hex(&input, &key_bytes)?;
            println!("Hex: {hex_out}");
            if let Ok(s) = String::from_utf8(plain) {
                println!("ASCII: {s}");
            }
        }
        Rc4Action::CipherAscii { input, key } => {
            let result = cipher_ascii_with_limit(&input, &key, MAX_RC4_BYTES)?;
            println!("Hex: {}", hex::encode(&result));
            if let Ok(s) = String::from_utf8(result) {
                println!("ASCII: {s}");
            }
        }
        Rc4Action::Sbox { key } => {
            let key_bytes = decode_rc4_hex_with_limit(&key, MAX_RC4_BYTES)?;
            let sbox = rc4_sbox(&key_bytes)?;
            print_sbox(&sbox);
        }
    }
    Ok(())
}

fn ksa(key: &[u8]) -> [u8; 256] {
    let mut s = [0u8; 256];
    for (i, val) in s.iter_mut().enumerate() {
        *val = i as u8;
    }
    let mut j: u8 = 0;
    let mut key_idx = 0;
    let key_len = key.len();

    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[key_idx]);
        s.swap(i, j as usize);

        key_idx += 1;
        if key_idx >= key_len {
            key_idx = 0;
        }
    }
    s
}

pub fn rc4(data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    if key.is_empty() {
        anyhow::bail!("RC4 key must not be empty");
    }
    let mut s = ksa(key);

    let mut i: u8 = 0;
    let mut j: u8 = 0;
    let mut result = Vec::with_capacity(data.len());

    for &b in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        result.push(b ^ k);
    }

    Ok(result)
}

pub fn rc4_sbox(key: &[u8]) -> Result<[u8; 256]> {
    if key.is_empty() {
        anyhow::bail!("RC4 key must not be empty");
    }
    Ok(ksa(key))
}

fn print_sbox(sbox: &[u8; 256]) {
    println!("S-box state after KSA:");
    println!("     00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F");
    println!("    ------------------------------------------------");
    for row in 0..16 {
        print!("{:02X} |", row * 16);
        for col in 0..16 {
            print!(" {:02X}", sbox[row * 16 + col]);
        }
        println!();
    }
}
