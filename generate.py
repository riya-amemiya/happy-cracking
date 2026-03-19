import os
import re

# Categories mapping logic based on existing AGENTS.md
encoding_files = [
    "a1z26.rs", "base32.rs", "base58.rs", "base62.rs", "base64.rs", "base85.rs",
    "base91.rs", "baudot.rs", "binary.rs", "braille.rs", "hex.rs", "morse.rs",
    "nato.rs", "phone.rs", "semaphore.rs", "tapcode.rs", "url.rs"
]

classic_ciphers_files = [
    "adfgvx.rs", "aes_cipher.rs", "affine.rs", "atbash.rs", "baconian.rs",
    "beaufort.rs", "bifid.rs", "caesar.rs", "columnar.rs", "des_cipher.rs",
    "foursquare.rs", "gronsfeld.rs", "hill.rs", "otp.rs", "playfair.rs",
    "polybius.rs", "railfence.rs", "rc4.rs", "rot.rs", "substitution.rs",
    "vigenere.rs", "xor.rs"
]

hash_crypto_files = [
    "crc32.rs", "crc32_forge.rs", "hash.rs", "hash_ext.rs", "hashid.rs",
    "hmac.rs", "jwt.rs", "rsa.rs"
]

advanced_crypto_files = [
    "dh.rs", "ec.rs"
]

utilities_files = [
    "autodecode.rs", "bitrot.rs", "chain.rs", "entropy.rs", "frequency.rs",
    "hexdump.rs", "mathtools.rs", "numbersys.rs", "padding.rs", "polybius_utils.rs",
    "primes.rs", "shared.rs", "strtools.rs"
]

tests = os.listdir("tests")
tests = [t for t in tests if t.endswith(".rs")]
tests.sort()

comments = {
    "a1z26.rs": "# A=1, Z=26 number-letter cipher",
    "base32.rs": "# Base32 encode/decode",
    "base58.rs": "# Base58 encode/decode",
    "base62.rs": "# Base62 encode/decode",
    "base64.rs": "# Base64 encode/decode",
    "base85.rs": "# Base85 (ASCII85) encode/decode",
    "base91.rs": "# Base91 encode/decode",
    "baudot.rs": "# Baudot/ITA2 telegraph code",
    "binary.rs": "# Binary (8-bit) encode/decode",
    "braille.rs": "# Braille encoding",
    "hex.rs": "# Hex encode/decode",
    "morse.rs": "# Morse code encode/decode",
    "nato.rs": "# NATO phonetic alphabet",
    "phone.rs": "# Phone keypad multi-tap",
    "semaphore.rs": "# Flag semaphore encoding",
    "tapcode.rs": "# Tap code encoding",
    "url.rs": "# URL encode/decode (percent-encoding)",
    "adfgvx.rs": "# ADFGVX cipher",
    "aes_cipher.rs": "# AES-128 ECB/CBC encrypt/decrypt",
    "affine.rs": "# Affine cipher (ax+b mod 26)",
    "atbash.rs": "# Atbash cipher (A↔Z substitution)",
    "baconian.rs": "# Baconian cipher (5-bit A/B encoding)",
    "beaufort.rs": "# Beaufort cipher (self-reciprocal Vigenere variant)",
    "bifid.rs": "# Bifid cipher",
    "caesar.rs": "# Caesar cipher encrypt/decrypt/bruteforce",
    "columnar.rs": "# Columnar transposition cipher",
    "des_cipher.rs": "# DES/Triple-DES encrypt/decrypt",
    "foursquare.rs": "# Four-square cipher",
    "gronsfeld.rs": "# Gronsfeld cipher (numeric Vigenere)",
    "hill.rs": "# Hill cipher (matrix-based)",
    "otp.rs": "# One-time pad",
    "playfair.rs": "# Playfair cipher (5x5 digraph substitution)",
    "polybius.rs": "# Polybius square cipher",
    "railfence.rs": "# Rail Fence transposition cipher",
    "rc4.rs": "# RC4 stream cipher",
    "rot.rs": "# ROT13, ROT47, and generic rotation cipher",
    "substitution.rs": "# Simple substitution cipher",
    "vigenere.rs": "# Vigenere polyalphabetic cipher",
    "xor.rs": "# XOR cipher and single-byte bruteforce",
    "crc32.rs": "# CRC32 checksum",
    "crc32_forge.rs": "# CRC32 forgery",
    "hash.rs": "# Generate MD5, SHA1, SHA256, SHA512",
    "hash_ext.rs": "# Hash length extension attack",
    "hashid.rs": "# Identify hash type from string",
    "hmac.rs": "# HMAC calculation",
    "jwt.rs": "# JWT decode and analysis",
    "rsa.rs": "# RSA utilities",
    "dh.rs": "# Diffie-Hellman key exchange",
    "ec.rs": "# Elliptic curve operations",
    "autodecode.rs": "# Auto-detect and decode common encodings",
    "bitrot.rs": "# Bit rotation operations",
    "chain.rs": "# Chain multiple operations (CyberChef-style)",
    "entropy.rs": "# Shannon entropy analysis",
    "frequency.rs": "# Character frequency analysis",
    "hexdump.rs": "# Hex dump display",
    "mathtools.rs": "# Number theory (GCD, modinv, modpow, Montgomery)",
    "numbersys.rs": "# Number base conversion (2-36)",
    "padding.rs": "# Padding scheme utilities",
    "polybius_utils.rs": "# Helper for Polybius square-based ciphers",
    "primes.rs": "# Prime factorization and primality test",
    "shared.rs": "# Shared crypto utilities",
    "strtools.rs": "# String tools (reverse, ord, chr)",
    "main.rs": "# CLI entry point with clap subcommands",
    "lib.rs": "# Library root, exposes crypto module",
    "mod.rs": "# Module exports"
}

test_comments = {
    "dh_dos.rs": "# Regression test for DH DoS",
    "ec_dos.rs": "# Regression test for EC DoS",
    "ec_security_test.rs": "# Security tests for Elliptic Curve",
    "mathtools_security_test.rs": "# Math security tests",
    "primes_dos.rs": "# Regression test for Pollard's Rho DoS",
    "railfence_dos.rs": "# Regression test for Rail Fence DoS",
    "rsa_attacks_test.rs": "# RSA attacks tests",
    "rsa_dos.rs": "# Regression test for RSA DoS",
    "rsa_zero_mod_fix.rs": "# Regression test for RSA zero modulus",
}

def get_comment(filename, is_test=False):
    if is_test:
        return test_comments.get(filename, "# Integration tests")
    return comments.get(filename, "# TODO")

def generate_section(files, align_col=32):
    lines = []
    files.sort()
    for i, file in enumerate(files):
        is_last = (i == len(files) - 1)
        tree_prefix = "└── " if is_last else "├── "
        # Inside `└── crypto/`
        base_str = f"│       {tree_prefix}{file}"
        comment = get_comment(file)

        visual_width = len(f"│       ├── {file}")
        pad_len = align_col - visual_width
        if pad_len < 1:
            pad_len = 1

        padded = base_str + (" " * pad_len) + comment
        lines.append(padded)
    return lines

lines = []
lines.append("happy-cracking/")
lines.append("├── src/")

align_col = 34

# main, lib
lines.append(f"│   ├── main.rs{' ' * (align_col - len('│   ├── main.rs'))}# CLI entry point with clap subcommands")
lines.append(f"│   ├── lib.rs{' ' * (align_col - len('│   ├── lib.rs'))}# Library root, exposes crypto module")
lines.append("│   └── crypto/           # Cryptographic operations")
lines.append(f"│       ├── mod.rs{' ' * (align_col - len('│       ├── mod.rs'))}# Module exports")
lines.append("│       │")
lines.append("│       │ # Encoding")
lines.extend(generate_section(encoding_files, align_col))
lines.append("│       │")
lines.append("│       │ # Classic Ciphers")
lines.extend(generate_section(classic_ciphers_files, align_col))
lines.append("│       │")
lines.append("│       │ # Hash / Crypto")
lines.extend(generate_section(hash_crypto_files, align_col))
lines.append("│       │")
lines.append("│       │ # Advanced Crypto")
lines.extend(generate_section(advanced_crypto_files, align_col))
lines.append("│       │")
lines.append("│       │ # Utilities")
lines.extend(generate_section(utilities_files, align_col))
lines.append("│")
lines.append("├── tests/                # Integration tests")

tests.sort()
for i, t in enumerate(tests):
    is_last = (i == len(tests) - 1)
    prefix = "└── " if is_last else "├── "
    base_str = f"│   {prefix}{t}"
    comment = get_comment(t, is_test=True)
    visual_width = len(f"│   ├── {t}")
    pad_len = align_col - visual_width
    if pad_len < 1:
        pad_len = 1
    lines.append(base_str + (" " * pad_len) + comment)

lines.append("│")
lines.append("├── Cargo.toml            # Project manifest (Rust 2024 edition)")
lines.append("├── Cargo.lock            # Dependency lock file")
lines.append("└── .github/workflows/")
lines.append(f"    └── static-check.yml{' ' * (align_col - len('    └── static-check.yml'))}# CI pipeline")

tree_text = "\n".join(lines) + "\n"

with open("tree.txt", "w") as f:
    f.write(tree_text)

replacement = f"```tree\n{tree_text}```"

for filename in ["AGENTS.md", "CLAUDE.md"]:
    with open(filename, "r") as f:
        content = f.read()

    new_content = re.sub(r'```tree\n.*?```', replacement, content, flags=re.DOTALL)

    with open(filename, "w") as f:
        f.write(new_content)
