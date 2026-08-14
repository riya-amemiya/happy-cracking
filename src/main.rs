use anyhow::Result;
use clap::{Parser, Subcommand};
use happy_cracking::crypto;

#[derive(Parser)]
#[command(name = "happy-cracking")]
#[command(about = "CTF toolkit for cryptography and more", long_about = None)]
#[command(
    after_help = "This crate also installs hgrep, a parallel grep-compatible line matcher. Run hgrep --help for usage."
)]
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
    #[command(about = "Base62 encode/decode")]
    Base62 {
        #[command(subcommand)]
        action: crypto::base62::Base62Action,
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
    #[command(about = "Baudot/ITA2 telegraph code")]
    Baudot {
        #[command(subcommand)]
        action: crypto::baudot::BaudotAction,
    },
    #[command(about = "Braille encoding")]
    Braille {
        #[command(subcommand)]
        action: crypto::braille::BrailleAction,
    },
    #[command(about = "NATO phonetic alphabet")]
    Nato {
        #[command(subcommand)]
        action: crypto::nato::NatoAction,
    },
    #[command(about = "Phone keypad multi-tap encoding")]
    Phone {
        #[command(subcommand)]
        action: crypto::phone::PhoneAction,
    },
    #[command(about = "Flag semaphore encoding")]
    Semaphore {
        #[command(subcommand)]
        action: crypto::semaphore::SemaphoreAction,
    },
    #[command(about = "Tap code encoding")]
    Tapcode {
        #[command(subcommand)]
        action: crypto::tapcode::TapcodeAction,
    },
    #[command(about = "Base45 (RFC 9285) encode/decode")]
    Base45 {
        #[command(subcommand)]
        action: crypto::base45::Base45Action,
    },
    #[command(about = "uuencode/uudecode")]
    Uuencode {
        #[command(subcommand)]
        action: crypto::uuencode::UuencodeAction,
    },
    #[command(about = "Quoted-Printable (RFC 2045) encode/decode")]
    Qp {
        #[command(subcommand)]
        action: crypto::quotedprintable::QpAction,
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
    #[command(about = "Gronsfeld cipher (numeric Vigenère)")]
    Gronsfeld {
        #[command(subcommand)]
        action: crypto::gronsfeld::GronsfeldAction,
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
    #[command(about = "Four-square cipher")]
    Foursquare {
        #[command(subcommand)]
        action: crypto::foursquare::FoursquareAction,
    },
    #[command(about = "Hill cipher (matrix-based)")]
    Hill {
        #[command(subcommand)]
        action: crypto::hill::HillAction,
    },
    #[command(about = "Bifid cipher")]
    Bifid {
        #[command(subcommand)]
        action: crypto::bifid::BifidAction,
    },
    #[command(about = "ADFGVX cipher")]
    Adfgvx {
        #[command(subcommand)]
        action: crypto::adfgvx::AdfgvxAction,
    },
    #[command(about = "Columnar transposition cipher")]
    Columnar {
        #[command(subcommand)]
        action: crypto::columnar::ColumnarAction,
    },
    #[command(about = "Enigma I/M3/M4 cipher")]
    Enigma {
        #[command(subcommand)]
        action: crypto::enigma::EnigmaAction,
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
    #[command(about = "One-time pad")]
    Otp {
        #[command(subcommand)]
        action: crypto::otp::OtpAction,
    },
    #[command(about = "AES-128 ECB/CBC encrypt/decrypt")]
    Aes {
        #[command(subcommand)]
        action: crypto::aes_cipher::AesCipherAction,
    },
    #[command(about = "DES/Triple-DES encrypt/decrypt")]
    Des {
        #[command(subcommand)]
        action: crypto::des_cipher::DesCipherAction,
    },
    #[command(about = "RC4 stream cipher")]
    Rc4 {
        #[command(subcommand)]
        action: crypto::rc4::Rc4Action,
    },
    #[command(about = "Simple substitution cipher (encode/decode/solve)")]
    Substitution {
        #[command(subcommand)]
        action: crypto::substitution::SubstitutionAction,
    },

    // === Hash / Crypto ===
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
    #[command(about = "HMAC calculation")]
    Hmac {
        #[command(subcommand)]
        action: crypto::hmac::HmacAction,
    },
    #[command(about = "CRC32 checksum")]
    Crc32 {
        #[command(subcommand)]
        action: crypto::crc32::Crc32Action,
    },
    #[command(about = "CRC32 forgery (append bytes for target CRC)")]
    Crc32Forge {
        #[command(subcommand)]
        action: crypto::crc32_forge::Crc32ForgeAction,
    },
    #[command(about = "RSA utilities (compute-d, encrypt, decrypt, attacks)")]
    Rsa {
        #[command(subcommand)]
        action: crypto::rsa::RsaAction,
    },
    #[command(about = "SHA-256 hash length extension attack")]
    HashExt {
        #[command(subcommand)]
        action: crypto::hash_ext::HashExtAction,
    },
    #[command(about = "JWT decode and vulnerability analysis")]
    Jwt {
        #[command(subcommand)]
        action: crypto::jwt::JwtAction,
    },

    #[command(about = "Crack hashes (dictionary, brute-force, table lookup)")]
    Hashcrack {
        #[command(subcommand)]
        action: crypto::hashcrack::HashcrackAction,
    },
    #[command(about = "Crack password-protected zip archives (dict/brute/info)")]
    Zipcrack {
        #[command(subcommand)]
        action: crypto::zipcrack::ZipcrackAction,
    },

    // === Attack / Recon ===
    #[command(about = "Aggressive auto-solve: encodings + classic cipher attacks")]
    Solve {
        #[command(subcommand)]
        action: crypto::solve::SolveAction,
    },
    #[command(about = "Parse nmap output or scan hosts for common open ports")]
    Portscan {
        #[command(subcommand)]
        action: crypto::portscan::PortscanAction,
    },

    // === Advanced Crypto ===
    #[command(about = "Elliptic curve operations over finite fields")]
    Ec {
        #[command(subcommand)]
        action: crypto::ec::EcAction,
    },
    #[command(about = "Diffie-Hellman key exchange and discrete log")]
    Dh {
        #[command(subcommand)]
        action: crypto::dh::DhAction,
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
    #[command(about = "Heuristically identify encoding/cipher of a text")]
    Cipherid {
        #[command(subcommand)]
        action: crypto::cipherid::CipheridAction,
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
    #[command(about = "Hex dump display (xxd-style)")]
    Hexdump {
        #[command(subcommand)]
        action: crypto::hexdump::HexdumpAction,
    },
    #[command(about = "Bit rotation operations")]
    Bitrot {
        #[command(subcommand)]
        action: crypto::bitrot::BitrotAction,
    },
    #[command(about = "Padding scheme utilities (PKCS7, zero padding)")]
    Padding {
        #[command(subcommand)]
        action: crypto::padding::PaddingAction,
    },
    #[command(about = "Identify file type from magic bytes")]
    Filetype {
        #[command(subcommand)]
        action: crypto::filetype::FiletypeAction,
    },
    #[command(about = "Extract printable strings from binary data")]
    Strings {
        #[command(subcommand)]
        action: crypto::strings::StringsAction,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // Encoding
        Commands::Base64 { action } => crypto::base64::run(action)?,
        Commands::Base32 { action } => crypto::base32::run(action)?,
        Commands::Base58 { action } => crypto::base58::run(action)?,
        Commands::Base62 { action } => crypto::base62::run(action)?,
        Commands::Base85 { action } => crypto::base85::run(action)?,
        Commands::Base91 { action } => crypto::base91::run(action)?,
        Commands::Hex { action } => crypto::hex::run(action)?,
        Commands::Url { action } => crypto::url::run(action)?,
        Commands::Binary { action } => crypto::binary::run(action)?,
        Commands::Morse { action } => crypto::morse::run(action)?,
        Commands::A1z26 { action } => crypto::a1z26::run(action)?,
        Commands::Baudot { action } => crypto::baudot::run(action)?,
        Commands::Braille { action } => crypto::braille::run(action)?,
        Commands::Nato { action } => crypto::nato::run(action)?,
        Commands::Phone { action } => crypto::phone::run(action)?,
        Commands::Semaphore { action } => crypto::semaphore::run(action)?,
        Commands::Tapcode { action } => crypto::tapcode::run(action)?,
        Commands::Base45 { action } => crypto::base45::run(action)?,
        Commands::Uuencode { action } => crypto::uuencode::run(action)?,
        Commands::Qp { action } => crypto::quotedprintable::run(action)?,

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
        Commands::Gronsfeld { action } => crypto::gronsfeld::run(action)?,
        Commands::Atbash { action } => crypto::atbash::run(action)?,
        Commands::Railfence { action } => crypto::railfence::run(action)?,
        Commands::Affine { action } => crypto::affine::run(action)?,
        Commands::Playfair { action } => crypto::playfair::run(action)?,
        Commands::Foursquare { action } => crypto::foursquare::run(action)?,
        Commands::Hill { action } => crypto::hill::run(action)?,
        Commands::Bifid { action } => crypto::bifid::run(action)?,
        Commands::Adfgvx { action } => crypto::adfgvx::run(action)?,
        Commands::Columnar { action } => crypto::columnar::run(action)?,
        Commands::Enigma { action } => crypto::enigma::run(action)?,
        Commands::Baconian { action } => crypto::baconian::run(action)?,
        Commands::Polybius { action } => crypto::polybius::run(action)?,
        Commands::Otp { action } => crypto::otp::run(action)?,
        Commands::Aes { action } => crypto::aes_cipher::run(action)?,
        Commands::Des { action } => crypto::des_cipher::run(action)?,
        Commands::Rc4 { action } => crypto::rc4::run(action)?,
        Commands::Substitution { action } => crypto::substitution::run(action)?,

        // Hash / Crypto
        Commands::Hash { action } => crypto::hash::run(action)?,
        Commands::Hashid { action } => crypto::hashid::run(action)?,
        Commands::Hmac { action } => crypto::hmac::run(action)?,
        Commands::Crc32 { action } => crypto::crc32::run(action)?,
        Commands::Crc32Forge { action } => crypto::crc32_forge::run(action)?,
        Commands::Rsa { action } => crypto::rsa::run(action)?,
        Commands::HashExt { action } => crypto::hash_ext::run(action)?,
        Commands::Jwt { action } => crypto::jwt::run(action)?,

        Commands::Hashcrack { action } => crypto::hashcrack::run(action)?,
        Commands::Zipcrack { action } => crypto::zipcrack::run(action)?,
        Commands::Solve { action } => crypto::solve::run(action)?,
        Commands::Portscan { action } => crypto::portscan::run(action)?,

        // Advanced Crypto
        Commands::Ec { action } => crypto::ec::run(action)?,
        Commands::Dh { action } => crypto::dh::run(action)?,

        // Utilities
        Commands::Frequency { action } => crypto::frequency::run(action)?,
        Commands::Auto { action } => crypto::autodecode::run(action)?,
        Commands::Entropy { action } => crypto::entropy::run(action)?,
        Commands::Cipherid { action } => crypto::cipherid::run(action)?,
        Commands::Math { action } => crypto::mathtools::run(action)?,
        Commands::Primes { action } => crypto::primes::run(action)?,
        Commands::Str { action } => crypto::strtools::run(action)?,
        Commands::Numconv { action } => crypto::numbersys::run(action)?,
        Commands::Chain { action } => crypto::chain::run(action)?,
        Commands::Hexdump { action } => crypto::hexdump::run(action)?,
        Commands::Bitrot { action } => crypto::bitrot::run(action)?,
        Commands::Padding { action } => crypto::padding::run(action)?,
        Commands::Filetype { action } => crypto::filetype::run(action)?,
        Commands::Strings { action } => crypto::strings::run(action)?,
    }

    Ok(())
}
