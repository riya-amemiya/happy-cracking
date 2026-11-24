use anyhow::Result;
use clap::{Parser, Subcommand};
use happy_cracking::crypto;

#[derive(Parser)]
#[command(name = "happy-cracking")]
#[command(about = "CTF toolkit for cryptography and more", long_about = None)]
#[command(
    after_help = "This crate also installs hgrep, a parallel grep-compatible line matcher, and hfind, a parallel find-compatible walker. hg and hrg are aliases for hgrep that search directories like rg. hfd is an alias for hfind. Run hgrep --help or hfind --help for usage."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(flatten)]
    Encoding(EncodingCommands),
    #[command(flatten)]
    ClassicCipher(ClassicCipherCommands),
    #[command(flatten)]
    HashCrypto(HashCryptoCommands),
    #[command(flatten)]
    AttackRecon(AttackReconCommands),
    #[command(flatten)]
    AdvancedCrypto(AdvancedCryptoCommands),
    #[command(flatten)]
    Utilities(UtilitiesCommands),
}

// === Encoding ===
#[derive(Subcommand)]
enum EncodingCommands {
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
}

impl EncodingCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Base64 { action } => crypto::base64::run(action),
            Self::Base32 { action } => crypto::base32::run(action),
            Self::Base58 { action } => crypto::base58::run(action),
            Self::Base62 { action } => crypto::base62::run(action),
            Self::Base85 { action } => crypto::base85::run(action),
            Self::Base91 { action } => crypto::base91::run(action),
            Self::Hex { action } => crypto::hex::run(action),
            Self::Url { action } => crypto::url::run(action),
            Self::Binary { action } => crypto::binary::run(action),
            Self::Morse { action } => crypto::morse::run(action),
            Self::A1z26 { action } => crypto::a1z26::run(action),
            Self::Baudot { action } => crypto::baudot::run(action),
            Self::Braille { action } => crypto::braille::run(action),
            Self::Nato { action } => crypto::nato::run(action),
            Self::Phone { action } => crypto::phone::run(action),
            Self::Semaphore { action } => crypto::semaphore::run(action),
            Self::Tapcode { action } => crypto::tapcode::run(action),
            Self::Base45 { action } => crypto::base45::run(action),
            Self::Uuencode { action } => crypto::uuencode::run(action),
            Self::Qp { action } => crypto::quotedprintable::run(action),
        }
    }
}

// === Classic Ciphers ===
#[derive(Subcommand)]
enum ClassicCipherCommands {
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
}

impl ClassicCipherCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Rot13 { input } => {
                println!("{}", crypto::rot::rot13(&input));
                Ok(())
            }
            Self::Rot47 { input } => {
                println!("{}", crypto::rot::rot47(&input));
                Ok(())
            }
            Self::Caesar { action } => crypto::caesar::run(action),
            Self::Xor { action } => crypto::xor::run(action),
            Self::Vigenere { action } => crypto::vigenere::run(action),
            Self::Beaufort { action } => crypto::beaufort::run(action),
            Self::Gronsfeld { action } => crypto::gronsfeld::run(action),
            Self::Atbash { action } => crypto::atbash::run(action),
            Self::Railfence { action } => crypto::railfence::run(action),
            Self::Affine { action } => crypto::affine::run(action),
            Self::Playfair { action } => crypto::playfair::run(action),
            Self::Foursquare { action } => crypto::foursquare::run(action),
            Self::Hill { action } => crypto::hill::run(action),
            Self::Bifid { action } => crypto::bifid::run(action),
            Self::Adfgvx { action } => crypto::adfgvx::run(action),
            Self::Columnar { action } => crypto::columnar::run(action),
            Self::Enigma { action } => crypto::enigma::run(action),
            Self::Baconian { action } => crypto::baconian::run(action),
            Self::Polybius { action } => crypto::polybius::run(action),
            Self::Otp { action } => crypto::otp::run(action),
            Self::Aes { action } => crypto::aes_cipher::run(action),
            Self::Des { action } => crypto::des_cipher::run(action),
            Self::Rc4 { action } => crypto::rc4::run(action),
            Self::Substitution { action } => crypto::substitution::run(action),
        }
    }
}

// === Hash / Crypto ===
#[derive(Subcommand)]
enum HashCryptoCommands {
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
    #[command(about = "Crack password-protected zip archives (dict/brute/mask/random/info)")]
    Zipcrack {
        #[command(subcommand)]
        action: crypto::zipcrack::ZipcrackAction,
    },
}

impl HashCryptoCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Hash { action } => crypto::hash::run(action),
            Self::Hashid { action } => crypto::hashid::run(action),
            Self::Hmac { action } => crypto::hmac::run(action),
            Self::Crc32 { action } => crypto::crc32::run(action),
            Self::Crc32Forge { action } => crypto::crc32_forge::run(action),
            Self::Rsa { action } => crypto::rsa::run(action),
            Self::HashExt { action } => crypto::hash_ext::run(action),
            Self::Jwt { action } => crypto::jwt::run(action),
            Self::Hashcrack { action } => crypto::hashcrack::run(action),
            Self::Zipcrack { action } => crypto::zipcrack::run(action),
        }
    }
}

// === Attack / Recon ===
#[derive(Subcommand)]
enum AttackReconCommands {
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
    #[command(
        name = "httpload",
        about = "Send concurrent HTTP requests and report throughput and latency"
    )]
    HttpLoad {
        #[command(subcommand)]
        action: crypto::httpload::HttpLoadAction,
    },
}

impl AttackReconCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Solve { action } => crypto::solve::run(action),
            Self::Portscan { action } => crypto::portscan::run(action),
            Self::HttpLoad { action } => crypto::httpload::run(action),
        }
    }
}

// === Advanced Crypto ===
#[derive(Subcommand)]
enum AdvancedCryptoCommands {
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
}

impl AdvancedCryptoCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Ec { action } => crypto::ec::run(action),
            Self::Dh { action } => crypto::dh::run(action),
        }
    }
}

// === Utilities ===
#[derive(Subcommand)]
enum UtilitiesCommands {
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
    #[command(about = "Generate random, exhaustive, or masked strings")]
    Wordgen {
        #[command(subcommand)]
        action: crypto::wordgen::WordgenAction,
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

impl UtilitiesCommands {
    fn run(self) -> Result<()> {
        match self {
            Self::Frequency { action } => crypto::frequency::run(action),
            Self::Auto { action } => crypto::autodecode::run(action),
            Self::Entropy { action } => crypto::entropy::run(action),
            Self::Cipherid { action } => crypto::cipherid::run(action),
            Self::Math { action } => crypto::mathtools::run(action),
            Self::Primes { action } => crypto::primes::run(action),
            Self::Str { action } => crypto::strtools::run(action),
            Self::Wordgen { action } => crypto::wordgen::run(action),
            Self::Numconv { action } => crypto::numbersys::run(action),
            Self::Chain { action } => crypto::chain::run(action),
            Self::Hexdump { action } => crypto::hexdump::run(action),
            Self::Bitrot { action } => crypto::bitrot::run(action),
            Self::Padding { action } => crypto::padding::run(action),
            Self::Filetype { action } => crypto::filetype::run(action),
            Self::Strings { action } => crypto::strings::run(action),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Encoding(command) => command.run(),
        Commands::ClassicCipher(command) => command.run(),
        Commands::HashCrypto(command) => command.run(),
        Commands::AttackRecon(command) => command.run(),
        Commands::AdvancedCrypto(command) => command.run(),
        Commands::Utilities(command) => command.run(),
    }
}
