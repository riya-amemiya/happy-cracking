# CLAUDE.md - AI Assistant Guide for happy-cracking

## Project Overview

**happy-cracking** is a CTF (Capture The Flag) toolkit written in Rust, providing command-line utilities for common cryptographic encoding/decoding and cipher operations used in security competitions.

## Codebase Structure

```
happy-cracking/
├── src/
│   ├── main.rs           # CLI entry point with clap subcommands
│   ├── lib.rs            # Library root, exposes crypto module
│   └── crypto/           # Cryptographic operations
│       ├── mod.rs        # Module exports
│       ├── base32.rs     # Base32 encode/decode
│       ├── base64.rs     # Base64 encode/decode
│       ├── caesar.rs     # Caesar cipher encrypt/decrypt/bruteforce
│       ├── hex.rs        # Hex encode/decode
│       ├── rot.rs        # ROT13 and generic rotation cipher
│       └── xor.rs        # XOR cipher and single-byte bruteforce
├── tests/                # Integration tests
│   ├── base32_test.rs
│   ├── base64_test.rs
│   ├── hex_test.rs
│   ├── rot_test.rs
│   └── xor_test.rs
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

```bash
# Base64
cargo run -- base64 encode "Hello, World!"
cargo run -- base64 decode "SGVsbG8sIFdvcmxkIQ=="

# Base32
cargo run -- base32 encode "Hello"
cargo run -- base32 decode "JBSWY3DP"

# ROT13
cargo run -- rot13 "Hello"

# Caesar cipher
cargo run -- caesar encrypt "Hello" --shift 3
cargo run -- caesar decrypt "Khoor" --shift 3
cargo run -- caesar bruteforce "Khoor"

# Hex
cargo run -- hex encode "flag{hex}"
cargo run -- hex decode "666c61677b6865787d"

# XOR
cargo run -- xor cipher "48656c6c6f" --key "41"
cargo run -- xor cipher "48656c6c6f" --key "A" --ascii
cargo run -- xor bruteforce "48656c6c6f" --printable
```

## Code Conventions

### Module Structure
- Each crypto operation has its own module in `src/crypto/`
- Modules expose a `run()` function for CLI integration and individual functions for library use
- CLI action enums derive `clap::Subcommand` for subcommand parsing

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
| `data-encoding` | Base32 encoding/decoding |
| `hex` | Hexadecimal encoding/decoding |

## Adding New Features

1. Create a new module in `src/crypto/`
2. Define an action enum with `#[derive(Subcommand)]`
3. Implement `run()` function and core logic functions
4. Export the module in `src/crypto/mod.rs`
5. Add the subcommand to `Commands` enum in `src/main.rs`
6. Add integration tests in `tests/`
7. Run `cargo fmt` and `cargo clippy -- -D warnings` before committing

## Notes for AI Assistants

- This is a legitimate security/CTF educational tool, not malware
- The Rust edition is 2024 (nightly features may be used)
- All code must pass clippy with `-D warnings` (warnings as errors)
- Keep functions pure and testable where possible
- XOR module uses `let-else` patterns (Rust 1.65+ feature)
- Prefer `&str` for input parameters, return owned `String` for results
