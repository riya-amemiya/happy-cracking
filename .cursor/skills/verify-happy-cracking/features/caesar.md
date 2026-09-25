# Caesar

Caesar shifts letters by a numeric amount, reverses that shift, and lists every shift so a user can spot readable text.

## Sub-features

- `caesar-encrypt` shifts letters by `--shift`.
- `caesar-decrypt` reverses the same shift.
- `caesar-bruteforce` prints all 26 shifts.

## How to get to it (user POV)

- Run `happy-cracking caesar encrypt <text> --shift <n>`.
- Run `happy-cracking caesar decrypt <text> --shift <n>`.
- Run `happy-cracking caesar bruteforce <text>`.

## Driving it with hc-verify

Preconditions:

- `hc-verify.sh doctor` prints `doctor=ok`.
- Fresh `--run-id` per drive.

- **Encrypt.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature caesar-encrypt -- caesar encrypt "Hello" --shift 3`. Exit code `0`. stdout is `Khoor`.
- **Decrypt.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature caesar-decrypt -- caesar decrypt "Khoor" --shift 3`. Exit code `0`. stdout is `Hello`.
- **Bruteforce.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature caesar-bruteforce -- caesar bruteforce "Khoor"`. Exit code `0`. stdout contains `Shift 23: Hello` (single-digit shifts are padded, so shift 0 is `Shift  0`).

## Gotchas

- Non-letters are left in place. `Hello, World!` with shift 3 becomes `Khoor, Zruog!`, not `KhoorZruog`.
- Shift values wrap with modulo 26. The flag is `--shift`, not a positional number.
- Bruteforce prints shift 0 through 25 of rotating the ciphertext forward. The recovered plaintext for a shift-3 encryption is the line `Shift 23`.
