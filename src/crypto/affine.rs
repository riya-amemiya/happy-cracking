use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum AffineAction {
    #[command(about = "Encrypt with Affine cipher (ax + b mod 26)")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Multiplier 'a' (must be coprime with 26)")]
        a: i32,
        #[arg(short, long, help = "Shift 'b'")]
        b: i32,
    },
    #[command(about = "Decrypt Affine cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Multiplier 'a' (must be coprime with 26)")]
        a: i32,
        #[arg(short, long, help = "Shift 'b'")]
        b: i32,
    },
    #[command(about = "Bruteforce all possible Affine cipher keys")]
    Bruteforce {
        #[arg(help = "Encrypted text")]
        input: String,
    },
}

pub fn run(action: AffineAction) -> Result<()> {
    match action {
        AffineAction::Encrypt { input, a, b } => {
            println!("{}", encrypt(&input, a, b)?);
        }
        AffineAction::Decrypt { input, a, b } => {
            println!("{}", decrypt(&input, a, b)?);
        }
        AffineAction::Bruteforce { input } => {
            bruteforce(&input);
        }
    }
    Ok(())
}

// Valid 'a' values (coprime with 26)
const VALID_A: [i32; 12] = [1, 3, 5, 7, 9, 11, 15, 17, 19, 21, 23, 25];

fn mod_inverse(a: i32, m: i32) -> Option<i32> {
    let a = ((a % m) + m) % m;
    (1..m).find(|&x| (a * x) % m == 1)
}

pub fn encrypt(input: &str, a: i32, b: i32) -> Result<String> {
    if crate::crypto::mathtools::gcd(a.unsigned_abs() as u128, 26) != 1 {
        anyhow::bail!("'a' must be coprime with 26. Valid values: {:?}", VALID_A);
    }

    let mut bytes = input.as_bytes().to_vec();
    for byte in &mut bytes {
        if byte.is_ascii_alphabetic() {
            let base = if byte.is_ascii_uppercase() {
                b'A'
            } else {
                b'a'
            };
            let x = (*byte - base) as i32;
            let encrypted = ((a * x + b) % 26 + 26) % 26;
            *byte = encrypted as u8 + base;
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("Internal error: produced invalid UTF-8 during encryption"))
}

pub fn decrypt(input: &str, a: i32, b: i32) -> Result<String> {
    let a_inv = mod_inverse(a, 26).ok_or_else(|| {
        anyhow::anyhow!("'a' must be coprime with 26. Valid values: {:?}", VALID_A)
    })?;

    let mut bytes = input.as_bytes().to_vec();
    for byte in &mut bytes {
        if byte.is_ascii_alphabetic() {
            let base = if byte.is_ascii_uppercase() {
                b'A'
            } else {
                b'a'
            };
            let y = (*byte - base) as i32;
            let decrypted = ((a_inv * (y - b)) % 26 + 26) % 26;
            *byte = decrypted as u8 + base;
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("Internal error: produced invalid UTF-8 during decryption"))
}

pub fn bruteforce(input: &str) {
    for &a in &VALID_A {
        for b in 0..26 {
            if let Ok(result) = decrypt(input, a, b) {
                println!("a={:2}, b={:2}: {}", a, b, result);
            }
        }
    }
}
