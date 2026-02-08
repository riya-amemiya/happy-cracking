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
    // === Encoding ===
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
    #[command(about = "Base58 encode/decode")]
    Base58 {
        #[command(subcommand)]
        action: crypto::base58::Base58Action,
    },
    #[command(about = "Base85 (ASCII85) encode/decode")]
    Base85 {
        #[command(subcommand)]
        action: crypto::base85::Base85Action,
    },
    #[command(about = "Base91 encode/decode")]
    Base91 {
        #[command(subcommand)]
        action: crypto::base91::Base91Action,
    },
    #[command(about = "Hex encode/decode")]
    Hex {
        #[command(subcommand)]
        action: crypto::hex::HexAction,
    },
    #[command(about = "URL encode/decode")]
    Url {
        #[command(subcommand)]
        action: crypto::url::UrlAction,
    },
    #[command(about = "Binary encode/decode")]
    Binary {
        #[command(subcommand)]
        action: crypto::binary::BinaryAction,
    },
    #[command(about = "Morse code encode/decode")]
    Morse {
        #[command(subcommand)]
        action: crypto::morse::MorseAction,
    },
    #[command(about = "A1Z26 cipher (A=1, B=2, ..., Z=26)")]
    A1z26 {
        #[command(subcommand)]
        action: crypto::a1z26::A1z26Action,
    },

    // === Classic Ciphers ===
    #[command(about = "ROT13 cipher")]
    Rot13 {
        #[arg(help = "Input text")]
        input: String,
    },
    #[command(about = "ROT47 cipher")]
    Rot47 {
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
    #[command(about = "Vigenère cipher")]
    Vigenere {
        #[command(subcommand)]
        action: crypto::vigenere::VigenereAction,
    },
    #[command(about = "Beaufort cipher")]
    Beaufort {
        #[command(subcommand)]
        action: crypto::beaufort::BeaufortAction,
    },
    #[command(about = "Atbash cipher (A↔Z)")]
    Atbash {
        #[command(subcommand)]
        action: crypto::atbash::AtbashAction,
    },
    #[command(about = "Rail Fence cipher")]
    Railfence {
        #[command(subcommand)]
        action: crypto::railfence::RailFenceAction,
    },
    #[command(about = "Affine cipher (ax+b mod 26)")]
    Affine {
        #[command(subcommand)]
        action: crypto::affine::AffineAction,
    },
    #[command(about = "Playfair cipher")]
    Playfair {
        #[command(subcommand)]
        action: crypto::playfair::PlayfairAction,
    },
    #[command(about = "Columnar transposition cipher")]
    Columnar {
        #[command(subcommand)]
        action: crypto::columnar::ColumnarAction,
    },
    #[command(about = "Baconian cipher (5-bit A/B encoding)")]
    Baconian {
        #[command(subcommand)]
        action: crypto::baconian::BaconianAction,
    },
    #[command(about = "Polybius square cipher")]
    Polybius {
        #[command(subcommand)]
        action: crypto::polybius::PolybiusAction,
    },

    // === Hash ===
    #[command(about = "Generate hash (MD5, SHA1, SHA256, SHA512)")]
    Hash {
        #[command(subcommand)]
        action: crypto::hash::HashAction,
    },
    #[command(about = "Identify hash type")]
    Hashid {
        #[command(subcommand)]
        action: crypto::hashid::HashIdAction,
    },

    // === Utilities ===
    #[command(about = "Character frequency analysis")]
    Frequency {
        #[command(subcommand)]
        action: crypto::frequency::FrequencyAction,
    },
    #[command(about = "Auto-detect and decode")]
    Auto {
        #[command(subcommand)]
        action: crypto::autodecode::AutoDecodeAction,
    },
    #[command(about = "Shannon entropy analysis")]
    Entropy {
        #[command(subcommand)]
        action: crypto::entropy::EntropyAction,
    },
    #[command(about = "Number theory tools (GCD, modular inverse, etc.)")]
    Math {
        #[command(subcommand)]
        action: crypto::mathtools::MathAction,
    },
    #[command(about = "Prime factorization and primality test")]
    Primes {
        #[command(subcommand)]
        action: crypto::primes::PrimesAction,
    },
    #[command(about = "String manipulation tools")]
    Str {
        #[command(subcommand)]
        action: crypto::strtools::StrToolsAction,
    },
    #[command(about = "Number base conversion (2-36)")]
    Numconv {
        #[command(subcommand)]
        action: crypto::numbersys::NumberSysAction,
    },
    #[command(about = "Chain multiple operations (CyberChef-style)")]
    Chain {
        #[command(subcommand)]
        action: crypto::chain::ChainAction,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // Encoding
        Commands::Base64 { action } => crypto::base64::run(action)?,
        Commands::Base32 { action } => crypto::base32::run(action)?,
        Commands::Base58 { action } => crypto::base58::run(action)?,
        Commands::Base85 { action } => crypto::base85::run(action)?,
        Commands::Base91 { action } => crypto::base91::run(action)?,
        Commands::Hex { action } => crypto::hex::run(action)?,
        Commands::Url { action } => crypto::url::run(action)?,
        Commands::Binary { action } => crypto::binary::run(action)?,
        Commands::Morse { action } => crypto::morse::run(action)?,
        Commands::A1z26 { action } => crypto::a1z26::run(action)?,

        // Classic Ciphers
        Commands::Rot13 { input } => {
            println!("{}", crypto::rot::rot13(&input));
        }
        Commands::Rot47 { input } => {
            println!("{}", crypto::rot::rot47(&input));
        }
        Commands::Caesar { action } => crypto::caesar::run(action)?,
        Commands::Xor { action } => crypto::xor::run(action)?,
        Commands::Vigenere { action } => crypto::vigenere::run(action)?,
        Commands::Beaufort { action } => crypto::beaufort::run(action)?,
        Commands::Atbash { action } => crypto::atbash::run(action)?,
        Commands::Railfence { action } => crypto::railfence::run(action)?,
        Commands::Affine { action } => crypto::affine::run(action)?,
        Commands::Playfair { action } => crypto::playfair::run(action)?,
        Commands::Columnar { action } => crypto::columnar::run(action)?,
        Commands::Baconian { action } => crypto::baconian::run(action)?,
        Commands::Polybius { action } => crypto::polybius::run(action)?,

        // Hash
        Commands::Hash { action } => crypto::hash::run(action)?,
        Commands::Hashid { action } => crypto::hashid::run(action)?,

        // Utilities
        Commands::Frequency { action } => crypto::frequency::run(action)?,
        Commands::Auto { action } => crypto::autodecode::run(action)?,
        Commands::Entropy { action } => crypto::entropy::run(action)?,
        Commands::Math { action } => crypto::mathtools::run(action)?,
        Commands::Primes { action } => crypto::primes::run(action)?,
        Commands::Str { action } => crypto::strtools::run(action)?,
        Commands::Numconv { action } => crypto::numbersys::run(action)?,
        Commands::Chain { action } => crypto::chain::run(action)?,
    }

    Ok(())
}
