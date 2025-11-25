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
            for shift in 0..26 {
                println!("Shift {:2}: {}", shift, rot::rotate(&input, shift));
            }
        }
    }
    Ok(())
}
