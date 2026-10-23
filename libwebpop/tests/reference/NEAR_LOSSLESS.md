# Near-lossless preprocessing, predictor leaves and public C oracle

The scalar near-lossless extension is complete through milestone 5 of
[`NEAR_LOSSLESS_PORT_PLAN.md`](../../../docs/NEAR_LOSSLESS_PORT_PLAN.md).
Standalone preprocessing, predictor quantization with reconstruction feedback,
per-candidate routing and the checked public Rust wrapper match pinned C.
Existing lossless callers continue to select strength 100. This record retains
the original leaf provenance and adds the integration evidence below.

`VP8ApplyNearLossless(argb, width, height, stride, quality)` takes an immutable
ARGB slice and a stride in pixels and returns owned, tightly packed pixels.
It checks dimensions, stride, minimum source length, arithmetic overflow, and
allocation failures. Quality must be below 100, matching the C algorithm's
positive-bit-count precondition; the encoder bypasses it at 100.

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
The original ten committed files in `tests/fixtures/near_lossless_public.txt`
cover 0/100, palettes, opaque and intermediate-alpha inputs; 16 additional
files cover strengths 20/40/60/80. A 65x5 noise case proves
visible RGB and intermediate alpha changes. Small noisy images can select the
standalone dimension shortcut, and alpha-only images can select a palette;
neither by itself establishes active quantization. The integrated tests now
compare all Rust/C file bytes and independently decoded RGBA, unchanged input
and 100/lossless identity. The corpus has grown to 2,133 cases, adding boundary
geometries and 128 seeded geometry/pattern pairs at all six strength buckets.
Candidate routing is established separately by the stream tests below.

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
upstream checkout are unchanged. Integration fixture provenance follows below.

## Predictor feedback, search and final residuals (milestone 3)

`reference/predictor.c` extends the included pinned translation unit with the
`NearLosslessFeedback` request. Existing strength-100 request layouts and
fixtures are unchanged. New operation layouts are:

- 0: `(op,w,h,mode,x,y,count,max_quantization,exact,subtract_green)`, current
  and upper rows (each w+1 words), then w initialized differences. Returns
  residuals, both modified context rows, and unchanged differences.
- 1: `(op,w,h,bits,low_effort,max_quantization,exact,subtract_green)`, predictor
  map, pixels. Returns final residual pixels.
- 2/3: `(op,w,h,min_bits,max_bits,low_effort,strength,exact,subtract_green)`,
  pixels. Returns selected bits/map and, for op 2, the final residual image.
- 4: `(op,w,h,bits,tile_x,tile_y,update_up_to_index,max_quantization,exact,
  subtract_green,min_mode,diff_mask)`, pixels. Returns all 14 histograms per
  sampling level, both physical context rows, and the logical difference row.
  Context rows start at zero and difference entries at `0xa5`; untouched
  initialized values are meaningful sentinels, not uninitialized C scratch.

There are 19,422 exact native comparisons and 30 committed fixtures in
`fixtures/near_lossless_feedback.txt`. They cover every predictor, every step,
mode 0, image borders, partial/full tiles, context wrap, transparent cleanup,
low-effort/exact bypass, active search and final row lookahead, with and without
subtract-green. Independent reconstruction checks bound restored visible-color
and alpha error, preserve alpha endpoints and establish intermediate-alpha
changes. Search uses the all-plane mask for active quantization. Production
strength-100 calls do not allocate difference rows.

```sh
cargo test -p libwebpop --test near_lossless_feedback
cargo test -p libwebpop --features native-oracle --test near_lossless_feedback -- --nocapture
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test near_lossless_feedback feedback_fixtures_match_pinned_reference
```

## Candidate routing and complete streams (milestones 0/4)

`NearLosslessStream` in `reference/huffman.c` takes
`(w,h,pixel_stride,method,quality,strength,exact,candidate_index)`, then prepared
ARGB. Index `ffffffff` runs the original complete candidate loop; other indices
replay one candidate from original analysis with valid state. Production
candidate selection is unchanged. The adapter returns original candidate
count, replay entropy (or `ffffffff`), current width, route, nine selected stats,
transformed pixel count/data, then byte count and stream bytes. The writer
starts with five bits `0x15` and ends with eleven bits `0x5ad`.

Routes are 0 (non-palette strength 100), 1 (standalone), 2 (predictor),
3 (predictor/subtract-green), 4 (palette), 5 (palette/prediction). Palette
predictors always receive strength 100. All routes compare transformed pixels,
stats and bytes in 132 portable/live fixtures, including low effort and inherited
method-6/quality-100 trials. These internal configurations exercise branches
without adding an advanced public near-lossless API.

The private Rust loop also records evaluated indices, pre-finish sizes and
features under `cfg(test)`. Full-loop traces are checked against individual C
replays, keeping duplicate suppression and strict first-minimum selection.
Trace totals in route order are evaluated `[17,18,10,6,18,27]`, selected
`[7,10,2,2,3,0]` over 24 full loops. The tests require skipped duplicates and
losing trials. Palette/prediction is evaluated and bypasses quantization but
never wins these cases. Full loops also compare the final scratch data and
selected stream to original C, exposing reuse across candidates.

```sh
cargo test -p libwebpop --lib candidate_routes_transforms_and_full_loop_match_pinned_c -- --nocapture
cargo test -p libwebpop --features native-oracle --test near_lossless_streams
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test near_lossless_streams
```

Fixture generation was reviewed against the unchanged pin and scalar toolchain:
GCC `15.2.0-16ubuntu1`, CMake `4.2.3`, Release `-O3 -DNDEBUG`, SIMD/threading
off and near-lossless on; rustc `1.101.0-nightly (7189d39d9 2026-10-22)`,
x86_64 Linux. Only new integration fixtures and the 16 additional public files
were generated. Existing leaf/strength-100 fixtures and upstream are unchanged.
Native requests/results, compiler/configuration provenance and minimized
subgraph mismatch replays stay under `target/native-oracle/`.

## Public API, validation and retained full Wasm (milestone 5)

`WebPEncodeRGBAWithNearLossless` fixes quality 70, method 4 and exact false;
its signed strength is validated before conversion. `LosslessOptions` retains
its lossless pixel contract. `InvalidNearLosslessStrength` is distinct from
quality and geometry errors. The public tests cover padding, minimum lengths,
unaligned starts, source immutability, signed extremes, overflow, invalid sizes
and skinny 16383-pixel dimension limits. All 101 strengths are checked for
bucket identity on representative inputs, and 100 matches simple lossless.
Intermediate alpha can change below 100; endpoints remain exact.

CLI integration adds `encode_near_lossless_rgba`, with explicit signed strength
and `LosslessOptions` for independent quality/method/exact controls. The custom
wrapper retains quality 70/method 4/exact false, while the CLI keeps defaults
75/4 and honors its options. The existing lossless API always selects 100.
`WebPEncodeNearLosslessOptions` extends the normal-library public oracle with
`(width,height,byte_stride,quality,method,exact,strength)`, followed by RGBA bytes.
There are 560 additional live C comparisons across skinny/small/65-pixel
geometries, palettes, alpha, low effort, quality 70/75/90/100, method 0/4/6,
exact on/off and strengths 0/20/60/99/100. No existing fixture was regenerated.
Executable CLI tests check independent effort/strength propagation, fractional
effort truncation, exact/noalpha, active RGB/alpha changes, 100 identity,
stdin/stdout, diagnostics and output preservation on invalid strengths.

```sh
cargo test -p cwebpop
cargo test -p libwebpop --features native-oracle --test near_lossless_public \
  near_lossless_options_match_pinned_reference -- --nocapture
```

The [separate full-Wasm artifact and host](full-wasm/README.md) provide 196
independently decoded semantic/export regressions. They require Node only with
`wasm-oracle`. Production library builds remain pure safe Rust, with no new
runtime crate, C/Wasm dependency, or build script.

Verified commands:

```sh
cargo fmt --all -- --check
cargo test
cargo test -p libwebpop --features native-oracle,wasm-oracle -- --nocapture
cargo test -p libwebpop --release --features native-oracle,wasm-oracle \
  --test near_lossless_public --test near_lossless_feedback --test near_lossless_wasm
cargo build -p libwebpop --lib
```

## Full-size real-image parity and measurements

The ignored `real_image_corpus_parity_and_metrics` test reads the five recorded
`samples/*.webp` files (see `samples/SOURCES.md` for provenance), independently
decodes them once into packed RGBA, and checks strengths 0/20/40/60/80/100.
`near_lossless_file.c` is a binary-input test adapter for these inputs, which
exceed the logical request word limit. It links the normal copied public
wrapper to the same scalar C library and checks unchanged input. No algorithm
is copied, and neither the older Wasm nor the supplied CLI is the oracle.
Rust compares complete file bytes, independently decoded reference RGBA,
dimensions, alpha endpoints and 100/simple-lossless identity.

```sh
cargo test -p libwebpop --release --features native-oracle \
  --test near_lossless_public real_image_corpus_parity_and_metrics -- --ignored --nocapture
```

Prepared RGBA, reference files and `metrics.csv` remain under
`target/native-oracle/near-lossless-corpus/`. Timing measures one Rust library
call per image/strength, excluding source decoding, C work, validation and
output writing. It is an exploratory single sample, not a comparative speed
claim. Error is maximum absolute decoded RGBA deviation; RGB excludes pixels
with zero source or decoded alpha. On Linux the memory column is process
high-water RSS including source pixels, Rust/C outputs, both independent
decodes and test infrastructure; it is cumulative across the run and is not
isolated encoder memory. The [committed measurement snapshot](../../../docs/benchmarks/2026-10-23-near-lossless.md)
and its CSV record all 30 cases, source hashes and measurement limitations.
