A "live port" of libwebp to rust.

A pinned commit of libwebp exists in upstream/.

Initially focused on simple encoding, then complex encoding, then decoding.

See the [lossless encoder porting plan](docs/LOSSLESS_PORT_PLAN.md) for the
`WebPEncodeLosslessRGBA` dependency map, implementation order, and testing strategy.
The follow-on [lossy encoder plan](docs/LOSSY_PORT_PLAN.md) maps the full Wasm
build's lossy RGBA and independent alpha-quality APIs, assuming lossless is complete.

The crate now contains scalar lossless leaves, the bit writer, and Huffman
construction and serialization (port milestones 1–2). Run `cargo test` for
portable fixtures and the Wasm regression, or
`cargo test --features native-oracle` to compare leaves with the pinned scalar
C implementation. See the [oracle guide](libwebpop/tests/reference/README.md)
for setup, fixture regeneration, and mismatch replay. The Rust public image
encoder is not implemented yet.

