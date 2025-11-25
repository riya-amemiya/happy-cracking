use anyhow::Result;
use clap::{Parser, Subcommand};
use happy_cracking::crypto;

#[derive(Parser)]
#[command(name = "happy-cracking")]
#[command(about = "CTF toolkit for cryptography and more", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Base64 encode/decode")]
    Base64 {
        #[command(subcommand)]
        action: crypto::base64::Base64Action,
    },
    #[command(about = "Base32 encode/decode")]
    Base32 {
        #[command(subcommand)]
        action: crypto::base32::Base32Action,
    },
    #[command(about = "ROT13 cipher")]
    Rot13 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "Caesar cipher")]
    Caesar {
        #[command(subcommand)]
        action: crypto::caesar::CaesarAction,
    },
    #[command(about = "XOR cipher")]
    Xor {
        #[command(subcommand)]
        action: crypto::xor::XorAction,
    },
    #[command(about = "Hex encode/decode")]
    Hex {
        #[command(subcommand)]
        action: crypto::hex::HexAction,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Base64 { action } => crypto::base64::run(action)?,
        Commands::Base32 { action } => crypto::base32::run(action)?,
        Commands::Rot13 { input } => {
            println!("{}", crypto::rot::rot13(&input));
        }
        Commands::Caesar { action } => crypto::caesar::run(action)?,
        Commands::Xor { action } => crypto::xor::run(action)?,
        Commands::Hex { action } => crypto::hex::run(action)?,
    }

    Ok(())
}
