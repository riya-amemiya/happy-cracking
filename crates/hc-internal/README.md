# hc-internal

The shared walker, search, printer, regex, encoding, and filesystem library behind [`hrg`](https://crates.io/crates/hrg) and [`hfd`](https://crates.io/crates/hfd), the companion tools of the [happy-cracking](https://github.com/riya-amemiya/happy-cracking) project. Other crates can depend on it directly.

## Installation

```toml
[dependencies]
hc-internal = "1.3.0"
```

The crate is imported as `hc_internal`. It is released together with `hrg`, `hfd`, and `happy-cracking`, and all four crates share one version number.

## Modules

| Module      | Contents                                                                                   |
| ----------- | ------------------------------------------------------------------------------------------ |
| `walker`    | Recursive directory walker (`WalkBuilder`, `DirEntry`) with gitignore, override, and file-type filters |
| `grep`      | Matcher, searcher, printer, and regex layers used by `hrg`'s ripgrep-style search          |
| `gnu`       | GNU grep regex, output, color, exclude, and locale handling                                |
| `pcre`      | Built-in PCRE2 compatible regex engine                                                     |
| `encoding`  | Text encoding decoding for searched files (`Encoding`, `DecodeReader`)                     |
| `gitconfig` | Git config file reading (`RepoOpts`, `read_config_file_with_limit`)                        |
| `ignore`    | Ignore-file loading and glob-to-regex conversion                                           |
| `nfc`       | Conversion of macOS decomposed (NFD) file names to NFC                                     |
| `unixhome`  | `~user` home directory lookup without libc `getpwnam_r` (Unix only)                        |

## Example

```rust
use hc_internal::walker::WalkBuilder;

fn main() {
    for entry in WalkBuilder::new(".").build() {
        match entry {
            Ok(entry) => println!("{}", entry.path().display()),
            Err(err) => eprintln!("{err}"),
        }
    }
}
```

## License

[MIT](https://github.com/riya-amemiya/happy-cracking/blob/main/LICENSE)
