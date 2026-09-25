---
name: verify-happy-cracking
description: "Drive the happy-cracking CTF CLI (and companion hgrep/hfind binaries) in an isolated tmux PTY, capture stdout/exit proof, and clean up only the session you started. Use when changing CLI behavior, encodings, ciphers, hashes, or the companion search binaries and you need evidence from the real binary."
---

# Verify happy-cracking

happy-cracking is a stateless Rust CLI. The primary surface is `happy-cracking` (`cargo run --` / `target/debug/happy-cracking`). Companion binaries `hgrep`, `hg`, `hrg`, `hfind`, and `hfd` are separate entry points in the same package. There is no server, port, account, or shared database. Two drives may run at once when each uses its own `--run-id`.

The helper is `.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh`. Invoke it from the repo root. Do not kill processes by name.

Proof directory (cleanup never deletes this): `/tmp/hc-verify-proof/<run-id>/<feature>/`

Scratch directory (cleanup deletes this): `/tmp/hc-verify-scratch/<run-id>/`

tmux session (cleanup kills only this session): `hc-verify-<run-id>`

## Launch

Build the binaries once per checkout (or after a code change):

```bash
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh build
```

Ready means `target/debug/happy-cracking` and `target/debug/hgrep` exist and `doctor` prints `doctor=ok`. There is no long-running server. Each drive starts its own tmux session and exits when the CLI exits.

Teardown of one run:

```bash
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh cleanup --run-id "$RUN_ID"
```

## Doctor

Run this before driving whenever the binary may be stale or missing:

```bash
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh doctor
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh doctor --bin hgrep
```

`doctor` requires an executable under `target/debug/`, runs `--help`, and checks that `happy-cracking` help contains `CTF toolkit`, hgrep help contains `grep-compatible`, and hfind help contains `--gitignore`. It prints `bin=`, `mtime=`, and `doctor=ok`. A non-zero exit means do not drive that binary.

## Drive

Pick a fresh `RUN_ID` (for example `base64-$$`). Read `.cursor/skills/verify-happy-cracking/features/README.md`, then the feature file. Start from the preconditions in that file.

```bash
RUN_ID="base64-$$"
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh drive \
  --run-id "$RUN_ID" --feature base64-encode -- \
  base64 encode "Hello, World!"
```

The command after `--` is passed to `target/debug/happy-cracking`. For a companion binary, add `--bin hgrep` before `--`.

`drive` refuses to reuse a live `hc-verify-$RUN_ID` session. It runs the CLI inside tmux via `script`, with the working directory set to the scratch dir, and writes:

- `command.txt` — argv
- `stdout.txt` / `stderr.txt` — captured streams
- `exit_code.txt` — process status
- `transcript.txt` — PTY transcript from `script`
- `pane.txt` — `tmux capture-pane` of that session

Assert the exit code and stdout against the feature file. A proof that only shows the final string without the command and exit code is incomplete.

## Evidence

Capture the user action and the resulting state.

- The action is `command.txt` plus the feature id.
- The result is `stdout.txt`, `stderr.txt`, and `exit_code.txt`.
- The PTY view is `transcript.txt`.
- When a command writes or reads files (hgrep), keep the input file contents in the proof directory as well (copy them out of scratch before cleanup).
- Do not mock cryptographic output. Run the real debug binary.
- These CLIs do not open a network connection for encode, cipher, hash, or local hgrep. If a drive's stderr or transcript shows an unexpected network attempt, treat the run as failed.

Copy a finished proof set you need to show a reviewer to `/opt/cursor/artifacts/verify-happy-cracking/` if that directory exists. The skill's own proof root remains `/tmp/hc-verify-proof`.

## Cleanup

```bash
.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh cleanup --run-id "$RUN_ID"
```

This kills tmux session `hc-verify-$RUN_ID` if it is still up and deletes `/tmp/hc-verify-scratch/$RUN_ID`. It leaves `/tmp/hc-verify-proof/$RUN_ID` in place. After cleanup, confirm the proof files still exist. Never `pkill` by binary name; other runs share `happy-cracking`.

## Helpers

| Script | Role |
| --- | --- |
| `.cursor/skills/verify-happy-cracking/scripts/hc-verify.sh` | `build`, `doctor`, `drive`, `cleanup` |

`drive` creates a one-shot `invoke.sh` inside the scratch directory. That file is deleted with scratch on cleanup.
