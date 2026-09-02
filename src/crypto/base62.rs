use anyhow::{Context, Result};
use clap::Subcommand;
use num_bigint::BigUint;
use num_traits::Zero;
use std::sync::LazyLock;

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

// Reverse lookup table: maps byte value -> alphabet index (0..61), or 0xFF for invalid.
static DECODE_TABLE: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0xFFu8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        table[c as usize] = i as u8;
    }
    table
});

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
    String::from_utf8(encoded).expect("Base62 alphabet is ASCII")
}

pub fn decode(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(String::new());
    }

    let leading_zeros = input.bytes().take_while(|&b| b == ALPHABET[0]).count();

    let mut digits = Vec::with_capacity(input.len());
    for c in input.bytes() {
        let digit = DECODE_TABLE[c as usize];
        if digit == 0xFF {
            anyhow::bail!("Invalid Base62 character: {}", c as char);
        }
        digits.push(digit);
    }

    let num = BigUint::from_radix_be(&digits, BASE).context("Invalid Base62 digit")?;

    let mut bytes = num.to_bytes_be();

    let mut result = vec![0u8; leading_zeros];
    result.append(&mut bytes);

    if result.is_empty() {
        return Ok(String::new());
    }

    String::from_utf8(result).context("Decoded data is not valid UTF-8")
}
