A "live port" (by agents) of libwebp to rust.

A pinned commit of libwebp exists in upstream/.

Initially focused on simple encoding, then complex encoding, then decoding.

License: ... same as upstream, I think? I think that's how it works.

--

See the [lossless encoder porting plan](docs/LOSSLESS_PORT_PLAN.md) for the
`WebPEncodeLosslessRGBA` dependency map, implementation order, and testing strategy.
Next is the [near-lossless VP8L extension](docs/NEAR_LOSSLESS_PORT_PLAN.md),
assuming lossless is complete. The later [lossy encoder plan](docs/LOSSY_PORT_PLAN.md)
maps the full Wasm build's lossy RGBA and independent alpha-quality APIs.

The lossless encoder port is complete through milestone 9, including automatic
transform analysis, palette trials, LZ77/cache selection and histogram clustering.
Run `cargo test` for portable fixtures and independent decoding checks, or
`cargo test --features native-oracle` to compare intermediate stages and complete
public output with the pinned scalar C implementation. See the
[oracle guide](libwebpop/tests/reference/README.md) for setup, fixture regeneration,
and mismatch replay.

Encode an image with the Rust lossless pipeline:

```sh
cargo run --release -p cwebpop -- input.png -o output.webp
```

Use `libwebpop::WebPEncodeLosslessRGBA(rgba, width, height, stride)` for the
upstream simple API defaults, or `libwebpop::encode_lossless_rgba` with
`LosslessOptions` for quality, method and exact transparent-RGB handling. Both
return an owned complete WebP file. Inputs use positive byte strides, dimensions
1–16,383, and RGBA channel order; final-row padding is optional. The source is
never modified. Default encoding preserves alpha and visible RGB; `exact: true`
also preserves RGB under fully transparent pixels.

Complete public outputs match the pinned scalar C encoder in the regression
corpus. This is a pure safe Rust encoder; normal builds do not compile or run C.
See the [CLI documentation](cwebpop/README.md) for settings and input formats and
the [lossless plan](docs/LOSSLESS_PORT_PLAN.md) for validation and timing baselines.

