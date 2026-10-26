# Encoder completion plan

For the initial 0.1.x release series, the crate-root `encode`, `OpImage`,
`OpConfig`, `OpQuality`, `OpAnimEncoder`, `OpAnimOptions`, `OpAnimPolicy`,
`OpAnimKeyframes`, `WebPEncodeRGBA`, `WebPEncodeLosslessRGBA`, and
`OpAnimError`/`WebPEncodingError` as `Error`/`Display` types carry compatibility guarantees.
The Rust API covers lossless and near-lossless fidelity, lossless effort/exact
handling, lossy color/alpha quality and SharpYUV, and methods 0–6. Custom
C-style wrappers, lower-level configurable encoders, timestamp-based animation APIs, and codec
helpers are exposed under `libwebpop::internal` and may change in patch
releases. References below to implemented or public APIs describe availability,
not additional stability guarantees.

Reclassified 2026-10-25 at repository revision `6fe2ce7`, against pinned libwebp
`1844042d1ca751e62e61df7617dba19c98588339` in [`upstream/`](../upstream).

The three port plans are complete for their stated configurations. This plan
separates remaining **core encoder behavior**, **utilities around the encoder**,
and **performance work**. These have separate completion gates. This inventory
compares with the pin, rather than a newer libwebp release.

Language bindings and C ABI compatibility are excluded. The library excludes compressed-image
decoding, image-file loaders, metadata extraction from input files, demuxing,
and importing existing WebP containers for editing. Raw pixel layouts and
RGB-to-YUV conversion remain in scope. Metadata writing and animation encoding
from prepared frames belong to the utility/output track.
The cwebpop CLI reuses external image loaders, including animated GIF import.

## Completion boundary

- **Core:** algorithms and settings that can change the coded image stream for
  the same source image: conversion, analysis, quantization, mode/transform
  selection, entropy modeling, filtering parameters and syntax. The effect
  may be confined to bytes; decoded pixels need not change. Internal trials and
  refinement passes count when they affect those coding decisions.
- **Utility:** configuration recipes, selection of quality to meet a target,
  input adapters and picture operations, observation/reporting, output APIs,
  container assembly and animation orchestration. A utility can change the
  eventual file by changing the input, settings or selected candidate without
  adding a core coding behavior. Presets need no encoding trial at all.
- **Performance only:** scheduling, SIMD, allocation and equivalent faster
  implementations. A change only to speed or memory does not count toward core
  completion. Classify the actual branch, rather than the option's name:
  `low_memory` also changes coding behavior and therefore has a core component.

This is a functional boundary, not a requirement that utilities live outside
the crate or that the core execute only one traversal. In particular,
`emulate_jpeg_size` and entropy-analysis `pass` are not merely wrappers that
re-encode at different public quality settings. Their pinned implementations
change the core, as traced below. Target-size/PSNR selection is utility policy,
although upstream integrates its search into the core's pass loops.

## Completed baseline

Preserve the behavior and parity coverage established by the
[lossless](LOSSLESS_PORT_PLAN.md), [near-lossless](NEAR_LOSSLESS_PORT_PLAN.md),
and [lossy](LOSSY_PORT_PLAN.md) plans:

| Public encoding route | Implemented scope |
| --- | --- |
| `WebPEncodeLosslessRGBA`, `encode_lossless_rgba` | Checked positive-stride RGBA; complete VP8L files; quality 0–100, methods 0–6, exact transparent RGB option |
| `WebPEncodeRGBAWithNearLossless`, `encode_near_lossless_rgba` | Strength 0–100 with standalone preprocessing and predictor quantization; configurable effort and exact handling |
| `WebPEncodeRGBA`, `WebPEncodeRGBAWithAlphaQuality` | Default-preset lossy VP8, method 4, fractional color quality 0–100; YUV420(A) import; exact or independently quantized alpha; complete still-image files |
| `encode_lossy_rgba` | Default-preset lossy methods 0–6 with `LossyOptions` selecting method, color/alpha quality and ordinary or scalar SharpYUV conversion; method 4 defaults and output-buffer guarantees retained |

Palette ordering, all lossless transforms, reference/cache trials, default
lossy segmentation/SNS, Intra4 search, token probability refresh, chroma error
diffusion, deterministic filter adjustment, and partition-0 budget retries
are already implemented. Separate ALPH compression, predictive alpha filters,
and raw-alpha fallback are also implemented. Their configurable variants must
not be mistaken for wholly missing subsystems.

The [additional lossy methods plan](LOSSY_METHODS_PLAN.md) is complete as of
2026-10-25: fast analysis/non-RD decisions, direct statistics/coding and fitted
skip syntax for methods 0–2, basic RD for methods 3/4, selected-mode trellis for
5, candidate trellis for 6, and method-dependent alpha effort/filter trials all
have pinned-C parity. Library and CLI expose methods 0–6. A
[42-case paired scalar baseline](benchmarks/2026-10-25-lossy-methods.md) records
larger-image bytes, quality, repeated time and whole-process RSS.

The older plans' deferred lists are historical: lossy VP8, ordinary YUV import,
near-lossless, and ALPH are no longer outstanding. No additional default-path
correctness defect was established by this source/documentation audit.

## Remaining core encoder features

The API/configuration reference is
[`encode.h`](../upstream/src/webp/encode.h), with defaults and validation in
[`config_enc.c`](../upstream/src/enc/config_enc.c). Rust currently exposes
`LosslessOptions { quality, method, exact }` and
`LossyOptions { quality, method, alpha_quality, use_sharp_yuv }` for methods 0–6.
Some public low-level modules expose
more knobs, but they do not provide a checked, integrated advanced encoder.

| Core behavior | Bitstream effect, current implementation and remaining work | Main sources |
| --- | --- | --- |
| Checked core configuration and routing | Expose the core settings below through a validated, integrated complete-file encoder. Keep convenience APIs/defaults. Internal `WebPEncode` is not a general public equivalent of upstream's config/picture API. | Rust [`config_enc.rs`](../libwebpop/src/enc/config_enc.rs), [`webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs), [`encode.rs`](../libwebpop/src/encode.rs) |
| Direct coding for methods 3–6 (`low_memory`) | Switches from buffered tokens to `VP8EncLoop`, with different probability/skip handling and potentially different bytes/distortion even with one partition. Direct coding is complete for methods 0–2; add checked routing and integration for 3–6. Memory savings alone do not count. | Upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c) `MapConfigToTools`, [`frame_enc.c`](../upstream/src/enc/frame_enc.c); Rust [`frame_enc.rs`](../libwebpop/src/enc/frame_enc.rs) |
| Multiple coefficient partitions (`partitions`) | Changes syntax and coefficient placement, even when pixels are identical. Only one is emitted. Add 2/4/8 partitions, size fields, row distribution and overflow checks. Buffered tokens force one partition; methods 3–6 require the direct route above to honor multiple partitions. | Upstream [`iterator_enc.c`](../upstream/src/enc/iterator_enc.c) `VP8IteratorSetRow`, [`syntax_enc.c`](../upstream/src/enc/syntax_enc.c) `GeneratePartition0` / `EmitPartitionsSize` |
| Entropy-analysis passes (`pass` 1–10) | Additional passes at fixed quality retain/refine token statistics, probabilities and coding costs; they can change modes/residuals and bytes. Port both direct and buffered loops with correct resets. Existing automatic partition-0 retries are separate. Target-driven quality updates belong to utilities below. | Upstream [`frame_enc.c`](../upstream/src/enc/frame_enc.c) `StatLoop` / `VP8EncTokenLoop` |
| Segmentation (`segments` 1–4) and SNS (`sns_strength` 0–100) | Analysis assigns macroblocks to segments; SNS changes segment quantizers, chroma deltas and method-dependent RD/trellis costs. These affect coded coefficients, segment maps and headers. Generalize the default analysis/setup paths and expose their knobs; some Rust helpers already accept parameters. Segments are not coefficient partitions. | Upstream [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c) `AssignSegments`, [`quant_enc.c`](../upstream/src/enc/quant_enc.c) `VP8SetSegmentParams` / `SetupMatrices`; Rust [`analysis_enc.rs`](../libwebpop/src/enc/analysis_enc.rs), [`quant_enc.rs`](../libwebpop/src/enc/quant_enc.rs) |
| Header budget (`partition_limit` 0–100) | Limits Intra4 header bits during mode selection, changing the Intra4/Intra16 decisions and stream. It does not set an overall target size or the number of partitions. Expose the configurable limit; default partition-0 budget retries already exist. | Upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c) `MapConfigToTools`, [`quant_enc.c`](../upstream/src/enc/quant_enc.c) `PickBestIntra4` |
| VP8 deblocking controls and `autofilter` | Select simple/strong filtering, sharpness and per-segment strengths signaled to the decoder. Deterministic adjustment exists; integrate public controls plus SSIM trials/automatic strength selection. This is independent of alpha prediction filters. | Rust [`filter_enc.rs`](../libwebpop/src/enc/filter_enc.rs); upstream [`filter_enc.c`](../upstream/src/enc/filter_enc.c), [`quant_enc.c`](../upstream/src/enc/quant_enc.c) `SetupFilterStrength`, [`syntax_enc.c`](../upstream/src/enc/syntax_enc.c) `PutFilterHeader` |
| Conversion and segment-map preprocessing | Add `preprocessing & 2` pseudo-random conversion dithering and `preprocessing & 1` segment-map smoothing. Scalar SharpYUV (`use_sharp_yuv` or bit 4) and its encoder/CLI routing are complete. These change samples or coding assignments, rather than execution speed alone. See timing/routing below and the [SharpYUV plan](SHARPYUV_PORT_PLAN.md). | Rust [`sharpyuv/`](../libwebpop/src/sharpyuv/), [`picture_csp_enc.rs`](../libwebpop/src/enc/picture_csp_enc.rs); upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c), [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c) `SmoothSegmentMap` |
| JPEG-like quantizer mapping (`emulate_jpeg_size`) | Uses image complexity and quality to replace the base quality-to-compression curve before segment quantizers are computed. Add the alternative mapping and its analysis dependency. There is no JPEG encode/size measurement or outer quality search. | Upstream [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c) `VP8EncAnalyze`, [`quant_enc.c`](../upstream/src/enc/quant_enc.c) `QualityToJPEGCompression` / `VP8SetSegmentParams` |
| Configurable alpha compression/filter | `alpha_compression` 0/1 changes raw/compressed ALPH representation; predictive filtering changes ALPH header/residuals and the chosen stream. Low-level Rust `EncodeAlpha` already supports compression and none/fast/best trials; public lossy encoding fixes compression=1/filter=fast. Method-dependent alpha effort is complete for 0–6. | Rust [`alpha_enc.rs`](../libwebpop/src/enc/alpha_enc.rs); upstream [`alpha_enc.c`](../upstream/src/enc/alpha_enc.c) `ApplyFiltersAndEncode` |
| Lossy transparent-area policy (`exact`) | Cleanup changes the YUVA input to coding and therefore can change bytes. Private configuration already gates cleanup for animation, but public still options and the CLI do not expose lossy `exact`. Connect that policy; lossy `exact` does not make RGB lossless. | Rust [`config_enc.rs`](../libwebpop/src/enc/config_enc.rs), [`webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs); upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c) |

`use_delta_palette` is reserved in the pinned public header and is not an
implemented encoder feature to port. Near-lossless below strength 100 can
change intermediate alpha by design; an exact-alpha near-lossless option would
be a separate extension, not a missing upstream capability.

### When conversion and filtering run

The pinned lossy path in [`WebPEncode`](../upstream/src/enc/webp_enc.c) is:

1. **Before VP8 analysis:** convert ARGB/RGBA to YUV420(A), using ordinary
   conversion, conversion dithering, or SharpYUV. SharpYUV iteratively improves
   the conversion itself; it does not rerun VP8 or tune the encoder quality.
   It changes the Y/U/V samples and can change the stream, so it counts as a
   core input-conversion feature even though it is a separate module. It is
   bypassed for supplied YUV, takes precedence over dithering, and falls back
   to ordinary conversion when either dimension is below 4. Transparent-area
   cleanup follows conversion unless `exact` is set.
2. **During analysis:** choose segments, optionally smooth the segment map
   (`preprocessing & 1`), and collect complexity for quantization/SNS. Segment
   smoothing changes assignments, not source pixels. Each coding pass then
   sets quantizers, SNS-dependent costs and initial filter strengths.
3. **During color encoding:** reconstruct unfiltered macroblocks for prediction
   and residual coding. With `autofilter`, `VP8StoreFilterStats` tries deblocking
   strengths on scratch reconstructions and compares SSIM. The trials cover
   internal subblock edges, not a complete filtered frame, and do not feed
   filtered pixels back into prediction. The buffered path collects these
   statistics only on its final pass.
4. **After the coding loop, before headers are written:**
   `VP8AdjustFilterStrength` chooses final strengths (SSIM-based with autofilter,
   deterministic otherwise). Segment/filter headers and the VP8 profile carry
   the resulting decoder instructions. The decoder applies deblocking to the
   reconstructed picture. `show_compressed` exports the unfiltered encoder
   reconstruction; it does not run that decoder filter.

Separately, **alpha prediction filtering** runs on the alpha plane before VP8L
compression within the ALPH job. Fast/best choose prediction candidates, encode
them and select the result, with raw fallback. These are internal coding trials
with different residuals/header bits, so they count as core. The alpha job can
run alongside color coding; concurrency itself is performance work. Sources:
[`frame_enc.c`](../upstream/src/enc/frame_enc.c),
[`filter_enc.c`](../upstream/src/enc/filter_enc.c),
[`alpha_enc.c`](../upstream/src/enc/alpha_enc.c), and
[`picture_csp_enc.c`](../upstream/src/enc/picture_csp_enc.c).

## Encoder utilities

### Configuration recipes and target selection

| Utility | Remaining work and boundary with the core | Main sources |
| --- | --- | --- |
| Picture/photo/drawing/icon/text presets | Initialize ordinary core settings; presets neither encode nor observe output. Add full default initialization, preset mappings and CLI ordering. The photo recipe enables conversion dithering, so preset parity depends on that core feature. | Upstream [`config_enc.c`](../upstream/src/enc/config_enc.c) `WebPConfigInitInternal` |
| Lossless preset levels (`-z` 0–9) | Map levels to existing method/quality settings and lossless mode; preserve subsequent option overrides. No new VP8L compression algorithm. | Upstream [`config_enc.c`](../upstream/src/enc/config_enc.c) `WebPConfigLosslessPreset`, [`cwebp.c`](../upstream/examples/cwebp.c) |
| Target size / PSNR (`target_size`, `target_PSNR`) | Port quality updates, estimates, convergence and target precedence. This is selection policy over the core's quality parameter. Upstream integrates partial/statistics passes into both coding loops, rather than independently re-encoding complete files; fresh public encode calls need not reproduce its bytes. Keep any stateful pass integration in the core while exposing target policy separately. | Upstream [`frame_enc.c`](../upstream/src/enc/frame_enc.c) `InitPassStats` / `ComputeNextQ` / `OneStatPass` / `VP8EncTokenLoop` |
| Quality bounds (`qmin`, `qmax`) | Normalize initial quality and bound target-driven updates. These introduce no new coding algorithm: `InitPassStats` clamps quality even when target search is disabled. Implement that single-pass clamp as well as search bounds; default 0/100 is already equivalent. | Upstream [`frame_enc.c`](../upstream/src/enc/frame_enc.c) `InitPassStats` / `ComputeNextQ` |

`pass` has two roles: the core entropy-analysis count and the utility search's
iteration budget. Without targets, the pinned loops repeat at fixed quality
and carry coding statistics forward; separate fresh encodes at the same quality
would discard that state. Also, `InitVP8Encoder` enables chroma error-diffusion
storage when `pass > 1`, even above its normal quality cutoff of 98; this can
change coding independently of target search. With targets, `do_search` enables
quality updates and, on the direct path, changes probe/RD behavior. A utility
implementation must preserve these dependencies to claim pinned search parity.

The bounds clamp the pass's working quality, not the original configuration
used earlier for conversion dithering and fast analysis. Rewriting the global
quality before the whole pipeline is not generally equivalent. Sources:
[`webp_enc.c`](../upstream/src/enc/webp_enc.c) `InitVP8Encoder` / `WebPEncode`,
[`analysis_enc.c`](../upstream/src/enc/analysis_enc.c) `FastMBAnalyze`, and
[`frame_enc.c`](../upstream/src/enc/frame_enc.c) `InitPassStats`.

At this pin, nonzero `target_size` wins over `target_PSNR` in `InitPassStats`,
despite the opposite precedence stated in `encode.h`. The size is an estimate,
not a guaranteed complete-file byte budget, and the PSNR uses coding-loop
distortion, not an external decode with deblocking and RGB comparison. The CLI
changes `pass == 1` to 6 when a target is present; the library does not. A
standalone `pass > 1` does not enable a 40 dB target search: that fallback value
is initialized, but `do_search` remains false. Search is lossy-only.

The official [cwebp manual](https://developers.google.com/speed/webp/docs/cwebp)
was also consulted. Its standalone-pass/40 dB and segment-option descriptions
do not match these pinned branches: `segments` is used in buffered encoding
too; it is coefficient `partitions` that buffered encoding forces to one.
Use the pinned implementation for parity and fixtures.

### Raw input, picture operations, output and observation

These are utility/API work, independent of core completion. Raw layout adapters
must preserve logical samples; RGB/YUV conversion algorithms remain core as
described above. Use safe Rust ownership and borrowing rather than reproducing
C allocation/free entry points or ABI structures.

- [ ] Add checked RGB, BGR and BGRA convenience routes, plus RGBX/BGRX import
  equivalents. Current top-level APIs accept only RGBA. Define signed-stride
  or bottom-up input using an explicit starting offset and checked row access.
- [ ] Accept caller-provided packed ARGB and planar YUV420(A), with validated
  plane lengths/strides and appropriate owned or borrowed picture views.
  `WebPYUVAPicture` and low-level YUVA helpers exist; complete-file encoding
  from these inputs is not exposed. Add the encoder-side conversion routes
  needed when the supplied representation differs from the selected codec.
- [ ] Add crop/view and rescale helpers, including alpha-aware resampling,
  aspect-preserving zero dimensions and upstream YUV crop alignment. Add alpha
  blending against a background. Discarding alpha with `--noalpha` already works.
- [ ] Add an output writer interface with write-error propagation, plus progress
  reporting and caller cancellation. Current APIs fill a caller-owned `&mut Vec<u8>`
  with a complete WebP file and return `Result<(), _>`; the CLI writes the
  buffer after encoding. A writer callback does not promise
  constant-memory or one-pass streaming: codec trials still need buffering.
- [ ] Return auxiliary statistics and lossy macroblock information maps. Some
  lower-level lossless stream statistics and test observer events already exist;
  they are not a complete public `WebPAuxStats`/`extra_info` equivalent.
- [ ] Support `show_compressed` reconstructed-picture output and the associated
  CLI dump. Add encoder-side `WebPPlaneDistortion`/`WebPPictureDistortion`
  equivalents for PSNR, SSIM and LSIM. Independent test decodes and metrics do
  not provide these production utilities. Upstream `show_compressed` exports
  reconstruction without applying the in-loop filter.

References: upstream [`encode.h`](../upstream/src/webp/encode.h),
[`picture_enc.c`](../upstream/src/enc/picture_enc.c),
[`picture_csp_enc.c`](../upstream/src/enc/picture_csp_enc.c),
[`picture_rescale_enc.c`](../upstream/src/enc/picture_rescale_enc.c),
[`picture_tools_enc.c`](../upstream/src/enc/picture_tools_enc.c), and
[`picture_psnr_enc.c`](../upstream/src/enc/picture_psnr_enc.c).

## Utility track: container and animation output

Container chunks, frame selection, timing, disposal/blending and animation
optimization change the output file, but compose the still-image core. Track
them independently; they do not gate still-image core completion. Any new
still-image coding behavior they require belongs to the core table above.

The [animation and multi-frame plan](ANIMATION_PORT_PLAN.md) expands this work
into output-layer, raw-frame, optimizer and CLI milestones. Basic full-frame
animation can ship using the current codecs before the advanced still-image
work is complete; optimized lossy blending uses a separate filter-control integration.
Isolated animation pixel comparisons, rectangle refinement/even snapping and
blend-eligibility leaves now have portable fixtures and pinned C comparisons
under `src/mux/`. Checked prepared-frame container assembly now matches pinned
`WebPMux`. The stateful raw-frame sequence API now supports full-canvas
replacement encoding with explicit timestamps; the optimizer remains open.
Checked borrowed subframe views and `GetSubRect`/`GetSubRects` now match 47
committed fixtures and 17,340 live scalar C cases. Isolated lossless candidate
encoding/selection now has 79 fixtures and 3,120 live C comparisons, including
winning carryover masks and preceding disposal, with 48 exact all-frame
decoder checks. Lossless carryover canvas updates now match 32 committed
fixtures and 768 live helper/sequence cases, including 5,376 frame transitions
and 15 exact all-frame decoder checks. The low-level lossless ARGB cache now
merges durations and inserts overflow holds with keyframes disabled: 32 cache
fixtures and 1,680 live cases cover 13,440 submitted inputs, including 1,470
complete public animation files, 210 cases before single-frame reconstruction
and 29 timed all-frame decoder checks. Near-lossless VP8L candidates and the same
cache now support strength 0–100, with 64 fixtures, 4,893 live C cases (3,773
candidates and 1,120 cached sequences) and 24 exact all-frame decoder checks.
Per-rectangle codec routing follows transparency substitution and retains the
near-lossless RGB/intermediate-alpha caveat. Isolated lossy EncodeCandidate now
disables filtering and selects VP8 profile 2 for blending: 56 fixtures and 4,704
live original-C comparisons cover methods 0–6, alpha quality and SharpYUV, with
independent header parsing and still decoding. Default still output is unchanged.
Lossy SetFrame, globally aligned FlattenSimilarBlocks and original-word carryover
updates now match 73 fixtures and 5,088 live cases: 1,392 flatten operations,
2,772 selections and 924 eight-frame sequences (7,392 transitions). Eighteen
assembled unit-duration sequences match independent compositing and the pinned
all-frame decoder exactly. Mixed selection/config reversal now matches 75
fixtures and 4,237 live cases: 9,008 complete trial files, 52 distinct-stream
size ties and 2,688 carryover transitions. Ten assembled mixed sequences match
independent compositing and pinned all-frame decoding exactly. Private VP8 exact
cleanup follows the shared configuration; still defaults remain unchanged.
Lossy/mixed timestamp caching, keyframe controls, single-frame reconstruction
and the RGBA Optimized policy remain open.

- [x] Write caller-supplied ICC, EXIF and XMP chunks, with correct VP8X flags,
  chunk sizes/padding and RIFF limits. `AnimationContainer` accepts prepared
  VP8L or VP8/ALPH frames and supplied metadata; a single full-canvas frame
  produces a still image with metadata. Existing still APIs are unchanged.
  Copying metadata out of input files is excluded from this plan.
- [x] Assemble animation containers from prepared encoded frames: canvas,
  background/loop settings, explicit durations, frame rectangles, blending,
  disposal, ANIM/ANMF chunks and validation. There are 33 committed mux fixtures
  and 193 live complete-byte comparisons to pinned `WebPMux`, plus independent
  field/padding/size checks. Timestamp handling belongs to the sequence API.
- [x] Encode raw full-canvas RGBA sequences with explicit final timing, preserving
  repeats and zero durations. `AnimationEncoder` supports per-frame lossless,
  near-lossless and lossy methods 0–6 plus supplied metadata. There are
  26 committed/live pinned WebPMux sequence comparisons and independent
  all-frame canvas/timing decodes. The explicit full-frame policy does not
  claim optimized `WebPAnimEncoder` parity; the CLI remains open.
- [ ] Port animation encoding from raw frames (`WebPAnimEncoder`): changed-frame
  rectangles, keyframe/subframe selection, frame caching, minimize-size mode,
  keyframe interval controls, mixed lossy/lossless trials and final assembly.
  Merely concatenating independently encoded still images is insufficient.
  Include caller-supplied metadata support on animation output.

The reference implementation is in [`mux.h`](../upstream/src/webp/mux.h),
[`mux_types.h`](../upstream/src/webp/mux_types.h), and
[`src/mux/`](../upstream/src/mux/), especially `anim_encode.c`, `muxedit.c` and
`muxinternal.c`. Only their output/encoding responsibilities are included;
container reading, extraction and editing existing files are excluded.

Full `WebPAnimEncoder` parity has an additional dependency: its single-frame
subrectangle optimization decodes and re-encodes a candidate. The animation
plan keeps this branch as an explicit completion gate against the production
decoding exclusion above. Ordinary mux cleanup of a full-canvas single frame
does not need decoding.

The animation plan's shared VP8L/VP8/mixed timestamp cache and overflow holds
(4a) are complete; the established lossless cache adapts to the shared owner.
Keyframe normalization/storage (4b), deferred selection (4c) and keyframes with
duration overflow (4d) are complete. A separate low-level constructor enables
keyframes with the shared timing/cache owner. The RGBA facade now exposes the
checked optimized policy (4e), retaining its existing unsupported `internal`
namespace and pre-reconstruction comparison boundary. Sequence CLI, resource
evidence and single-frame reconstruction remain open. The plan defines acceptance
cases before implementation and focused comparisons before broad validation.
Production decoding remains excluded;
discussion must precede any decision to change that boundary. The full-frame
sequence CLI and initial resource baseline can proceed independently.

## CLI integration by track

[`cwebpop/src/main.rs`](../cwebpop/src/main.rs) declares many upstream-style
options but deliberately rejects unsupported ones before opening files.
Complete the underlying features above, then wire and test these options:

| Track | Area | Currently rejected options |
| --- | --- | --- |
| Core | Lossy analysis and deblocking | `--segments`, `--sns`, `-f`, `--sharpness`, `--strong`, `--nostrong`, `--af`, `--partition_limit` |
| Core | Entropy passes, conversion and quantization | `--pass` (also a search budget), `--pre` values other than 0/4, `--jpeg_like` |
| Core | Alpha coding and direct coding | `--alpha_method`, `--alpha_filter`, `--low_memory` |
| Utility | Configuration recipes and target selection | `-z`, `--preset`, `--size`, `--psnr`, `--qrange` |
| Utility | Input and picture operations | Raw YUV `-s`, `--crop`, `--resize`, `--resize_mode`, `--blend_alpha`, legacy `--alpha_cleanup` |
| Utility | Reports and reconstruction | `--progress`, `--map`, `--print_psnr`, `--print_ssim`, `--print_lsim`, `-d` |
| Utility/output | Metadata output | `--metadata` currently has no implementation; writing supplied metadata is in scope, input-file extraction/copying is excluded |
| Performance/compatibility | Scheduling and allocation hints | `--mt`, `--hint`; neither adds a core coding algorithm at this pin |

Permit lossy `--exact` when the public still configuration exposes it.
`--lossy -m` already supports 0–6.
`--alpha_q`, `--sharp_yuv` and `--pre 0/4` already work for lossy encoding.
`--noasm` is accepted and is
currently a no-op because there is no explicit accelerated backend. `--short`
currently prints the byte count; upstream's richer statistics depend on the
reporting work above. Animation now has a full-frame library API and still
needs a sequence frontend; it is not a missing mode of upstream `cwebp` itself.

## Performance and compatibility: outside core completion

None of this section gates functional core completion. Functional completion
does not establish performance parity. Measurements below are dated observations
for particular inputs/builds, not acceptance thresholds or claims about every
configuration.

- `thread_level` schedules VP8L configuration trials, lossy analysis jobs and
  alpha/color coding concurrently. Porting workers adds no new coding tool.
  Preserve candidate order, size ties, results and error propagation when
  implementing it. The [threading plan](THREADING_PLAN.md) inventories these
  boundaries and proposes Rayon scheduling with exact-byte acceptance gates.
  Sources: [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c)
  `VP8LEncodeStream`, [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c)
  `VP8EncAnalyze`, [`alpha_enc.c`](../upstream/src/enc/alpha_enc.c).
- `image_hint` is an allocation/compatibility option here: `WEBP_HINT_GRAPH`
  changes initial VP8L writer capacity; the other hints add no compression
  branch. It does not belong in a list of missing coding algorithms. Source:
  [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c) `VP8LEncodeImage`.
- `low_memory` has a separate core obligation above because it changes the
  coding path. Measuring or improving that path's memory/CPU usage belongs here.

| Gap or evidence | Recorded result and implication |
| --- | --- |
| Lossless speed versus normal SIMD-enabled C | The eight-case [2026-10-23 benchmark](LOSSLESS-BENCHMARKS.md) found method-4 Rust about 32% slower on the map, 13% on the photo, 14% on the screenshot and 3% on the alpha image. Method-0 photo was effectively tied. The 81.37 MP invitation follow-up was about 6% slower. Outputs were byte-identical. |
| Concrete scalar predictor overhead | Map profiling attributed about 82% of excess estimated cycles to the predictor transform. Current [`PredictPixel`](../libwebpop/src/enc/predictor_enc.rs) builds a three-word context and dispatches through the predictor table per interior pixel; checks/context copies/indirect calls remain optimization targets. |
| Missing explicit SIMD and dispatch | Rust has no explicit SIMD backend. Disabling C runtime SIMD narrowed the map gap but left Rust about 17.2% slower in that session; it closed about 38% of the absolute time gap. Missing SIMD alone does not explain the remaining difference. `-noasm` still permits compiler auto-vectorization and SIMD in libc. |
| Prototype improvements remain unmerged | [Palette investigation](PALETTE-INVESTIGATION.md) retained patches using borrowed context and per-span predictor specialization. Separate paired sessions measured about 2% and then 6.8% improvements on the map. Predictor 10's copied context caused measured store-forwarding stalls. These are temporary-source experiments, not current production optimizations or demonstrated corpus-wide wins. Their baseline also used experimental `#[inline(always)]`, which is not the repository's inline convention. |
| Extra histogram clearing | Current predictor search clears all completed histogram planes/modes; upstream selectively clears active entries. This is a source-confirmed difference with equivalent results; the speed benefit of changing it is unmeasured. Palette mapping/sorting and traceback were smaller secondary gaps in the recorded profile. |
| Synchronous execution and buffered memory | Encoder workers and public low-memory selection remain missing. Direct VP8 coding is complete for methods 0–2; methods 3–6 use buffered tokens. Existing records do not isolate the gain or memory savings of adding low-memory routing to those methods. |
| Paired scalar lossy baseline | The [2026-10-25 record](benchmarks/2026-10-25-lossy-methods.md) measures 42 exact-byte Rust/C pairs across all methods, two image sizes and opaque/alpha inputs, with repeated encode time, quality and whole-process RSS. Rust uses the session's native CPU target; C uses scalar Release dispatch. This establishes no global speed ranking, SIMD-enabled-C comparison or isolated encoder-memory gap. |
| Near-lossless comparison missing | The [2026-10-23 near-lossless record](benchmarks/2026-10-23-near-lossless.md) has exact C parity across 30 real-image cases, but its Rust timings are single exploratory samples and RSS is cumulative test-process memory. It establishes no comparative speed or encoder-only memory gap. |

The current source still has copied predictor context and whole-range histogram
clearing; the archived experimental patches have not been applied. Compiler
flag changes alone did not close the recorded lossless gaps: native targeting
gave roughly 0–2% versus x86-64-v3, while ThinLTO plus one codegen unit generally
regressed 1–4% in that session. Do not turn either result into a global flag
recommendation without new measurements.

Performance work remaining:

- [x] Establish a paired, repeated scalar lossy Rust/C baseline across all
  methods, larger inputs and alpha, with matching settings, bytes and metrics.
- [ ] Establish paired, repeated native Rust/C benchmarks for near-lossless,
  and extend lossy comparisons to ordinary SIMD-enabled C. Match configurations,
  compiler targets and thread policies; report encoded bytes and verify parity.
- [ ] Measure isolated encoder peak memory/allocation behavior for each mode
  and large-image cases. Existing whole-process RSS does not establish a Rust/C
  memory regression. Profile allocations/copies before attributing a slowdown
  to them; owned buffers alone are not evidence of a defect.
- [ ] Evaluate borrowed predictor context, selection outside pixel loops,
  structurally reduced bounds checks, selective histogram reset, and a
  no-transparent-pixels fast path. Preserve transformed-pixel alpha semantics,
  edge handling, exact paths and near-lossless reconstruction dependencies.
  Reprofile after each retained change; the old profile cannot locate a new
  bottleneck.
- [ ] Implement measured SIMD kernels and appropriate feature selection for
  supported targets, retaining scalar fallbacks. Cover lossless predictors/
  transforms, lossy transforms/quantization, color conversion and alpha kernels
  as profiling warrants. [`INTRINSICS.md`](INTRINSICS.md) is an assessment, not
  an implemented backend; keep safe Rust and exact integer semantics. Check
  native and Wasm targets, including scalar/SIMD Wasm artifact selection.
- [ ] Measure threading throughput and peak-memory tradeoffs, and low-memory
  mode's CPU/memory tradeoff, once those features exist.

## Implementation order and separate completion checks

The completed [additional lossy methods plan](LOSSY_METHODS_PLAN.md) records
methods 0–6, their direct-loop/trellis/alpha dependencies, checked API/CLI
integration and release baseline. Prioritize the remaining core independently
of utility and performance work:

1. **Core configuration:** validate and integrate existing alpha controls,
   lossy exact policy, configurable segment/SNS/header-budget and manual filter
   settings through the complete-file APIs. Preserve convenience defaults.
2. **Core coding paths:** extend direct coding to methods 3–6, then multiple
   coefficient partitions and fixed-quality entropy passes. Cover direct and
   buffered statistics/reset behavior; these are prerequisites for faithful
   utility target search, not reasons to bundle that policy into this milestone.
3. **Core algorithms:** add automatic filter selection, segment-map smoothing,
   conversion dithering and JPEG-like quantizer mapping. SharpYUV is already
   complete; verify its routing with the new options and prepared YUV inputs.
4. **Utilities:** build presets/level recipes once their core settings work;
   add bounded target search using the established pass machinery. Input
   adapters, writers, progress/cancellation, statistics, reconstruction,
   metrics, picture operations and their CLI options can proceed independently.
   Follow the [animation plan](ANIMATION_PORT_PLAN.md) for the output/sequence
   work and its separate production-decoding boundary.
5. **Performance/compatibility:** carry profiling, scalar improvements, SIMD,
   workers and allocation hints as a separate backlog. Benchmarks inform that
   work and do not delay the functional core gate.

For each feature, extend the existing portable fixtures, independent checks
and pinned native oracle rather than introducing another reference stack.
Compare intermediates at new algorithm branches and complete files for matching
configurations. Include a deterministic case where each claimed core setting
changes bytes or parsed syntax, alongside cases where the setting is inactive
or quantizes to the same result. Check decoded pixels separately: partitions
and entropy choices can change bytes without changing reconstruction. Retain
the pin and fixture provenance. Carry upstream `WEBP_INLINE` as plain `#[inline]`.

- **Core complete:** every remaining core-table behavior has a checked,
  integrated Rust interface, matching complete-file output against the pin and
  independent validation. Cover segments 1–4, all coefficient partition counts
  on supported routes, fixed-quality passes without targets, direct/buffered
  coding, manual/automatic filtering, preprocessing precedence and supplied-YUV
  bypass. Check invalid values, buffer/geometry limits and partition overflow.
  Utility wrappers and performance work are not prerequisites.
- **Utilities complete:** the selected utility/API/CLI and animation milestones
  have their own parity and failure checks. Verify preset equivalence to explicit
  settings, CLI ordering, target precedence/convergence, single-pass quality
  clamping, cancellation, write failures and RIFF limits. Verify that observation
  features leave encoded bytes unchanged. Claim pinned target-search parity
  only with its internal pass state and distortion/size metrics preserved.
- **Performance work:** report paired speed/memory evidence with matching
  settings and bytes. Workers, SIMD and allocation-only hints do not earn
  additional core feature credit. Preserve bitstream behavior when optimizing.

Run relevant tests, the full portable and native-oracle suites, formatting and
a production library build as code features land. This reclassification changes
documentation only; no encoder implementation or fixtures are changed.
