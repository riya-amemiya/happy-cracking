# hfd

A file walker that accepts find expressions and fd 10.4.2 command lines, from the [happy-cracking](https://github.com/riya-amemiya/happy-cracking) project.

## Installation

```bash
cargo install hfd
```

This installs two binaries that share one implementation: `hfd` and `hfind`.

## Binaries

| Command | Description                                                        |
| ------- | ------------------------------------------------------------------ |
| `hfind` | Walker accepting find expressions and fd 10.4.2 command lines      |
| `hfd`   | Same binary that reads bare positionals as fd's pattern and paths  |

A command line containing a find expression (`-name`, `-type`, `-print0`, and so on) is parsed as find syntax; otherwise it is parsed as fd syntax, except that bare positional arguments are search paths under `hfind` and fd's `[pattern] [path...]` under `hfd`.

## Usage

```bash
hfind .
hfind -L src -name '*.rs'
hfind . -type f -size -10k
hfind --no-ignore . -name '*.log'
hfind --help
hfd '\.rs$' src
hfd -e rs -x wc -l
hfd -H -I -t f config
```

[Why hgrep and hfind outrun ripgrep and fd](https://github.com/riya-amemiya/happy-cracking/blob/main/docs/hgrep-hfind-performance.md) walks through the design differences behind their speed, with diagrams.

## Related crates

- [`hrg`](https://crates.io/crates/hrg): GNU grep and ripgrep compatible searcher
- [`hc-internal`](https://crates.io/crates/hc-internal): shared walker, search, printer, regex, and encoding library that `hfd` is built on

## License

[MIT](https://github.com/riya-amemiya/happy-cracking/blob/main/LICENSE)
