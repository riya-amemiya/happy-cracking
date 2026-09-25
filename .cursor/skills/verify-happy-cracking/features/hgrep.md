# hgrep

hgrep prints lines of a file that match a pattern. This map covers a fixed-string search of one file the user creates first.

## Sub-features

- `hgrep-fixed` prints the matching line and hides the non-matching line.
- `hgrep-miss` exits non-zero when nothing matches.

## How to get to it (user POV)

- Build with the verification helper, then run `hgrep <pattern> <file>`.
- `hg` and `hrg` are directory-search aliases. This feature drives the `hgrep` binary on an explicit file.

## Driving it with hc-verify

Preconditions:

- `hc-verify.sh doctor --bin hgrep` prints `doctor=ok`.
- Fresh `--run-id`. `drive` sets the tmux working directory to `/tmp/hc-verify-scratch/<run-id>/`.
- Before `drive`, create the fixture in that scratch directory (the directory may not exist until the first `drive`; create it yourself so the file is present when hgrep starts):

```bash
mkdir -p "/tmp/hc-verify-scratch/$RUN_ID"
printf 'alpha flag{hgrep}\nbeta\n' >"/tmp/hc-verify-scratch/$RUN_ID/sample.txt"
cp "/tmp/hc-verify-scratch/$RUN_ID/sample.txt" "/tmp/hc-verify-proof/$RUN_ID/hgrep-fixed/sample.txt" 2>/dev/null || true
```

Create the proof copy after `drive` as well if the directory was missing before. The copy must exist before `cleanup`, because cleanup deletes scratch.

- **Fixed match.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature hgrep-fixed --bin hgrep -- -F flag sample.txt`. Exit code `0`. stdout is `alpha flag{hgrep}`.
- **Miss.** Run `hc-verify.sh drive --run-id "$RUN_ID2" --feature hgrep-miss --bin hgrep -- -F missing sample.txt` only after copying `sample.txt` into the new scratch dir. Exit code is non-zero (`1` when there is no match). stdout is empty.

## Gotchas

- `hgrep` disables the default help flag. `hgrep --help` still works via an explicit `--help` option.
- A pattern that is also a filename is positional: pattern first, then files, unless `-e` is set.
- `hg` / `hrg` turn a directory operand into a recursive gitignore-aware walk. This feature passes a file to `hgrep` so gitignore does not apply.
- Copy `sample.txt` into the proof directory before cleanup. Scratch is removed on cleanup.
