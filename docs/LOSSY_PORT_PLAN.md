# libwebpop: bottom-up lossy encoder port after lossless

## Target and compatibility

The implementation order is [lossless](LOSSLESS_PORT_PLAN.md), then
[near-lossless VP8L](NEAR_LOSSLESS_PORT_PLAN.md), then the lossy RGBA entry
points covered here, exposed by quad-image's full WebP Wasm encoder. The
near-lossless extension reuses VP8L and needs none of this plan's VP8/YUV/ALPH
milestones. Both lossless and near-lossless are complete for the initial
scalar scope, including their public APIs and pinned-reference validation.
Lossy milestone 0 is complete; the VP8/YUVA/ALPH encoder remains to port. Preserve
upstream function names and source layout where practical. The result is a
pure Rust library accepting prepared RGBA pixels and returning a complete
still-image WebP file containing VP8, with a separate alpha stream when needed.

The algorithm reference remains the pinned [`upstream/`](../upstream) revision
`1844042d1ca751e62e61df7617dba19c98588339`. Keep that pin throughout this port.
The Wasm artifact supplies the historical feature inventory and an optional
public-API regression backend; its older libwebp version does not replace the pinned
source as the byte-exact oracle. [`ALGORITHM.md`](ALGORITHM.md) describes an
older checkout and a different Rust encoder; it is useful background, not the
implementation baseline here.

Suggested Rust APIs, retaining the exported names:

```rust
pub fn WebPEncodeRGBA(
    rgba: &[u8], width: u32, height: u32, stride: usize, quality: f32,
) -> Result<Vec<u8>, WebPEncodingError>;

pub fn WebPEncodeRGBAWithAlphaQuality(
    rgba: &[u8], width: u32, height: u32, stride: usize,
    quality: f32, alpha_quality: i32,
) -> Result<Vec<u8>, WebPEncodingError>;
```

`WebPEncodeRGBAWithAlphaQuality` is quad-image's wrapper, not an upstream API.
The standard entry point must behave identically to it at `alpha_quality=100`.
Accept finite floating-point color quality in `0..=100`, including fractional
values, and integer alpha quality in `0..=100`. Reject NaN and infinities as the
current pin does. Use the lossless API's checked positive-stride geometry:
dimensions in `1..=16383`, `stride >= width * 4`, and input length at least
`(height - 1) * stride + width * 4`, with checked arithmetic throughout. Negative
strides, raw pointers, and C ABI ownership remain deferred.

The existing `WebPEncodingError` aliases `LosslessEncodeError`. Preserve existing
lossless callers and `LosslessOptions { quality: u32, method, exact }`; add
lossy configuration/error handling without rounding fractional color quality
into that lossless effort field. Current picture/config glue implements only
the ARGB/lossless subset of the upstream structs and APIs.

Aim for byte-for-byte agreement with the pinned scalar C encoder under a
recorded toolchain/configuration, including a test-only copy of the custom
alpha-quality wrapper. Independently decode output and check dimensions and
alpha semantics. Lossy RGB must be compared against decoded reference output
and quality measurements, rather than against the original pixels for equality.
Color quality 100 still uses YUV420 and quantization; it is not lossless RGB.

## Current repository (revalidated 2026-10-23)

Continued from `701f3c7` against the unchanged, clean upstream pin.
**Milestones 0 and 1 are complete.** The scalar VP8 prediction, transform,
quantization and metric leaves match C. The dependency graph below still applies;
the public lossy Rust APIs remain unimplemented.

- **Lossless milestones 1–9 are complete.** The public simple API and
  configurable `encode_lossless_rgba` use the full synchronous analysis/trial
  loop, all four transforms, palette search, LZ77/cache selection, histogram
  clustering and Huffman coding. Quality `0..=100`, methods `0..=6` and both
  exact settings are already supported. The
  [lossless plan](LOSSLESS_PORT_PLAN.md#current-repository) records scalar C
  parity, independent decoding and a size/timing baseline.
- **ALPH's VP8L backend is implemented.**
  [`VP8LEncodeStream`](../libwebpop/src/enc/vp8l_enc.rs) accepts method, quality
  and exact directly, writes into a supplied `VP8LBitWriter`, and leaves image
  headers and RIFF to its caller. Method 4/quality 32/`exact=true` requires
  targeted alpha-stream comparisons, not a new configurable encoder. The
  [current stream corpus](../libwebpop/tests/support/streams.rs) does not yet
  explicitly cover that combination or alpha-to-green inputs.
- **Near-lossless milestones 0–5 are complete.** Standalone preprocessing,
  reconstructed predictor feedback, candidate routing, configurable/public
  APIs and CLI integration match the pin. The
  [near-lossless plan](NEAR_LOSSLESS_PORT_PLAN.md) and
  [reference record](../libwebpop/tests/reference/NEAR_LOSSLESS.md) retain the
  acceptance evidence. These are VP8L algorithms; ALPH still needs its separate
  centroid `QuantizeLevels`, filters and alpha-plane DSP, and VP8 still needs
  its segment setup, mode search and frame coding.
- **Lossy oracle milestone 0 is complete.**
  [`dsp/enc.rs`](../libwebpop/src/dsp/enc.rs) ports `FTransform_C`, with 1,727
  exact native comparisons and an independent exhaustive constant-byte-pair
  check. Both public lossy C entry points use the normally built pinned
  library; the custom alpha-quality wrapper is copied with provenance.
  There are 48 portable fixtures (16 leaves, 32 complete reference files), and
  3,566 independently decoded C files cover fractional color quality, alpha
  quantization, both raw/compressed ALPH paths, default/custom-100 identity and
  opaque invariance. Invalid float and signed alpha qualities are rejected.
  The [lossy reference record](../libwebpop/tests/reference/LOSSY.md) documents
  protocols, wrapper provenance, measured paths and fixture regeneration.
- **Milestone 1 transforms are ported.** `FTransform2_C`, `ITransform_C` /
  `ITransformOne` and `FTransformWHT_C` extend the encoder DSP; the new
  [`dsp/dec.rs`](../libwebpop/src/dsp/dec.rs) supplies only the encoder-required
  inverse `TransformWHT_C`. There are 21,728 additional exact C comparisons
  and 90 portable fixtures, with independent Hadamard matrix products, DC
  rounding/clipping checks and boundary/padding checks.
- **Milestone 1 scalar predictions are ported.** `Intra16Preds_C`,
  `IntraChromaPreds_C` and all ten modes in `Intra4Preds_C` retain the upstream
  scratch layout, missing-border defaults, chroma corner spacing and explicit
  13-byte Intra4 boundary window. Prediction offsets now live in
  [`enc/vp8i_enc.rs`](../libwebpop/src/enc/vp8i_enc.rs). There are 4,910 exact C
  comparisons and 51 portable fixtures, including full scratch preservation.
  Independent tests check rounded border means, directional transposition,
  true-motion clipping, byte averaging, replicated edges and minimum buffers.
  Iterator construction/rotation of these borders remains in milestone 3.
- **Milestone 1 is complete.** The remaining DSP now includes `QuantizeBlock_C`
  (also WHT), `Quantize2Blocks_C`, SSE and texture distortion, `Mean16x4_C`,
  histogram collection/reduction, block copies, zigzag and scan/offset tables.
  `VP8Matrix`, `BIAS` and wrapping `QUANTDIV` retain the pinned field widths and
  arithmetic. Another 6,189 C comparisons and 133 portable fixtures cover
  thresholds, sharpening, level saturation, both coefficient outputs, paired
  nonzero masks, full-width weights, empty histograms and padding. Across all
  DSP batches, 34,554 exact native requests and 290 portable DSP fixtures pass.
  `enc/quant_enc.rs` currently contains scan/offset tables; segment-quality
  mapping and matrix construction remain in milestone 4.
- **Shared input/output glue remains ARGB/VP8L-only.** Checked geometry,
  owned pixels/output and error propagation exist. `WebPPicture` has only
  width, height and packed ARGB; import, transparency detection and cleanup
  operate on that representation. YUVA storage/import/cleanup, `VP8BitWriter`,
  VP8 syntax and `VP8X`/`ALPH` assembly have not been ported.
- **Reuse the native oracle and committed corpus.** The
  [CMake harness](../libwebpop/tests/reference/CMakeLists.txt) links the full
  pinned encoder with near-lossless on, SIMD off and threading off. Lossy
  adapters now extend the existing logical protocol. The independent `image`
  decoder exercises VP8/ALPH as well as VP8L; the existing
  [real-image sample](../libwebpop/tests/fixtures/corpus/README.md) participates.
  The retained older full Wasm has opt-in near-lossless wrapper regression;
  [artifact/host provenance](../libwebpop/tests/reference/full-wasm/README.md)
  records its setup. Extending that backend to lossy remains optional, outside
  the acceptance gates.

## Inspected Wasm build and feature boundary

Inspected on 2026-10-22 in the sibling quad-image checkout at
`1ee887300721ad754c5632b56349d5daad17c11e`, with no local changes to the inspected
build directory or full Wasm artifact:

- [`webp-wasm/README.md`](../../quad-image/webp-wasm/README.md),
  [`Dockerfile`](../../quad-image/webp-wasm/Dockerfile), and
  [`simd.patch`](../../quad-image/webp-wasm/simd.patch).
- [`alpha-quality.c`](../../quad-image/webp-wasm/alpha-quality.c) and
  [`near-lossless.c`](../../quad-image/webp-wasm/near-lossless.c).
- [`web/assets/webp-encode.wasm`](../../quad-image/web/assets/webp-encode.wasm)
  and the [browser host](../../quad-image/web/locket/webp-wasm.ts).

These historical links require the sibling checkout. The reviewed custom
wrappers are now copied into this repository's test support with provenance;
tests do not depend on that checkout being present.

The actual full artifact is **440,786 bytes**, SHA-256
`f6317d0cce217547b165a768f82647c2e923c489b5ab75b8dd0310722cffa477`.
The sizes in its README describe earlier builds. Its recipe selects libwebp
v1.6.0, Emscripten 6.0.10, Release libraries, and an `-O3 -msimd128` final link.
The patch enables SSE2/SSE4.1 translated to Wasm SIMD and fixes CPU dispatch.
Threads are disabled. The lossless-only patch is applied after the full library
is built and does not restrict the full module.

Direct module inspection confirmed the following codec exports:

| Export | Behavior | This plan |
| --- | --- | --- |
| `WebPEncodeRGBA` | Default lossy preset, caller's color quality, lossless alpha | Required |
| `WebPEncodeRGBAWithAlphaQuality` | Same lossy preset with independent alpha quality; values below 100 quantize alpha | Required, including quantization |
| `WebPEncodeLosslessRGBA` | VP8L, quality 70/method 4 | Inherited from the completed lossless plan |
| `WebPEncodeRGBAWithNearLossless` | VP8L at quality 70/method 4 with caller-selected near-lossless strength | Implement first under the [near-lossless plan](NEAR_LOSSLESS_PORT_PLAN.md); not a lossy VP8 feature |

The other exports provide linear memory, allocation/free, an indirect function
table, initialization, stack helpers, and exception support. The only import is
`env.emscripten_notify_memory_growth`. There is no exported advanced config,
decoder, muxer, animation encoder, SharpYUV selector, method selector, or internal
leaf wrapper. Linking `libsharpyuv.a` does not make SharpYUV reachable through
these default RGBA wrappers.

A Node v24.20.0 smoke check instantiated the actual artifact, initialized its
stack and limits before `_initialize`, and copied outputs before freeing guest
memory. Six geometries (`1×1`, `1×17`, `17×1`, `3×5`, `17×19`, `64×48`), opaque
and varying-alpha pixels, padded strides with exact minimum input lengths, and
color qualities `0, 30, 50, 75, 98, 98.5, 99, 100` established:

- All 96 standard/custom-alpha-100 pairs were byte-identical.
- Another 576 encodes at alpha qualities `0, 10, 30, 70, 71, 99` succeeded;
  opaque outputs matched the standard API. Parsed RIFF lengths and chunk
  boundaries were valid: opaque cases used `VP8 `, transparent cases used
  `VP8X`, `ALPH`, `VP8 ` in order.
- Quantized-alpha headers included `0x10`, `0x11`, and `0x1d`, demonstrating
  raw and compressed payloads and a gradient-filtered compressed payload.
- All 12 near-lossless-100/lossless pairs were byte-identical.
- Color qualities -1, 101, and infinity were rejected; invalid alpha qualities
  -1 and 101 were rejected with a cleared output slot. **NaN color quality was
  accepted by the older artifact**, unlike the current pin. The custom wrapper
  clears its output slot on ordinary failure; the standard export can leave
  the slot untouched on early configuration failure. Rust returns `Result`
  instead of reproducing pointer-slot behavior.

These were structural and byte-comparison smoke checks, not pixel decoding,
branch-complete coverage, or a comparison against Rust lossy output. No lossy
Rust implementation is added by this document. These historical artifact checks
do not require retaining a Wasm runtime dependency; use the pinned native oracle
for ongoing differential testing.

## Configuration actually reachable

Both lossy wrappers initialize `WEBP_PRESET_DEFAULT`, leave `picture.use_argb`
at zero, and call `WebPPictureImportRGBA` before `WebPEncode`. The importer
therefore converts directly to YUVA; it does not take the ARGB import branch
used by the lossless API. Settings below come from
[`config_enc.c`](../upstream/src/enc/config_enc.c),
[`picture_enc.c`](../upstream/src/enc/picture_enc.c), and the custom wrapper.

| Setting | Value | Consequence |
| --- | --- | --- |
| `lossless`, `picture.use_argb` | `0`, `0` | VP8 path; direct RGBA → YUV420(A) import |
| `quality` | Caller-selected finite `0..=100` | Nonlinear quantizer mapping and quality-dependent branches |
| `method` | `4` | `RD_OPT_BASIC`; Intra16/Intra4 and UV rate-distortion search, no trellis |
| `segments`, `sns_strength` | `4`, `50` | Susceptibility analysis, segment assignment and perceptual quantizer adaptation |
| `filter_strength`, `filter_type`, `filter_sharpness` | `60`, `1`, `0` | Strong/normal VP8 filter signaling and strength adjustment |
| `autofilter`, `show_compressed` | `0`, `0` | No SSIM filter trials or exported reconstructed picture |
| `partitions`, `low_memory` | `0`, `0` | One coefficient partition; buffered-token path enabled |
| `pass`, `target_size`, `target_PSNR` | `1`, `0`, `0` | No target-size/PSNR search; partition-0 overflow can still cause retries |
| `partition_limit`, `qmin`, `qmax` | `0`, `0`, `100` | Default Intra4 header budget and quality limits |
| `preprocessing`, `use_sharp_yuv` | `0`, `0` | No dithering, segment smoothing, or SharpYUV conversion |
| `exact` | `0` | YUVA transparent-area cleanup is reachable |
| `alpha_compression`, `alpha_filtering` | `1`, `1` | VP8L alpha with fast filter selection and raw fallback |
| `alpha_quality` | `100`, or custom caller value | Exact alpha, or adaptive alpha-level quantization |
| `thread_level` | `0` | Analysis and alpha jobs execute synchronously |
| `emulate_jpeg_size` | `0` | Normal quality mapping |
| Stats, progress hook, extra info | Unset | No public statistics/callback framework required |

Do not expand this into every libwebp option just because the full library was
linked. Conversely, segmentation, spatial noise shaping, Intra4, dynamic token
probabilities, chroma DC error diffusion, alpha filters, and raw-alpha fallback
are within this target; a legal fixed-mode VP8 encoder would not complete it.

Important branch boundaries in this pin:

- Chroma DC error-diffusion storage is allocated at color quality **<=98**
  (`ERROR_DIFFUSION_QUALITY`), with pass=1. Qualities above 98 take another path.
- `VP8EncTokenLoop` is the selected frame loop. Its probability refresh interval
  is `max(macroblock_count >> 3, 96)`; refresh happens when its countdown becomes
  negative. Small images alone do not test this mid-frame feedback.
- This token loop starts with `use_skip_proba=0` and never enables it. It records
  residual tokens even for all-zero macroblocks. Preserve `mb.skip` for other
  decisions, but skip-probability fitting and skipped residual emission belong
  to the deferred direct loop.
- One configured pass does not eliminate partition-0 budget retries. The token
  loop can tighten `max_i4_header_bits` and restart a frame.
- Alpha quality below 100 requests `2 + quality / 5` levels through quality 70,
  then `16 + (quality - 70) * 8` levels. This is a requested centroid count,
  not a uniform bit-depth conversion. Already-small palettes may be unchanged.

## What the completed lossless port supplies

These components are implemented today. Extend them without repeating the
lossless milestones or introducing another oracle stack.

| Inherited component | Reuse and additional work |
| --- | --- |
| Checked RGBA geometry, errors, `LosslessOptions`, owned ARGB picture and output | Add YUVA storage/import and VP8 routing; preserve the public lossless options and error contract |
| `PutLE32`, checked VP8L image/RIFF writers | Reuse size-checking/ownership patterns; add VP8 container writers, `VP8X`/`ALPH`, 24-bit fields and limits |
| `VP8LBitWriter`, Huffman, LZ77, histograms, transforms, palette search | Reuse for alpha's internal VP8L stream; not VP8 coefficient entropy coding |
| Configurable headerless `VP8LEncodeStream` | Validate existing quality **32**, method **4**, `exact=1` on alpha-to-green data; call without top-level VP8L headers or RIFF |
| ARGB transparency detection, replacement and lane/packing helpers | Add 8-bit alpha detection/extraction/green dispatch, filters, centroid quantization and YUVA cleanup |
| Near-lossless preprocessing and predictor quantization leaves | Preserve shared VP8L behavior; ALPH uses strength 100 and its own alpha quantizer, so these add no lossy algorithm coverage |
| Scalar native oracle, logical fixture protocol, seeded cases and independent decoder | Add VP8 leaves/subgraphs and custom wrapper to the existing opt-in harness |

Alpha is the main additional dependency on lossless, but is not a call to
`WebPEncodeLosslessRGBA` followed by stripping bytes. `alpha_enc.c::EncodeLossless`
dispatches the filtered alpha samples into the **green** channel of an internal
ARGB image and calls `VP8LEncodeStream` with `lossless=1`, `exact=1`, method 4,
quality `8 * method = 32`, and near-lossless 100. Dispatch sets each pixel to
`alpha_sample << 8`, leaving A/R/B zero, so exact handling must preserve green
even though every internal pixel is transparent. Call the existing stream
interface directly; the public image path would add VP8L size/version headers
and RIFF. Add explicit q32/method4/exact1 comparisons of analysis, transform
trials and complete headerless streams, using constant, palette, filtered,
noisy and odd-sized alpha planes. Keep both raw and compressed ALPH paths.
When near-lossless routing is integrated, these calls must explicitly retain
strength 100 and exact handling.

## Function dependency map

This is a source-derived dependency map, not a compiler-generated exhaustive
call graph or literal execution trace. Repeated subgraphs use letters; small
accessors, allocation, reset, and cleanup helpers are grouped. Paths below are
relative to `upstream/src/`. `[conditional]` means reachable for some valid
inputs/settings above and required for completion. `[inherited]` means provided
by the current lossless implementation, sometimes with the extensions described
above. Inherited initialization/writer nodes denote equivalent Rust behavior;
the port does not reproduce the full C ABI, config struct or callback framework.

```text
WebPEncodeRGBA                                               enc/picture_enc.c
└── Encode [lossless=0, caller quality]
    ├── WebPConfigPreset → WebPConfigInitInternal / Validate  [extend inherited subset]
    ├── WebPPictureInit / WebPMemoryWriterInit                [inherited]
    ├── WebPPictureImportRGBA                                [A]
    └── WebPEncode [lossy branch]                            enc/webp_enc.c
        ├── WebPValidateConfig / WebPValidatePicture          [extend inherited]
        ├── WebPCleanupTransparentArea [YUVA]                 [A, conditional]
        ├── InitVP8Encoder
        │   ├── MapConfigToTools → RD_OPT_BASIC / use_tokens=1
        │   ├── ResetSegmentHeader / ResetFilterHeader / ResetBoundaryPredictions
        │   ├── VP8DefaultProbas                             [C]
        │   ├── VP8EncDspInit / VP8EncDspCostInit             [B, C]
        │   ├── VP8TBufferInit                               [C]
        │   └── VP8EncInitAlpha                              [I]
        ├── VP8EncAnalyze                                    [E]
        ├── VP8EncStartAlpha → CompressAlphaJob [synchronous] [I, conditional]
        ├── VP8EncTokenLoop                                  [G]
        ├── VP8EncFinishAlpha [synchronous completion]
        └── VP8EncWrite                                      [H]

WebPEncodeRGBAWithAlphaQuality                               quad-image wrapper
├── WebPConfigPreset [caller quality] / config.alpha_quality = caller value
├── WebPValidateConfig / WebPPictureInit / owned memory writer
├── WebPPictureImportRGBA                                    [A]
└── WebPEncode [same lossy branch above]
```

There is no second ARGB-to-YUV pass at `WebPEncode` for these entry points:
the importer already supplied Y/U/V. Port that actual import route first.

```text
[A] RGBA import, YUVA planes, transparent cleanup
└── WebPPictureImportRGBA → Import [use_argb=0]               enc/picture_csp_enc.c
    ├── CheckStride / WebPValidatePicture
    └── ImportYUVAFromRGBA
        ├── CheckNonOpaque → WebPHasAlpha32b / HasAlpha32b_C  dsp/alpha_processing.c
        ├── WebPPictureAllocYUVA                             enc/picture_enc.c
        └── WebPImportYUVAFromRGBA / ...FromRGBALastLine       dsp/yuv.c
            ├── ConvertRGBToY_C / VP8RGBToY                  dsp/yuv.{c,h}
            ├── WebPExtractAlpha / ExtractAlpha_C            dsp/alpha_processing.c
            ├── WebPAccumulateRGB / WebPAccumulateRGBA        dsp/yuv.c
            │   ├── GammaToLinear / LinearToGamma / Interpolate
            │   └── LinearToGammaWeighted / inverse-alpha table [conditional]
            └── WebPConvertRGBA32ToUV_C → VP8RGBToU/V         dsp/yuv.{c,h}
WebPCleanupTransparentArea [YUVA]                            enc/picture_tools_enc.c
└── SmoothenBlock / Flatten [including partial right/bottom blocks]

[B] Scalar VP8 prediction, transform, reconstruction and metrics
├── Intra16Preds_C / IntraChromaPreds_C                       dsp/enc.c
│   └── DCMode / VerticalPred / HorizontalPred / TrueMotion
├── Intra4Preds_C → DC4 / TM4 and directional predictors      dsp/enc.c
├── FTransform_C / FTransform2_C / FTransformWHT_C             dsp/enc.c
├── ITransform_C → ITransformOne                             dsp/enc.c
├── VP8TransformWHT → TransformWHT_C                          dsp/dec.c
├── QuantizeBlock_C / Quantize2Blocks_C [also WHT dispatch]   dsp/enc.c
├── SSE16x16_C / SSE16x8_C / SSE8x8_C / SSE4x4_C              dsp/enc.c
├── Disto4x4_C / Disto16x16_C → TTransform                   dsp/enc.c
├── CollectHistogram_C → VP8FTransform / VP8SetHistogramData  dsp/enc.c
└── Copy4x4_C / Copy16x8_C / scan tables / clipping helpers   dsp/enc.c

[C] Boolean arithmetic coding, residual costs, and buffered tokens
├── VP8BitWriterInit / VP8PutBit / VP8PutBitUniform            utils/bit_writer_utils.c
│   └── kNorm / kNewRange / Flush [carry and delayed 0xff runs]
├── VP8PutBits / VP8PutSignedBits / VP8BitWriterFinish
├── VP8BitWriterAppend / size / position / wipe helpers
├── VP8DefaultProbas / coefficient and mode probability tables enc/tree_enc.c
├── VP8EntropyCost / VP8LevelFixedCosts                      dsp/cost.c
├── VP8CalculateLevelCosts / VariableLevelCost                enc/cost_enc.c
├── VP8InitResidual / VP8SetResidualCoeffs → SetResidualCoeffs_C
│                                                           enc/cost_enc.c, dsp/cost.c
├── VP8GetResidualCost → GetResidualCost_C                    dsp/cost.c
├── VP8GetCostLuma4 / VP8GetCostLuma16 / VP8GetCostUV          enc/cost_enc.c
└── VP8TBufferInit / Clear                                   enc/token_enc.c
    ├── VP8RecordCoeffTokens → AddToken / AddConstantToken
    │   └── VP8RecordStats / coefficient categories and band contexts
    └── VP8EmitTokens → VP8PutBit / VP8PutBitUniform

[D] Macroblock iterator and coding state                     enc/iterator_enc.c
├── VP8IteratorInit / Reset / SetRow → InitLeft / InitTop
├── VP8IteratorImport → ImportBlock / ImportLine [replicate incomplete edges]
├── VP8IteratorNzToBytes / VP8IteratorBytesToNz
├── VP8IteratorSaveBoundary / VP8IteratorNext
├── VP8SetIntra16Mode / VP8SetIntra4Mode / VP8SetIntraUVMode   enc/vp8i_enc.h
├── VP8SetSegment / VP8SetSkip
└── VP8IteratorStartI4 / VP8IteratorRotateI4

[E] Analysis and segmentation                               enc/analysis_enc.c
└── VP8EncAnalyze → InitSegmentJob / DoSegmentsJob [synchronous]
    ├── VP8IteratorImport [source boundaries for analysis]    [D]
    ├── MBAnalyze
    │   ├── MBAnalyzeBestIntra16Mode / MBAnalyzeBestUVMode
    │   │   └── VP8MakeLuma16Preds / VP8MakeChroma8Preds      [F]
    │   │       and VP8CollectHistogram / GetAlpha           [B]
    │   └── FinalAlphaValue / susceptibility histogram
    └── AssignSegments → SetSegmentAlphas [k-means, integer ties]

[F] Segment parameters and rate-distortion mode selection    enc/quant_enc.c
├── VP8SetSegmentParams
│   ├── QualityToCompression / pow / quantizer clamp
│   ├── SetupFilterStrength → VP8FilterStrengthFromDelta      enc/filter_enc.c
│   ├── SimplifySegments [merge/remap equivalent segments]
│   └── SetupMatrices → ExpandMatrix [q/iq/bias/zthresh/sharpen/lambdas]
├── VP8MakeLuma16Preds / VP8MakeChroma8Preds / MakeIntra4Preds [B]
├── ReconstructIntra16 → FTransform2 / FTransformWHT           [B]
│   └── quantize Y2 + Y AC → TransformWHT / ITransform        [B]
├── ReconstructIntra4 → FTransform / quantize / ITransform    [B]
├── ReconstructUV → FTransform2 / quantize / ITransform       [B]
│   └── CorrectDCValues → QuantizeSingle [conditional error diffusion]
└── VP8Decimate [RD_OPT_BASIC]
    ├── InitScore / SetRDScore / score and scratch-buffer swaps
    ├── PickBestIntra16 → ReconstructIntra16 / VP8GetCostLuma16
    │   └── SSE / texture distortion / StoreMaxDelta         [B, C]
    ├── PickBestIntra4 → StartI4 / RotateI4 / ReconstructIntra4
    │   └── GetCostModeI4 / VP8GetCostLuma4 / mode costs      [C, D]
    ├── PickBestUV → ReconstructUV / VP8GetCostUV             [B, C]
    │   └── StoreDiffusionErrors [conditional]
    └── VP8SetSkip [record all-zero result; no token-loop skip syntax]

[G] Default frame loop and probability feedback              enc/frame_enc.c
└── VP8EncTokenLoop
    ├── InitPassStats / quality clamp
    ├── SetLoopParams
    │   ├── VP8SetSegmentParams                              [F]
    │   ├── SetSegmentProbas → ResetSegments / GetProba
    │   └── ResetStats / VP8CalculateLevelCosts               [C]
    ├── VP8IteratorInit / Import / SaveBoundary / Next        [D]
    ├── FinalizeTokenProbas → CalcTokenProba / BranchCost     [periodic + final]
    │   └── VP8CalculateLevelCosts                           [C, periodic]
    ├── VP8Decimate                                          [F]
    ├── RecordTokens → VP8RecordCoeffTokens                   [C]
    │   └── NzToBytes / BytesToNz [Y2, Y, UV context updates] [D]
    ├── partition-0 size estimate / header-budget retry [conditional]
    ├── InitBitWriters / VP8EmitTokens                        [C]
    └── PostLoopFinalize
        ├── VP8BitWriterFinish                               [C]
        └── VP8AdjustFilterStrength → VP8FilterStrengthFromDelta
                                                            enc/filter_enc.c

[H] VP8 syntax and WebP container                            enc/syntax_enc.c
└── VP8EncWrite
    ├── GeneratePartition0
    │   ├── PutSegmentHeader / PutFilterHeader / PutQuant
    │   ├── VP8WriteProbas                                   enc/tree_enc.c
    │   ├── VP8CodeIntraModes                                enc/tree_enc.c
    │   │   └── PutSegment / PutI16Mode / PutI4Mode / PutUVMode
    │   └── VP8BitWriterFinish                               [C]
    ├── PutWebPHeaders
    │   ├── PutRIFFHeader / PutVP8Header / PutVP8FrameHeader
    │   ├── PutVP8XHeader / PutAlphaChunk                     [conditional]
    │   └── PutLE24 / PutLE32 / checked file and partition sizes
    └── EmitPartitionsSize / owned writer / PutPaddingByte

[I] Separate alpha stream                                   enc/alpha_enc.c
└── VP8EncInitAlpha / StartAlpha → CompressAlphaJob
    ├── WebPPictureHasTransparency [8-bit YUVA alpha]          [extend inherited]
    └── EncodeAlpha
        ├── WebPCopyPlane
        ├── QuantizeLevels [quality<100, centroid remapping]  utils/quant_levels_utils.c
        └── ApplyFiltersAndEncode
            ├── GetFilterMap → GetNumColors / WebPEstimateBestFilter
            │                                               utils/filters_utils.c
            └── EncodeAlphaInternal [each selected filter, strict size ties]
                ├── WebPFilters → Horizontal/Vertical/GradientFilter_C
                │                                           dsp/filters.c
                ├── EncodeLossless → WebPDispatchAlphaToGreen
                │   │                                       dsp/alpha_processing.c
                │   └── VP8LEncodeStream [method=4, q=32, exact=1] [inherited; validate]
                ├── raw fallback if compressed payload exceeds width*height
                └── ALPH control byte + payload → VP8BitWriterAppend
```

The default color converter includes gamma-compensated chroma subsampling and
alpha weighting, not merely integer matrix conversion with a box average.
Preserve the gamma tables/interpolation, inverse-alpha arithmetic, zero-alpha
fallback, and odd-edge handling. The current pin moved some importer work to
`dsp/yuv.c`; follow this revision's functions rather than older descriptions.

The `alpha`/`beta` values in segmentation measure coding susceptibility, not
transparency. Likewise, `VP8BitWriter` is a boolean range coder distinct from
the inherited `VP8LBitWriter`. Sharing their source file does not make their
algorithms interchangeable. ALPH uses the former's raw append utility without
arithmetic-coding the appended bytes.

## Rust layout and translation rules

Extend the completed lossless tree rather than creating a second encoder crate:

```text
src/
  encode.rs                                                # extend public APIs
  webp/{encode,format_constants}.rs                         # extend errors/constants
  enc/{config_enc,picture_enc,picture_csp_enc,picture_tools_enc,webp_enc}.rs
  enc/{vp8i_enc,analysis_enc,iterator_enc,quant_enc,cost_enc}.rs
  enc/{frame_enc,token_enc,tree_enc,filter_enc,syntax_enc,alpha_enc}.rs
  dsp/{enc,cost,yuv,filters,alpha_processing}.rs
  dsp/dec.rs                                                # encoder-required inverse WHT only
  utils/{bit_writer_utils,filters_utils,quant_levels_utils}.rs
  # existing VP8L modules remain shared with ALPH
```

As in the lossless port, retain function names, source references, constants,
tables, tie-breaking, and license/patent notices. Carry upstream `WEBP_INLINE`
annotations, including those expanded through macros, into plain `#[inline]`
on corresponding Rust functions. Use owned buffers, slices,
explicit offsets, and `Result`. Preserve recognizable `VP8Encoder`,
`VP8EncIterator`, `VP8ModeScore`, `VP8Matrix`, `VP8SegmentInfo`, `VP8EncProba`, and
residual/token state. Replace C pointer swaps with explicit owned scratch slots;
token storage need not reproduce linked allocation pages, but must preserve
token order, probability-table indices, fixed-probability bits, and statistics.

Bind DSP names directly to scalar functions and run analysis/alpha jobs
synchronously. SIMD and worker frameworks can follow after parity. Include the
inverse WHT from `dsp/dec.c` without requiring a decoder port. Analysis uses
source boundaries; actual coding predicts from **unfiltered reconstructed**
neighbors. Intra4 reconstruction also feeds subsequent subblocks inside a
macroblock. Trial candidates must not commit losing modes, borders, contexts,
or diffusion errors. Preserve the upstream scratch/commit sequence.

Use sufficiently wide signed intermediates for transforms, scores, and cost
products; preserve arithmetic right shifts, integer truncation, coefficient
clipping, and quantizer rounding. Quantization returns levels for tokens and
dequantized coefficients for reconstruction: both must describe the same
choice. Use checked arithmetic for image/plane lengths, macroblock counts,
offsets, RIFF sizes, and partition lengths. Limit wrapping to demonstrated C
unsigned arithmetic, including the boolean coder's state.

Numerical parity needs explicit attention beyond the lossless leaves. Segment
quality mapping uses double-precision `pow`; gamma-table construction also uses
`pow`, and alpha centroid quantization uses floating-point convergence tests.
Compare the oracle's tables and values around integer/centroid boundaries.
Retain the C evaluation order and f32/f64 conversions; do not substitute generic
color conversion, a linear quality map, uniform alpha quantization, or a
different rate-distortion formula. Record libm/platform differences and isolate
their first divergent intermediate before deciding on a compatibility remedy.

## Reference testing

Extend the existing [scalar C oracle](../libwebpop/tests/reference/README.md).
Its build already links the full pinned encoder with SIMD/threading disabled;
production Rust must remain independent of C, CMake, and Wasm. Keep the chosen
backend and logical fixture protocol. Add same-translation-unit wrappers for
static helpers outside `upstream/`, one source file at a time, with no duplicate
definitions when linking. Preserve the inherited lossless corpus and fixtures.

Milestone 0 adds the reviewed custom
[`alpha_quality_wrapper.c`](../libwebpop/tests/reference/alpha_quality_wrapper.c)
as test-only C, following the existing near-lossless integration pattern and
retaining provenance/notices. Both public references call the normally built
library. The first VP8 leaf uses its scalar DSP dispatch entry; future static
leaves use included/renamed translation units. Fractional color quality travels
in the u32-word protocol using its f32 bit pattern, preserving NaN/infinity
validation. See the [lossy reference record](../libwebpop/tests/reference/LOSSY.md).

Full-Wasm export/ABI and semantic regression for lossy remains optional. The
historically inspected full artifact is retained for near-lossless regression,
with its hash, provenance, SIMD host and initialization sequence recorded in
test support. The old lossless-only artifact is no longer bundled. Do not
require a sibling checkout at test time or identical bytes across the old SIMD
build and the newer scalar pin. The native oracle and portable C fixtures
remain sufficient for required lossy regression coverage.

Compare logical outputs in layers:

1. **Leaves:** Y/U/V/A samples, gamma tables, predictions, transforms, quantized
   and dequantized coefficients, nonzero masks, costs, probability counters,
   arithmetic-writer state and bytes, alpha remapping and filtered planes.
2. **Subgraphs:** segmentation maps/centers, per-segment matrices and lambdas,
   each reconstructed candidate, winning modes and scores, diffusion errors,
   saved borders, coefficient contexts, token sequences, probability refreshes,
   partition bytes, filter strengths, and ALPH filter trials/payloads.
3. **Public APIs:** exact complete bytes against the pinned scalar reference,
   independent decoding with the existing Rust decoder, and semantic checks
   against decoded C output; optional wrapper regressions through the older
   full Wasm backend. A decoder accepting the output alone does not establish
   encoder parity. Milestone 0 adds independently decoded C VP8/ALPH files;
   extend these to Rust lossy output as the public encoder becomes available.

At alpha quality 100, alpha must equal the original plane everywhere, including
fully transparent pixels. Below 100, compare the quantized plane and emitted
ALPH data exactly to C. ALPH's preprocessing flag can cause a decoder to smooth
quantized alpha; compare final alpha to the same reference file decoded by the
same independent decoder, not automatically to the raw centroid plane. Preserve
the source min/max during centroid quantization and test already-small palettes.
For color, record decoded-reference differences and PSNR/size baselines; do not
invent an RGB tolerance as a substitute for pinned-reference byte parity.

Keep deterministic seeds, compact portable fixtures, toolchain/build provenance,
the first differing logical intermediate, and minimized replay inputs as in the
lossless plan. Measure branch coverage with wrappers/counters; synthetic image
names do not prove that Intra4, diffusion, raw fallback, or frame retries ran.

## Implementation order and acceptance gates

The completed lossless and near-lossless ports satisfy the project order.
**Lossy milestones 0 and 1 are complete.** The next checkpoints are the boolean
writer/cost/token foundations (2) and YUVA import/iterator (3). Milestone 8 reuses
the implemented configurable VP8L stream. Split large files into leaf/subgraph
work; integrate parents only after their dependencies match the oracle.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 (complete) | Extend existing oracle/fixtures with a VP8 leaf, both lossy public C wrappers and their provenance | Completed lossless (satisfied) | `FTransform_C`, both public C entry points, 48 portable fixtures and 3,566 independent VP8/ALPH decodes; lossless and near-lossless regressions pass |
| 1 (complete) | VP8 tables/scans, clipping, predictions, transforms/inverse WHT, scalar quantization, SSE/texture/histogram DSP `[B]` | 0 | 34,554 exact native requests and 290 portable DSP fixtures; independent border, rounding, Hadamard, quantization, histogram and padding checks |
| 2 | Boolean bit writer, cost/probability tables, residual cost leaves, buffered token recording/emission `[C]` | 0 | Exact bits/bytes and state across carry/0xff runs, probabilities 0/255, finish boundaries, coefficient categories and token storage boundaries |
| 3 | YUVA picture storage, default RGBA import and cleanup `[A]`; iterator/block geometry/borders/contexts `[D]` | 0 | Exact YUVA planes, gamma/alpha-weighting results, padded blocks, row transitions and Intra4 rotations |
| 4 | Analysis/segment assignment `[E]`; segment params, matrix expansion and filter-strength leaves `[F foundations]` | 1, 3 | Exact susceptibility histograms/centers, segment remaps, q/deltas, matrices/lambdas and initial filter strengths |
| 5 | Candidate reconstruction, mode costs, Intra16/Intra4/UV selection, diffusion and scratch commits `[F remainder]` | 1, 2, 3, 4 | Exact levels, reconstructed blocks, scores, winning modes, saved contexts/borders and diffusion errors |
| 6 | VP8 partition-0 syntax, modes, frame headers and opaque RIFF assembly `[H]` | 2, 3, 4 | Synthetic coding state emits exact header bytes; test-only fixed-mode opaque image decodes independently |
| 7 | Default token frame loop, probability feedback, segment probabilities, header-budget retries, final filter adjustment `[G]` | 2, 4, 5, 6 | Exact tokens, update timing/probabilities, modes, partitions/filter headers and complete opaque default-path files |
| 8 | Alpha plane DSP, centroid quantization/filter leaves, existing VP8L q32/exact1 validation, filter trials/raw fallback and ALPH/VP8X assembly `[I]` | 0; inherited VP8L; 2 and 6 for assembly | Exact quantized/filtered planes, headerless alpha-to-green VP8L streams, selected ALPH payloads/control bytes and transparent chunk layout |
| 9 | Public `WebPEncodeRGBA` / custom alpha-quality wrapper and final shared config/picture integration | 7, 8 | Pinned-reference bytes and independent decoding across public corpus; invalid inputs return errors; default/custom-100 identity and lossless regressions pass |

Milestone 6 is an internal bitstream checkpoint, not a replacement public encoder
with simplified heuristics. Alpha leaf work in milestone 8 can start after
milestone 0 and proceed independently of VP8 mode search; its final container
integration needs milestone 6. Finish the public API only once the real default
path and alpha feature range are covered.

### Next implementation batches

The lossy oracle and scalar VP8 DSP milestones are complete. Next small units
(milestones 2–3, and independent alpha leaves) are:

1. Boolean writer traces with carry propagation, deferred 0xff runs and finish.
2. Probability/cost tables, residual last-nonzero detection and costs, then
   buffered token recording/emission with coefficient contexts and statistics.
3. Default RGB→Y and UV accumulation/conversion, gamma/interpolation and
   alpha-weighted cases; import last-row/odd-column handling before whole images.
4. Alpha-to-green dispatch and direct comparisons of the existing headerless
   `VP8LEncodeStream` at method 4/quality 32/`exact=1`, then alpha filter kernels,
   filter estimator and centroid `QuantizeLevels`. This branch can proceed
   independently of VP8 mode search; near-lossless quantization is not a
   substitute for the centroid algorithm.

## Corpus and completion criteria

Extend the shared synthetic image cases and reuse the committed
[`test_ref.ppm`](../libwebpop/tests/fixtures/corpus/README.md) sample and its
provenance. Add lossy/ALPH reference outputs separately from its existing
lossless `.webp`; add further small real-image cases where they establish
missing coverage. Include:

- `1×1`, `1×N`, `N×1`, odd sizes, and boundaries around 4, 8, 16 and 32 pixels;
  multiple rows of macroblocks, missing borders and partial chroma blocks.
- Color qualities `0, 1, 30, 50, 75, 98, 98.5, 99, 100`, fractional values,
  and direct parameter cases around quality-mapping/quantizer boundaries.
- Opaque, zero-alpha, binary-alpha, sparse-alpha and smooth-alpha pixels;
  nonzero hidden RGB; colored edges with differing alpha in one chroma cell;
  transparent blocks and right/bottom cleanup leftovers.
- Alpha qualities `0, 4, 5, 10, 30, 69, 70, 71, 99, 100`; constant/two-level
  alpha and more levels than requested; filter-estimator color-count thresholds
  16/17 and 192/193, compressed/raw and equal-size trial boundaries.
- Solid fields, gradients, impulses, diagonal edges, fine text/checkerboards,
  noise, and separate smooth/textured regions to exercise segment clustering,
  equivalent-segment merging, Intra16/Intra4 selection and texture distortion.
- Zero/DC-only/high-frequency residuals, each coefficient category and sign,
  long zero runs, all block types, coefficient-context row boundaries, and
  macroblocks whose coding state has `mb.skip=1` without skip syntax.
- Frames on either side of the 96-macroblock countdown boundary and larger
  frames where the refresh interval grows; forced partition-0 header-budget
  retries with recorded restart state. Use direct subgraph tests if public
  inputs large enough to force a retry are unsuitable for routine tests.
- Padded strides, unaligned slice starts, exact minimum buffers, short buffers,
  invalid dimensions/strides, overflow attempts, finite quality endpoints,
  NaN/infinities and out-of-range color/alpha qualities. Test dimension limits
  with skinny images; test file/partition limits through synthetic sizes.

Finish when every reachable default-preset VP8 function and both alpha quality
paths above are ported, public outputs match the pinned reference over the
agreed corpus and seeded comparisons, and independent decoding validates the
results. Require standard/custom-alpha-100 byte identity; alpha quality must
not alter opaque output. Keep all completed lossless and near-lossless behavior
and tests passing.

Run formatting, relevant portable and opt-in native-oracle tests, public input
validation in debug/release, and a normal library build proving no production
C/Wasm dependency. Record encoded sizes, decode quality and encode-time/memory
baselines. SIMD performance parity is a subsequent target; the initial scalar
port need not match the old full Wasm's speed.

Deferred: other presets/methods, trellis, the low-memory/direct `VP8EncLoop`
path and skip-probability fitting, multiple coefficient partitions, target-size
or PSNR search, user-configured repeated passes, automatic SSIM filter search,
SharpYUV, dithering, segment smoothing, caller-provided YUV/ARGB APIs, public
advanced config/callback/statistics APIs, worker threads, SIMD, decoding beyond
the encoder-required inverse WHT, animation, metadata muxing, file loaders,
negative strides, and C ABI compatibility. Default-path header-budget retries
and deterministic filter-strength adjustment remain required.

### Earlier extension: near-lossless VP8L

`WebPEncodeRGBAWithNearLossless` and configurable `encode_near_lossless_rgba`
are complete, following the dedicated
[near-lossless plan](NEAR_LOSSLESS_PORT_PLAN.md). They extend the completed VP8L
pipeline with standalone preprocessing and reconstructed predictor quantization,
selected per transform candidate. Public output matches the pin, strength 100
matches the simple lossless API, and lower strengths preserve alpha endpoints.
This does not depend on the separate ALPH stream or supply ALPH's centroid
quantizer. Preserve this behavior while adding VP8 and ALPH.
