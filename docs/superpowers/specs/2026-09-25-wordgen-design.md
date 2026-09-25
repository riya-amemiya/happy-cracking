# Word Generator Design

## Goal

Add a dedicated `wordgen` CLI command that streams random strings, complete
character-set combinations, and partially fixed mask combinations to standard
output with low allocation overhead.

## CLI

```text
happy-cracking wordgen random --length 16 --count 1000
happy-cracking wordgen random --length 12 --count 100 --charset abc123
happy-cracking wordgen enumerate --charset abc --min-len 2 --max-len 4
happy-cracking wordgen mask 'AB?c?cZ' --charset 01
```

### `random`

- `--length` is required and counts Unicode scalar values.
- `--count` defaults to `1`.
- `--charset` defaults to ASCII uppercase letters, lowercase letters, and
  digits.
- Each output string independently samples each position from the character
  set using an operating-system-seeded random number generator.

### `enumerate`

- `--charset` is required.
- `--min-len` defaults to `1` and `--max-len` defaults to `4`.
- Output starts at the shortest length and uses the supplied character order
  within each length.
- Every candidate is written exactly once.

### `mask`

- The positional mask is required.
- `--charset` supplies choices for each `?c` token.
- Other mask characters are fixed literals.
- `??` emits one literal question mark.
- A dangling `?` or any other `?x` token is an error.

All modes accept `--force`. Without it, more than 1,000,000,000 requested
outputs are rejected before generation starts. `--force` removes this candidate
count guard but does not disable integer-overflow or input-size validation.

## Validation and Limits

- Character sets must contain at least one character.
- Duplicate character-set entries are removed while preserving their first
  occurrence.
- A character set may contain at most 256 unique characters.
- Lengths must be between 1 and 4,096 Unicode scalar values.
- `--max-len` must be greater than or equal to `--min-len`.
- `--count` must be at least one.
- Candidate-space calculations use checked `u128` arithmetic.
- A mask may contain at most 4,096 output positions.

## Architecture

`src/crypto/wordgen.rs` owns command parsing, validation, candidate-space
calculation, and generation. `src/crypto/mod.rs` exports the module, while
`src/main.rs` connects it to the top-level command.

The CLI locks standard output once and wraps it in a large `BufWriter`.
Enumeration uses a mixed-radix odometer and a reusable output buffer, so it
does not retain the candidate set or allocate a new collection per candidate.
Mask generation compiles literals and `?c` positions before output begins.
Random generation similarly reuses a candidate buffer.

Generation remains single-threaded because output is serialized and ordered;
parallel generation would add synchronization and reordering overhead without
increasing standard-output throughput. A closed downstream pipe is treated as
normal termination.

The latest compatible `rand` crate provides the operating-system-seeded random
number generator. No other new dependency is required.

## Errors

Validation failures return `anyhow` errors with the offending option and limit.
Write failures are propagated except for `BrokenPipe`, which allows commands
such as `wordgen enumerate ... | head` to finish without a spurious error.
No partial generation occurs after a validation failure.

## Testing

Integration tests in `tests/wordgen_test.rs` cover:

- deterministic enumeration order over multiple lengths;
- fixed mask expansion and literal-question-mark escaping;
- Unicode character sets and duplicate removal;
- single-character random output shape and count;
- empty sets, invalid lengths, malformed masks, and candidate-space limits;
- top-level CLI wiring and expected line-oriented output.

Verification runs the focused integration test, the complete test suite,
format checking, Clippy with warnings denied, and representative CLI commands
for all three modes.
