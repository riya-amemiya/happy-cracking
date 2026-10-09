use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::BigUint;
use num_traits::Zero;

const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BASE: u32 = 58;

const DECODE_TABLE: [u8; 256] = {
    let mut table = [255u8; 256];
    let mut i = 0;
    while i < 58 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

#[derive(Subcommand)]
pub enum Base58Action {
    #[command(about = "Encode to Base58")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base58")]
    Decode {
        #[arg(help = "Base58 encoded string")]
        input: String,
    },
}

pub fn run(action: Base58Action) -> Result<()> {
    match action {
        Base58Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base58Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

#[must_use]
pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();
    let num = BigUint::from_bytes_be(bytes);

    let digits = if num.is_zero() {
        Vec::new()
    } else {
        num.to_radix_be(BASE)
    };

    let mut encoded = Vec::with_capacity(leading_zeros + digits.len());
    encoded.resize(leading_zeros, ALPHABET[0]);
    for d in digits {
        encoded.push(ALPHABET[d as usize]);
    }
    String::from_utf8(encoded).expect("Base58 alphabet is ASCII")
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let bytes = input.as_bytes();
    let leading_zeros = bytes.iter().take_while(|&&b| b == ALPHABET[0]).count();
    let rest = &bytes[leading_zeros..];

    let mut result = vec![0u8; leading_zeros];
    if !rest.is_empty() {
        let mut digits = Vec::with_capacity(rest.len());
        for &c in rest {
            let digit = DECODE_TABLE[c as usize];
            if digit == 255 {
                anyhow::bail!("Invalid base58 character: {}", c as char);
            }
            digits.push(digit);
        }

        let num = BigUint::from_radix_be(&digits, BASE).context("Failed to decode Base58")?;
        if !num.is_zero() {
            result.extend(num.to_bytes_be());
        }
    }

    String::from_utf8(result).context("Decoded data is not valid UTF-8")
}
