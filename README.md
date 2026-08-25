# happy-cracking

A fast, comprehensive CTF (Capture The Flag) toolkit written in Rust. Provides 70+ command-line utilities for cryptographic encoding/decoding, classic ciphers, hash operations, password cracking, forensics, and analysis tools commonly used in security competitions.

## Installation

```bash
cargo install happy-cracking
```

This produces `target/release/happy-cracking` and the companion binaries `target/release/hgrep`, `target/release/hg`, `target/release/hfind`, and `target/release/hfd`.

## Features

### Encoding (20 tools)

| Command     | Description                                      |
| ----------- | ------------------------------------------------ |
| `base64`    | Base64 encode/decode                             |
| `base32`    | Base32 encode/decode                             |
| `base58`    | Base58 encode/decode (Bitcoin-style)             |
| `base62`    | Base62 encode/decode                             |
| `base85`    | Base85 (ASCII85) encode/decode                   |
| `base91`    | Base91 encode/decode                             |
| `base45`    | Base45 (RFC 9285) encode/decode                  |
| `hex`       | Hexadecimal encode/decode                        |
| `url`       | URL percent-encoding encode/decode               |
| `binary`    | Binary (8-bit) encode/decode                     |
| `morse`     | Morse code encode/decode                         |
| `uuencode`  | uuencode/uudecode                                |
| `qp`        | Quoted-Printable (RFC 2045) encode/decode        |
| `a1z26`     | A=1, B=2, ..., Z=26 number-letter conversion     |
| `baudot`    | Baudot/ITA2 telegraph code                       |
| `braille`   | Braille encoding                                 |
| `nato`      | NATO phonetic alphabet                           |
| `phone`     | Phone keypad multi-tap encoding                  |
| `semaphore` | Flag semaphore encoding                          |
| `tapcode`   | Tap code encoding                                |

### Classic Ciphers (24 tools)

| Command        | Description                                              |
| -------------- | -------------------------------------------------------- |
| `rot13`        | ROT13 cipher                                             |
| `rot47`        | ROT47 cipher (ASCII 33-126 range)                        |
| `caesar`       | Caesar cipher with encrypt/decrypt/bruteforce            |
| `vigenere`     | Vigenere polyalphabetic cipher                           |
| `beaufort`     | Beaufort cipher (self-reciprocal Vigenere variant)       |
| `gronsfeld`    | Gronsfeld cipher (numeric Vigenere)                      |
| `atbash`       | Atbash cipher (A↔Z substitution)                         |
| `affine`       | Affine cipher (ax+b mod 26)                              |
| `railfence`    | Rail Fence transposition cipher                          |
| `playfair`     | Playfair cipher (5x5 digraph substitution)               |
| `foursquare`   | Four-square cipher                                       |
| `hill`         | Hill cipher (matrix-based)                               |
| `bifid`        | Bifid cipher                                             |
| `adfgvx`       | ADFGVX cipher                                            |
| `columnar`     | Columnar transposition cipher                            |
| `enigma`       | Enigma I/M3/M4 encrypt/decrypt and crib recovery         |
| `baconian`     | Baconian cipher (5-bit A/B steganographic encoding)      |
| `polybius`     | Polybius square cipher                                   |
| `otp`          | One-time pad                                             |
| `aes`          | AES-128 ECB/CBC encrypt/decrypt and ECB detection        |
| `des`          | DES and Triple-DES encrypt/decrypt                       |
| `rc4`          | RC4 stream cipher                                        |
| `substitution` | Simple substitution cipher (encode/decode/solve)         |
| `xor`          | XOR cipher, single-byte bruteforce, key-length, crib-drag |

### Hash / Crypto (8 tools)

| Command       | Description                                              |
| ------------- | -------------------------------------------------------- |
| `hash`        | Generate MD5, SHA1, SHA256, SHA512 hashes                |
| `hashid`      | Identify hash type from a hash string                    |
| `hmac`        | HMAC-MD5/SHA1/SHA256/SHA512 compute and constant-time verify |
| `crc32`       | CRC32 checksum compute/verify                            |
| `crc32-forge` | CRC32 forgery (append bytes for a target CRC)            |
| `rsa`         | RSA encrypt/decrypt, factor, and common CTF attacks      |
| `hash-ext`    | SHA-256 hash length extension attack                     |
| `jwt`         | JWT decode, analyze, crack, alg=none forge, key confusion |

### Cracking (2 tools)

| Command     | Description                                                                                          |
| ----------- | ---------------------------------------------------------------------------------------------------- |
| `hashcrack` | Recover hashes via dictionary, brute-force, or table lookup (MD5/SHA1/SHA256/SHA512/MD4/NTLM, optional salt, rayon-parallel) |
| `zipcrack`  | Crack password-protected ZIP archives (ZipCrypto and WinZip AES) via dictionary or brute-force, plus archive info |

### Attack / Recon (2 tools)

| Command    | Description                                                         |
| ---------- | ------------------------------------------------------------------- |
| `solve`    | Aggressive auto-solve: encodings plus classic cipher attacks        |
| `portscan` | Parse nmap output or scan hosts for common open ports               |

### Advanced Crypto (2 tools)

| Command | Description                                              |
| ------- | -------------------------------------------------------- |
| `ec`    | Elliptic curve add, multiply, order, Pohlig-Hellman      |
| `dh`    | Diffie-Hellman public key, shared secret, discrete log   |

### Utilities (14 tools)

| Command     | Description                                                              |
| ----------- | ------------------------------------------------------------------------ |
| `auto`      | Auto-detect and decode common encodings                                  |
| `chain`     | Chain multiple operations together (CyberChef-style)                     |
| `cipherid`  | Heuristically identify the encoding/cipher of a text                     |
| `entropy`   | Shannon entropy analysis                                                 |
| `frequency` | Character frequency analysis, chi-squared, index of coincidence          |
| `filetype`  | Identify file type from magic bytes                                      |
| `strings`   | Extract printable strings (ASCII and UTF-16LE) from binary data          |
| `math`      | Number theory tools (GCD, LCM, modular inverse, modular exponentiation)  |
| `primes`    | Prime factorization and primality test                                   |
| `str`       | String tools (reverse, ord, chr)                                         |
| `numconv`   | Number base conversion (bases 2-36)                                      |
| `hexdump`   | Hex dump display and reverse (xxd-style)                                 |
| `bitrot`    | Bit rotation (rotate left / rotate right)                                |
| `padding`   | PKCS7 and zero padding / unpadding                                       |

### Companion binaries

| Command | Description                                      |
| ------- | ------------------------------------------------ |
| `hgrep` | Parallel grep-compatible line matcher            |
| `hg`    | Alias for `hgrep`                                |
| `hfind` | Parallel find-compatible walker                  |
| `hfd`   | Alias for `hfind`                                |

`hgrep` and `hfind` are separate binaries installed alongside `happy-cracking`. They are listed at the bottom of `happy-cracking --help`. `hg` is another argv0 for `hgrep`. `hfd` is another argv0 for `hfind`.

## Usage

### Encoding

```bash
# Base64
happy-cracking base64 encode "Hello, World!"
happy-cracking base64 decode "SGVsbG8sIFdvcmxkIQ=="

# Base62
happy-cracking base62 encode "Hello"     # 5TP3P3v
happy-cracking base62 decode "5TP3P3v"

# Base85 (ASCII85)
happy-cracking base85 encode "Hello"
happy-cracking base85 decode "87cURDZ"

# A1Z26
happy-cracking a1z26 encode "HELLO"        # 8-5-12-12-15
happy-cracking a1z26 decode "8-5-12-12-15"  # HELLO

# Baudot / Braille / NATO / phone / semaphore / tap code
happy-cracking baudot encode "hello"
happy-cracking braille encode "Hello"
happy-cracking nato encode "hello"
happy-cracking phone encode "hello"
happy-cracking semaphore encode "hello"
happy-cracking tapcode encode "hello"

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
happy-cracking vigenere crack "RIJVSRIJVSRIJVS"
happy-cracking vigenere key-length "RIJVSRIJVSRIJVS"

# Gronsfeld / Beaufort / Atbash
happy-cracking gronsfeld encrypt "HELLO" --key "123"
happy-cracking beaufort encrypt "HELLO" --key "KEY"
happy-cracking atbash transform "HELLO"

# Playfair / Four-square / Hill / Bifid / ADFGVX
happy-cracking playfair encrypt "HELLO WORLD" --key "MONARCHY"
happy-cracking foursquare encrypt "HELLO WORLD" --key1 "KEYONE" --key2 "KEYTWO"
happy-cracking hill encrypt "HELLO" --key "6 24 1 13 16 10 20 17 15"
happy-cracking bifid encrypt "HELLO" --key "KEY"
happy-cracking adfgvx encrypt "HELLO" --key "default" --transposition-key "KEY"

# Columnar transposition
happy-cracking columnar encrypt "HELLO WORLD" --key "ZEBRA"

# Enigma I/M3/M4
happy-cracking enigma encrypt "HELLO"
happy-cracking enigma decrypt "ILBDA"
happy-cracking enigma crack "BDZGO" --crib "AAAAA"

# One-time pad / AES / DES / RC4 / substitution
happy-cracking otp encrypt "Hello" --key "0102030405"
happy-cracking aes ecb-encrypt "00112233445566778899aabbccddeeff" --key "000102030405060708090a0b0c0d0e0f"
happy-cracking des encrypt "0123456789abcdef" --key "133457799bbcdff1"
happy-cracking rc4 cipher-ascii "Hello" --key "Key"
happy-cracking substitution solve "Itssg"

# ROT47 (numbers and symbols too)
happy-cracking rot47 "Hello, World! 123"

# XOR
happy-cracking xor cipher "48656c6c6f" --key "41"
happy-cracking xor bruteforce "48656c6c6f" --printable
happy-cracking xor keylength "00112233445566778899aabbccddeeff" --max-len 40
```

### Hash / Crypto

```bash
happy-cracking hash sha256 "password"
happy-cracking hashid identify "5d41402abc4b2a76b9719d911017c592"

happy-cracking hmac sha256 "message" --key "secret"
happy-cracking hmac verify-sha256 "message" --key "secret" --tag "8b5f48702995c1598c573db1e21866a9b825d4a794d169d7060a03605796360b"

happy-cracking crc32 compute "hello"
happy-cracking crc32-forge forge "deadbeef" --target "cafebabe"

happy-cracking rsa compute-d --p 61 --q 53 --e 17
happy-cracking rsa encrypt --m "123" --e "17" --n "3233"
happy-cracking rsa auto --n "3233" --e "17"

happy-cracking hash-ext sha256-extend "<hash>" --original-len 8 --append "admin=true"

happy-cracking jwt decode "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
happy-cracking jwt analyze "eyJhbG..."
happy-cracking jwt crack "eyJhbG..." --wordlist words.txt
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

### Attack / Recon

```bash
# Try encodings and classic cipher attacks
happy-cracking solve run "Khoor" --top 10
happy-cracking solve run "ZmxhZ3tiNjR9" --aggressive

# Parse nmap output or scan a host
happy-cracking portscan parse scan.nmap
happy-cracking portscan scan 127.0.0.1
```

### Advanced Crypto

```bash
happy-cracking ec add "1,2" "3,4" --a 1 --b 1 --p 17
happy-cracking ec multiply "1,2" --n 5 --a 1 --b 1 --p 17

happy-cracking dh pubkey --g 2 --p 23 --a 6
happy-cracking dh shared-secret --public-key 8 --p 23 --a 15
happy-cracking dh dlog --g 2 --p 23 --target 18 --order 22
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

# Hexdump and bit rotation
happy-cracking hexdump dump "Hello"
happy-cracking bitrot rotl "deadbeef" --bits 1 --width 32

# Padding
happy-cracking padding pkcs7-pad "deadbeef"
happy-cracking padding pkcs7-unpad "646561646265656604040404" --block-size 8

# Chain operations (CyberChef-style pipeline)
happy-cracking chain run "Hello" --ops "base64-encode"
happy-cracking chain run "SGVsbG8=" --ops "base64-decode,rot13"
happy-cracking chain run "Hello" --ops "upper,hex-encode"
```

The `chain` command supports the following operations: `base64-encode`, `base64-decode`, `base32-encode`, `base32-decode`, `hex-encode`, `hex-decode`, `url-encode`, `url-decode`, `binary-encode`, `binary-decode`, `rot13`, `rot47`, `reverse`, `upper`, `lower`.

### hgrep

```bash
hgrep needle src/
hgrep -n -i flag firmware.bin
hgrep -r --gitignore TODO .
hgrep --help
hg needle src/
```

### hfind

```bash
hfind .
hfind -L src -name '*.rs'
hfind . -type f -size -10k
hfind --gitignore . -name '*.log'
hfind --help
hfd . -print0
```

## Development

```bash
cargo build            # Build happy-cracking, hgrep, and hfind
cargo test             # Run all tests
cargo fmt              # Format code
cargo clippy -- -D warnings  # Lint
```

`cargo run -- <command>` runs `happy-cracking`. Companion binaries ship in the same package: `cargo run --bin hgrep -- <args>` and `cargo run --bin hfind -- <args>`. `hg` is an alias for `hgrep`. `hfd` is an alias for `hfind`.

## License

This project is for educational and CTF competition purposes.
