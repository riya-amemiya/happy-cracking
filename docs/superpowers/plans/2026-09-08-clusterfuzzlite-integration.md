# ClusterFuzzLite Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate happy-cracking with ClusterFuzzLite so GitHub Actions can build this crate’s libFuzzer targets with AddressSanitizer and fuzz parser/decoder surfaces on pull requests and on a schedule.

**Architecture:** ClusterFuzzLite for Rust does not use the C/C++ `$CC`/`$LIB_FUZZING_ENGINE` linking path. A nested `cargo-fuzz` package at `fuzz/` builds libFuzzer binaries; `.clusterfuzzlite/{project.yaml,Dockerfile,build.sh}` teach ClusterFuzzLite how to compile them inside `gcr.io/oss-fuzz-base/base-builder-rust`; GitHub Actions call `google/clusterfuzzlite/actions/{build,run}_fuzzers@v1` with `language: rust`. First-wave targets call existing public `happy_cracking::crypto::*` APIs only (no `pub(crate)` hfind/hgrep parsers, no RSA/EC/DH/primes factorization). Crashes are treated as bugs; `Err(_)` from invalid input is expected and ignored.

**Tech Stack:** Rust 2024 library, cargo-fuzz + libfuzzer-sys, ClusterFuzzLite (`base-builder-rust`, sanitizer `address` only), GitHub Actions.

## Global Constraints

- Language is `rust` in both `.clusterfuzzlite/project.yaml` and every workflow `language:` input. Do not copy the C++ snippets from the generic ClusterFuzzLite guide.
- The only ClusterFuzzLite-supported sanitizer for Rust is `address`. Do not enable `undefined` or `memory` in any matrix.
- Fuzz target binary names contain only `[A-Za-z0-9_-]` (no `.` and no file extension).
- `build.sh` must not delete source files (coverage needs them).
- Fuzz harnesses must not print to stdout, must not `unwrap`/`expect` on library `Result`s, and must cap input (64 KiB) plus any algorithm-specific bounds already in the library.
- Do not fuzz `rsa`, `ec`, `dh`, `primes`, `hashcrack`, `zipcrack`, `jwt::crack_hmac_secret`, or `substitution::solve`. Those are CPU/DoS surfaces already covered by `tests/*_dos.rs`.
- Do not create a corpus storage git repo or `PERSONAL_ACCESS_TOKEN` in this change. Corpora/crashes stay on GitHub Actions artifacts.
- Do not add `.github/workflows/cflite_build.yml` (continuous builds) in this change. Rust ASan fuzz binaries plus Docker layers are large; skip until artifact quota is known.
- Do not add `fuzz/` to the root Cargo workspace. The fuzz package is a nested workspace (`[workspace] members = ["."]`).
- Do not add a repo-root `rust-toolchain.toml` that pins nightly (that would break the existing stable `cargo test` / `clippy` CI).
- Parent crate `edition = "2024"` stays as-is. The fuzz package also uses `edition = "2024"` so it builds with the same nightly the OSS-Fuzz Rust builder already provides.
- Match existing GitHub Actions style: `permissions` are explicit; default branch is `main`.
- `cargo fmt` / clippy on the library must still pass. Fuzz targets are formatted via `cargo +nightly fmt --manifest-path fuzz/Cargo.toml`.
- Every fuzz target file starts with `#![no_main]` and `use libfuzzer_sys::fuzz_target;`.

## Approaches considered

1. **Recommended (this plan):** `cargo-fuzz` + ClusterFuzzLite GitHub Actions, Rust builder image, AddressSanitizer only, eight parser fuzz targets, PR + batch + prune + coverage workflows, no storage repo and no continuous builds.
2. **Minimal:** PR workflow and one `jwt_decode` target only. Faster to land, but batch/prune/coverage and the decoder surface are missing, so ClusterFuzzLite never grows a corpus.
3. **C/C++ FFI harnesses:** Hand-written `LLVMFuzzerTestOneInput` calling Rust via a C ABI. Rejected: fights the official Rust guide, duplicates what `cargo fuzz` already does, and still needs nightly sanitizer builds.

## File map

| Path | Responsibility |
| ---- | -------------- |
| `fuzz/Cargo.toml` | Nested cargo-fuzz package, libfuzzer-sys, path dep on `happy-cracking`, eight `[[bin]]` targets |
| `fuzz/Cargo.lock` | Lockfile for the fuzz workspace (generated, committed) |
| `fuzz/fuzz_targets/jwt_decode.rs` | JWT decode / analyze / forge-none |
| `fuzz/fuzz_targets/autodecode.rs` | Auto-detect decode + bounded decode tree |
| `fuzz/fuzz_targets/chain.rs` | CyberChef-style operation chain |
| `fuzz/fuzz_targets/encodings_decode.rs` | Multiplexed encoding decoders |
| `fuzz/fuzz_targets/hexdump_reverse.rs` | Hexdump reverse parser |
| `fuzz/fuzz_targets/padding.rs` | PKCS7 / zero pad and unpad |
| `fuzz/fuzz_targets/hashid_identify.rs` | Hash-type identification |
| `fuzz/fuzz_targets/filetype_identify.rs` | Magic-byte file type identification |
| `fuzz/corpus/<target>/` | Seed inputs for local `cargo fuzz` and for `$OUT/<target>_seed_corpus.zip` |
| `.clusterfuzzlite/project.yaml` | `language: rust` metadata for helper.py / CFL |
| `.clusterfuzzlite/Dockerfile` | `base-builder-rust`, copy sources, copy `build.sh` |
| `.clusterfuzzlite/build.sh` | `cargo fuzz build -O --debug-assertions`, copy binaries and seed zips to `$OUT` |
| `.github/workflows/cflite_pr.yml` | PR code-change fuzzing, AddressSanitizer, 600s |
| `.github/workflows/cflite_batch.yml` | Scheduled batch fuzzing, 3600s |
| `.github/workflows/cflite_cron.yml` | Daily corpus prune + coverage |
| `.gitignore` | Ignore `fuzz/target`, `fuzz/artifacts`, `fuzz/coverage` (keep `fuzz/corpus`) |
| `.github/workflows/static-check.yml` | Also `cargo fmt --check` the fuzz package |
| `AGENTS.md` / `CLAUDE.md` | Fuzz build/run commands |

---

### Task 1: cargo-fuzz package scaffolding

**Files:**
- Create: `fuzz/Cargo.toml`
- Modify: `.gitignore`
- Test: `fuzz/Cargo.toml` (compile comes in Task 2 once a target exists)

**Interfaces:**
- Consumes: existing package name `happy-cracking` (Rust crate name `happy_cracking`) at repo root
- Produces: nested workspace package `happy-cracking-fuzz` with `cargo-fuzz = true`, dependency `happy-cracking = { path = ".." }`, release profile `debug = 1` (no fat LTO)

- [ ] **Step 1: Add fuzz output dirs to `.gitignore`**

Replace the current `.gitignore`:

```
/target
/.claude
```

with:

```
/target
/.claude
/fuzz/target
/fuzz/artifacts
/fuzz/coverage
```

Do not ignore `fuzz/corpus/` or `fuzz/Cargo.lock`.

- [ ] **Step 2: Write `fuzz/Cargo.toml` without `[[bin]]` entries yet**

```toml
[package]
name = "happy-cracking-fuzz"
version = "0.0.0"
edition = "2024"
publish = false

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"

[dependencies.happy-cracking]
path = ".."

[workspace]
members = ["."]

[profile.release]
debug = 1
```

The `[workspace] members = ["."]` block is required so Cargo does not treat this nested package as part of a parent workspace.

Do not copy the root `[profile.release] lto = "fat"` settings into this file. Fat LTO makes ClusterFuzzLite image builds too slow.

- [ ] **Step 3: Confirm the nested package is isolated from root CI**

Run from the repo root:

```bash
cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys; print(json.load(sys.stdin)['packages'][0]['name'])"
```

Expected: `happy-cracking` only (the fuzz package is not in the root workspace).

Run:

```bash
test ! -f fuzz/fuzz_targets/jwt_decode.rs && echo "no targets yet"
```

Expected: `no targets yet`

- [ ] **Step 4: Commit**

```bash
git add .gitignore fuzz/Cargo.toml
git commit -m "chore: add nested cargo-fuzz package scaffolding"
```

---

### Task 2: Wave-1 fuzz targets

**Files:**
- Create: `fuzz/fuzz_targets/jwt_decode.rs`
- Create: `fuzz/fuzz_targets/autodecode.rs`
- Create: `fuzz/fuzz_targets/chain.rs`
- Create: `fuzz/fuzz_targets/encodings_decode.rs`
- Create: `fuzz/fuzz_targets/hexdump_reverse.rs`
- Create: `fuzz/fuzz_targets/padding.rs`
- Create: `fuzz/fuzz_targets/hashid_identify.rs`
- Create: `fuzz/fuzz_targets/filetype_identify.rs`
- Modify: `fuzz/Cargo.toml` (add eight `[[bin]]` tables)

**Interfaces:**
- Consumes: public functions
  - `happy_cracking::crypto::jwt::{decode, extract_algorithm, find_vulnerabilities, forge_none}`
  - `happy_cracking::crypto::autodecode::{detect_and_decode, decode_tree}`
  - `happy_cracking::crypto::chain::chain`
  - `happy_cracking::crypto::{hex, base32, base45, base58, base62, base64, base85, base91, binary, quotedprintable, url, uuencode}` decode functions
  - `happy_cracking::crypto::hexdump::reverse`
  - `happy_cracking::crypto::padding::{pkcs7_pad, pkcs7_unpad, zero_pad, zero_unpad}`
  - `happy_cracking::crypto::hashid::identify`
  - `happy_cracking::crypto::filetype::identify`
- Produces: eight libFuzzer binaries named `jwt_decode`, `autodecode`, `chain`, `encodings_decode`, `hexdump_reverse`, `padding`, `hashid_identify`, `filetype_identify`

Shared harness rules (every target):
- Skip non-UTF-8 inputs when the API takes `&str`.
- Return early if payload length `> 65536`.
- Use `let _ = ...` on every `Result` / Vec return. Never `unwrap`.

- [ ] **Step 1: Append eight `[[bin]]` tables to `fuzz/Cargo.toml`**

Add after the `[profile.release]` block:

```toml
[[bin]]
name = "jwt_decode"
path = "fuzz_targets/jwt_decode.rs"
test = false
doc = false
bench = false

[[bin]]
name = "autodecode"
path = "fuzz_targets/autodecode.rs"
test = false
doc = false
bench = false

[[bin]]
name = "chain"
path = "fuzz_targets/chain.rs"
test = false
doc = false
bench = false

[[bin]]
name = "encodings_decode"
path = "fuzz_targets/encodings_decode.rs"
test = false
doc = false
bench = false

[[bin]]
name = "hexdump_reverse"
path = "fuzz_targets/hexdump_reverse.rs"
test = false
doc = false
bench = false

[[bin]]
name = "padding"
path = "fuzz_targets/padding.rs"
test = false
doc = false
bench = false

[[bin]]
name = "hashid_identify"
path = "fuzz_targets/hashid_identify.rs"
test = false
doc = false
bench = false

[[bin]]
name = "filetype_identify"
path = "fuzz_targets/filetype_identify.rs"
test = false
doc = false
bench = false
```

`test = false` is required: these bins are `#![no_main]` and must not be executed by `cargo test`.

- [ ] **Step 2: Write `fuzz/fuzz_targets/jwt_decode.rs`**

```rust
#![no_main]

use happy_cracking::crypto::jwt;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    if let Ok(parts) = jwt::decode(s) {
        let _ = jwt::extract_algorithm(&parts.header);
        let _ = jwt::find_vulnerabilities(&parts.header);
        let _ = jwt::forge_none(&parts.payload);
    }
});
```

- [ ] **Step 3: Write `fuzz/fuzz_targets/autodecode.rs`**

```rust
#![no_main]

use happy_cracking::crypto::autodecode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    let _ = autodecode::detect_and_decode(s);
    // Keep the tree tiny so aggressive mode cannot dominate a CFL time slice.
    let _ = autodecode::decode_tree(s, 3, 16);
});
```

- [ ] **Step 4: Write `fuzz/fuzz_targets/chain.rs`**

```rust
#![no_main]

use happy_cracking::crypto::chain;
use libfuzzer_sys::fuzz_target;

const OPS: &[&str] = &[
    "base64-encode",
    "base64-decode",
    "base32-encode",
    "base32-decode",
    "hex-encode",
    "hex-decode",
    "url-encode",
    "url-decode",
    "binary-encode",
    "binary-decode",
    "rot13",
    "rot47",
    "reverse",
    "upper",
    "lower",
];

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let n_ops = (data[0] % 8) + 1;
    let mut ops = String::new();
    for i in 0..n_ops {
        if i > 0 {
            ops.push(',');
        }
        let idx = data.get(1 + i as usize).copied().unwrap_or(0) as usize % OPS.len();
        ops.push_str(OPS[idx]);
    }
    let payload = &data[1 + n_ops as usize..];
    if payload.len() > 65_536 {
        return;
    }
    let Ok(s) = std::str::from_utf8(payload) else {
        return;
    };
    let _ = chain::chain(s, &ops);
});
```

- [ ] **Step 5: Write `fuzz/fuzz_targets/encodings_decode.rs`**

```rust
#![no_main]

use happy_cracking::crypto::{
    base32, base45, base58, base62, base64, base85, base91, binary, hex, quotedprintable, url,
    uuencode,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let selector = data[0];
    let payload = &data[1..];
    if payload.len() > 65_536 {
        return;
    }
    let Ok(s) = std::str::from_utf8(payload) else {
        return;
    };
    match selector % 12 {
        0 => {
            let _ = hex::decode(s);
        }
        1 => {
            let _ = base64::decode(s);
        }
        2 => {
            let _ = base32::decode(s);
        }
        3 => {
            let _ = base58::decode(s);
        }
        4 => {
            let _ = base62::decode(s);
        }
        5 => {
            let _ = base85::decode(s);
        }
        6 => {
            let _ = base91::decode(s);
        }
        7 => {
            let _ = base45::decode(s);
        }
        8 => {
            let _ = binary::decode(s);
        }
        9 => {
            let _ = url::decode(s);
        }
        10 => {
            let _ = quotedprintable::decode(s);
        }
        _ => {
            let _ = uuencode::decode(s);
        }
    }
});
```

- [ ] **Step 6: Write `fuzz/fuzz_targets/hexdump_reverse.rs`**

```rust
#![no_main]

use happy_cracking::crypto::hexdump;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    let _ = hexdump::reverse(s);
});
```

- [ ] **Step 7: Write `fuzz/fuzz_targets/padding.rs`**

```rust
#![no_main]

use happy_cracking::crypto::padding;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let block_size = (data[0] as usize % 255) + 1;
    let payload = &data[1..];
    if payload.len() > 65_536 {
        return;
    }
    let _ = padding::pkcs7_pad(payload, block_size);
    let _ = padding::pkcs7_unpad(payload, block_size);
    let _ = padding::zero_pad(payload, block_size);
    let _ = padding::zero_unpad(payload);
});
```

- [ ] **Step 8: Write `fuzz/fuzz_targets/hashid_identify.rs`**

```rust
#![no_main]

use happy_cracking::crypto::hashid;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if s.len() > 65_536 {
        return;
    }
    let _ = hashid::identify(s);
});
```

- [ ] **Step 9: Write `fuzz/fuzz_targets/filetype_identify.rs`**

```rust
#![no_main]

use happy_cracking::crypto::filetype;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let slice = if data.len() > 65_536 {
        &data[..65_536]
    } else {
        data
    };
    let _ = filetype::identify(slice);
});
```

- [ ] **Step 10: Install cargo-fuzz and nightly, then compile every target**

```bash
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz --locked
cargo +nightly fmt --manifest-path fuzz/Cargo.toml
cargo +nightly fuzz build
```

Expected: eight binaries under `fuzz/target/x86_64-unknown-linux-gnu/debug/` (or `fuzz/target/<triple>/debug/`) named exactly `jwt_decode`, `autodecode`, `chain`, `encodings_decode`, `hexdump_reverse`, `padding`, `hashid_identify`, `filetype_identify`.

If `cargo fuzz` is not on PATH after install, use `$HOME/.cargo/bin/cargo-fuzz`.

If nightly fails on edition 2024, stop and record the rustc version (`rustc +nightly --version`). Do not downgrade the library edition.

- [ ] **Step 11: Smoke-run each target for 1000 libFuzzer iterations**

```bash
for t in jwt_decode autodecode chain encodings_decode hexdump_reverse padding hashid_identify filetype_identify; do
  cargo +nightly fuzz run "$t" -- -runs=1000 -max_len=4096
done
```

Expected: each command exits 0. A crash (non-zero, `ERROR: libFuzzer`, `panic`) is a real bug: copy the crashing input into `tests/` as a regression test, fix the library, then re-run. Do not `unwrap` the crash away in the harness.

- [ ] **Step 12: Generate and commit `fuzz/Cargo.lock`**

`cargo +nightly fuzz build` writes `fuzz/Cargo.lock`. Commit it.

```bash
git add fuzz/Cargo.toml fuzz/Cargo.lock fuzz/fuzz_targets/*.rs
git commit -m "feat: add libFuzzer targets for parsers and decoders"
```

---

### Task 3: Seed corpora

**Files:**
- Create: `fuzz/corpus/jwt_decode/jwt_hs256`
- Create: `fuzz/corpus/jwt_decode/jwt_none`
- Create: `fuzz/corpus/autodecode/b64_hello`
- Create: `fuzz/corpus/autodecode/hex_flag`
- Create: `fuzz/corpus/chain/hex_then_reverse`
- Create: `fuzz/corpus/encodings_decode/b64_hello`
- Create: `fuzz/corpus/hexdump_reverse/hello_dump`
- Create: `fuzz/corpus/padding/pkcs7_hello`
- Create: `fuzz/corpus/hashid_identify/md5`
- Create: `fuzz/corpus/filetype_identify/png_magic`

**Interfaces:**
- Consumes: known-good fixtures from `tests/jwt_test.rs` and README examples
- Produces: one or more seed files per target directory `fuzz/corpus/<bin_name>/`

Seeds are raw files (no extension). libFuzzer treats the whole file as one input. For multiplex targets (`chain`, `encodings_decode`, `padding`) the first bytes are selector bytes, so the seed must include them.

- [ ] **Step 1: Write JWT seeds**

`fuzz/corpus/jwt_decode/jwt_hs256` (single line, no trailing commentary):

```
eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c
```

`fuzz/corpus/jwt_decode/jwt_none`:

```
eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJmbGFnIjoiZmxhZ3tqd3RfZDNjMGQzZH0iLCJhZG1pbiI6dHJ1ZX0.
```

- [ ] **Step 2: Write autodecode / encodings seeds**

`fuzz/corpus/autodecode/b64_hello`:

```
SGVsbG8sIFdvcmxkIQ==
```

`fuzz/corpus/autodecode/hex_flag`:

```
666c61677b6865787d
```

`fuzz/corpus/encodings_decode/b64_hello` is selector `1` (base64) plus the same payload. Write it with Python so the leading byte is exact:

```bash
python3 -c 'open("fuzz/corpus/encodings_decode/b64_hello","wb").write(bytes([1])+b"SGVsbG8sIFdvcmxkIQ==")'
```

- [ ] **Step 3: Write chain, hexdump, padding, hashid, filetype seeds**

Chain seed: `data[0] = 0` so `n_ops = (0 % 8) + 1 = 1`, op index `5` (`hex-decode`), then hex of `Hi`:

```bash
python3 -c 'open("fuzz/corpus/chain/hex_then_reverse","wb").write(bytes([0,5])+b"4869")'
```

Hexdump seed (text matching `hexdump dump "Hello"` style). Generate from the library so the reverse parser sees a real dump:

```bash
cargo run --quiet -- hexdump dump "Hello" > fuzz/corpus/hexdump_reverse/hello_dump
```

Padding seed: `data[0] = 15` so `block_size = (15 % 255) + 1 = 16`, then 16 data bytes:

```bash
python3 -c 'open("fuzz/corpus/padding/pkcs7_hello","wb").write(bytes([15])+b"HELLO WORLD!!!!\x01")'
```

`fuzz/corpus/hashid_identify/md5`:

```
5d41402abc4b2a76b9719d911017c592
```

Filetype PNG magic:

```bash
python3 -c 'open("fuzz/corpus/filetype_identify/png_magic","wb").write(bytes.fromhex("89504e470d0a1a0a0000000d49484452"))'
```

- [ ] **Step 4: Re-run one target against its seed corpus**

```bash
cargo +nightly fuzz run jwt_decode fuzz/corpus/jwt_decode -- -runs=10
```

Expected: exit 0. libFuzzer should log that it loaded the 2 seed inputs.

- [ ] **Step 5: Commit**

```bash
git add fuzz/corpus
git commit -m "test: add libFuzzer seed corpora for wave-1 targets"
```

---

### Task 4: ClusterFuzzLite build integration

**Files:**
- Create: `.clusterfuzzlite/project.yaml`
- Create: `.clusterfuzzlite/Dockerfile`
- Create: `.clusterfuzzlite/build.sh`

**Interfaces:**
- Consumes: `fuzz/` package from Tasks 1–3; image `gcr.io/oss-fuzz-base/base-builder-rust` (nightly rustc + `cargo fuzz` on `PATH`)
- Produces: `$OUT/<target>` executables and `$OUT/<target>_seed_corpus.zip` for each `fuzz/fuzz_targets/*.rs`

Happy-cracking has no C library or OpenSSL build dependency (`zip` is a Rust crate). The Dockerfile does not `apt-get install` extra packages.

- [ ] **Step 1: Write `.clusterfuzzlite/project.yaml`**

```yaml
homepage: "https://github.com/riya-amemiya/happy-cracking"
language: rust
primary_contact: "riya-amemiya"
main_repo: "https://github.com/riya-amemiya/happy-cracking"
```

The only required field for ClusterFuzzLite is `language: rust`. Extra fields are harmless metadata.

- [ ] **Step 2: Write `.clusterfuzzlite/Dockerfile`**

```dockerfile
FROM gcr.io/oss-fuzz-base/base-builder-rust
COPY . $SRC/happy-cracking
WORKDIR $SRC/happy-cracking
COPY ./.clusterfuzzlite/build.sh $SRC/
```

Do not `git clone` the project. ClusterFuzzLite copies the checkout in during `docker build` (the documented difference from OSS-Fuzz).

- [ ] **Step 3: Write `.clusterfuzzlite/build.sh`**

```bash
#!/bin/bash -eu
#
# Build all cargo-fuzz targets and copy them to $OUT.
# cargo-fuzz applies sanitizer/coverage flags from the ClusterFuzzLite
# environment; do not pass extra -Z sanitizer flags here.

cd "$SRC/happy-cracking"

cargo fuzz build -O --debug-assertions

FUZZ_TARGET_OUTPUT_DIR="fuzz/target/x86_64-unknown-linux-gnu/release"
if [[ ! -d "$FUZZ_TARGET_OUTPUT_DIR" ]]; then
  FUZZ_TARGET_OUTPUT_DIR="target/x86_64-unknown-linux-gnu/release"
fi

for f in fuzz/fuzz_targets/*.rs; do
  FUZZ_TARGET_NAME="$(basename "${f%.*}")"
  cp "${FUZZ_TARGET_OUTPUT_DIR}/${FUZZ_TARGET_NAME}" "$OUT/"
  corpus_dir="fuzz/corpus/${FUZZ_TARGET_NAME}"
  if [[ -d "$corpus_dir" ]] && compgen -G "${corpus_dir}/*" > /dev/null; then
    zip -j "$OUT/${FUZZ_TARGET_NAME}_seed_corpus.zip" "${corpus_dir}"/*
  fi
done
```

Mark executable:

```bash
chmod +x .clusterfuzzlite/build.sh
```

- [ ] **Step 4: Local Docker verification if Docker can pull `gcr.io`**

This step is optional in environments that cannot pull Google Container Registry. When Docker works:

```bash
git clone --depth 1 https://github.com/google/oss-fuzz.git /tmp/oss-fuzz
export PATH_TO_PROJECT="$(pwd)"
python3 /tmp/oss-fuzz/infra/helper.py build_image --external "$PATH_TO_PROJECT"
python3 /tmp/oss-fuzz/infra/helper.py build_fuzzers --external "$PATH_TO_PROJECT" --sanitizer address
python3 /tmp/oss-fuzz/infra/helper.py check_build --external "$PATH_TO_PROJECT" --sanitizer address
```

Expected: binaries appear in `/tmp/oss-fuzz/build/out/happy-cracking/` (project name = repo root directory). `check_build` reports the targets are instrumented with ASan and survive a few seconds of fuzzing.

If Docker/GCR is unavailable, skip this step and rely on Task 2’s `cargo +nightly fuzz build` plus GitHub Actions on the PR.

Do not run `helper.py generate`. That scaffolds a C++ Dockerfile; this task already wrote the Rust files.

- [ ] **Step 5: Commit**

```bash
git add .clusterfuzzlite/project.yaml .clusterfuzzlite/Dockerfile .clusterfuzzlite/build.sh
git commit -m "build: add ClusterFuzzLite Rust build integration"
```

---

### Task 5: GitHub Actions workflows

**Files:**
- Create: `.github/workflows/cflite_pr.yml`
- Create: `.github/workflows/cflite_batch.yml`
- Create: `.github/workflows/cflite_cron.yml`

**Interfaces:**
- Consumes: `.clusterfuzzlite/*` from Task 4; actions `google/clusterfuzzlite/actions/build_fuzzers@v1` and `run_fuzzers@v1`
- Produces: PR job `mode: code-change` (600s), scheduled batch `mode: batch` (3600s), daily `mode: prune` and `mode: coverage`

All workflows:
- `language: rust`
- sanitizer matrix is only `address` (coverage job uses `coverage`)
- pass `github-token: ${{ secrets.GITHUB_TOKEN }}` to build and run (needed if the repo is ever private; harmless on public)
- do not set `storage-repo`
- include `workflow_dispatch` so a maintainer can run them without waiting for cron/PR
- restrict PR paths so markdown-only changes do not pay for a Docker fuzz build

SARIF upload needs `security-events: write`. Do not copy `permissions: read-all` from the generic docs.

- [ ] **Step 1: Write `.github/workflows/cflite_pr.yml`**

```yaml
name: ClusterFuzzLite PR fuzzing
on:
  pull_request:
    paths:
      - "src/**"
      - "fuzz/**"
      - ".clusterfuzzlite/**"
      - "Cargo.toml"
      - "Cargo.lock"
      - ".github/workflows/cflite_*.yml"
  workflow_dispatch:
permissions:
  contents: read
  actions: read
  security-events: write
  pull-requests: read
jobs:
  PR:
    runs-on: ubuntu-latest
    concurrency:
      group: ${{ github.workflow }}-${{ matrix.sanitizer }}-${{ github.ref }}
      cancel-in-progress: true
    strategy:
      fail-fast: false
      matrix:
        sanitizer:
          - address
    steps:
      - name: Build Fuzzers (${{ matrix.sanitizer }})
        id: build
        uses: google/clusterfuzzlite/actions/build_fuzzers@v1
        with:
          language: rust
          github-token: ${{ secrets.GITHUB_TOKEN }}
          sanitizer: ${{ matrix.sanitizer }}
      - name: Run Fuzzers (${{ matrix.sanitizer }})
        id: run
        uses: google/clusterfuzzlite/actions/run_fuzzers@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          fuzz-seconds: 600
          mode: "code-change"
          sanitizer: ${{ matrix.sanitizer }}
          output-sarif: true
```

There is no `actions/checkout` step. The ClusterFuzzLite build action clones/copies the repo into the builder image.

- [ ] **Step 2: Write `.github/workflows/cflite_batch.yml`**

```yaml
name: ClusterFuzzLite batch fuzzing
on:
  schedule:
    - cron: "0 0/6 * * *"
  workflow_dispatch:
permissions:
  contents: read
  actions: read
  security-events: write
jobs:
  BatchFuzzing:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        sanitizer:
          - address
    steps:
      - name: Build Fuzzers (${{ matrix.sanitizer }})
        id: build
        uses: google/clusterfuzzlite/actions/build_fuzzers@v1
        with:
          language: rust
          github-token: ${{ secrets.GITHUB_TOKEN }}
          sanitizer: ${{ matrix.sanitizer }}
      - name: Run Fuzzers (${{ matrix.sanitizer }})
        id: run
        uses: google/clusterfuzzlite/actions/run_fuzzers@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          fuzz-seconds: 3600
          mode: "batch"
          sanitizer: ${{ matrix.sanitizer }}
          output-sarif: true
```

Batch fuzzing without a storage repo still runs; the corpus is uploaded as a GitHub Actions artifact and is cold on the next run. That is acceptable for v1.

- [ ] **Step 3: Write `.github/workflows/cflite_cron.yml`**

Pruning is required whenever batch fuzzing is enabled.

```yaml
name: ClusterFuzzLite cron tasks
on:
  schedule:
    - cron: "0 0 * * *"
  workflow_dispatch:
permissions:
  contents: read
  actions: read
  security-events: write
jobs:
  Pruning:
    runs-on: ubuntu-latest
    steps:
      - name: Build Fuzzers
        id: build
        uses: google/clusterfuzzlite/actions/build_fuzzers@v1
        with:
          language: rust
          github-token: ${{ secrets.GITHUB_TOKEN }}
      - name: Run Fuzzers
        id: run
        uses: google/clusterfuzzlite/actions/run_fuzzers@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          fuzz-seconds: 600
          mode: "prune"
          output-sarif: true
  Coverage:
    runs-on: ubuntu-latest
    steps:
      - name: Build Fuzzers
        id: build
        uses: google/clusterfuzzlite/actions/build_fuzzers@v1
        with:
          language: rust
          github-token: ${{ secrets.GITHUB_TOKEN }}
          sanitizer: coverage
      - name: Run Fuzzers
        id: run
        uses: google/clusterfuzzlite/actions/run_fuzzers@v1
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          fuzz-seconds: 600
          mode: "coverage"
          sanitizer: "coverage"
```

- [ ] **Step 4: YAML sanity check**

```bash
python3 -c "import pathlib,yaml; [yaml.safe_load(p.read_text()) for p in pathlib.Path('.github/workflows').glob('cflite_*.yml')]; print('ok')"
```

If PyYAML is missing:

```bash
python3 -c "
import pathlib
for p in pathlib.Path('.github/workflows').glob('cflite_*.yml'):
    text = p.read_text()
    assert 'language: rust' in text, p
    assert 'undefined' not in text, p
    assert 'memory' not in text or 'memory:' not in text
print('ok')
"
```

Expected: `ok`. Every file contains `language: rust`. No sanitizer matrix entry is `undefined` or `memory`.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/cflite_pr.yml .github/workflows/cflite_batch.yml .github/workflows/cflite_cron.yml
git commit -m "ci: run ClusterFuzzLite on PRs, batch, prune, and coverage"
```

---

### Task 6: Docs and static-check fmt for the fuzz package

**Files:**
- Modify: `.github/workflows/static-check.yml`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: `fuzz/Cargo.toml` from Task 1
- Produces: CI format check of the nested fuzz package; documented local fuzz commands

- [ ] **Step 1: Add a fuzz format check to the existing `lint` job**

In `.github/workflows/static-check.yml`, after the Clippy step, append:

```yaml
      - name: Check format (fuzz)
        run: cargo +nightly fmt --manifest-path fuzz/Cargo.toml -- --check
```

Do not run `cargo clippy` on the fuzz package in this change (`libfuzzer-sys` + `#![no_main]` bins are noisy and are not part of `--workspace`).

- [ ] **Step 2: Add a Fuzzing section to `AGENTS.md` and `CLAUDE.md`**

Insert the same block in both files after the Development Commands section:

````markdown
## Fuzzing (ClusterFuzzLite / cargo-fuzz)

Fuzz targets live in `fuzz/fuzz_targets/` and are built by ClusterFuzzLite from `.clusterfuzzlite/`. They are a nested Cargo workspace and are not part of `cargo test --workspace`.

```bash
# One-time
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz --locked

# Build and smoke-run a target
cargo +nightly fuzz build
cargo +nightly fuzz run jwt_decode -- -runs=1000

# Reproduce a crash file saved by cargo-fuzz
cargo +nightly fuzz run jwt_decode fuzz/artifacts/jwt_decode/crash-<hash>
```

ClusterFuzzLite GitHub Actions use `language: rust` and AddressSanitizer only. Do not add `undefined` or `memory` sanitizers.
````

- [ ] **Step 3: Run format checks**

```bash
cargo +nightly fmt --all -- --check
cargo +nightly fmt --manifest-path fuzz/Cargo.toml -- --check
cargo clippy --workspace -- -D warnings
```

Expected: all three exit 0. Root `cargo fmt --all` still does not format `fuzz/` (separate workspace); that is why the second command exists.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/static-check.yml AGENTS.md CLAUDE.md
git commit -m "docs: document cargo-fuzz and format the fuzz package in CI"
```

---

### Task 7: End-to-end verification

**Files:**
- None new. This task only runs commands and, if something fails, fixes files from earlier tasks.

**Interfaces:**
- Consumes: all artifacts from Tasks 1–6
- Produces: evidence that (1) library tests still pass, (2) all eight fuzz targets compile and survive 1000 runs, (3) ClusterFuzzLite config is Rust-shaped

- [ ] **Step 1: Library regression**

```bash
cargo test --verbose
cargo clippy --workspace -- -D warnings
cargo +nightly fmt --all -- --check
```

Expected: PASS. Fuzz files must not break the existing static-check job.

- [ ] **Step 2: List and smoke-run fuzz targets**

```bash
cargo +nightly fuzz list
```

Expected stdout (order may vary):

```
autodecode
chain
encodings_decode
filetype_identify
hashid_identify
hexdump_reverse
jwt_decode
padding
```

Then:

```bash
for t in $(cargo +nightly fuzz list); do
  cargo +nightly fuzz run "$t" -- -runs=1000 -max_len=4096
done
```

Expected: eight exit codes 0.

- [ ] **Step 3: Assert ClusterFuzzLite files match the Rust guide**

```bash
grep -qx 'language: rust' .clusterfuzzlite/project.yaml
grep -q 'FROM gcr.io/oss-fuzz-base/base-builder-rust' .clusterfuzzlite/Dockerfile
grep -q 'cargo fuzz build' .clusterfuzzlite/build.sh
grep -q 'language: rust' .github/workflows/cflite_pr.yml
grep -q 'language: rust' .github/workflows/cflite_batch.yml
grep -q 'language: rust' .github/workflows/cflite_cron.yml
test ! -f .github/workflows/cflite_build.yml
```

Expected: all greps succeed; continuous-build workflow is absent.

- [ ] **Step 4: After the PR exists, confirm the `ClusterFuzzLite PR fuzzing` workflow starts**

Open a pull request that touches `src/` or `fuzz/` (this implementation PR qualifies). In GitHub Actions, job `PR` / matrix `address` must get past **Build Fuzzers**. If build fails, read the Docker/`build.sh` log; typical failures are a wrong `$SRC/happy-cracking` path or binaries not under `fuzz/target/x86_64-unknown-linux-gnu/release/` — fix `build.sh` using the fallback directory already in Task 4.

Crashes from **Run Fuzzers** are uploaded as artifacts on the workflow summary. Download the artifact, then:

```bash
cargo +nightly fuzz run <target> /path/to/downloaded/crash -- -runs=1
```

Turn the crash into a `tests/` regression and a library fix in a follow-up commit. Do not disable the target.

- [ ] **Step 5: Commit any verification fixes**

Only if Step 1–4 required code changes:

```bash
git add -A
git commit -m "fix: address ClusterFuzzLite verification failures"
```

If nothing failed, do not create an empty commit.

---

## Follow-ups (not in this plan)

- Separate git corpus/coverage storage repo plus repository secret `PERSONAL_ACCESS_TOKEN`, then uncomment `storage-repo` on all CFL steps. Needed for warm batch corpora and HTML coverage at `https://<user>.github.io/<storage-repo>/coverage/latest/report/linux/report.html`.
- `.github/workflows/cflite_build.yml` continuous builds once artifact size is measured.
- Wave-2 targets: `jwt::forge_alg_confusion`, `morse::decode`, `railfence::{encrypt,decode}` with `rails` in `2..=32`, `numbersys::convert`, public wrappers for `hfind::expr::parse` / `hgrep` matchers.
- Dictionaries (`$OUT/<target>.dict`) for JWT `.` / base64 alphabets once the first corpus exists.

## Spec coverage (self-review)

| ClusterFuzzLite doc section | Task |
| --- | --- |
| Integrate fuzz targets with the codebase | Task 1–2 |
| `project.yaml` / `language` | Task 4 Step 1 (`rust`, not `c++`) |
| Dockerfile copies sources (no `git clone`) | Task 4 Step 2 |
| `build.sh` writes `$OUT`, keeps sources | Task 4 Step 3 |
| Rust: `base-builder-rust` + `cargo fuzz` | Task 4 (not C++ `$LIB_FUZZING_ENGINE`) |
| Only AddressSanitizer for Rust | Global Constraints + Task 5 |
| Binary names `[A-Za-z0-9_-]` | Task 2 bin names |
| Seed corpora / efficient fuzzing | Task 3 + zip in `build.sh` |
| Local `helper.py` / `cargo fuzz` testing | Task 2, Task 4 Step 4, Task 7 |
| `cflite_pr.yml` code-change | Task 5 Step 1 |
| `cflite_batch.yml` batch | Task 5 Step 2 |
| Corpus pruning required with batch | Task 5 Step 3 |
| Coverage reports | Task 5 Step 3 |
| Continuous builds | Explicitly skipped (Global Constraints) |
| Storage git repo | Follow-up, not this plan |
| Private repo `github-token` | Passed on every build/run step |
| DoS-prone crypto | Excluded from targets (Global Constraints) |
