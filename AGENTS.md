# AGENTS.md - AI Assistant Guide for happy-cracking

## Project Overview

**happy-cracking** is a CTF (Capture The Flag) toolkit written in Rust, providing command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, and analysis tools commonly used in security competitions. It emphasizes correctness, performance, and robustness against Denial of Service (DoS) attacks in its cryptographic implementations.

## Codebase Structure

```tree
happy-cracking/
├── Cargo.toml            # Workspace root and happy-cracking package
├── crates/
│   ├── hc-internal/      # Shared walker, search, printer, regex, encoding, and fs helpers
│   ├── hrg/              # GNU grep + ripgrep compatible searcher, binaries hrg, hgrep, hg (argv0 picks colliding flag meanings)
│   └── hfd/              # find + fd compatible walker, binaries hfd, hfind
├── src/
│   ├── main.rs           # CLI entry point with clap subcommands
│   ├── lib.rs            # Library root, exposes crypto
│   └── crypto/           # Cryptographic operations
│       ├── mod.rs        # Module exports
│       │
│       │ # Encoding
│       ├── a1z26.rs      # A=1, Z=26 number-letter cipher
│       ├── base32.rs     # Base32 encode/decode
│       ├── base45.rs     # Base45 (RFC 9285) encode/decode
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
│       ├── quotedprintable.rs # Quoted-Printable (RFC 2045) encode/decode
│       ├── semaphore.rs  # Flag semaphore encoding
│       ├── tapcode.rs    # Tap code encoding
│       ├── url.rs        # URL encode/decode (percent-encoding)
│       ├── uuencode.rs   # uuencode/uudecode
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
│       ├── enigma.rs     # Enigma I/M3/M4 encrypt/decrypt and crib recovery
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
│       ├── xor.rs        # XOR cipher, single-byte bruteforce, key-length, crack, crib-drag
│       │
│       │ # Hash / Crypto
│       ├── crc32.rs      # CRC32 checksum
│       ├── crc32_forge.rs # CRC32 forgery
│       ├── hash.rs       # Generate MD5, SHA1, SHA256, SHA512
│       ├── hash_ext.rs   # Hash length extension attack
│       ├── hashid.rs     # Identify hash type from string
│       ├── hmac.rs       # HMAC calculation
│       ├── jwt.rs        # JWT decode, analysis, and attacks
│       ├── rsa.rs        # RSA utilities
│       │
│       │ # Cracking
│       ├── hashcrack.rs  # Hash cracking (dictionary, rules, mask, hybrid, brute-force, lookup)
│       ├── zipcrack.rs   # ZIP password cracking (ZipCrypto and WinZip AES)
│       │
│       │ # Attack / Recon
│       ├── httpload/     # Concurrent HTTP load generation
│       ├── portscan.rs   # nmap output parsing and port scanning
│       ├── solve.rs      # Aggressive auto-solve across encodings and classic ciphers
│       │
│       │ # Advanced Crypto
│       ├── dh.rs         # Diffie-Hellman key exchange
│       ├── ec.rs         # Elliptic curve operations
│       │
│       │ # Utilities
│       ├── autodecode.rs # Auto-detect and decode common encodings
│       ├── bitrot.rs     # Bit rotation operations
│       ├── chain.rs      # Chain multiple operations (CyberChef-style)
│       ├── cipherid.rs   # Heuristic encoding/cipher identification
│       ├── entropy.rs    # Shannon entropy analysis
│       ├── filetype.rs   # File type identification from magic bytes
│       ├── frequency.rs  # Character frequency analysis
│       ├── hexdump.rs    # Hex dump display
│       ├── mathtools.rs  # Number theory (GCD, modinv, modpow, Montgomery)
│       ├── numbersys.rs  # Number base conversion (2-36)
│       ├── padding.rs    # Padding scheme utilities
│       ├── polybius_utils.rs # Helper for Polybius square-based ciphers
│       ├── primes.rs     # Prime factorization and primality test
│       ├── shared.rs     # Shared crypto utilities
│       ├── strings.rs    # Printable string extraction (ASCII and UTF-16LE)
│       ├── strtools.rs   # String tools (reverse, ord, chr)
│       └── wordgen.rs    # Random, exhaustive, and mask string generation
│
├── tests/                # Integration tests
│   ├── a1z26_test.rs # Integration tests
│   ├── adfgvx_test.rs # Integration tests
│   ├── aes_cipher_test.rs # Integration tests
│   ├── affine_test.rs # Integration tests
│   ├── atbash_test.rs # Integration tests
│   ├── autodecode_aggressive_test.rs # Integration tests
│   ├── autodecode_test.rs # Integration tests
│   ├── baconian_test.rs # Integration tests
│   ├── base32_test.rs # Integration tests
│   ├── base45_test.rs # Integration tests
│   ├── base58_test.rs # Integration tests
│   ├── base62_test.rs # Integration tests
│   ├── base64_test.rs # Integration tests
│   ├── base85_test.rs # Integration tests
│   ├── base91_test.rs # Integration tests
│   ├── baudot_test.rs # Integration tests
│   ├── beaufort_test.rs # Integration tests
│   ├── bifid_test.rs # Integration tests
│   ├── binary_test.rs # Integration tests
│   ├── bitrot_test.rs # Integration tests
│   ├── braille_test.rs # Integration tests
│   ├── caesar_test.rs # Integration tests
│   ├── chain_test.rs # Integration tests
│   ├── cipherid_test.rs # Integration tests
│   ├── cli_help_test.rs # CLI help output tests
│   ├── columnar_dos.rs # Regression test for Columnar DoS
│   ├── columnar_test.rs # Integration tests
│   ├── crc32_forge_test.rs # Integration tests
│   ├── crc32_test.rs # Integration tests
│   ├── des_cipher_test.rs # Integration tests
│   ├── dh_dos.rs     # Regression test for DH DoS
│   ├── dh_test.rs    # Integration tests
│   ├── ec_dos.rs     # Regression test for EC DoS
│   ├── ec_security_test.rs # Security tests for Elliptic Curve
│   ├── ec_test.rs    # Integration tests
│   ├── enigma_test.rs # Integration tests
│   ├── entropy_test.rs # Integration tests
│   ├── filetype_test.rs # Integration tests
│   ├── foursquare_test.rs # Integration tests
│   ├── frequency_chi_ioc_test.rs # Integration tests
│   ├── frequency_test.rs # Integration tests
│   ├── gronsfeld_test.rs # Integration tests
│   ├── hash_ext_test.rs # Integration tests
│   ├── hash_test.rs  # Integration tests
│   ├── hashcrack_ext_test.rs # Integration tests
│   ├── hashcrack_test.rs # Integration tests
│   ├── hashid_test.rs # Integration tests
│   ├── hex_test.rs   # Integration tests
│   ├── hexdump_test.rs # Integration tests
│   ├── hill_test.rs  # Integration tests
│   ├── hmac_test.rs  # Integration tests
│   ├── httpload_test.rs # Integration tests
│   ├── jwt_attack_test.rs # JWT attack tests
│   ├── jwt_test.rs   # Integration tests
│   ├── mathtools_security_test.rs # Math security tests
│   ├── mathtools_test.rs # Integration tests
│   ├── morse_test.rs # Integration tests
│   ├── nato_test.rs  # Integration tests
│   ├── numbersys_test.rs # Integration tests
│   ├── otp_test.rs   # Integration tests
│   ├── padding_test.rs # Integration tests
│   ├── phone_test.rs # Integration tests
│   ├── playfair_test.rs # Integration tests
│   ├── polybius_test.rs # Integration tests
│   ├── portscan_test.rs # Integration tests
│   ├── primes_dos.rs # Regression test for Pollard's Rho DoS
│   ├── primes_test.rs # Integration tests
│   ├── quotedprintable_test.rs # Integration tests
│   ├── railfence_dos.rs # Regression test for Rail Fence DoS
│   ├── railfence_test.rs # Integration tests
│   ├── rc4_test.rs   # Integration tests
│   ├── rot_test.rs   # Integration tests
│   ├── rsa_attacks_test.rs # RSA attacks tests
│   ├── rsa_auto_test.rs # RSA auto attack tests
│   ├── rsa_compute_d_dos.rs # Regression test for RSA compute-d DoS
│   ├── rsa_dos.rs    # Regression test for RSA DoS
│   ├── rsa_test.rs   # Integration tests
│   ├── rsa_zero_mod_fix.rs # Regression test for RSA zero modulus
│   ├── semaphore_test.rs # Integration tests
│   ├── solve_test.rs # Integration tests
│   ├── strings_test.rs # Integration tests
│   ├── strtools_test.rs # Integration tests
│   ├── substitution_test.rs # Integration tests
│   ├── tapcode_test.rs # Integration tests
│   ├── url_test.rs   # Integration tests
│   ├── uuencode_test.rs # Integration tests
│   ├── vigenere_crack_test.rs # Integration tests
│   ├── vigenere_test.rs # Integration tests
│   ├── wordgen_test.rs # Integration tests
│   ├── xor_crack_test.rs # Integration tests
│   ├── xor_keylength_test.rs # Integration tests
│   ├── xor_test.rs   # Integration tests
│   └── zipcrack_test.rs # Integration tests
│
├── Cargo.toml            # Project manifest (Rust 2024 edition)
├── Cargo.lock            # Dependency lock file
└── .github/workflows/
    ├── cflite_batch.yml  # ClusterFuzzLite batch fuzzing
    ├── cflite_cron.yml   # ClusterFuzzLite pruning and coverage
    ├── cflite_pr.yml     # ClusterFuzzLite PR fuzzing
    ├── release.yml       # Publish every crate to crates.io on a version tag
    └── static-check.yml  # CI pipeline
```

## Development Commands

```bash
# Build the project
cargo build --workspace

# Run tests
cargo test --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Format code (required for CI)
cargo fmt --all

# Run clippy linter (must pass with no warnings)
cargo clippy --workspace -- -D warnings

# Run the CLI
cargo run -- <command>

# Companion binaries in the hrg and hfd packages
cargo run -p hrg --bin hgrep -- <args>
cargo run -p hfd --bin hfind -- <args>
```

## Fuzzing (ClusterFuzzLite / cargo-fuzz)

Fuzz targets live in `fuzz/fuzz_targets/` and are built by ClusterFuzzLite from `.clusterfuzzlite/`. They are a nested Cargo workspace and are not part of `cargo test --workspace`.

```bash
# One-time
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz --locked

# Build and smoke-run a target
cargo +nightly fuzz build
cargo +nightly fuzz run jwt_decode -- -runs=1000

# Reproduce a crash file saved by cargo-fuzz
cargo +nightly fuzz run jwt_decode fuzz/artifacts/jwt_decode/crash-<hash>
```

ClusterFuzzLite GitHub Actions use `language: rust` and AddressSanitizer only. Do not add `undefined` or `memory` sanitizers.

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

# Base45 (RFC 9285)
cargo run -- base45 encode "AB"
cargo run -- base45 decode "BB8"

# uuencode
cargo run -- uuencode encode "flag{uu}"

# Quoted-Printable (RFC 2045)
cargo run -- qp encode "café="
cargo run -- qp decode "caf=C3=A9=3D"
```

### Classic Ciphers

```bash
# ROT13
cargo run -- rot13 "Hello"

# ROT47
cargo run -- rot47 "Hello"

# Caesar cipher
cargo run -- caesar encrypt "Hello" --shift 3
cargo run -- caesar decrypt "Khoor" --shift 3
cargo run -- caesar bruteforce "Khoor"

# XOR
cargo run -- xor cipher "48656c6c6f" --key "41"
cargo run -- xor cipher "48656c6c6f" --key "A" --ascii
cargo run -- xor bruteforce "48656c6c6f" --printable
cargo run -- xor keylength "00112233445566778899aabbccddeeff" --max-len 40
cargo run -- xor crack "<hex>" --max-len 40
cargo run -- xor crib "<hex>" --crib "flag{"

# Vigenere cipher
cargo run -- vigenere encrypt "HELLO" --key "KEY"
cargo run -- vigenere decrypt "RIJVS" --key "KEY"
cargo run -- vigenere crack "RIJVSRIJVSRIJVS"
cargo run -- vigenere key-length "RIJVSRIJVSRIJVS"

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
cargo run -- railfence decrypt "HOREL OLLWD" --rails 3
cargo run -- railfence bruteforce "HOREL OLLWD"

# Affine cipher (ax+b mod 26)
cargo run -- affine encrypt "HELLO" --a 5 --b 8
cargo run -- affine decrypt "RCLLA" --a 5 --b 8
cargo run -- affine bruteforce "RCLLA"

# Playfair cipher
cargo run -- playfair encrypt "HELLO WORLD" --key "MONARCHY"
cargo run -- playfair decrypt "CFSUPMOMZPD" --key "MONARCHY"

# Four-square cipher
cargo run -- foursquare encrypt "HELLO WORLD" --key1 "KEYONE" --key2 "KEYTWO"
cargo run -- foursquare decrypt "FYGFHXHQLK" --key1 "KEYONE" --key2 "KEYTWO"

# Hill cipher
cargo run -- hill encrypt "HELLO" --key "6 24 1 13 16 10 20 17 15"
cargo run -- hill decrypt "TFJJZX" --key "6 24 1 13 16 10 20 17 15"

# Bifid cipher
cargo run -- bifid encrypt "HELLO" --key "KEY"
cargo run -- bifid decrypt "CMODH" --key "KEY"

# ADFGVX cipher
cargo run -- adfgvx encrypt "HELLO" --key "default" --transposition-key "KEY"
cargo run -- adfgvx decrypt "DDXDVDFAXF" --key "default" --transposition-key "KEY"

# Columnar Transposition cipher
cargo run -- columnar encrypt "HELLO" --key "KEY"
cargo run -- columnar decrypt "EOHLLX" --key "KEY"

# Baconian cipher
cargo run -- baconian encode "HELLO"
cargo run -- baconian decode "AABBB AABAA ABABB ABABB ABBAB"

# Polybius square
cargo run -- polybius encrypt "HELLO"
cargo run -- polybius decrypt "23 15 31 31 34"

# One-time pad
cargo run -- otp encrypt "Hello" --key "0102030405"
cargo run -- otp decrypt "49676f686a" --key "0102030405"
cargo run -- otp generate 16

# AES-128
cargo run -- aes ecb-encrypt "00112233445566778899aabbccddeeff" --key "000102030405060708090a0b0c0d0e0f"
cargo run -- aes ecb-decrypt "69c4e0d86a7b0430d8cdb78070b4c55a" --key "000102030405060708090a0b0c0d0e0f"
cargo run -- aes cbc-encrypt "00112233445566778899aabbccddeeff" --key "000102030405060708090a0b0c0d0e0f" --iv "00000000000000000000000000000000"
cargo run -- aes cbc-decrypt "7649abac8119b246cee98e9b12e9197d" --key "000102030405060708090a0b0c0d0e0f" --iv "00000000000000000000000000000000"
cargo run -- aes ecb-detect "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"

# DES/Triple-DES
cargo run -- des encrypt "0123456789abcdef" --key "133457799bbcdff1"
cargo run -- des decrypt "85e813540f0ab405" --key "133457799bbcdff1"
cargo run -- des tdes-encrypt "0123456789abcdef" --key "0123456789abcdef0123456789abcdef0123456789abcdef"
cargo run -- des tdes-decrypt "56cc09e7cfdc4cef" --key "0123456789abcdef0123456789abcdef0123456789abcdef"

# RC4
cargo run -- rc4 cipher "48656c6c6f" --key "4b6579"
cargo run -- rc4 cipher-ascii "Hello" --key "Key"
cargo run -- rc4 sbox --key "4b6579"

# Simple Substitution
cargo run -- substitution encode "Hello" --alphabet "QWERTYUIOPASDFGHJKLZXCVBNM"
cargo run -- substitution decode "Itssg" --alphabet "QWERTYUIOPASDFGHJKLZXCVBNM"
cargo run -- substitution solve "Itssg"

# Enigma I/M3/M4
cargo run -- enigma encrypt "HELLO"
cargo run -- enigma decrypt "ILBDA"
cargo run -- enigma crack "BDZGO" --crib "AAAAA"
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
cargo run -- hmac md5 "message" --key "secret"
cargo run -- hmac sha1 "message" --key "secret"
cargo run -- hmac sha256 "message" --key "secret"
cargo run -- hmac sha512 "message" --key "secret"
cargo run -- hmac verify-sha256 "message" --key "secret" --tag "8b5f48702995c1598c573db1e21866a9b825d4a794d169d7060a03605796360b"

# CRC32
cargo run -- crc32 compute "hello"
cargo run -- crc32 verify "hello" --checksum "3610a686"
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
cargo run -- rsa auto --n "3233" --e "17"

# Hash Extension Attack
cargo run -- hash-ext sha256-extend "5d41402abc4b2a76b9719d911017c592" --original-len 8 --append "admin=true"

# JWT
cargo run -- jwt decode "eyJhbG..."
cargo run -- jwt analyze "eyJhbG..."
cargo run -- jwt crack "eyJhbG..." --wordlist words.txt
```

### Cracking

```bash
# Hash cracking
cargo run -- hashcrack dict "5d41402abc4b2a76b9719d911017c592" --wordlist words.txt
cargo run -- hashcrack brute "<hash>" --algo md5 --preset alnum --min-len 1 --max-len 4
cargo run -- hashcrack lookup "5d41402abc4b2a76b9719d911017c592" --table rainbow.txt
cargo run -- hashcrack rule "<hash>" --wordlist words.txt --rules rules.txt
cargo run -- hashcrack mask "<hash>" --mask 'flag{?d?d?d}' --algo md5
cargo run -- hashcrack hybrid "<hash>" --wordlist words.txt --max-digits 4

# ZIP cracking (ZipCrypto or WinZip AES)
cargo run -- zipcrack dict --file secret.zip --wordlist words.txt
cargo run -- zipcrack brute --file secret.zip --charset "0123456789" --min-len 1 --max-len 6
cargo run -- zipcrack mask --file secret.zip --mask 'AB?c?cZ' --charset 01
cargo run -- zipcrack random --file secret.zip --length 4 --count 1000 --charset 0123456789
cargo run -- zipcrack info --file secret.zip
```

### Attack / Recon

```bash
cargo run -- solve run "Khoor" --top 10
cargo run -- portscan parse scan.nmap
cargo run -- portscan scan 127.0.0.1
cargo run -- portscan commons
cargo run -- httpload run http://127.0.0.1:8080/ -n 200 -c 50
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

# Encoding/cipher identification, file type, and strings
cargo run -- cipherid analyze "SGVsbG8gV29ybGQ="
cargo run -- filetype identify "89504e470d0a1a0a"
cargo run -- strings extract --file firmware.bin --min-len 6 --encoding both

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

# String generation
cargo run -- wordgen random --length 16 --count 1000
cargo run -- wordgen enumerate --charset abc --min-len 2 --max-len 4
cargo run -- wordgen mask 'AB?c?cZ' --charset 01

# Number base conversion
cargo run -- numconv convert 255 --from 10 --to 16
cargo run -- numconv convert ff --from 16 --to 2

# Chain operations
# Ops: base64/32/hex/url/binary-encode/decode, rot13, rot47, reverse, upper, lower
cargo run -- chain run "Hello" --ops "base64-encode,hex-encode"
cargo run -- chain run "48656c6c6f" --ops "hex-decode,reverse"

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

Integration tests go in `tests/` directory; tests for the companion binaries live in `crates/hrg/tests/` and `crates/hfd/tests/`. Test files follow `{module}_test.rs` naming. Include empty string tests, roundtrip tests, error case tests, and CTF-style test data (e.g., `flag{...}` format). Include security regression tests (e.g., `tests/ec_dos.rs`, `tests/rsa_dos.rs`, `tests/railfence_dos.rs`) to prevent performance regressions or DoS vulnerabilities.

### Security & Robustness Guidelines

- **DoS Prevention:** Always check for zero moduli, zero denominators, or invalid parameters that could cause panics before performing mathematical operations.
- **Resource Limits:** Respect resource limits (e.g., `chain` module's 50-operation limit and 50MB output size). Avoid infinite loops or unbounded recursion.
- **Algorithm Efficiency:** Use `BigUint` optimized methods (like `nth_root`) over naive loops. Use Pollard's Rho for factorization instead of trial division for large numbers to prevent hangs. Use `Montgomery` optimization in `mathtools.rs` for `u128` modular arithmetic to avoid `BigUint` allocation overhead.
- **Input Validation:** Validate all inputs before processing, especially for complex structures like JWTs or elliptic curve points.

## CI/CD Pipeline

The GitHub Actions workflow (`.github/workflows/static-check.yml`) runs on PRs and pushes to main:

1. **Build** - `cargo build --workspace --verbose`
2. **Test** - `cargo test --workspace --verbose`
3. **Lint** - `cargo +nightly fmt --all -- --check`, `cargo +nightly clippy --workspace -- -D warnings`, and `cargo +nightly fmt --manifest-path fuzz/Cargo.toml -- --check`

All checks must pass before merging.

`.github/workflows/release.yml` runs when a version tag such as `1.3.0` is pushed. It checks that the tag matches `version` in `[workspace.package]` and publishes `happy-cracking`, `hc-internal`, `hrg`, and `hfd` to crates.io with `cargo publish --workspace` through Trusted Publishing. The `cflite_*.yml` workflows run ClusterFuzzLite on pull requests and on a schedule.

## Dependencies

| Crate | Purpose |
| ----- | --------- |
| `aes` | AES block cipher |
| `anyhow` | Error handling with context |
| `base64` | Base64 engines for `jwt`, `chain`, `auto`, and `httpload` |
| `cipher` | Block cipher traits and padding for AES/DES |
| `clap` | CLI argument parsing with derive macros |
| `des` | DES block cipher |
| `hdrhistogram` | Latency histograms for `httpload` |
| `hex` | Hexadecimal encoding/decoding |
| `httparse` | HTTP response parsing for `httpload` |
| `md-5` | MD5 hash generation |
| `md4` | MD4/NTLM hashes for `hashcrack` |
| `num-bigint` | Big integer arithmetic (math, primes, Base58, Base62) |
| `num-integer` | Integer traits and operations |
| `num-traits` | Numeric traits |
| `rand` | Random sampling for `wordgen` and `zipcrack` |
| `rayon` | Parallel search in `hashcrack`, `zipcrack`, `jwt`, and `enigma` |
| `rustls` / `webpki-roots` | TLS for `httpload` |
| `serde` / `serde_json` | JSON for `jwt` and `httpload` output |
| `sha1` | SHA1 hash generation |
| `sha2` | SHA256/SHA512 hash generation |
| `socket2` | Socket setup for `httpload` |
| `umt_rust` | Base64 helpers behind the `base64` command |
| `url` | URL parsing |
| `urlencoding` | URL percent-encoding |
| `zip` | ZIP archives for `zipcrack` |

`hc-internal`, `hrg`, and `hfd` also use `memchr`, `memmap2`, `regex`, `rustix` (Unix), and `unicode-normalization` (macOS). Their versions live in `[workspace.dependencies]` in the root `Cargo.toml`.

## Adding New Features

1. Create a new module in `src/crypto/`
2. Define an action enum with `#[derive(Subcommand)]`
3. Implement `run()` function and core logic functions
4. Export the module in `src/crypto/mod.rs`
5. Add the subcommand to `Commands` enum in `src/main.rs` (in appropriate category)
6. Add integration tests in `tests/`
7. Run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before committing

Companion binaries ship from separate workspace packages: `cargo install hrg` installs `hrg`, `hgrep`, and `hg`, and `cargo install hfd` installs `hfd` and `hfind`. `cargo install happy-cracking` installs only `happy-cracking`. Shared walker, search, printer, regex, encoding, and filesystem helpers live in the public `hc-internal` library at `crates/hc-internal/`, which other crates can depend on. Each crate under `crates/` has its own `README.md`, which crates.io shows on the crate page. Every package takes its version from `[workspace.package]`, and `hc-internal` is pinned with `=` in `[workspace.dependencies]`, so bump both together; the release workflow publishes all four packages from one tag.

## Notes for AI Assistants

- This is a legitimate security/CTF educational tool, not malware
- The Rust edition is 2024 (nightly features may be used)
- All code must pass clippy with `-D warnings` (warnings as errors)
- Keep functions pure and testable where possible
- Prefer `&str` for input parameters, return owned `String` for results
- Commands in `main.rs` are organized by category with comments
- Base32, Base45, Base58, Base62, Base85, Base91, uuencode, and Quoted-Printable are implemented without external encoding crates
- The `chain` module references other crypto modules via `crate::crypto::*`
- Math and primes modules use `u128` for number representation, with Montgomery optimization for odd moduli
- Security regression tests in `tests/` (like `ec_dos.rs`) ensure robustness against DoS attacks
