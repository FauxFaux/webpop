# libwebpop: near-lossless VP8L extension after lossless

## Target and order

The [lossless port](LOSSLESS_PORT_PLAN.md) is complete for its initial scalar
RGBA scope, including the public API, native oracle, independent decoding and
recorded size/timing baseline. `WebPEncodeRGBAWithNearLossless` is now
implemented, ahead of the [lossy VP8 port](LOSSY_PORT_PLAN.md). This document records the completed
scalar near-lossless extension and its acceptance evidence.

Near-lossless encoding changes pixels within the existing VP8L pipeline and
writes the same still-image WebP container. Reuse the completed ARGB importer,
transform selection, lossless compression, writer, validation, and oracle.
No VP8 macroblock encoder, YUV conversion, boolean coder, `ALPH` chunk, alpha
quality API, or new container format is a prerequisite. Those remain in the
later lossy plan. The new algorithm work is concentrated in
[`near_lossless_enc.c`](../upstream/src/enc/near_lossless_enc.c), the previously
unreachable branches of
[`predictor_enc.c`](../upstream/src/enc/predictor_enc.c), and their selection in
[`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c).

Keep the upstream pin at `1844042d1ca751e62e61df7617dba19c98588339`. The pinned
scalar C implementation, with near-lossless support enabled, is the byte-exact
oracle under the recorded build/toolchain. The older full Wasm is an additional
wrapper/export regression backend, not the source of algorithm truth.

## Current repository (2026-10-23)

- **Milestone 1 is complete.**
  [`enc/near_lossless_enc.rs`](../libwebpop/src/enc/near_lossless_enc.rs)
  implements discretization, smoothness tests, saved-row passes and checked
  `VP8ApplyNearLossless`, reusing `VP8LNearLosslessBits`. It packs immutable
  strided ARGB before the first pass, then processes the owned buffer in place.
  Its stride is in pixels; the public RGBA API uses a byte stride.
- The standalone [test/provenance record](../libwebpop/tests/reference/NEAR_LOSSLESS.md)
  documents 49 committed C fixtures and 26,201 native comparisons. Portable
  tests independently check rounding and saved-row neighborhoods, dimension
  shortcuts, active quantization, alpha endpoints, strength buckets, unchanged
  input and validation. Reuse these adapters and cases rather than recreating
  the preprocessing harness.
- **Milestone 2 is complete.** Predictor neighborhood differences, component
  and pixel residual quantization, and subtract-green compensation are ported
  in `enc/predictor_enc.rs`. There are 120 committed C fixtures and 645,384
  native comparisons, including exhaustive component value/prediction pairs
  at every step size, varied boundaries and reconstructed pixels. Independent
  checks bound restored-channel error and verify neighborhood rows, alpha
  endpoints and half-step boundary handling.
- **Milestones 0 and 3–5 are complete.** `GetResidual`, tile histograms,
  predictor search and final generation use quantization with reconstructed
  context. The private configuration and per-candidate routing connect both
  algorithms; the public wrapper is exported with checked signed strength.
  Existing lossless callers retain strength 100 and their pixel contract.
- There are 19,422 exact native feedback/search comparisons and 30 committed
  feedback fixtures, plus 132 candidate replay/full-loop fixtures comparing
  routing, initialized transformed pixels, stats and internal stream bytes.
  Test-only traces verify all six route categories, losing trials, palette
  duplicate suppression and strict first-minimum selection. Palette prediction
  is exercised even though it does not win this trace corpus.
- The expanded public corpus compares 2,133 complete Rust/C files and decoded
  RGBA, including deterministic random geometries/patterns, all 101 strengths
  on representative images and every bucket boundary. There are 26 portable
  public fixtures. Another 30 comparisons cover all five full-size recorded
  real images at every bucket, up to 81 million pixels. Strength 100 matches
  the simple lossless API; input stays
  unchanged at every setting. Validation includes signed strength, geometry,
  overflow, minimum buffers, padding, unaligned input and skinny limit sizes.
- The retained full Wasm has a separate opt-in `wasm-oracle` regression:
  196 files independently decode, lower strengths change RGB/intermediate
  alpha, endpoints are preserved, and 100 matches its own simple lossless API.
  See its [artifact/host record](../libwebpop/tests/reference/full-wasm/README.md).
- CLI integration is complete. `--near_lossless 0..100` selects fidelity
  independently of `-q`, `-m` and `--exact`, keeping CLI defaults 75/4.
  `encode_near_lossless_rgba` accepts strength plus the existing effort/cleanup
  options; the custom wrapper retains fixed quality 70/method 4/exact false.
  Another 560 public C comparisons cover these configurable combinations.

The [reference record](../libwebpop/tests/reference/NEAR_LOSSLESS.md) documents
request layouts, fixture regeneration, toolchain provenance and full-size
real-image measurements. Production builds use neither C nor Wasm.

## Public API and compatibility

Retain the wrapper's exported name:

```rust
pub fn WebPEncodeRGBAWithNearLossless(
    rgba: &[u8], width: u32, height: u32, stride: usize,
    near_lossless: i32,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError>;
```

All RGBA entry points take the output buffer last and return `Result<(), _>`.
Success replaces its contents with one complete WebP file, reusing its
capacity; errors leave its contents unchanged. The encoded size is
`output.len()`. Internal bitstream and picture working buffers remain owned
by the encoder.

This is quad-image's custom wrapper, not an upstream simple API. The reviewed
source from `webp-wasm/near-lossless.c` at
`1ee887300721ad754c5632b56349d5daad17c11e` is copied into
[`reference/near_lossless_wrapper.c`](../libwebpop/tests/reference/near_lossless_wrapper.c).
It retains source/copyright provenance and uses this repository's BSD license
with the author's permission.

| Setting | Value | Consequence |
| --- | --- | --- |
| Preset | `WEBP_PRESET_DEFAULT` | Same initialization as the simple lossless API |
| `lossless`, `picture.use_argb` | `1`, `1` | VP8L, packed ARGB import, no YUV conversion |
| `quality`, `method` | `70`, `4` | Inherited lossless compression effort and search |
| `near_lossless` | Caller integer `0..=100` | Lower values permit stronger pixel changes; 100 disables them |
| `exact` | `0` | Inherited transparent RGB cleanup remains active |
| `thread_level`, `image_hint` | `0`, `WEBP_HINT_DEFAULT` | Synchronous inherited default path |
| Stats, progress hook, extra info | Unset | No new public diagnostics/callback API |

Reject near-lossless values outside `0..=100`. Preserve the lossless API's
checked positive-stride geometry: dimensions in `1..=16383`,
`stride >= width * 4`, and input length at least
`(height - 1) * stride + width * 4`, with checked arithmetic. Write encoded bytes into the caller's buffer
or return an error; C pointer ownership details are not Rust API requirements. Input pixels must remain unmodified.

Require `near_lossless=100` to produce exactly the same bytes as
`WebPEncodeLosslessRGBA` for every valid input. Keep the existing API on its
unchanged default setting; share a private config-bearing VP8L path rather than
creating another encoder or re-importing already quantized pixels through the
simple lossless API.

The existing `WebPEncodingError` is an alias for `LosslessEncodeError`; retain
that API and add a distinct invalid-near-lossless-strength error. The current
public `LosslessOptions { quality, method, exact }` and `encode_lossless_rgba`
must retain their lossless pixel contract. Introduce a private configuration
carrying near-lossless strength, with existing callers selecting 100 and the
new wrapper selecting quality 70, method 4 and exact false. Validate the signed
strength before converting it to an unsigned internal value.

The subsequent CLI integration adds
`encode_near_lossless_rgba(rgba, width, height, stride, near_lossless, &options, &mut output)`.
It accepts the existing `LosslessOptions` for effort/cleanup and validates
strength separately. `encode_lossless_rgba` always selects 100, preserving its
lossless contract. The named custom wrapper keeps its fixed defaults. At 100,
the configurable entry point matches lossless encoding with the same options.

**Alpha is not generally exact below 100.** The standalone preprocessor rounds
all four ARGB channels. Predictor quantization preserves alpha 0 and 255 but
can quantize intermediate alpha values. Both routes preserve fully transparent
and fully opaque endpoints. Compare all decoded RGBA channels against the same
pinned-reference file decoded by the same independent decoder. At 100, retain
the completed lossless contract: exact alpha and exact RGB where alpha is
nonzero. Hidden RGB may change at every setting because `exact=0`.

Do not change `exact` to preserve alpha: that changes wrapper compatibility and
also bypasses the predictor's near-lossless residual path. A future API promising
lossless alpha would require a separate design.

## Ported behavior

The shared [`VP8LNearLosslessBits`](../upstream/src/dsp/lossless_common.h) helper
maps the public setting to quantization strength:

| `near_lossless` | Bits | Maximum predictor quantization step |
| --- | --- | --- |
| `100` | `0` | `1` (no quantization) |
| `80..99` | `1` | `2` |
| `60..79` | `2` | `4` |
| `40..59` | `3` | `8` |
| `20..39` | `4` | `16` |
| `0..19` | `5` | `32` |

These are quantizer controls, not a uniform RGBA bit-depth conversion or a
promise that every pixel changes. Keep compression quality fixed at 70.

Selection occurs **per inherited transform candidate**, in `EncodeStreamHook`:

| Candidate | Required near-lossless behavior |
| --- | --- |
| Palette, with or without prediction | No standalone preprocessing; predictor strength forced to 100 for packed palette indices |
| Non-palette, without prediction | At settings below 100, run `VP8ApplyNearLossless` before any subtract-green transform |
| Non-palette, with prediction | Quantize residuals during predictor search and final residual generation; no standalone preprocessing |
| Any candidate at 100 | Inherited lossless path |

Preserve analysis on the inherited input, candidate order, palette duplicate
suppression, scratch resets, byte-size comparisons and strict tie handling.
Changed pixels can change downstream transforms, references, cache choice,
histogram clustering, and the winning stream. A universal preprocessing pass
before analysis would produce a different encoder.

### Standalone pixel preprocessing

Already ported in `enc/near_lossless_enc.rs`; preserve these behaviors while
connecting `VP8ApplyNearLossless` to the candidate loop:

- `FindClosestDiscretized` rounds to a multiple of `1 << bits`, with banker
  ties and a special clamp to 255. Apply it independently to all ARGB lanes.
- `IsNear` uses strict channel differences within the limit; equality to the
  positive or negative limit is outside. `IsSmooth` checks four neighbors.
- `NearLossless` retains smooth pixels and the image border, and discretizes
  other pixels. Three saved rows preserve the source neighborhood during
  in-place passes.
- Start with `VP8LNearLosslessBits(quality)`, then repeat at each lower bit
  count down to 1. The first pass reads picture ARGB with its pixel stride;
  later passes read and write the tightly packed destination. The existing Rust
  port packs first, then runs every pass in place with equivalent neighborhoods.
- Copy without preprocessing when **both** dimensions are below 64, or height
  is below 3. This rule belongs only to the standalone route; it does not turn
  off predictor quantization on small images. Widths 1 and 2 have no interior
  pixels to adjust.

Do not call this function at strength 100: its upstream domain requires a
positive bit count. Keep checked buffer sizing and allocation errors in Rust.

### Prediction and reconstruction feedback

The extension retains the inherited predictor implementation and mode search. The ported leaves are `MaxDiffBetweenPixels`, `MaxDiffAroundPixel`,
`AddGreenToBlueAndRed`, `MaxDiffsForRow`, `NearLosslessComponent`,
`NearLosslessDiff`, and predictor-local `NearLossless`.

Preserve modulo-256 residual arithmetic, boundary-aware rounding and ties,
step reduction based on local maximum differences, and the no-quantization
case at `max_diff <= 2`. With subtract-green active, measure neighborhood
differences in restored color space and compensate red/blue for the quantized
green change. Otherwise two channel errors would accumulate.

`GetResidual` bypasses quantization at strength 100, for mode 0, and on the
image border. In the active path it reconstructs each adjusted pixel into the
current row before later predictions use it. Preserve transparent RGB cleanup
and the extra top-right context slot at the right edge, including updates when
the leftmost pixel changes. Treat candidate scratch rows separately from the
source image.

Both `ComputeResidualsForTile` and `CopyImageWithPrediction` must receive the
new behavior. Search must evaluate quantized candidate residuals, not choose
modes using lossless residuals and quantize only afterward. `GetPlaneDiffMask`
returns all four planes when `max_quantization > 1`; an initially constant
plane can change during quantization. Preserve its mode bounds and histogram
effects. Final generation computes lower-row difference data before
overwriting pixels with residuals; retain that ordering and the two difference
row buffers.

## Dependency map and Rust layout

This map shows the extension to the completed lossless pipeline, not a new
complete call graph. Paths are relative to `upstream/src/`.

```text
WebPEncodeRGBAWithNearLossless                        custom test wrapper / Rust API
├── config preset q70/method4, lossless=1, caller near_lossless
├── checked RGBA import [ARGB]                        inherited
└── WebPEncode [lossless branch]                      enc/webp_enc.c
    ├── WebPReplaceTransparentPixels [exact=0]        inherited
    └── VP8LEncodeImage → VP8LEncodeStream             enc/vp8l_enc.c, inherited
        ├── EncoderAnalyze / EncoderInit             inherited
        └── EncodeStreamHook [candidate loop]        extend routing/state
            ├── no palette/predict, strength<100
            │   └── VP8ApplyNearLossless              ported; connect routing
            │       └── NearLossless → IsSmooth / ClosestDiscretizedArgb
            ├── palette encode/map                   inherited, no quantization
            ├── ApplySubtractGreen                   inherited, after preprocessing
            ├── ApplyPredictFilter                   extend strength routing
            │   └── VP8LResidualImage                enc/predictor_enc.c, extend
            │       ├── GetPlaneDiffMask             pass active quantization
            │       ├── predictor search → ComputeResidualsForTile
            │       │   ├── MaxDiffsForRow
            │       │   └── GetResidual → NearLossless → NearLosslessComponent
            │       └── CopyImageWithPrediction
            │           ├── MaxDiffsForRow [look ahead before overwrite]
            │           └── GetResidual [reconstruction feedback]
            └── cross-color / LZ77 / cache / histograms / Huffman / RIFF
                                                        inherited, changed inputs
```

Reuse `libwebpop/src/enc/near_lossless_enc.rs`; extend the inherited
`enc/{predictor_enc,vp8l_enc,config_enc,webp_enc}.rs`, `encode.rs`, error type
and crate-root exports as their responsibilities require. The checked picture
importer already supplies the required owned ARGB input. Reuse
`dsp/lossless_common.rs` and its strength mapping. Keep upstream function names,
source references, constants, license and patent notices; carry `WEBP_INLINE`
annotations into new ports as plain `#[inline]`, including macro-generated
helpers. The two C files each have a static `NearLossless`; retain separate
module scopes.

Carry `near_lossless` through the private config and preserve
`argb_content`/`kEncoderNearLossless` semantics, buffer invalidation, and input
copies across trials. The current Rust loop copies immutable source pixels for
each non-palette trial instead of tracking `argb_content`; preserve equivalent
candidate inputs and never reuse quantized pixels for an unrelated trial.
Retain the synchronous scalar implementation. Use owned buffers, explicit row
offsets, checked lengths and `Result`; restrict wrapping
to the source's demonstrated pixel arithmetic. No new crate, DSP dispatch
framework, production C build, or production Wasm dependency is needed.

## Oracle and verification

Extend the existing [native oracle](../libwebpop/tests/reference/README.md),
logical request protocol, fixture evaluator and seeded corpus. Its CMake build
already has `WEBP_NEAR_LOSSLESS=ON`, SIMD off and threading off. Standalone
adapters and their evaluator already exist in `reference/near_lossless.c` and
`support/near_lossless.rs`. The copied custom public wrapper uses the normally
built reference library, and `reference/predictor.c` exposes the quantization
leaves. Its residual/search adapters now expose active quantization outside
`upstream/`, while retaining the old strength-100 requests. Rename
external definitions as in the existing wrappers to avoid duplicate symbols;
public encoding comparisons must still use the normally built reference library.

Compare in layers:

1. Leaves: discretized bytes/ARGB, smoothness tests, neighborhood differences,
   strength mapping, residual components, reconstructed channels and
   subtract-green compensation.
2. Subgraphs: each preprocessing pass and complete output; candidate residuals,
   changed scratch rows, max-difference rows, histograms, plane masks, predictor
   maps and final residual images; candidate routing, transform data and
   winning stream. Compare logical initialized data, never unused scratch,
   pointers, capacities or struct padding.
3. Public APIs: complete bytes against the pin, independently decoded dimensions
   and RGBA against decoded reference output, unchanged input, and byte identity
   with the simple lossless API at 100. Preserve all inherited tests.

Use counters or traces to establish that standalone preprocessing, predictor
quantization with and without subtract-green, and palette bypass actually run.
Track evaluated candidates as well as the selected one. If ordinary public
fixtures do not reach a branch, add direct subgraph tests with valid synthetic
state; do not replace production selection with a forced path.

The full Wasm artifact identified in the
[lossy plan's inspection record](LOSSY_PORT_PLAN.md#inspected-wasm-build-and-feature-boundary)
exports this wrapper. Its durable regression and separate retained artifact
are documented in `tests/reference/full-wasm/README.md`, with SHA-256
`f6317d0cce217547b165a768f82647c2e923c489b5ab75b8dd0310722cffa477`.
The Node host uses SIMD and the recorded initialization sequence. The new
regression checks lower strengths and near-lossless-100/lossless identity; the
earlier inspection established only the latter for 12 pairs. Old-Wasm/new-pin
byte identity is not required.

## Implementation order and acceptance gates

The lossless prerequisite is satisfied. Standalone preprocessing was completed
ahead of the full milestone-0 oracle work. Dependencies below are additional
near-lossless milestones; no lossy VP8 milestone is required.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 (complete) | Custom public wrapper, predictor adapters and candidate traces | Completed lossless | All inherited routes evaluated, selected routes recorded; 132 C candidate/full-loop fixtures and 2,133 public comparisons |
| 1 (complete) | Strength mapping, discretization/smoothness leaves, row buffering and complete `VP8ApplyNearLossless` | Standalone oracle adapters (complete) | 26,201 exact native comparisons and 49 portable fixtures; independent rounding/neighborhood checks, stride, dimension shortcuts, in-place updates and invalid inputs |
| 2 (complete) | Predictor neighborhood and component quantization leaves, modulo boundaries and subtract-green compensation | Predictor leaf adapters (complete) | 645,384 exact native comparisons and 120 portable fixtures; independent restored-color error bounds, alpha endpoints and strided neighborhood checks |
| 3 (complete) | `GetResidual`, plane masks, candidate predictor search and final residual generation with reconstruction feedback | 2 | 19,422 native comparisons and 30 portable fixtures: exact histograms, mode maps, sampling bits, scratch/context updates and residual images on active/bypass branches |
| 4 (complete) | Per-candidate preprocessing/palette/predictor routing, config propagation, buffer state and inherited stream integration | 1, 3 | 132 portable/live C fixtures: exact candidate transform data, stream selection and internal VP8L bytes; no changes at 100 |
| 5 (complete) | Public Rust wrapper, portable fixtures, native and full-Wasm regression coverage | 4 (complete) | 2,133 native public comparisons, 26 portable files, full-size real corpus, debug/release validation and 196 full-Wasm semantic regressions; decoded-reference RGBA agreement, valid errors, unchanged input and 100 identity |

Completed integration batch:

1. Extended the residual/search adapters to accept strength and subtract-green
   state, returning initialized reconstructed rows and difference data, while
   retaining the existing strength-100 requests and fixtures.
2. Threaded quantization through predictor search and final generation, testing
   reconstructed rows and right-edge context before stream integration
   (milestone 3), using the existing plane-mask guard.
3. Added the private setting, per-candidate routing and public wrapper
   (milestones 4–5), retaining all inherited strength-100 regressions and
   reusing milestone 1's preprocessing unchanged.
   Completed milestone 0's branch traces for evaluated and selected candidates.

CLI support now uses the configurable near-lossless entry point, preserving
quality 75/method 4 defaults and explicit effort/exact options. It validates
strength before file access and supports files, stdin/stdout and `--noalpha`.
CLI regression tests check byte equality with the configured library API,
100/lossless identity, active RGB/intermediate-alpha changes and failure output
preservation. See [LEVELS.md](LEVELS.md) for option meanings and interactions.

## Corpus and completion

Extend the shared deterministic synthetic corpus and use the lossless port's
recorded real-image corpus. Include:

- All 101 public strengths on small representative cases; focused larger cases
  at `0, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100`. Verify same-bucket byte
  identity as implied by the pinned mapping and routing, without assuming size
  or measured error is monotonic across buckets.
- `1×N`, `2×N`, `N×1`, `N×2`, height 3, and dimensions 63/64/65 in both
  orientations, including `63×63`, `64×3`, `3×64`, and odd sizes. Separately
  cover predictor tile boundaries and partial tiles; small images alone do not
  prove standalone quantization ran.
- Smooth ramps, constant channels, impulses, noisy patches and sharp edges;
  differences just below, at and above every smoothness/step threshold;
  rounding ties, values near 0/255, modulo residual boundaries, mode 0 and
  all image borders.
- Opaque, transparent, binary-alpha and intermediate-alpha images, hidden RGB,
  varying alpha with constant RGB, and alpha values near 0/255. Require endpoint
  preservation and reference agreement; include a recorded case demonstrating
  that lower strengths can change intermediate alpha.
- Verified palette bypass (including palette-plus-prediction), standalone
  preprocessing, prediction with/without subtract-green, and multiple transform
  candidates to expose source reuse, row overwrite and losing-trial state.
- Padded strides, unaligned slice starts, exact minimum input buffers, invalid
  strengths `-1`/`101`, invalid dimensions/strides, short buffers and checked
  overflow. Test dimension limits with skinny images and size limits through
  synthetic state rather than huge allocations.

Finish when both near-lossless algorithms and all reachable routing branches
match the pin, complete files match over the corpus and seeded comparisons,
independent decoding agrees with reference output, and 100 is byte-identical to
the inherited lossless API. Keep minimized mismatch replays, deterministic
seeds, portable fixtures, source/toolchain provenance, and the first differing
logical intermediate. Record maximum per-channel decoded deviations, sizes,
encode time and memory; quality measurements supplement exact parity.

Verified formatting, the workspace portable suite, the full opt-in native
suite, debug/release public validation, feedback checks, the Wasm regression and
a normal library build. Reproduce the full checks with:

```sh
cargo fmt --all -- --check
cargo test
cargo test -p libwebpop --features native-oracle,wasm-oracle -- --nocapture
cargo test -p libwebpop --release --features native-oracle,wasm-oracle \
  --test near_lossless_public --test near_lossless_feedback --test near_lossless_wasm
cargo build -p libwebpop --lib
# Optional full-size samples plus sizes/error/timing/process-memory record:
cargo test -p libwebpop --release --features native-oracle \
  --test near_lossless_public real_image_corpus_parity_and_metrics -- --ignored --nocapture
```

The manual corpus test uses the five recorded `samples/*.webp` images and a
binary-input adapter linked to the same pinned scalar library. It retains
prepared pixels, C outputs and `metrics.csv` under
`target/native-oracle/near-lossless-corpus/`. See the reference record for the
committed measurement snapshot and its limitations.

Deferred to the later lossy plan: `WebPEncodeRGBA`, independent alpha quality,
VP8/YUV/ALPH and their dependencies. Also deferred: the remaining upstream
advanced configuration, an exact-alpha near-lossless API, workers,
SIMD optimization, negative strides, C ABI compatibility, animation and muxing.
