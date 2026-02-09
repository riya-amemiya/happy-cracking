use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::BigUint;
use num_traits::Zero;

#[derive(Subcommand)]
pub enum Base62Action {
    #[command(about = "Encode to Base62")]
    Encode {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Decode from Base62")]
    Decode {
        #[arg(help = "Base62 encoded string")]
        input: String,
    },
}

pub fn run(action: Base62Action) -> Result<()> {
    match action {
        Base62Action::Encode { input } => {
            println!("{}", encode(&input));
        }
        Base62Action::Decode { input } => {
            println!("{}", decode(&input)?);
        }
    }
    Ok(())
}

const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: u32 = 62;

pub fn encode(input: &str) -> String {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }

    let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();

    let mut num = BigUint::from_bytes_be(bytes);
    let base = BigUint::from(BASE);
    let mut encoded = Vec::new();

    while !num.is_zero() {
        let remainder = &num % &base;
        let digit = remainder.to_u32_digits().first().copied().unwrap_or(0) as usize;
        encoded.push(ALPHABET[digit] as char);
        num /= &base;
    }

    for _ in 0..leading_zeros {
        encoded.push(ALPHABET[0] as char);
    }

    encoded.iter().rev().collect()
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let leading_zeros = input.bytes().take_while(|&b| b == ALPHABET[0]).count();

    let base = BigUint::from(BASE);
    let mut num = BigUint::zero();

    for c in input.bytes() {
        let digit = ALPHABET
            .iter()
            .position(|&a| a == c)
            .context(format!("Invalid Base62 character: {}", c as char))?;
        num = num * &base + BigUint::from(digit as u32);
    }

    let mut bytes = num.to_bytes_be();

    let mut result = vec![0u8; leading_zeros];
    result.append(&mut bytes);

    if result.is_empty() {
        return Ok(String::new());
    }

    String::from_utf8(result).context("Decoded data is not valid UTF-8")
}
