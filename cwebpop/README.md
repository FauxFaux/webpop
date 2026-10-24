# cwebpop

Encode image files as lossless, near-lossless or lossy WebP using Rust `libwebpop`.

```sh
cargo run -p cwebpop -- --help
cargo run -p cwebpop -- --version
cargo run -p cwebpop -- --lossless -q 80 input.png -o output.webp
cargo run --release -p cwebpop -- --exact input.png -o output.webp
cargo run --release -p cwebpop -- --near_lossless 60 -q 90 -m 6 input.png -o output.webp
cargo run --release -p cwebpop -- --lossy -q 98.5 --alpha_q 70 input.png -o output.webp
cargo run --release -p cwebpop -- --lossy --sharp_yuv input.png -o output.webp
cat input.png | target/release/cwebpop --quiet - -o - > output.webp
```

Lossless encoding is the default; `--lossless` is optional and `-o` is required.
The CLI reads bytes, decodes them with `image`, converts them to RGBA, calls
the selected `libwebpop` encoder, and writes the complete WebP. Supported
input formats are PNG, JPEG, PNM, TGA, TIFF, BMP and WebP. Content determines the input
format, with the filename extension as a fallback for formats such as TGA.
Use `-` for stdin or stdout. Diagnostics and statistics go to stderr.

Supported settings are `-q` (lossless compression effort, 0–100, default 75),
`-m` (effort method, 0–6, default 4), `--exact` (disable transparent RGB cleanup),
`--noalpha`, `--quiet`, `--short` (only the byte count) and `-v` (timing).
In VP8L, fractional quality values are truncated to integer effort.
`--near_lossless N` selects fidelity from 0–100, independently
of compression effort; the default 100 disables pixel quantization. Lower
strengths can change RGB and intermediate alpha. Alpha 0 and 255 stay exact.
`--exact` also bypasses predictor quantization, but standalone preprocessing can
still change pixels, including alpha. `--noasm` is accepted; the implementation
is portable Rust. See [quality, effort and levels](../docs/LEVELS.md).

`--lossy` selects the default-preset VP8 encoder through
`encode_lossy_rgba`. Here `-q` controls color fidelity, accepts
finite values from 0–100 and preserves fractional values; the default is 75.
Quality 100 still uses YUV420 and quantization, so RGB can change.
`--alpha_q N` accepts integer alpha quality 0–100 (default 100, exact alpha).
Lower values permit alpha quantization; opaque output ignores alpha quality.
The lossy path currently supports method 4 only, including explicit `-m 4`.
`--lossy` conflicts with `--lossless`, `--near_lossless` and `--exact`;
`--alpha_q` requires `--lossy`. `--noalpha`, streams and reporting options work
in either mode. Invalid or unsupported settings fail before reading input.

`--lossy --sharp_yuv` enables the scalar SharpYUV RGB-to-YUV conversion;
`--lossy --pre 4` is equivalent. Images with width or height below 4 retain
ordinary conversion, matching upstream. Alpha is preserved by conversion and
then encoded according to `--alpha_q`. `--pre 0` selects ordinary conversion
unless `--sharp_yuv` is also set. Other preprocessing values remain
unsupported; both options require `--lossy`. The library exposes this choice
as `LossyOptions::use_sharp_yuv`; existing simple APIs keep ordinary conversion.

The existing upstream-style option inventory is retained, but unsupported
options, including crop/resize, presets and metadata, fail before opening input
or output. Invalid input or encoding failure leaves the output untouched.

The library uses upstream entropy analysis and configuration trials, including
palette ordering, transform, LZ77 and cache selection. Library defaults match
the pinned scalar libwebp `WebPEncodeLosslessRGBA` implementation in the
regression corpus. Quality, method and exact settings are also compared with
the corresponding upstream configuration. The CLI defaults to quality 75,
matching upstream `cwebp -lossless`; the library simple API defaults to 70.
Pass `-q 70` to match `WebPEncodeLosslessRGBA`.
With `--near_lossless N -q 70 -m 4` and no `--exact`, output matches the custom
`WebPEncodeRGBAWithNearLossless` wrapper. `--near_lossless 100` matches ordinary
lossless encoding at the same effort and exact settings.

This uses clap conventions: upstream's multi-letter `-lossless`, `-alpha_q`, etc.
are spelled `--lossless`, `--alpha_q`, etc. Single-letter encoding options retain
`-q`, `-m`, `-z`, `-s`, `-f`, `-d`, `-o`, and `-v`. Use `-h`/`--help` for help
and `-V`/`--version` for the cwebpop package version. Use `--` before an input
filename beginning with a hyphen.

Run `cargo test -p cwebpop` for parser checks and executable round trips,
including files, stdin/stdout, alpha, format detection and failure handling.
