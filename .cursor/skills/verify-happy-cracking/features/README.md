# happy-cracking verification map

This directory is the maintained source for verifying user-facing CLI behavior. Read the index before driving, then use the matching feature file.

## Baseline preconditions

- Repo root is the happy-cracking checkout.
- Run `.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh build` after code changes.
- Run `hc-verify.sh doctor` and require `doctor=ok` for the binary you will drive.
- Each drive uses a new `--run-id`. Do not reuse a live `hc-verify-<run-id>` session.
- The CLI is stateless. Scratch under `/tmp/hc-verify-scratch/<run-id>/` is only for files the scenario creates.

## Driving conventions

- Invoke `.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh drive` so the process runs in its own tmux PTY.
- Treat every command after `--` as literal argv for `target/debug/happy-cracking` unless the recipe sets `--bin`.
- Quote user text exactly as the feature file shows it.
- Record the feature id in the proof directory name (`--feature`).

## Proof and skip reporting

- Capture `command.txt`, `stdout.txt`, `stderr.txt`, `exit_code.txt`, and `transcript.txt`.
- A visible stdout line without the exit code is not proof.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped sub-feature as verified through a different sub-feature.

## Feature entry contract

Each feature file starts with an H1 and one paragraph, then exactly four H2 sections: `Sub-features`, `How to get to it (user POV)`, `Driving it with hc-verify`, and `Gotchas`.

## Features

- [Base64](./base64.md) covers encode and decode of a known string, including a rejected decode.
- [Caesar](./caesar.md) covers encrypt, decrypt, and bruteforce on the terminal.
- [Hash](./hash.md) covers MD5 and SHA-256 of a known password string.
- [XOR](./xor.md) covers a single-byte hex XOR and the printable bruteforce listing.
- [hgrep](./hgrep.md) covers fixed-string search of a file the user creates in scratch.
