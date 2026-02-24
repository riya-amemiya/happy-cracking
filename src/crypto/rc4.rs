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

pub fn run(action: Rc4Action) -> Result<()> {
    match action {
        Rc4Action::Cipher { input, key } => {
            let key_bytes = hex::decode(key.trim()).context("Failed to decode hex key")?;
            let input_bytes = hex::decode(input.trim()).context("Failed to decode hex input")?;
            let result = rc4(&input_bytes, &key_bytes)?;
            println!("Hex: {}", hex::encode(&result));
            if let Ok(s) = String::from_utf8(result) {
                println!("ASCII: {}", s);
            }
        }
        Rc4Action::CipherAscii { input, key } => {
            let result = rc4(input.as_bytes(), key.as_bytes())?;
            println!("Hex: {}", hex::encode(&result));
            if let Ok(s) = String::from_utf8(result) {
                println!("ASCII: {}", s);
            }
        }
        Rc4Action::Sbox { key } => {
            let key_bytes = hex::decode(key.trim()).context("Failed to decode hex key")?;
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

    // Optimization: Use a manual counter instead of expensive modulo operator
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

    // Optimization: Stream processing to avoid intermediate allocation of keystream vector
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
