# Contributing to happy-cracking

Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) before opening an issue or
pull request.

## Setup

This repository is a Rust 2024 package. Stable Rust builds and tests the
project. CI formats with nightly `rustfmt`.

```sh
cargo build
cargo test
```

`cargo run -- <command>` runs `happy-cracking`. Companion binaries are
`cargo run --bin hgrep -- <args>` and `cargo run --bin hfind -- <args>`.

## Checks

Pull requests to `main` run:

```sh
cargo build --verbose
cargo test --verbose
cargo +nightly fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo +nightly fmt --manifest-path fuzz/Cargo.toml -- --check
```

Install nightly rustfmt before the format check:

```sh
rustup toolchain install nightly --profile minimal --component rustfmt
```

`cargo fmt` rewrites the main package. Clippy must pass with no warnings.

## Tests

Integration tests live in `tests/` and are named `{module}_test.rs`. When
encoding or cipher behavior changes, cover empty input, round trips, error
cases, and a `flag{...}` example. Reject zero moduli and other inputs that
would loop or panic.

## Pull requests

Target `main` and fill in the pull request template.

## Security

Report vulnerabilities as described in [SECURITY.md](SECURITY.md). Do not open
a public issue for a security problem.
