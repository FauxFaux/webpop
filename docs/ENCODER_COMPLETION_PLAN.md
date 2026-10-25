# Encoder completion plan

Audited 2026-10-24 at repository revision `937e2d9`, against pinned libwebp
`1844042d1ca751e62e61df7617dba19c98588339` in [`upstream/`](../upstream).

The three port plans are complete for their stated configurations. Remaining
encoder work is principally the advanced still-image API and its additional
algorithms, encoder utilities, animation/container output, and performance.
This inventory compares with the pin, rather than a newer libwebp release.

Language bindings and C ABI compatibility are excluded. So are compressed-image
decoding, image-file loaders, metadata extraction from input files, demuxing,
and importing existing WebP containers for editing. Raw pixel layouts and
RGB-to-YUV conversion remain encoder features. Metadata writing and animation
encoding from prepared frames remain in scope.

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

## Remaining still-image features

The API/configuration reference is
[`encode.h`](../upstream/src/webp/encode.h), with defaults and validation in
[`config_enc.c`](../upstream/src/enc/config_enc.c). Rust currently exposes
`LosslessOptions { quality, method, exact }` and
`LossyOptions { quality, method, alpha_quality, use_sharp_yuv }` for methods 0–6.
Some public low-level modules expose
more knobs, but they do not provide a checked, integrated advanced encoder.

| Gap | Current implementation and remaining work | Main sources |
| --- | --- | --- |
| Advanced Rust configuration and routing | Add checked configuration covering the supported upstream fields and connect it to complete-file encoding. Keep the existing convenience APIs and defaults. Internal `WebPEncode` is not a general public equivalent of upstream's config/picture API. | Rust [`config_enc.rs`](../libwebpop/src/enc/config_enc.rs), [`webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs), [`encode.rs`](../libwebpop/src/encode.rs) |
| Public low-memory mode and additional direct-loop integrations | `VP8EncLoop`, single-pass statistics, fitted skip probabilities and skipped residual handling are complete for normal methods 0–2. Methods 3–6 use buffered tokens. Add checked `low_memory` routing for methods 3–6 and integrate direct coding with future partitions/passes/search. | Upstream [`frame_enc.c`](../upstream/src/enc/frame_enc.c), Rust [`frame_enc.rs`](../libwebpop/src/enc/frame_enc.rs) |
| Multiple coefficient partitions | Only one is emitted. Add 2/4/8 partitions, partition sizes, row distribution and overflow checks. Match upstream routing: its buffered token path forces one partition, so a partitions field alone is insufficient. | Upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c), [`syntax_enc.c`](../upstream/src/enc/syntax_enc.c) |
| Rate control and repeated passes | Add `target_size`, `target_PSNR`, user-selected `pass` 1–10, and `qmin`/`qmax`; port quality search, convergence and state resets. Default header-budget retries already work and do not implement these features. Match the pin's target precedence and approximate-target behavior. | Upstream [`frame_enc.c`](../upstream/src/enc/frame_enc.c) |
| Segmentation, SNS and header-budget controls | Expose and validate `segments`, `sns_strength`, and `partition_limit`; generalize the analysis/setup paths currently used with default settings. Some quantizer/SNS helpers already accept parameters. | Rust [`analysis_enc.rs`](../libwebpop/src/enc/analysis_enc.rs), [`quant_enc.rs`](../libwebpop/src/enc/quant_enc.rs), [`vp8i_enc.rs`](../libwebpop/src/enc/vp8i_enc.rs) |
| Filter controls and automatic filter search | Integrate `filter_strength`, `filter_sharpness`, simple/strong `filter_type`, and `autofilter`. Deterministic adjustment exists; SSIM statistics, filter trials and automatic selection do not. | Rust [`filter_enc.rs`](../libwebpop/src/enc/filter_enc.rs), upstream [`filter_enc.c`](../upstream/src/enc/filter_enc.c) |
| Preprocessing and SharpYUV | Add segment smoothing and pseudo-random conversion dithering. Scalar SharpYUV and its encoder/CLI routing are complete, including `--pre 4` and upstream's small-image fallback. The caller adapts the independent module without adding encoder dependencies to it. See the [SharpYUV plan](SHARPYUV_PORT_PLAN.md). | Rust [`sharpyuv/`](../libwebpop/src/sharpyuv/), [`picture_csp_enc.rs`](../libwebpop/src/enc/picture_csp_enc.rs), upstream [`picture_csp_enc.c`](../upstream/src/enc/picture_csp_enc.c), [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c) |
| Presets and JPEG-size emulation | Add picture/photo/drawing/icon/text presets, full default-preset initialization, and `emulate_jpeg_size`. Several presets depend on the missing preprocessing or configurable segmentation/filter behavior. | Upstream [`config_enc.c`](../upstream/src/enc/config_enc.c), [`quant_enc.c`](../upstream/src/enc/quant_enc.c) |
| Lossless preset levels and image hints | `-z` levels 0–9 need the upstream method/quality mapping and option ordering; the underlying methods already exist. Add `image_hint`. At this pin, `WEBP_HINT_GRAPH` changes initial VP8L writer allocation; the other hint values do not imply additional missing compression algorithms. | Upstream [`config_enc.c`](../upstream/src/enc/config_enc.c), [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c) |
| Configurable alpha method/filter | Integrate public `alpha_compression` 0/1 and filtering none/fast/best. Low-level Rust `EncodeAlpha` already accepts these and tries the filters; public lossy encoding fixes compression=1/filter=fast. Effort, VP8L quality and fast-filter selection now follow the selected method for all methods 0–6. | Rust [`alpha_enc.rs`](../libwebpop/src/enc/alpha_enc.rs) |
| Lossy transparent-area policy | Add lossy `exact` handling: current routing always performs YUVA cleanup and the CLI rejects `--lossy --exact`. Follow upstream's cleanup semantics; lossy `exact` does not make RGB lossless. | Rust [`webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs), upstream [`webp_enc.c`](../upstream/src/enc/webp_enc.c) |
| Encoder threading | Add `thread_level`: split VP8L configuration trials, split lossy analysis where upstream does, and run the alpha job concurrently with VP8 coding. Present execution is synchronous. Preserve selection/tie behavior and error propagation for the requested thread mode. | Upstream [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c), [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c), [`alpha_enc.c`](../upstream/src/enc/alpha_enc.c), [`thread_utils.c`](../upstream/src/utils/thread_utils.c) |

`use_delta_palette` is reserved in the pinned public header and is not an
implemented encoder feature to port. Near-lossless below strength 100 can
change intermediate alpha by design; an exact-alpha near-lossless option would
be a separate extension, not a missing upstream capability.

## Raw input, output and encoder utilities

These are encoder API features even though they do not introduce new bitstream
coding tools. Use safe Rust ownership and borrowing rather than reproducing C
allocation/free entry points or ABI structures.

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

## Container and animation output

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
Lossy selection/carryover, mixed candidates, keyframe controls, single-frame
reconstruction and the RGBA Optimized policy remain open.

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
  near-lossless and method-4 lossy settings plus supplied metadata. There are
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

## CLI completion

[`cwebpop/src/main.rs`](../cwebpop/src/main.rs) declares many upstream-style
options but deliberately rejects unsupported ones before opening files.
Complete the underlying features above, then wire and test these options:

| Area | Currently rejected options |
| --- | --- |
| Presets and hints | `-z`, `--preset`, `--hint` |
| Lossy analysis and filtering | `--segments`, `--sns`, `-f`, `--sharpness`, `--strong`, `--nostrong`, `--af`, `--partition_limit` |
| Search and preprocessing | `--size`, `--psnr`, `--pass`, `--qrange`, `--pre` values other than 0/4, `--jpeg_like` |
| Alpha controls | `--alpha_method`, `--alpha_filter`, `--blend_alpha`, legacy `--alpha_cleanup` |
| Input pixel operations | Raw YUV `-s`, `--crop`, `--resize`, `--resize_mode` |
| Resource policy | `--mt`, `--low_memory` |
| Reports and reconstruction | `--progress`, `--map`, `--print_psnr`, `--print_ssim`, `--print_lsim`, `-d` |
| Metadata output | `--metadata` currently has no implementation; writing supplied metadata is in scope, input-file extraction/copying is excluded |

Also extend `--lossy -m` beyond 4 and permit lossy `--exact` when implemented.
`--alpha_q`, `--sharp_yuv` and `--pre 0/4` already work for lossy encoding.
`--noasm` is accepted and is
currently a no-op because there is no explicit accelerated backend. `--short`
currently prints the byte count; upstream's richer statistics depend on the
reporting work above. Animation now has a full-frame library API and still
needs a sequence frontend; it is not a missing mode of upstream `cwebp` itself.

## Known performance gaps

Functional completion does not establish performance parity. Measurements
below are dated observations for particular inputs/builds, not acceptance
thresholds or claims about every configuration.

| Gap or evidence | Recorded result and implication |
| --- | --- |
| Lossless speed versus normal SIMD-enabled C | The eight-case [2026-10-23 benchmark](BENCHMARKS.md) found method-4 Rust about 32% slower on the map, 13% on the photo, 14% on the screenshot and 3% on the alpha image. Method-0 photo was effectively tied. The 81.37 MP invitation follow-up was about 6% slower. Outputs were byte-identical. |
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

## Suggested implementation order and completion checks

The completed [additional lossy methods plan](LOSSY_METHODS_PLAN.md) records
methods 0–6, their direct-loop/trellis/alpha dependencies, checked API/CLI
integration and release baseline. Other advanced encoder options remain below.

1. **Configuration foundation and small integrations:** define the checked
   advanced Rust API; wire existing alpha controls, configurable scalar knobs,
   lossless level mapping and hints. Add raw layout adapters without changing
   the existing convenience APIs.
2. **Additional VP8 configurations:** extend direct-loop routing to public
   low-memory mode for methods 3–6. Add multiple partitions, repeated
   passes and target-size/PSNR search with the correct routing dependencies.
3. **Quality controls and presets:** finish configurable analysis/filtering,
   automatic filter search, preprocessing, SharpYUV, JPEG-size emulation and
   preset behavior. Test conversion routing through prepared ARGB and YUV inputs.
4. **Integration and tools:** add writers, progress/cancellation, statistics,
   reconstruction, metrics, picture operations and CLI support. Metadata output
   can proceed independently; build animation on the output container layer.
   Follow the [animation plan](ANIMATION_PORT_PLAN.md) for an earlier usable
   raw-frame release, then lossless/lossy/mixed optimization and a sequence CLI.
5. **Performance:** establish missing comparison baselines early; carry scalar
   predictor improvements, SIMD, workers and memory work alongside the relevant
   algorithm milestones. Rebenchmark the final supported configurations.

For each feature, extend the existing portable fixtures, independent checks
and pinned native oracle rather than introducing another reference stack.
Compare intermediates at new algorithm branches and complete files for matching
configurations, including requested thread mode. Check presets and CLI option
ordering, invalid values, short/padded buffers, geometry limits, partition/RIFF
overflow, cancellation and write failures where applicable. Retain the pin and
fixture provenance. Carry upstream `WEBP_INLINE` as plain `#[inline]`.

Completion means the in-scope pinned encoder behaviors above have checked Rust
interfaces, their CLI equivalents are connected where applicable, new paths
have parity and independent validation, and performance limitations are measured
and documented. Do not claim a generic speed/memory parity result from the
existing lossless-only comparisons. Run the relevant tests, the full portable
and native-oracle suites, formatting, and a production library build as features
land; this audit itself changes documentation only.
