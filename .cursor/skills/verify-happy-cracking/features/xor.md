# XOR

XOR combines a hex ciphertext with a key and can list single-byte candidates that decode to printable ASCII.

## Sub-features

- `xor-cipher` applies a hex key to hex input.
- `xor-bruteforce-printable` lists printable single-byte decodings.

## How to get to it (user POV)

- Run `happy-cracking xor cipher <hex> --key <hex>`.
- Run `happy-cracking xor cipher <hex> --key <ascii> --ascii`.
- Run `happy-cracking xor bruteforce <hex> --printable`.

## Driving it with hc-verify

Preconditions:

- `hc-verify.sh doctor` prints `doctor=ok`.
- Fresh `--run-id` per drive.
- Input `48656c6c6f` is the hex of `Hello`.

- **Cipher.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature xor-cipher -- xor cipher "48656c6c6f" --key "41"`. Exit code `0`. stdout contains `Hex: 09242d2d2e`.
- **Printable bruteforce.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature xor-bruteforce -- xor bruteforce "09242d2d2e" --printable`. Exit code `0`. stdout contains `Key 0x41: Hello`.

## Gotchas

- Without `--ascii`, `--key` is hex. A one-character ASCII key still needs `--ascii`.
- `--printable` hides non-printable candidates. Omitting it prints every byte key.
