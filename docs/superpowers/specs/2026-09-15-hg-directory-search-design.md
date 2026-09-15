# hg directory search

## Goal

`hg PATTERN DIR` recursively searches `DIR` without `-r`, and stays faster than default `rg` on the same tree. `hgrep` stays GNU grep compatible.

## Why argv0 differs

`hg` and `hgrep` already share one implementation. Directory search is the point of the `hg` alias: ripgrep-style tree walk, not grep's "Is a directory" error.

Measured on this repository (release, warmed cache, literal `fn `):

| Command | Files | Wall |
| --- | --- | --- |
| `hgrep -r` (all files) | 430 | ~22ms |
| `hgrep -r --gitignore` | 189 | ~2ms |
| `rg` (default) | 187 | ~4ms |
| `rg -uuu` | 430 | ~46ms |

Default `rg` is faster than unrestricted `hgrep -r` because it skips gitignored paths (`target/`, `.git`, …). `hg` therefore turns gitignore on when it walks a tree. That path is already ~2× default `rg` here. `--no-ignore` keeps the unrestricted walker, which is already faster than `rg -uuu`.

`hg` does not skip hidden paths. This is a CTF tool; `.github` and similar trees should match. The two extra hidden files versus default `rg` are not a speed issue.

## Behavior

### `hg`

- Directory operand: recurse. Prefix matches with the file name (same as `-r` today).
- No operand and stdin is a TTY: search `.`.
- No operand and stdin is a pipe: search stdin (unchanged).
- Tree walk enables `--gitignore` unless `--no-ignore`.
- `-r` remains valid and still means recurse; with no operand it searches `.`.

### `hgrep`

Unchanged: directory operand without `-r` prints `Is a directory` and exits 2. `--gitignore` still requires `-r`.

### Flags

- `--no-ignore`: do not apply gitignore (hg directory walks only need this to opt out).
- Existing `--gitignore` / `-r` keep current clap rules so `hgrep` tests stay stable.

## Non-goals

- File-type filters, hidden-file skipping, `--unrestricted` stacking (`-u`/`-uu`/`-uuu`).
- Changing `hfind` / `hfd`.
- Replacing the gitignore walker with `unixdir`; it is already faster than default `rg`.

## Testing

Integration tests spawn `CARGO_BIN_EXE_hg` on scratch trees: recurse without `-r`, show filenames, honor gitignore by default, include gitignored files with `--no-ignore`. `hgrep` directory-without-`-r` and `--gitignore` requires `-r` tests stay.

## Performance bar

On this repo, release `hg PATTERN .` must beat default `rg PATTERN .` wall time on a warmed cache. `hg --no-ignore PATTERN .` must beat `rg -uuu PATTERN .`.
