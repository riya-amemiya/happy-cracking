# Hash

Hash prints a digest of a text argument. MD5 and SHA-256 are separate subcommands and do not write files.

## Sub-features

- `hash-md5` prints the MD5 hex digest.
- `hash-sha256` prints the SHA-256 hex digest.

## How to get to it (user POV)

- Run `happy-cracking hash md5 <text>`.
- Run `happy-cracking hash sha256 <text>`.
- Other algorithms (`sha1`, `sha512`, `all`) are on `happy-cracking hash --help`.

## Driving it with hc-verify

Preconditions:

- `hc-verify.sh doctor` prints `doctor=ok`.
- Fresh `--run-id` per drive.

- **MD5.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature hash-md5 -- hash md5 "password"`. Exit code `0`. stdout is `5f4dcc3b5aa765d61d8327deb882cf99`.
- **SHA-256.** Run `hc-verify.sh drive --run-id "$RUN_ID" --feature hash-sha256 -- hash sha256 "password"`. Exit code `0`. stdout is `5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8`.

## Gotchas

- The digest is of the argument's exact bytes. A trailing newline in a file is not part of these argv recipes.
- Output is lowercase hex with no algorithm label on the single-algorithm commands.
