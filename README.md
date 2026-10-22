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

The crate now contains scalar lossless leaves, the bit writer, Huffman
construction and serialization, token/reference storage, and histogram
accumulation and cost estimation, hash chains, LZ77 strategies, cache selection,
and traceback refinement (port milestones 1–4). Run `cargo test` for
portable fixtures and the Wasm regression, or
`cargo test --features native-oracle` to compare leaves with the pinned scalar
C implementation. See the [oracle guide](libwebpop/tests/reference/README.md)
for setup, fixture regeneration, and mismatch replay. The Rust public image
encoder is not implemented yet.

