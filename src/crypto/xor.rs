use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum XorAction {
    #[command(about = "XOR with a key")]
    Cipher {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(short, long, help = "Key (hex string or ASCII with --ascii flag)")]
        key: String,
        #[arg(long, help = "Treat key as ASCII string")]
        ascii: bool,
    },
    #[command(about = "Brute force single-byte XOR")]
    Bruteforce {
        #[arg(help = "Input (hex string)")]
        input: String,
        #[arg(long, help = "Only show printable ASCII results")]
        printable: bool,
    },
}

pub fn run(action: XorAction) -> Result<()> {
    match action {
        XorAction::Cipher { input, key, ascii } => run_cipher(&input, &key, ascii),
        XorAction::Bruteforce { input, printable } => run_bruteforce(&input, printable),
    }
}

fn run_cipher(input: &str, key: &str, ascii_key: bool) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;

    let key_bytes = if ascii_key {
        key.as_bytes().to_vec()
    } else {
        hex::decode(key.trim()).context("Failed to decode key hex")?
    };

    let result = xor_bytes(&input_bytes, &key_bytes);
    println!("Hex: {}", hex::encode(&result));

    if let Ok(s) = String::from_utf8(result.clone()) {
        println!("ASCII: {}", s);
    }

    Ok(())
}

fn run_bruteforce(input: &str, printable_only: bool) -> Result<()> {
    let input_bytes = hex::decode(input.trim()).context("Failed to decode input hex")?;

    for (key, result) in single_byte_xor_bruteforce(&input_bytes) {
        if printable_only {
            if result.iter().all(|&b| b.is_ascii_graphic() || b == b' ')
                && let Ok(s) = String::from_utf8(result)
            {
                println!("Key 0x{:02x}: {}", key, s);
            }
        } else if let Ok(s) = String::from_utf8(result.clone()) {
            println!("Key 0x{:02x}: {}", key, s);
        }
    }

    Ok(())
}

pub fn xor_bytes(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    data.iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect()
}

pub fn single_byte_xor_bruteforce(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
    (0..=255)
        .map(|key| (key, xor_bytes(data, &[key])))
        .collect()
}
