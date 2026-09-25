# Base64

Base64 lets a user turn text into a standard Base64 string and turn that string back into text from the terminal.

## Sub-features

- `base64-encode` prints the Base64 form of a text argument.
- `base64-decode` prints the original text for a valid Base64 argument.
- `base64-decode-invalid` exits non-zero on a string that is not valid Base64.

## How to get to it (user POV)

- Run `happy-cracking base64 encode <text>`.
- Run `happy-cracking base64 decode <text>`.
- Help: `happy-cracking base64 --help`.

## Driving it with hc-verify

Preconditions:

- `hc-verify.sh doctor` prints `doctor=ok` for `happy-cracking`.
- Use a fresh `--run-id` per drive. These commands do not read the scratch directory.

- **Encode.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature base64-encode -- base64 encode "Hello, World!"`. Exit code `0`. stdout is `SGVsbG8sIFdvcmxkIQ==`.
- **Decode.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature base64-decode -- base64 decode "SGVsbG8sIFdvcmxkIQ=="`. Exit code `0`. stdout is `Hello, World!`.
- **Invalid decode.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature base64-decode-invalid -- base64 decode "!!!!"`. Exit code is non-zero. stderr mentions Base64. stdout is empty.

## Gotchas

- One tmux session exists per `--run-id`. A second `drive` with the same id fails while that session is alive. Use a new id, or `cleanup` first.
- Decode trims surrounding whitespace in the library, but the CLI argument should be passed without extra quotes inside the encoded text.
- `cargo run --` adds compile noise to stderr. Drive the built `target/debug/happy-cracking` through `hc-verify.sh`, which is what `drive` does.
