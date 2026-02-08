# CLAUDE.md - AI Assistant Guide for happy-cracking

## Project Overview

**happy-cracking** is a CTF (Capture The Flag) toolkit written in Rust, providing command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, and analysis tools commonly used in security competitions.

## Codebase Structure

```tree
happy-cracking/
├── src/
│   ├── main.rs           # CLI entry point with clap subcommands
│   ├── lib.rs            # Library root, exposes crypto module
│   └── crypto/           # Cryptographic operations (32 modules)
│       ├── mod.rs        # Module exports
│       │
│       │ # Encoding
│       ├── a1z26.rs      # A=1, Z=26 number-letter cipher
│       ├── base32.rs     # Base32 encode/decode
│       ├── base58.rs     # Base58 encode/decode (Bitcoin-style)
│       ├── base64.rs     # Base64 encode/decode
│       ├── base85.rs     # Base85 (ASCII85) encode/decode
│       ├── base91.rs     # Base91 encode/decode
│       ├── binary.rs     # Binary (8-bit) encode/decode
│       ├── hex.rs        # Hex encode/decode
│       ├── morse.rs      # Morse code encode/decode
│       ├── numbersys.rs  # Number base conversion (2-36)
│       ├── url.rs        # URL encode/decode (percent-encoding)
│       │
│       │ # Classic Ciphers
│       ├── affine.rs     # Affine cipher (ax+b mod 26)
│       ├── atbash.rs     # Atbash cipher (A↔Z substitution)
│       ├── baconian.rs   # Baconian cipher (5-bit A/B encoding)
│       ├── beaufort.rs   # Beaufort cipher (self-reciprocal Vigenere variant)
│       ├── caesar.rs     # Caesar cipher encrypt/decrypt/bruteforce
│       ├── columnar.rs   # Columnar transposition cipher
│       ├── playfair.rs   # Playfair cipher (5x5 digraph substitution)
│       ├── polybius.rs   # Polybius square cipher
│       ├── railfence.rs  # Rail Fence transposition cipher
│       ├── rot.rs        # ROT13, ROT47, and generic rotation cipher
│       ├── vigenere.rs   # Vigenere polyalphabetic cipher
│       ├── xor.rs        # XOR cipher and single-byte bruteforce
│       │
│       │ # Hash
│       ├── hash.rs       # Generate MD5, SHA1, SHA256, SHA512
│       ├── hashid.rs     # Identify hash type from string
│       │
│       │ # Utilities
│       ├── autodecode.rs # Auto-detect and decode common encodings
│       ├── chain.rs      # Chain multiple operations (CyberChef-style)
│       ├── entropy.rs    # Shannon entropy analysis
│       ├── frequency.rs  # Character frequency analysis
│       ├── mathtools.rs  # Number theory (GCD, modinv, modpow)
│       ├── primes.rs     # Prime factorization and primality test
│       └── strtools.rs   # String tools (reverse, ord, chr)
│
├── tests/                # Integration tests (31 test files)
│   ├── a1z26_test.rs
│   ├── affine_test.rs
│   ├── atbash_test.rs
│   ├── autodecode_test.rs
│   ├── baconian_test.rs
│   ├── base32_test.rs
│   ├── base58_test.rs
│   ├── base64_test.rs
│   ├── base85_test.rs
│   ├── base91_test.rs
│   ├── beaufort_test.rs
│   ├── binary_test.rs
│   ├── chain_test.rs
│   ├── columnar_test.rs
│   ├── entropy_test.rs
│   ├── frequency_test.rs
│   ├── hash_test.rs
│   ├── hashid_test.rs
│   ├── hex_test.rs
│   ├── mathtools_test.rs
│   ├── morse_test.rs
│   ├── numbersys_test.rs
│   ├── playfair_test.rs
│   ├── polybius_test.rs
│   ├── primes_test.rs
│   ├── railfence_test.rs
│   ├── rot_test.rs
│   ├── strtools_test.rs
│   ├── url_test.rs
│   ├── vigenere_test.rs
│   └── xor_test.rs
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

# Base58 (Bitcoin-style)
cargo run -- base58 encode "Hello"
cargo run -- base58 decode "9Ajdvzr"

# Base85 (ASCII85)
cargo run -- base85 encode "Hello"
cargo run -- base85 decode "87cURDZ"

# Base91
cargo run -- base91 encode "Hello, World!"
cargo run -- base91 decode "nY,IR<OjJe"

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

# A1Z26 (A=1, Z=26)
cargo run -- a1z26 encode "HELLO"
cargo run -- a1z26 decode "8-5-12-12-15"

# Number base conversion
cargo run -- numconv convert 255 --from 10 --to 16
cargo run -- numconv convert ff --from 16 --to 2
```

### Classic Ciphers

```bash
# ROT13
cargo run -- rot13 "Hello"

# ROT47
cargo run -- rot47 "Hello, World! 123"

# Caesar cipher
cargo run -- caesar encrypt "Hello" --shift 3
cargo run -- caesar decrypt "Khoor" --shift 3
cargo run -- caesar bruteforce "Khoor"

# Vigenere cipher
cargo run -- vigenere encrypt "HELLO" --key "KEY"
cargo run -- vigenere decrypt "RIJVS" --key "KEY"

# Beaufort cipher
cargo run -- beaufort encrypt "HELLO" --key "KEY"
cargo run -- beaufort decrypt "DANZQ" --key "KEY"

# Atbash cipher (A<->Z)
cargo run -- atbash cipher "Hello"

# Rail Fence cipher
cargo run -- railfence encrypt "HELLO WORLD" --rails 3
cargo run -- railfence decrypt "HORELWLOLD L" --rails 3

# Affine cipher (ax+b mod 26)
cargo run -- affine encrypt "HELLO" --a 5 --b 8
cargo run -- affine decrypt "RCLLA" --a 5 --b 8

# Playfair cipher
cargo run -- playfair encrypt "HELLO WORLD" --key "MONARCHY"
cargo run -- playfair decrypt "CFSUPMOMZPD" --key "MONARCHY"

# Columnar transposition cipher
cargo run -- columnar encrypt "HELLO WORLD" --key "ZEBRA"
cargo run -- columnar decrypt "ELROHWDLLO" --key "ZEBRA"

# Baconian cipher
cargo run -- baconian encode "HELLO"
cargo run -- baconian decode "AABBB AABAA ABABB ABABB ABBAB"

# Polybius square
cargo run -- polybius encrypt "HELLO"
cargo run -- polybius decrypt "23 15 31 31 34"

# XOR
cargo run -- xor cipher "48656c6c6f" --key "41"
cargo run -- xor cipher "48656c6c6f" --key "A" --ascii
cargo run -- xor bruteforce "48656c6c6f" --printable
```

### Hash Operations

```bash
# Generate hashes
cargo run -- hash md5 "password"
cargo run -- hash sha1 "password"
cargo run -- hash sha256 "password"
cargo run -- hash sha512 "password"

# Identify hash type
cargo run -- hashid identify "5d41402abc4b2a76b9719d911017c592"
```

### Utilities

```bash
# Character frequency analysis
cargo run -- frequency analyze "Hello World"

# Auto-detect and decode
cargo run -- auto decode "SGVsbG8gV29ybGQ="
cargo run -- auto chain "SGVsbG8gV29ybGQ="

# Shannon entropy analysis
cargo run -- entropy analyze "Hello World"

# Number theory tools
cargo run -- math gcd 12345 67890
cargo run -- math modinv 3 26
cargo run -- math modpow 2 10 1000

# Prime factorization
cargo run -- primes factorize 84
cargo run -- primes isprime 104729

# String tools
cargo run -- str reverse "flag{hello}"
cargo run -- str ord "ABC"
cargo run -- str chr "72 101 108 108 111"

# Chain operations (CyberChef-style)
cargo run -- chain run "Hello" --ops "base64-encode"
cargo run -- chain run "SGVsbG8=" --ops "base64-decode,rot13"
cargo run -- chain run "Hello" --ops "upper,hex-encode"
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
| `anyhow` | Error handling with context |
| `clap` | CLI argument parsing with derive macros |
| `base64` | Base64 encoding/decoding |
| `bs58` | Base58 encoding/decoding |
| `data-encoding` | Base32 encoding/decoding |
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
