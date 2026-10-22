# cwebpop

Encode image files as lossless WebP using the Rust `libwebpop` pipeline.

```sh
cargo run -p cwebpop -- --help
cargo run -p cwebpop -- --version
cargo run -p cwebpop -- --lossless -q 80 input.png -o output.webp
cargo run --release -p cwebpop -- --exact input.png -o output.webp
cat input.png | target/release/cwebpop --quiet - -o - > output.webp
```

Lossless encoding is the default; `--lossless` is optional and `-o` is required.
The CLI reads bytes, decodes them with `image`, converts them to RGBA, calls
`libwebpop::encode_lossless_rgba`, and writes the complete WebP. Supported input
formats are PNG, JPEG, PNM, TGA, TIFF, BMP and WebP. Content determines the input
format, with the filename extension as a fallback for formats such as TGA.
Use `-` for stdin or stdout. Diagnostics and statistics go to stderr.

Supported settings are `-q` (lossless compression effort, 0–100, default 70),
`-m` (effort method, 0–6, default 4), `--exact` (preserve hidden RGB),
`--noalpha`, `--quiet`, `--short` (only the byte count) and `-v` (timing).
Fractional quality values are truncated to the integer effort used by the
current kernels. `--near_lossless 100` and `--noasm` also work; preprocessing
is disabled and the implementation is portable Rust.

The existing upstream-style option inventory is retained, but unsupported
options, including crop/resize, presets, metadata and near-lossless values
below 100, fail before opening input or output. Invalid input or encoding
failure leaves the output untouched.

The library currently uses a fixed palette/predictor/cross-color policy.
Automatic upstream entropy analysis and configuration trials are still being
ported, so output is valid lossless WebP but compressed bytes and sizes need
not match libwebp's simple API.

This uses clap conventions: upstream's multi-letter `-lossless`, `-alpha_q`, etc.
are spelled `--lossless`, `--alpha_q`, etc. Single-letter encoding options retain
`-q`, `-m`, `-z`, `-s`, `-f`, `-d`, `-o`, and `-v`. Use `-h`/`--help` for help
and `-V`/`--version` for the cwebpop package version. Use `--` before an input
filename beginning with a hyphen.

Run `cargo test -p cwebpop` for parser checks and executable round trips,
including files, stdin/stdout, alpha, format detection and failure handling.
