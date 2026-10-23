# Near-lossless preprocessing, predictor leaves and public C oracle

This preparatory batch implements the standalone algorithm in
`src/enc/near_lossless_enc.rs`, corresponding to milestone 1 of
[`NEAR_LOSSLESS_PORT_PLAN.md`](../../../docs/NEAR_LOSSLESS_PORT_PLAN.md).
The lossless baseline is now complete. Milestone 2's predictor quantization
leaves and the custom public C oracle are also implemented; predictor feedback,
candidate routing, and the public Rust near-lossless wrapper remain for
subsequent work. The existing `VP8LNearLosslessBits` mapping is reused.

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

## Predictor quantization leaves (milestone 2)

`src/enc/predictor_enc.rs` now ports the pinned neighborhood and quantization
helpers. `reference/predictor.c` includes the original translation unit with
external definitions renamed; the new requests call its static helpers directly.
The predictor-local `NearLossless` is exposed as `PredictorNearLossless` to avoid
the standalone preprocessor's request name. `support/near_lossless_predictors.rs`
owns the Rust evaluator and deterministic corpus.

| Function | Input words | Output |
| --- | --- | --- |
| `MaxDiffBetweenPixels` | ARGB A, ARGB B | maximum component difference |
| `MaxDiffAroundPixel` | current, up, down, left, right | maximum neighbor/component difference |
| `AddGreenToBlueAndRed` | ARGB | restored ARGB |
| `MaxDiffsForRow` | width, pixel stride, subtract-green flag, three strided rows | middle-row differences; border sentinels retained |
| `NearLosslessComponent` | value, prediction, inclusive boundary, step | residual byte, reconstructed byte |
| `NearLosslessDiff` | byte A, byte B | modulo-256 difference |
| `PredictorNearLossless` | ARGB value, ARGB prediction, maximum step, maximum difference, subtract-green flag | residual ARGB, reconstructed ARGB |

Steps are powers of two in 1..=32. Row requests omit final-row padding and
initialize difference entries to `a5`; untouched borders are compared, not
uninitialized scratch. The Rust row helper takes an explicit pixel offset for
the current row and never modifies its source. Widths 1/2 have no interior and
do not read any rows. Residual/search adapters from the lossless port still use
strength 100; connecting these leaves to them is milestone 3.

The live suite makes 645,384 exact C comparisons. It exhausts all value and
prediction bytes at boundary 255 for all six steps, samples ties/wraps at every
possible boundary, exhausts byte subtraction and green restoration inputs,
and covers every local difference with alpha endpoints/intermediate values
and both subtract-green states. Row tests include padded strides and skinny
images. Independent checks measure linear error after rotating the inclusive
boundary to 255, and bound restored RGB error to demonstrate that green error
does not accumulate. Alpha endpoints are preserved while intermediate alpha
is shown to change.

`tests/fixtures/near_lossless_predictors.txt` holds 120 C-generated cases:

```sh
cargo test -p libwebpop --test near_lossless_predictors
cargo test -p libwebpop --features native-oracle --test near_lossless_predictors -- --nocapture
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test near_lossless_predictors predictor_quantization_fixtures_match_pinned_reference
```

## Custom public C wrapper (milestone 0 preparation)

`near_lossless_wrapper.c` copies `webp-wasm/near-lossless.c` from quad-image at
`1ee887300721ad754c5632b56349d5daad17c11e`, with an added provenance header.
The author granted permission to relicense it under this repository's BSD
license (`libwebpop/COPYING`); the copyright attribution is retained.
Tests need no sibling checkout. This wrapper links to the normally built
pinned scalar library, keeping included/renamed adapters out of public encoding.

`WebPEncodeRGBAWithNearLossless` takes width, height, byte stride, strength,
then RGBA bytes (one protocol word each), and returns complete file bytes.
Input starts at an unaligned address and omits final-row padding. The adapter
checks unchanged input. `NearLosslessWrapperErrors` has no input words and
returns five booleans: null-output rejection, then zero return and reset output
slot for strengths -1 and 101.

`support/near_lossless_public.rs` supplies 1,035 reference encodings, including
all 101 strengths on representative palette/non-palette images, bucket
boundaries, skinny and 63/64/65 dimensions, alpha variation and the committed
real sample. Every file independently decodes. Same-bucket bytes agree and
strength 100 matches the Rust simple API (and its existing pinned C oracle).
The ten committed files in `tests/fixtures/near_lossless_public.txt` cover
0/100, palettes, opaque and intermediate-alpha inputs. A 65x5 noise case proves
visible RGB and intermediate alpha changes. Small noisy images can select the
standalone dimension shortcut, and alpha-only images can select a palette;
neither by itself establishes active quantization. These tests do not yet
establish lower-strength Rust parity or trace candidate routing.

```sh
cargo test -p libwebpop --test near_lossless_public
cargo test -p libwebpop --features native-oracle --test near_lossless_public -- --nocapture
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test near_lossless_public near_lossless_public_fixtures_match_pinned_reference
```

New fixture provenance (2026-10-23): same upstream pin and C configuration as
the standalone fixtures, reviewed before generation: GCC `15.2.0-16ubuntu1`,
CMake `4.2.3`, Release `-O3 -DNDEBUG`, no extra C flags, SIMD/threading off,
near-lossless on. Rust is `1.101.0-nightly (7189d39d9 2026-10-22)` on x86_64 Linux.
Only the two new fixture files were generated; existing fixtures and the
upstream checkout are unchanged. Candidate traces remain for integration.
