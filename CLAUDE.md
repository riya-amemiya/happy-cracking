# CLAUDE.md - AI Assistant Guide for happy-cracking

## Project Overview

**happy-cracking** is a CTF (Capture The Flag) toolkit written in Rust, providing command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, and analysis tools commonly used in security competitions.

## Codebase Structure

```
happy-cracking/
├── src/
│   ├── main.rs           # CLI entry point with clap subcommands
│   ├── lib.rs            # Library root, exposes crypto module
│   └── crypto/           # Cryptographic operations (18 modules)
│       ├── mod.rs        # Module exports
│       │
│       │ # Encoding
│       ├── base32.rs     # Base32 encode/decode
│       ├── base58.rs     # Base58 encode/decode (Bitcoin-style)
│       ├── base64.rs     # Base64 encode/decode
│       ├── binary.rs     # Binary (8-bit) encode/decode
│       ├── hex.rs        # Hex encode/decode
│       ├── morse.rs      # Morse code encode/decode
│       ├── url.rs        # URL encode/decode (percent-encoding)
│       │
│       │ # Classic Ciphers
│       ├── affine.rs     # Affine cipher (ax+b mod 26)
│       ├── atbash.rs     # Atbash cipher (A↔Z substitution)
│       ├── caesar.rs     # Caesar cipher encrypt/decrypt/bruteforce
│       ├── railfence.rs  # Rail Fence transposition cipher
│       ├── rot.rs        # ROT13 and generic rotation cipher
│       ├── vigenere.rs   # Vigenère polyalphabetic cipher
│       ├── xor.rs        # XOR cipher and single-byte bruteforce
│       │
│       │ # Hash
│       ├── hash.rs       # Generate MD5, SHA1, SHA256, SHA512
│       ├── hashid.rs     # Identify hash type from string
│       │
│       │ # Utilities
│       ├── autodecode.rs # Auto-detect and decode common encodings
│       └── frequency.rs  # Character frequency analysis
│
├── tests/                # Integration tests (18 test files)
│   ├── affine_test.rs
│   ├── atbash_test.rs
│   ├── autodecode_test.rs
│   ├── base32_test.rs
│   ├── base58_test.rs
│   ├── base64_test.rs
│   ├── binary_test.rs
│   ├── frequency_test.rs
│   ├── hash_test.rs
│   ├── hashid_test.rs
│   ├── hex_test.rs
│   ├── morse_test.rs
│   ├── railfence_test.rs
│   ├── rot_test.rs
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
```

### Classic Ciphers

```bash
# ROT13
cargo run -- rot13 "Hello"

# Caesar cipher
cargo run -- caesar encrypt "Hello" --shift 3
cargo run -- caesar decrypt "Khoor" --shift 3
cargo run -- caesar bruteforce "Khoor"

# Vigenère cipher
cargo run -- vigenere encrypt "HELLO" --key "KEY"
cargo run -- vigenere decrypt "RIJVS" --key "KEY"

# Atbash cipher (A↔Z)
cargo run -- atbash cipher "Hello"

# Rail Fence cipher
cargo run -- railfence encrypt "HELLO WORLD" --rails 3
cargo run -- railfence decrypt "HORELWLOLD L" --rails 3

# Affine cipher (ax+b mod 26)
cargo run -- affine encrypt "HELLO" --a 5 --b 8
cargo run -- affine decrypt "RCLLA" --a 5 --b 8

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
cargo run -- frequency top "Hello World" --count 5

# Auto-detect and decode
cargo run -- auto decode "SGVsbG8gV29ybGQ="
cargo run -- auto chain "SGVsbG8gV29ybGQ="  # Try multiple decodings
```

## Code Conventions

### Module Structure
- Each crypto operation has its own module in `src/crypto/`
- Modules expose a `run()` function for CLI integration and individual functions for library use
- CLI action enums derive `clap::Subcommand` for subcommand parsing
- Commands are organized into categories: Encoding, Classic Ciphers, Hash, Utilities

### Error Handling
- Use `anyhow::Result` for fallible operations
- Use `.context()` to add error context for better messages
- Return `Result<()>` from `run()` functions

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
- Integration tests go in `tests/` directory
- Test files follow `{module}_test.rs` naming
- Include: empty string tests, roundtrip tests, error case tests
- Use CTF-style test data (e.g., `flag{...}` format)

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/static-check.yml`) runs on PRs and pushes to main:

1. **Build** - `cargo build --verbose`
2. **Test** - `cargo test --verbose`
3. **Lint** - `cargo fmt --all -- --check` and `cargo clippy -- -D warnings`

All checks must pass before merging.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `anyhow` | Error handling with context |
| `clap` | CLI argument parsing with derive macros |
| `base64` | Base64 encoding/decoding |
| `bs58` | Base58 encoding/decoding |
| `data-encoding` | Base32 encoding/decoding |
| `hex` | Hexadecimal encoding/decoding |
| `md-5` | MD5 hash generation |
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
