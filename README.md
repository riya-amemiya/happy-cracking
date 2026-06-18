# happy-cracking

A fast, comprehensive CTF (Capture The Flag) toolkit written in Rust. Provides 50+ command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, password cracking, forensics, and analysis tools commonly used in security competitions.

## Installation

```bash
git clone https://github.com/riya-amemiya/happy-cracking.git
cd happy-cracking
cargo build --release
```

The binary will be available at `target/release/happy-cracking`.

## Features

### Encoding (14 tools)

| Command | Description |
| ------- | ----------- |
| `base64` | Base64 encode/decode |
| `base32` | Base32 encode/decode |
| `base58` | Base58 encode/decode (Bitcoin-style) |
| `base85` | Base85 (ASCII85) encode/decode |
| `base91` | Base91 encode/decode |
| `base45` | Base45 (RFC 9285) encode/decode |
| `hex` | Hexadecimal encode/decode |
| `url` | URL percent-encoding encode/decode |
| `binary` | Binary (8-bit) encode/decode |
| `morse` | Morse code encode/decode |
| `uuencode` | uuencode/uudecode |
| `qp` | Quoted-Printable (RFC 2045) encode/decode |
| `a1z26` | A=1, B=2, ..., Z=26 number-letter conversion |
| `numconv` | Number base conversion (bases 2-36) |

### Classic Ciphers (12 tools)

| Command | Description |
| ------- | ----------- |
| `rot13` | ROT13 cipher |
| `rot47` | ROT47 cipher (ASCII 33-126 range) |
| `caesar` | Caesar cipher with encrypt/decrypt/bruteforce |
| `vigenere` | Vigenere polyalphabetic cipher |
| `beaufort` | Beaufort cipher (self-reciprocal Vigenere variant) |
| `atbash` | Atbash cipher (A↔Z substitution) |
| `affine` | Affine cipher (ax+b mod 26) |
| `railfence` | Rail Fence transposition cipher |
| `playfair` | Playfair cipher (5x5 digraph substitution) |
| `columnar` | Columnar transposition cipher |
| `baconian` | Baconian cipher (5-bit A/B steganographic encoding) |
| `polybius` | Polybius square cipher |
| `xor` | XOR cipher with single-byte bruteforce |

### Hash Operations (2 tools)

| Command | Description |
| ------- | ----------- |
| `hash` | Generate MD5, SHA1, SHA256, SHA512 hashes |
| `hashid` | Identify hash type from a hash string |

### Cracking (2 tools)

| Command | Description |
| ------- | ----------- |
| `hashcrack` | Recover hashes via dictionary, brute-force, or table lookup (MD5/SHA1/SHA256/SHA512/MD4/NTLM, optional salt, rayon-parallel) |
| `zipcrack` | Crack password-protected ZIP archives (ZipCrypto and WinZip AES) via dictionary or brute-force, plus archive info |

### Utilities (10 tools)

| Command | Description |
| ------- | ----------- |
| `auto` | Auto-detect and decode common encodings |
| `chain` | Chain multiple operations together (CyberChef-style) |
| `cipherid` | Heuristically identify the encoding/cipher of a text |
| `entropy` | Shannon entropy analysis |
| `frequency` | Character frequency analysis |
| `filetype` | Identify file type from magic bytes |
| `strings` | Extract printable strings (ASCII and UTF-16LE) from binary data |
| `math` | Number theory tools (GCD, LCM, modular inverse, modular exponentiation) |
| `primes` | Prime factorization and primality test |
| `str` | String tools (reverse, ord, chr) |

## Usage

### Encoding

```bash
# Base64
happy-cracking base64 encode "Hello, World!"
happy-cracking base64 decode "SGVsbG8sIFdvcmxkIQ=="

# Base85 (ASCII85)
happy-cracking base85 encode "Hello"
happy-cracking base85 decode "87cURDZ"

# A1Z26
happy-cracking a1z26 encode "HELLO"        # 8-5-12-12-15
happy-cracking a1z26 decode "8-5-12-12-15"  # HELLO

# Number base conversion
happy-cracking numconv convert 255 --from 10 --to 16   # ff
happy-cracking numconv convert ff --from 16 --to 2      # 11111111

# Base45 (RFC 9285)
happy-cracking base45 encode "AB"        # BB8
happy-cracking base45 decode "BB8"       # AB

# uuencode / uudecode
happy-cracking uuencode encode "flag{uu}"
happy-cracking uuencode decode "$(happy-cracking uuencode encode 'flag{uu}')"

# Quoted-Printable (RFC 2045)
happy-cracking qp encode "café="        # caf=C3=A9=3D
happy-cracking qp decode "caf=C3=A9=3D"
```

### Classic Ciphers

```bash
# Caesar cipher
happy-cracking caesar encrypt "Hello" --shift 3
happy-cracking caesar bruteforce "Khoor"

# Vigenere cipher
happy-cracking vigenere encrypt "HELLO" --key "KEY"
happy-cracking vigenere decrypt "RIJVS" --key "KEY"

# Playfair cipher
happy-cracking playfair encrypt "HELLO WORLD" --key "MONARCHY"

# Columnar transposition
happy-cracking columnar encrypt "HELLO WORLD" --key "ZEBRA"

# ROT47 (numbers and symbols too)
happy-cracking rot47 "Hello, World! 123"
```

### Hash Operations

```bash
happy-cracking hash sha256 "password"
happy-cracking hashid identify "5d41402abc4b2a76b9719d911017c592"
```

### Cracking

```bash
# Hash cracking with a wordlist (algorithm auto-detected from length)
happy-cracking hashcrack dict "5d41402abc4b2a76b9719d911017c592" --wordlist words.txt
happy-cracking hashcrack dict "<hash>" --wordlist words.txt --algo ntlm --salt "s4lt" --salt-position prefix

# Incremental brute-force over a charset (bounded search space)
happy-cracking hashcrack brute "<hash>" --algo md5 --preset alnum --min-len 1 --max-len 4

# Reverse lookup against a precomputed "hash:plaintext" table
happy-cracking hashcrack lookup "5d41402abc4b2a76b9719d911017c592" --table rainbow.txt

# Crack a password-protected zip (ZipCrypto or WinZip AES)
happy-cracking zipcrack dict --file secret.zip --wordlist words.txt
happy-cracking zipcrack brute --file secret.zip --charset "0123456789" --min-len 1 --max-len 6
happy-cracking zipcrack info --file secret.zip
```

### Utilities

```bash
# Shannon entropy analysis
happy-cracking entropy analyze "Hello World"

# Identify the likely encoding/cipher of a text
happy-cracking cipherid analyze "SGVsbG8gV29ybGQ="

# Identify a file type from magic bytes (hex input or --file)
happy-cracking filetype identify "89504e470d0a1a0a"
happy-cracking filetype identify --file suspicious.bin

# Extract printable strings (hex input or --file)
happy-cracking strings extract --file firmware.bin --min-len 6 --encoding both

# Number theory
happy-cracking math gcd 12345 67890
happy-cracking math modinv 3 26        # Modular inverse
happy-cracking math modpow 2 10 1000   # Modular exponentiation

# Prime factorization
happy-cracking primes factorize 84      # 2^2 × 3 × 7
happy-cracking primes isprime 104729

# String tools
happy-cracking str reverse "flag{hello}"
happy-cracking str ord "ABC"                    # A=65 B=66 C=67
happy-cracking str chr "72 101 108 108 111"     # Hello

# Chain operations (CyberChef-style pipeline)
happy-cracking chain run "Hello" --ops "base64-encode"
happy-cracking chain run "SGVsbG8=" --ops "base64-decode,rot13"
happy-cracking chain run "Hello" --ops "upper,hex-encode"
```

The `chain` command supports the following operations: `base64-encode`, `base64-decode`, `base32-encode`, `base32-decode`, `hex-encode`, `hex-decode`, `url-encode`, `url-decode`, `binary-encode`, `binary-decode`, `rot13`, `rot47`, `reverse`, `upper`, `lower`.

## Development

```bash
cargo build            # Build
cargo test             # Run all tests (847 tests)
cargo fmt              # Format code
cargo clippy -- -D warnings  # Lint
```

## License

This project is for educational and CTF competition purposes.
