use crate::crypto::rot;
use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum CaesarAction {
    #[command(about = "Encrypt with Caesar cipher")]
    Encrypt {
        #[arg(help = "Input text")]
        input: String,
        #[arg(short, long, help = "Shift amount (0-25)")]
        shift: u8,
    },
    #[command(about = "Decrypt with Caesar cipher")]
    Decrypt {
        #[arg(help = "Encrypted text")]
        input: String,
        #[arg(short, long, help = "Shift amount (0-25)")]
        shift: u8,
    },
    #[command(about = "Brute force all 26 shifts")]
    Bruteforce {
        #[arg(help = "Encrypted text")]
        input: String,
    },
}

pub fn run(action: CaesarAction) -> Result<()> {
    match action {
        CaesarAction::Encrypt { input, shift } => {
            println!("{}", rot::rotate(&input, shift % 26));
        }
        CaesarAction::Decrypt { input, shift } => {
            println!("{}", rot::rotate(&input, (26 - (shift % 26)) % 26));
        }
        CaesarAction::Bruteforce { input } => {
            check_bruteforce_input_len(&input, MAX_BRUTEFORCE_BYTES)?;
            for shift in 0..26 {
                println!("Shift {:2}: {}", shift, rot::rotate(&input, shift));
            }
        }
    }
    Ok(())
}

pub const MAX_BRUTEFORCE_BYTES: usize = 64 * 1024;

pub fn check_bruteforce_input_len(input: &str, max_bytes: usize) -> Result<()> {
    let max_bytes = max_bytes.min(MAX_BRUTEFORCE_BYTES);
    if input.len() > max_bytes {
        anyhow::bail!(
            "Input exceeds maximum size of {max_bytes} bytes to prevent Denial of Service"
        );
    }
    Ok(())
}
