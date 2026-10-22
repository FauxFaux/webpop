# Standalone near-lossless preprocessing

This preparatory batch implements the standalone algorithm in
`src/enc/near_lossless_enc.rs`, corresponding to milestone 1 of
[`NEAR_LOSSLESS_PORT_PLAN.md`](../../../docs/NEAR_LOSSLESS_PORT_PLAN.md).
It can be tested before the lossless encoder is complete. Predictor
quantization, candidate routing, and the public near-lossless wrapper remain
for subsequent work. The existing `VP8LNearLosslessBits` mapping is reused.

`VP8ApplyNearLossless(argb, width, height, stride, quality)` takes an immutable
ARGB slice and a stride in pixels and returns owned, tightly packed pixels.
It checks dimensions, stride, minimum source length, arithmetic overflow, and
allocation failures. Quality must be below 100, matching the C algorithm's
positive-bit-count precondition; the future encoder must bypass it at 100.

The Rust implementation packs the source before the first pass so every pass
can use the same safe in-place algorithm. Three rotating saved rows preserve
the input neighborhood during writes. Differential tests compare this with
both the C implementation's separate strided source and its aliased input and
output. No encoder selection behavior is added by this batch.

The new adapter, `near_lossless.c`, includes the pinned translation unit with
its external entry point renamed. The existing native oracle supplies the
build, protocol, provenance, and mismatch replay. Its new requests are:

| Function | Input words | Output |
| --- | --- | --- |
| `FindClosestDiscretized` | byte, bits (1..5) | discretized byte |
| `ClosestDiscretizedArgb` | ARGB, bits | discretized ARGB |
| `IsNear` | ARGB A, ARGB B, limit | boolean |
| `IsSmooth` | center, left, right, up, down, limit | boolean |
| `NearLossless` | width, height, pixel stride, bits, in-place flag, source pixels | one complete packed pass |
| `VP8ApplyNearLossless` | width, height, pixel stride, quality (0..99), source pixels | complete packed preprocessing output |

For image requests, source words include inter-row padding but omit final-row
padding. The in-place pass requires stride equal to width. Only initialized
logical output pixels cross the protocol; saved-row scratch is not compared.
`tests/support/near_lossless.rs` owns these cases and the Rust evaluator, keeping
the lossless fixture file and shared evaluator unchanged.

Run the portable checks or opt into pinned C comparisons:

```sh
cargo test -p libwebpop --test near_lossless
cargo test -p libwebpop --features native-oracle --test near_lossless -- --nocapture
```

The live corpus contains 26,201 requests. It exhausts all byte values for each
discretization bit count, every channel difference in both signs and every
lane at each smoothness threshold,
and each of the four smoothness neighbors just below, at, and above the limit.
Image cases cover widths 1/2, heights 1/2/3, dimensions 63/64/65 in both
orientations, padding, in-place passes, smooth ramps, impulses, noise,
constant planes, binary alpha, and intermediate alpha. Portable checks compare
row buffering with immutable source neighborhoods, all 100 active strengths
for same-bucket identity, border and alpha endpoint preservation, unchanged
inputs, skinny maximum dimensions, and invalid geometry/strength handling.
An active 64x3 case explicitly demonstrates changes to intermediate alpha and
RGB, so the small-image shortcut cannot mask a missing quantizer.

The separate `tests/fixtures/near_lossless.txt` contains 49 C-generated leaf,
pass, shortcut, and active-route outputs. Regenerate with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test near_lossless preprocessing_fixtures_match_pinned_reference
```

Initial fixture provenance (2026-10-22): Ubuntu x86_64 Linux, GCC
`15.2.0-16ubuntu1`, CMake `4.2.3`, Release `-O3 -DNDEBUG`, no additional C flags.
The source pin remains `1844042d1ca751e62e61df7617dba19c98588339`, with static
libwebp, near-lossless enabled, SIMD and threading disabled. Each live run
records the toolchain and exact flags in `target/native-oracle/provenance.txt`;
see [README.md](README.md) for the build configuration and replay format. No
production C or Wasm dependency is introduced.
