# CLAUDE.md - AI Assistant Guide for happy-cracking

## Project Overview

**happy-cracking** is a CTF (Capture The Flag) toolkit written in Rust, providing command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, and analysis tools commonly used in security competitions.

## Codebase Structure

```tree
happy-cracking/
├── src/
│   ├── main.rs           # CLI entry point with clap subcommands
│   ├── lib.rs            # Library root, exposes crypto module
│   └── crypto/           # Cryptographic operations
│       ├── mod.rs        # Module exports
│       │
│       │ # Encoding
│       ├── a1z26.rs      # A=1, Z=26 number-letter cipher
│       ├── base32.rs     # Base32 encode/decode
│       ├── base58.rs     # Base58 encode/decode
│       ├── base62.rs     # Base62 encode/decode
│       ├── base64.rs     # Base64 encode/decode
│       ├── base85.rs     # Base85 (ASCII85) encode/decode
│       ├── base91.rs     # Base91 encode/decode
│       ├── baudot.rs     # Baudot/ITA2 telegraph code
│       ├── binary.rs     # Binary (8-bit) encode/decode
│       ├── braille.rs    # Braille encoding
│       ├── hex.rs        # Hex encode/decode
│       ├── morse.rs      # Morse code encode/decode
│       ├── nato.rs       # NATO phonetic alphabet
│       ├── phone.rs      # Phone keypad multi-tap
│       ├── semaphore.rs  # Flag semaphore encoding
│       ├── tapcode.rs    # Tap code encoding
│       ├── url.rs        # URL encode/decode (percent-encoding)
│       │
│       │ # Classic Ciphers
│       ├── adfgvx.rs     # ADFGVX cipher
│       ├── aes_cipher.rs # AES-128 ECB/CBC encrypt/decrypt
│       ├── affine.rs     # Affine cipher (ax+b mod 26)
│       ├── atbash.rs     # Atbash cipher (A↔Z substitution)
│       ├── baconian.rs   # Baconian cipher (5-bit A/B encoding)
│       ├── beaufort.rs   # Beaufort cipher (self-reciprocal Vigenere variant)
│       ├── bifid.rs      # Bifid cipher
│       ├── caesar.rs     # Caesar cipher encrypt/decrypt/bruteforce
│       ├── columnar.rs   # Columnar transposition cipher
│       ├── des_cipher.rs # DES/Triple-DES encrypt/decrypt
│       ├── foursquare.rs # Four-square cipher
│       ├── gronsfeld.rs  # Gronsfeld cipher (numeric Vigenere)
│       ├── hill.rs       # Hill cipher (matrix-based)
│       ├── otp.rs        # One-time pad
│       ├── playfair.rs   # Playfair cipher (5x5 digraph substitution)
│       ├── polybius.rs   # Polybius square cipher
│       ├── railfence.rs  # Rail Fence transposition cipher
│       ├── rc4.rs        # RC4 stream cipher
│       ├── rot.rs        # ROT13, ROT47, and generic rotation cipher
│       ├── substitution.rs # Simple substitution cipher
│       ├── vigenere.rs   # Vigenere polyalphabetic cipher
│       ├── xor.rs        # XOR cipher and single-byte bruteforce
│       │
│       │ # Hash / Crypto
│       ├── crc32.rs      # CRC32 checksum
│       ├── crc32_forge.rs # CRC32 forgery
│       ├── hash.rs       # Generate MD5, SHA1, SHA256, SHA512
│       ├── hash_ext.rs   # Hash length extension attack
│       ├── hashid.rs     # Identify hash type from string
│       ├── hmac.rs       # HMAC calculation
│       ├── jwt.rs        # JWT decode and analysis
│       ├── rsa.rs        # RSA utilities
│       │
│       │ # Advanced Crypto
│       ├── dh.rs         # Diffie-Hellman key exchange
│       ├── ec.rs         # Elliptic curve operations
│       │
│       │ # Utilities
│       ├── autodecode.rs # Auto-detect and decode common encodings
│       ├── bitrot.rs     # Bit rotation operations
│       ├── chain.rs      # Chain multiple operations (CyberChef-style)
│       ├── entropy.rs    # Shannon entropy analysis
│       ├── frequency.rs  # Character frequency analysis
│       ├── hexdump.rs    # Hex dump display
│       ├── mathtools.rs  # Number theory (GCD, modinv, modpow)
│       ├── numbersys.rs  # Number base conversion (2-36)
│       ├── padding.rs    # Padding scheme utilities
│       ├── primes.rs     # Prime factorization and primality test
│       └── strtools.rs   # String tools (reverse, ord, chr)
│
├── tests/                # Integration tests
│   ├── ...
│
├── Cargo.toml            # Project manifest (Rust 2024 edition)
├── Cargo.lock            # Dependency lock file
└── .github/workflows/
    └── static-check.yml  # CI pipeline
```

## Development Commands

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Format code (required for CI)
cargo fmt

# Run clippy linter (must pass with no warnings)
cargo clippy -- -D warnings

# Run the CLI
cargo run -- <command>
```

## CLI Usage Examples

### Encoding

```bash
# Base64
cargo run -- base64 encode "Hello, World!"
cargo run -- base64 decode "SGVsbG8sIFdvcmxkIQ=="

# Base32
cargo run -- base32 encode "Hello"
cargo run -- base32 decode "JBSWY3DP"

# Base58
cargo run -- base58 encode "Hello"
cargo run -- base58 decode "9A8o"

# Base62
cargo run -- base62 encode "Hello"
cargo run -- base62 decode "73XpUgyM"

# Base85 (ASCII85)
cargo run -- base85 encode "Hello"
cargo run -- base85 decode "87cURDZ"

# Base91
cargo run -- base91 encode "Hello"
cargo run -- base91 decode ">OwJh"

# Hex
cargo run -- hex encode "flag{hex}"
cargo run -- hex decode "666c61677b6865787d"

# URL encoding
cargo run -- url encode "hello world&foo=bar"
cargo run -- url decode "hello%20world%26foo%3Dbar"

# Binary
cargo run -- binary encode "Hi"
cargo run -- binary decode "01001000 01101001"

# Morse code
cargo run -- morse encode "SOS"
cargo run -- morse decode "... --- ..."

# A1Z26 cipher (A=1, B=2, ..., Z=26)
cargo run -- a1z26 encode "Hello"
cargo run -- a1z26 decode "8-5-12-12-15"

# Baudot code
cargo run -- baudot encode "hello"
cargo run -- baudot decode "10100 00001 01001 01001 11000"

# Braille
cargo run -- braille encode "Hello"
cargo run -- braille decode "⠓⠑⠇⠇⠕"

# NATO phonetic alphabet
cargo run -- nato encode "hello"
cargo run -- nato decode "HOTEL ECHO LIMA LIMA OSCAR"

# Phone keypad
cargo run -- phone encode "hello"
cargo run -- phone decode "44 33 555 555 666"

# Flag semaphore
cargo run -- semaphore encode "hello"
cargo run -- semaphore decode "2-3 1-6 3-2 3-2 3-6"

# Tap code
cargo run -- tapcode encode "hello"
cargo run -- tapcode decode ". ..... . ..... ... . ... . ... ....."
```

### Classic Ciphers

```bash
# ROT13
cargo run -- rot13 "Hello"

# ROT47
cargo run -- rot47 "Hello"

# Caesar cipher
cargo run -- caesar encrypt "Hello" --shift 3
cargo run -- caesar bruteforce "Khoor"

# XOR
cargo run -- xor cipher "48656c6c6f" --key "41"
cargo run -- xor cipher "48656c6c6f" --key "A" --ascii
cargo run -- xor bruteforce "48656c6c6f" --printable

# Vigenere cipher
cargo run -- vigenere encrypt "HELLO" --key "KEY"
cargo run -- vigenere decrypt "RIJVS" --key "KEY"

# Beaufort cipher
cargo run -- beaufort encrypt "HELLO" --key "KEY"
cargo run -- beaufort decrypt "DANZQ" --key "KEY"

# Gronsfeld cipher (numeric Vigenere)
cargo run -- gronsfeld encrypt "HELLO" --key "123"
cargo run -- gronsfeld decrypt "IGOMQ" --key "123"

# Atbash cipher (A↔Z)
cargo run -- atbash transform "HELLO"

# Rail Fence cipher
cargo run -- railfence encrypt "HELLO WORLD" --rails 3
cargo run -- railfence decrypt "HORELWLOLD L" --rails 3

# Affine cipher (ax+b mod 26)
cargo run -- affine encrypt "HELLO" --a 5 --b 8
cargo run -- affine decrypt "RCLLA" --a 5 --b 8

# Playfair cipher
cargo run -- playfair encrypt "HELLO WORLD" --key "MONARCHY"
cargo run -- playfair decrypt "CFSUPMOMZPD" --key "MONARCHY"

# Four-square cipher
cargo run -- foursquare encrypt "HELLO WORLD" --key1 "KEYONE" --key2 "KEYTWO"

# Hill cipher
cargo run -- hill encrypt "HELLO" --key "6 24 1 13 16 10 20 17 15"

# Bifid cipher
cargo run -- bifid encrypt "HELLO" --key "KEY"

# ADFGVX cipher
cargo run -- adfgvx encrypt "HELLO" --key "default" --transposition-key "KEY"

# Columnar Transposition cipher
cargo run -- columnar encrypt "HELLO" --key "KEY"

# Baconian cipher
cargo run -- baconian encode "HELLO"
cargo run -- baconian decode "AABBB AABAA ABABB ABABB ABBAB"

# Polybius square
cargo run -- polybius encrypt "HELLO"

# One-time pad
cargo run -- otp encrypt "Hello" --key "0102030405"
cargo run -- otp generate 16

# AES-128
cargo run -- aes ecb-encrypt "00112233445566778899aabbccddeeff" --key "000102030405060708090a0b0c0d0e0f"
cargo run -- aes ecb-decrypt "69c4e0d86a7b0430d8cdb78070b4c55a" --key "000102030405060708090a0b0c0d0e0f"
cargo run -- aes cbc-encrypt "00112233445566778899aabbccddeeff" --key "000102030405060708090a0b0c0d0e0f" --iv "00000000000000000000000000000000"
cargo run -- aes cbc-decrypt "7649abac8119b246cee98e9b12e9197d" --key "000102030405060708090a0b0c0d0e0f" --iv "00000000000000000000000000000000"
cargo run -- aes ecb-detect "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"

# DES/Triple-DES
cargo run -- des encrypt "0123456789abcdef" --key "133457799bbcdff1"
cargo run -- des tdes-encrypt "0123456789abcdef" --key "0123456789abcdef0123456789abcdef0123456789abcdef"

# RC4
cargo run -- rc4 cipher "48656c6c6f" --key "Key" --ascii

# Simple Substitution
cargo run -- substitution encode "Hello" --alphabet "QWERTYUIOPASDFGHJKLZXCVBNM"
cargo run -- substitution solve "Itssg"
```

### Hash / Crypto

```bash
# Generate hashes
cargo run -- hash md5 "password"
cargo run -- hash sha1 "password"
cargo run -- hash sha256 "password"
cargo run -- hash sha512 "password"
cargo run -- hash all "password"

# Identify hash type
cargo run -- hashid identify "5d41402abc4b2a76b9719d911017c592"

# HMAC
cargo run -- hmac sha256 "message" --key "secret"

# CRC32
cargo run -- crc32 compute "hello"
cargo run -- crc32-forge forge "deadbeef" --target "cafebabe"
cargo run -- crc32-forge verify "deadbeef" --suffix "12345678" --target "cafebabe"

# RSA
cargo run -- rsa compute-d --p 61 --q 53 --e 17
cargo run -- rsa encrypt --m "123" --e "17" --n "3233"
cargo run -- rsa decrypt --c "855" --d "2753" --n "3233"
cargo run -- rsa factorize-n --n "3233"
cargo run -- rsa wiener --n "3233" --e "17"
cargo run -- rsa small-e --c "123" --e "3"
cargo run -- rsa hastad --e 3 --ciphertexts "c1,c2,c3" --moduli "n1,n2,n3"
cargo run -- rsa common-modulus --n "3233" --e1 "17" --e2 "257" --c1 "c1" --c2 "c2"
cargo run -- rsa pollard-p1 --n "3233"
cargo run -- rsa pollard-rho --n "3233"

# Hash Extension Attack
cargo run -- hash-ext sha256-extend "5d41402abc4b2a76b9719d911017c592" --original-len 8 --append "admin=true"

# JWT
cargo run -- jwt decode "eyJhbG..."
cargo run -- jwt analyze "eyJhbG..."
```

### Advanced Crypto

```bash
# Elliptic Curve
cargo run -- ec add "1,2" "3,4" --a 1 --b 1 --p 17
cargo run -- ec multiply "1,2" --n 5 --a 1 --b 1 --p 17
cargo run -- ec order "1,2" --a 1 --b 1 --p 17
cargo run -- ec pohlig-hellman "1,2" "3,4" --a 1 --b 1 --p 17 --order 18

# Diffie-Hellman
cargo run -- dh pubkey --g 2 --p 23 --a 6
cargo run -- dh shared-secret --public-key 8 --p 23 --a 15
cargo run -- dh dlog --g 2 --p 23 --target 18 --order 22
```

### Utilities

```bash
# Character frequency analysis
cargo run -- frequency analyze "Hello World"
cargo run -- frequency chi-squared "Hello World"
cargo run -- frequency ioc "Hello World"

# Auto-detect and decode
cargo run -- auto decode "SGVsbG8gV29ybGQ="
cargo run -- auto decode "SGVsbG8=" --recursive

# Shannon entropy analysis
cargo run -- entropy analyze "Hello World"

# Number theory tools
cargo run -- math gcd 12345 67890
cargo run -- math lcm 12 18
cargo run -- math modinv 3 26
cargo run -- math modpow 2 10 1000

# Prime factorization
cargo run -- primes factorize 84
cargo run -- primes isprime 104729

# String tools
cargo run -- str reverse "hello"
cargo run -- str ord "hello"
cargo run -- str chr "104 101 108 108 111"

# Number base conversion
cargo run -- numconv convert 255 --from 10 --to 16
cargo run -- numconv convert ff --from 16 --to 2

# Chain operations
cargo run -- chain run "Hello" --ops "base64-encode"
cargo run -- chain run "SGVsbG8=" --ops "base64-decode,reverse"

# Hexdump
cargo run -- hexdump dump "Hello World"
cargo run -- hexdump reverse "00000000: 4865 6c6c 6f20 576f 726c 64              Hello World"

# Bit rotation
cargo run -- bitrot rotl "deadbeef" --bits 1 --width 32
cargo run -- bitrot rotr "deadbeef" --bits 1 --width 32

# Padding
cargo run -- padding pkcs7-pad "deadbeef"
cargo run -- padding pkcs7-unpad "646561646265656604040404" --block-size 8
cargo run -- padding zero-pad "deadbeef" --block-size 8
cargo run -- padding zero-unpad "64656164626565660000000000000000"
```

## Code Conventions

### Module Structure

Each crypto operation has its own module in `src/crypto/`. Modules expose a `run()` function for CLI integration and individual functions for library use. CLI action enums derive `clap::Subcommand` for subcommand parsing. Commands are organized into categories in `main.rs` with section comments.

### Error Handling

Use `anyhow::Result` for fallible operations. Use `.context()` to add error context for better messages. Return `Result<()>` from `run()` functions.

### Pattern for New Crypto Modules

```rust
use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand)]
pub enum MyAction {
    #[command(about = "Description")]
    Operation {
        #[arg(help = "Help text")]
        input: String,
    },
}

pub fn run(action: MyAction) -> Result<()> {
    match action {
        MyAction::Operation { input } => {
            println!("{}", my_function(&input)?);
        }
    }
    Ok(())
}

pub fn my_function(input: &str) -> Result<String> {
    // Implementation
}
```

### Testing Conventions

Integration tests go in `tests/` directory. Test files follow `{module}_test.rs` naming. Include empty string tests, roundtrip tests, error case tests, and CTF-style test data (e.g., `flag{...}` format).

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/static-check.yml`) runs on PRs and pushes to main:

1. **Build** - `cargo build --verbose`
2. **Test** - `cargo test --verbose`
3. **Lint** - `cargo fmt --all -- --check` and `cargo clippy -- -D warnings`

All checks must pass before merging.

## Dependencies

| Crate | Purpose |
| ----- | --------- |
| `aes` | AES block cipher |
| `anyhow` | Error handling with context |
| `clap` | CLI argument parsing with derive macros |
| `base64` | Base64 encoding/decoding |
| `bs58` | Base58 encoding/decoding |
| `cipher` | Crypto traits |
| `data-encoding` | Base32 encoding/decoding |
| `des` | DES block cipher |
| `hex` | Hexadecimal encoding/decoding |
| `md-5` | MD5 hash generation |
| `num-bigint` | Big integer arithmetic (math/primes) |
| `num-integer` | Integer traits and operations |
| `num-traits` | Numeric traits |
| `sha1` | SHA1 hash generation |
| `sha2` | SHA256/SHA512 hash generation |
| `urlencoding` | URL percent-encoding |

## Adding New Features

1. Create a new module in `src/crypto/`
2. Define an action enum with `#[derive(Subcommand)]`
3. Implement `run()` function and core logic functions
4. Export the module in `src/crypto/mod.rs`
5. Add the subcommand to `Commands` enum in `src/main.rs` (in appropriate category)
6. Add integration tests in `tests/`
7. Run `cargo fmt` and `cargo clippy -- -D warnings` before committing

## Notes for AI Assistants

- This is a legitimate security/CTF educational tool, not malware
- The Rust edition is 2024 (nightly features may be used)
- All code must pass clippy with `-D warnings` (warnings as errors)
- Keep functions pure and testable where possible
- XOR module uses `let-else` patterns (Rust 1.65+ feature)
- Prefer `&str` for input parameters, return owned `String` for results
- Commands in `main.rs` are organized by category with comments
- Base85 and Base91 are implemented without external crates
- The `chain` module references other crypto modules via `crate::crypto::*`
- Math and primes modules use `u128` for number representation
