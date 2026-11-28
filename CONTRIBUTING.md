# Contributing to happy-cracking

Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) before opening an issue or
pull request.

## Setup

This repository is a Rust 2024 Cargo workspace. The root package is
`happy-cracking`, and `crates/` holds `hrg`, `hfd`, and their shared
`hc-internal` library. Stable Rust builds and tests the project. CI formats
with nightly `rustfmt`.

```sh
cargo build --workspace
cargo test --workspace
```

`cargo run -- <command>` runs `happy-cracking`. Companion binaries are
`cargo run -p hrg --bin hgrep -- <args>` and
`cargo run -p hfd --bin hfind -- <args>`.

## Checks

Pull requests to `main` run:

```sh
cargo build --workspace --verbose
cargo test --workspace --verbose
cargo +nightly fmt --all -- --check
cargo +nightly clippy --workspace -- -D warnings
cargo +nightly fmt --manifest-path fuzz/Cargo.toml -- --check
```

Install nightly rustfmt and clippy before the format and lint checks:

```sh
rustup toolchain install nightly --profile minimal --component rustfmt,clippy
```

`cargo fmt --all` rewrites every workspace package. Clippy must pass with no
warnings.

## Tests

Integration tests live in `tests/`, or in `crates/<package>/tests/` for `hrg`
and `hfd`, and are named `{module}_test.rs`. When
encoding or cipher behavior changes, cover empty input, round trips, error
cases, and a `flag{...}` example. Reject zero moduli and other inputs that
would loop or panic.

## Pull requests

Target `main` and fill in the pull request template.

## Releases

`happy-cracking`, `hc-internal`, `hrg`, and `hfd` share one version. A release
bumps `version` in `[workspace.package]` and the `=` pin on `hc-internal` in
`[workspace.dependencies]` to the same number. After that change is merged,
pushing a tag with the bare version number (for example `1.3.0`) runs
`.github/workflows/release.yml`, which publishes all four crates to crates.io.

## Security

Report vulnerabilities as described in [SECURITY.md](SECURITY.md). Do not open
a public issue for a security problem.
