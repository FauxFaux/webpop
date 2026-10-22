# cwebpop

CLI scaffold for the Rust WebP encoder. The option inventory follows
`upstream/examples/cwebp.c`, including `resize_mode` and the legacy
`alpha_cleanup` option absent from the host's help.

```sh
cargo run -p cwebpop -- --help
cargo run -p cwebpop -- --version
cargo run -p cwebpop -- --lossless -q 80 input.png -o output.webp
```

Help and version are implemented. Encoding options are parsed but have no effect:
an encoding invocation reports that encoding is not implemented, exits with
status 1, and does not open input or create output files. The public Rust encoder
is not available yet.

This uses clap conventions: upstream's multi-letter `-lossless`, `-alpha_q`, etc.
are spelled `--lossless`, `--alpha_q`, etc. Single-letter encoding options retain
`-q`, `-m`, `-z`, `-s`, `-f`, `-d`, `-o`, and `-v`. Use `-h`/`--help` for help
and `-V`/`--version` for the cwebpop package version. Use `--` before an input
filename beginning with a hyphen.

Upstream defaults and numeric ranges in help are documentation for future
implementation. Omitted values remain unset; numeric range checks, preset
ordering/overrides, and interactions between options are not implemented.
Enumerated choices, numeric types, and multi-value argument counts are checked.

Run `cargo test -p cwebpop` to check the CLI schema and parsing.
