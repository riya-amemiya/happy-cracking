# hrg

A searcher that accepts GNU grep 3.12 and ripgrep 15.1.0 command lines, from the [happy-cracking](https://github.com/riya-amemiya/happy-cracking) project.

## Installation

```bash
cargo install hrg
```

This installs three binaries that share one implementation: `hrg`, `hgrep`, and `hg`.

## Binaries

| Command | Description                                                        |
| ------- | ------------------------------------------------------------------ |
| `hgrep` | Searcher accepting GNU grep 3.12 and ripgrep 15.1.0 command lines  |
| `hg`    | Same binary with ripgrep meanings for colliding short flags        |
| `hrg`   | Same as `hg`                                                       |

The short flags whose meanings collide between GNU grep and ripgrep (`-r -h -L -s -E -z -I -T -U -u -0`) take the GNU grep meaning under `hgrep` and the ripgrep meaning under `hg` and `hrg`; their other meaning stays reachable through the long option. Directory operands without `-r` are searched like ripgrep (ignore files, hidden-file and binary filtering); `-r`/`-R` search like GNU grep. `hgrep` defaults to GNU grep's BRE syntax and output conventions, `hg`/`hrg` to ripgrep's regex syntax and output conventions.

## Usage

```bash
hgrep -rn needle src/
hgrep -E 'foo|bar' notes.txt
hgrep -n -i flag firmware.bin
hgrep -r --gitignore TODO .
hgrep needle src/
hgrep -t rust -g '!target' needle
hgrep --help
hg needle src/
hg -r replacement 'needle' src/
hrg --json needle src/
```

[Why hgrep and hfind outrun ripgrep and fd](https://github.com/riya-amemiya/happy-cracking/blob/main/docs/hgrep-hfind-performance.md) walks through the design differences behind their speed, with diagrams.

## Related crates

- [`hfd`](https://crates.io/crates/hfd): find and fd compatible file walker
- [`hc-internal`](https://crates.io/crates/hc-internal): shared walker, search, printer, regex, and encoding library that `hrg` is built on

## License

[MIT](https://github.com/riya-amemiya/happy-cracking/blob/main/LICENSE)
