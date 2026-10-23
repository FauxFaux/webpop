# libwebpop: near-lossless VP8L extension after lossless

## Target and order

The [lossless port](LOSSLESS_PORT_PLAN.md) is complete for its initial scalar
RGBA scope, including the public API, native oracle, independent decoding and
recorded size/timing baseline. Complete `WebPEncodeRGBAWithNearLossless` next,
before the [lossy VP8 port](LOSSY_PORT_PLAN.md). Standalone near-lossless
preprocessing is already implemented; this plan tracks the remaining extension.

Near-lossless encoding changes pixels within the existing VP8L pipeline and
returns the same still-image WebP container. Reuse the completed ARGB importer,
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
- **Milestone 0 is partial.** The copied custom C wrapper and predictor leaf
  adapters are implemented. Ten public fixtures at 0/100 and 1,035 live C
  encodings independently decode; strength 100 matches Rust's simple lossless
  API. Candidate branch traces and active residual/search adapters remain for
  integration. No tests require the sibling checkout.
- **Milestones 3–5 remain.** `GetResidual`, predictor search and final residual
  generation are specialized to strength 100. `GetPlaneDiffMask` already has
  the active-quantization guard, but search currently calls it with 1.
  `VP8LEncodeStream`/`EncodeStreamHook` have no near-lossless setting or routing.
  The public wrapper is not exported, and `cwebpop` rejects strengths below 100.

The next batch should connect the quantization leaves to predictor search and
final generation, including reconstruction feedback and branch traces.

## Public API and compatibility

Retain the wrapper's exported name:

```rust
pub fn WebPEncodeRGBAWithNearLossless(
    rgba: &[u8], width: u32, height: u32, stride: usize,
    near_lossless: i32,
) -> Result<Vec<u8>, WebPEncodingError>;
```

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
`(height - 1) * stride + width * 4`, with checked arithmetic. Return owned bytes
or an error; C output-slot and pointer ownership details are not Rust API
requirements. Input pixels must remain unmodified.

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

## Additional behavior to port

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

Extend the inherited predictor implementation rather than replacing its mode
search. The ported leaves are `MaxDiffBetweenPixels`, `MaxDiffAroundPixel`,
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
leaves. Extend its residual/search adapters to expose active quantization
(those calls are still specialized to 100), outside `upstream/`. Rename
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
exports this wrapper. When adding its durable regression, copy it separately
from the bundled lossless-only artifact and retain its provenance and SHA-256
`f6317d0cce217547b165a768f82647c2e923c489b5ab75b8dd0310722cffa477`.
Enable SIMD in the Wasm runtime and use the recorded initialization sequence.
Check the export at lower strengths and near-lossless-100/lossless identity;
the earlier inspection established only the latter for 12 pairs. Do not infer
lower-strength coverage or require old-Wasm/new-pin byte identity.

## Implementation order and acceptance gates

The lossless prerequisite is satisfied. Standalone preprocessing was completed
ahead of the full milestone-0 oracle work. Dependencies below are additional
near-lossless milestones; no lossy VP8 milestone is required.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 (partial) | Custom public wrapper and predictor leaf adapters complete; add candidate branch traces during integration | Completed lossless | Ten public fixtures and 1,035 independent C decodes pass; 100 equals simple lossless; candidate traces remain |
| 1 (complete) | Strength mapping, discretization/smoothness leaves, row buffering and complete `VP8ApplyNearLossless` | Standalone oracle adapters (complete) | 26,201 exact native comparisons and 49 portable fixtures; independent rounding/neighborhood checks, stride, dimension shortcuts, in-place updates and invalid inputs |
| 2 (complete) | Predictor neighborhood and component quantization leaves, modulo boundaries and subtract-green compensation | Predictor leaf adapters (complete) | 645,384 exact native comparisons and 120 portable fixtures; independent restored-color error bounds, alpha endpoints and strided neighborhood checks |
| 3 | `GetResidual`, plane masks, candidate predictor search and final residual generation with reconstruction feedback | 2 | Exact histograms, mode maps, sampling bits, scratch/context updates and residual images on active/bypass branches |
| 4 | Per-candidate preprocessing/palette/predictor routing, config propagation, buffer state and inherited stream integration | 1, 3 | Exact candidate transform data, stream selection and complete internal VP8L bytes; no changes at 100 |
| 5 | Public Rust wrapper, portable fixtures, native and full-Wasm regression coverage | 4 | Exact complete bytes against pin across corpus; decoded-reference RGBA agreement, valid errors, unchanged input and 100 identity |

For the next implementation batch:

1. Extend the residual/search adapters to accept strength and subtract-green
   state, returning initialized reconstructed rows and difference data. Retain
   the existing strength-100 requests and fixtures.
2. Thread quantization through predictor search and final generation, testing
   reconstructed rows and right-edge context before stream integration
   (milestone 3). Reuse the existing plane-mask guard.
3. Add the private setting, per-candidate routing and public wrapper
   (milestones 4–5), retaining all inherited strength-100 regressions. Reuse
   milestone 1's preprocessing unchanged unless integration reveals a mismatch.
   Complete milestone 0's branch traces for evaluated and selected candidates.

CLI support below 100 can follow the public wrapper by replacing the existing
explicit rejection with a call to it; it is not a prerequisite for the library
API. Keep the wrapper's fixed effort/exact settings explicit when deciding how
the CLI combines near-lossless with its existing options.

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

Run formatting, the relevant portable and opt-in native-oracle suites, public
validation in debug/release, the Wasm regression, and a normal library build
proving no production C/Wasm dependency. Preprocessing, predictor leaves and
the C public wrapper can already be verified with:

```sh
cargo test -p libwebpop --test near_lossless --test near_lossless_predictors --test near_lossless_public
cargo test -p libwebpop --features native-oracle --test near_lossless --test near_lossless_predictors --test near_lossless_public -- --nocapture
```

Add predictor feedback, stream and Rust public-API coverage as their milestones
land; the current suites do not establish near-lossless Rust encoder parity.

Deferred to the later lossy plan: `WebPEncodeRGBA`, independent alpha quality,
VP8/YUV/ALPH and their dependencies. Also deferred: public advanced config,
other compression qualities/methods, an exact-alpha near-lossless API, workers,
SIMD optimization, negative strides, C ABI compatibility, animation and muxing.
