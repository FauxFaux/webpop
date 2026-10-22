# Lossy WebP encoder: phases and a partial Rust implementation

This document maps the lossy encoder in this libwebp checkout to the code in
`../image-webp`, to support choosing small, independently useful improvements to
a pure Rust encoder. It describes still images; VP8 interframes, motion search,
and animation encoding are outside this path.

Sources inspected:

- **libwebp:** `a1d89ff209ca01e7a87aca64317201890bac2749` (version 1.6.0).
- **image-webp:** `f4d80bd965df2c81e65b6f43c1f70e0750bd4b0f` (Cargo version
  0.2.4, with changes beyond that release).
- **Upstream tracker, checked 2026-10-03:** all-state issue and PR lists through
  `gh` in `../image-webp`, selected descriptions/discussions, and the isolated
  diffs of the lossy feature stack. GitHub's `main` still points to the Rust
  revision above. PR status below is a snapshot, not a promise about future merges.
- The public format references are [RFC 6386, VP8 Data Format and Decoding
  Guide](https://www.rfc-editor.org/rfc/rfc6386.html) and [RFC 9649, WebP Image
  Format](https://www.rfc-editor.org/rfc/rfc9649.html). The local
  [container specification](doc/webp-container-spec.txt) covers WebP framing and
  alpha. RFC 6386's relevant sections are 7–14 for coding and reconstruction and
  15 for filtering; its reference decoder and published errata should accompany
  the prose when implementing syntax.

Function names below refer to these revisions. Relative links resolve from this
document, including links into the sibling Rust checkout. C internals are
algorithm references, not a proposed runtime dependency. Recommendations are
engineering judgments from the source, not measured compression gains.

## The Rust checkout already contains a partial lossy encoder

[WebPEncoder::encode](../image-webp/src/encoder.rs) dispatches to
`encode_frame_lossy` when `EncoderParams.use_lossy` is true. The parameters in
[lossless/encoder/mod.rs](../image-webp/src/lossless/encoder/mod.rs) include
`lossy_quality` (default 95); default encoding remains lossless. The constructor
documentation's lossless-only claim is stale for this revision. Inspection of
the local `v0.2.4` tag confirms it predates the lossy encoder; the merged basic
encoder PR and later alpha work are described in the upstream section below.

The existing [lossy/encoder.rs](../image-webp/src/lossy/encoder.rs) implements RGB
conversion, intra prediction, forward transforms, scalar quantization, local
reconstruction, coefficient tokens, boolean arithmetic coding, and VP8 frame
headers. The outer encoder supports `VP8 ` chunks, metadata, and a losslessly
compressed `ALPH` chunk. It has no content-dependent mode search:
`Vp8Encoder::choose_macroblock_info` always selects 16×16 luma DC prediction,
chroma DC prediction, no segmentation, and no coefficient skipping.

The opportunity in this pair of checkouts is therefore to repair and extend an
existing partial VP8 encoder. Much of the mandatory machinery has already been
written; the missing C features mostly improve quality, compression, or speed.

## What changes compared with the lossless encoder?

Lossy WebP stores a VP8 keyframe; lossless WebP stores a VP8L stream. These are
different bitstreams. Quantizing pixels or predictor residuals before the
existing VP8L Huffman writer will not produce VP8 lossy WebP.

The C branch point is [WebPEncode](src/enc/webp_enc.c): the lossless branch calls
`VP8LEncodeImage` in [vp8l_enc.c](src/enc/vp8l_enc.c); the lossy branch constructs
a `VP8Encoder` and follows the phases below.

| Concern | Existing Rust VP8L path | VP8 lossy path |
| --- | --- | --- |
| Pixel representation | Expand input to RGBA; preserve samples | Convert color to Y/U/V with 4:2:0 chroma subsampling; handle alpha separately |
| Prediction | Reversible subtract-green and optional predictor transform | Predict 16×16 or 4×4 luma and 8×8 chroma from reconstructed neighboring samples |
| Residual representation | Pixel differences modulo 256 | Signed prediction differences transformed into frequency coefficients |
| Deliberate information loss | None | Chroma subsampling and coefficient quantization; color conversion also rounds |
| Encoder feedback | Lossless transforms are reversible | Dequantize and inverse-transform encoded coefficients to reconstruct prediction boundaries |
| Entropy coding | Huffman codes and repeated-pixel runs | Context-dependent coefficient token trees encoded by a boolean arithmetic coder |
| Display smoothing | None required by VP8L | Optional signaled VP8 deblocking filter |
| Container payload | `VP8L` | `VP8 `, plus `VP8X`/`ALPH` when transparency is present |

This table describes this Rust VP8L encoder, not every possible VP8L encoder.
Its [encode_frame_lossless](../image-webp/src/lossless/encoder/mod.rs) uses
subtract-green, a fixed predictor when enabled, frequency collection, Huffman
trees, and `count_run`/`write_run`. It disables color-cache and meta-Huffman
features. C's VP8L path has additional transform and backward-reference searches
that also remain independent of VP8 encoding.

## Overall data flow

```text
RGB / RGBA / grayscale input
  │
  ├── alpha extraction ── lossless alpha stream or raw alpha ── ALPH
  │
  └── YUV420 conversion and edge extension
        │
        ├── optional image analysis → segment map / quantizer settings
        │
        └── raster-order macroblocks
              prediction candidates
                    ↓
              source − prediction
                    ↓
              4×4 transform (+ Y2 Walsh-Hadamard for 16×16 luma)
                    ↓
              quantized coefficient levels
                    ├── dequantize → inverse transform → add prediction
                    │                   ├── distortion for mode selection
                    │                   └── chosen unfiltered boundaries
                    │                         feed subsequent predictions
                    └── chosen modes + coefficient tokens / statistics
                              ↓
              optional probability fitting / repeated passes
                              ↓
              arithmetic-coded header and coefficient partitions
                              ↓
              VP8 frame + RIFF/WebP container (+ ALPH)
```

This is a dependency graph, not a series of whole-image transforms. Prediction,
transform, quantization, reconstruction, and candidate scoring are interleaved
within each macroblock. Statistical passes may repeat that loop.

The C coordinator is `WebPEncode` → `VP8EncAnalyze` → `VP8EncStartAlpha` →
`VP8EncLoop` **or** `VP8EncTokenLoop` → `VP8EncFinishAlpha` → `VP8EncWrite`.
`MapConfigToTools` in [webp_enc.c](src/enc/webp_enc.c) selects the optimization
level: methods 0–2 avoid full rate-distortion optimization, 3–4 use basic
optimization, 5 adds final trellis quantization, and 6 uses trellis during
candidate evaluation. Token buffering is enabled for methods ≥3 when permitted
by build and memory settings. A partial port need not reproduce all methods.

## Phase 1: color conversion and macroblock geometry

**Input/output:** packed pixels → full-resolution Y, half-resolution U/V, and
an optional full-resolution alpha plane. A macroblock covers 16×16 Y samples
and 8×8 samples of each chroma plane. Dimensions need not be multiples of 16;
the final macroblocks are extended beyond the visible rectangle.

**C references:** `WebPEncode` calls `WebPPictureARGBToYUVADithered` or
`WebPPictureSharpARGBToYUVA` in
[picture_csp_enc.c](src/enc/picture_csp_enc.c). `ImportYUVAFromRGBA` handles
conversion and incomplete rows; the standard path can weight chroma averaging
by alpha. `PreprocessARGB` calls `SharpYuvConvert` in
[sharpyuv.c](sharpyuv/sharpyuv.c) for the optional iterative conversion.
`VP8IteratorImport` and `ImportBlock` in
[iterator_enc.c](src/enc/iterator_enc.c) replicate final samples into incomplete
macroblocks. `WebPCleanupTransparentArea` in
[picture_tools_enc.c](src/enc/picture_tools_enc.c) is an optional cleanup when
`config.exact` is false.

**Rust references:** `convert_image_yuv` and `convert_image_y` in
[yuv.rs](../image-webp/src/lossy/yuv.rs), invoked by `Vp8Encoder::encode_image`.
They allocate planes rounded to macroblock dimensions.

**Partial implementation:** basic integer conversion and box chroma averaging
are sufficient. SharpYUV, dithering, and transparent-color cleanup are optional.
Correct odd-edge conversion and deliberate padding are necessary for useful
output. Chroma subsampling already loses color detail, including at quality
100; lossy quality 100 does not mean lossless RGB preservation.

## Phase 2: image analysis and segment assignment

**Input/output:** source macroblocks → coarse prediction choices and a map of
up to four coding segments. Each segment can have its own quantizer and filter
strength. Here, “segment” is a macroblock category, not a separate image or an
independently decodable partition.

**C references:** [analysis_enc.c](src/enc/analysis_enc.c): `VP8EncAnalyze`,
`MBAnalyze`, `FastMBAnalyze`, `MBAnalyzeBestIntra16Mode`,
`MBAnalyzeBestUVMode`, `AssignSegments`, `SmoothSegmentMap`, and
`SetSegmentAlphas`. Residual-transform histograms estimate susceptibility to
quantization; `AssignSegments` clusters the resulting scores with a small
k-means procedure. Analysis uses source boundaries as a provisional estimate,
not final quantized reconstruction. Later coding can revise the modes.

The variables named `alpha` and `beta` in this analysis describe coding
susceptibility/complexity. They are unrelated to transparency.

**Rust status:** `segments_enabled` is false. `encode_segment_updates` and
segment-ID emission in `write_macroblock_header` contain `todo!()` branches.

**Partial implementation:** omit the analysis pass and disable segmentation.
One global quantizer is legal. Add segments later if measurements show a useful
benefit; doing so also requires header syntax, map probabilities, and consistent
per-segment quantization throughout reconstruction and token generation.

## Phase 3: choose quantizers and encoder cost parameters

**Input/output:** quality and optional segment statistics → the base quantizer,
DC/AC step sizes for Y, Y2, and UV, and search weights. VP8 defines the
dequantization tables and signaled deltas. The encoder chooses which levels to
emit and how the user's quality number maps to those settings.

**C references:** [quant_enc.c](src/enc/quant_enc.c): `VP8SetSegmentParams`,
`QualityToCompression`, `QualityToJPEGCompression`, `SetupMatrices`,
`ExpandMatrix`, and `SetupFilterStrength`. C uses a nonlinear quality mapping,
optional spatial noise shaping (`sns_strength`), chroma deltas, and quantizer
dependent rate-distortion weights. `ExpandMatrix` prepares reciprocal steps,
biases, zero thresholds, and luma sharpening terms. `SimplifySegments` merges
equivalent segment settings.

**Rust status:** `Vp8Encoder::setup_encoding` uses
`q = 127 - quality * 127 / 100`, zero deltas, and tables `DC_QUANT`/`AC_QUANT` in
[common.rs](../image-webp/src/lossy/common.rs). All blocks use segment zero.

**Partial implementation:** a single global quantizer and zero deltas work.
The Rust and C quality numbers are not equivalent. Exposing the quantizer index
directly would help comparisons. Port the nonlinear mapping separately from
segmentation, sharpening, or perceptual tuning. Decoder-consistent step sizes
are mandatory regardless of the mapping.

## Phase 4: intra prediction and mode selection

**Input/output:** current source block plus previously reconstructed borders →
a prediction mode, residual coefficients, and reconstruction for the chosen
candidate. Luma can use one of four 16×16 predictors (DC, vertical, horizontal,
true-motion) or sixteen 4×4 predictions, each chosen from ten modes. Chroma uses
one shared U/V mode from four 8×8 predictors. DC averages available borders;
true-motion extrapolates `left + top - top_left`, with clipping.

**C references:** `VP8Decimate` in [quant_enc.c](src/enc/quant_enc.c) orchestrates
`PickBestIntra16`, `PickBestIntra4`, `PickBestUV`, or the cheaper
`RefineUsingDistortion`. Prediction helpers are `VP8MakeLuma16Preds`,
`VP8MakeChroma8Preds`, and `MakeIntra4Preds`. Scalar DSP implementations are
`Intra16Preds_C`, `IntraChromaPreds_C`, and `Intra4Preds_C` in
[dsp/enc.c](src/dsp/enc.c).

C's `SetRDScore` compares distortion plus estimated coefficient and header
costs, schematically `D + λ * (R + H)`, with fixed-point scaling and optional
texture distortion. [cost_enc.c](src/enc/cost_enc.c) provides
`VP8GetCostLuma16`, `VP8GetCostLuma4`, and `VP8GetCostUV`;
[tree_enc.c](src/enc/tree_enc.c) supplies mode coding costs. An Intra4 mode costs
more header bits and changes neighboring mode contexts, so smallest residual
error alone need not produce the smallest file.

**Rust status:** all predictors already exist in
[prediction.rs](../image-webp/src/lossy/prediction.rs).
`get_predicted_luma_block_16x16`, `get_predicted_chroma_block`,
`transform_luma_blocks_4x4`, and `write_macroblock_header` contain the machinery
for additional modes, but `choose_macroblock_info` never selects them.

**Partial implementation:** start by evaluating four Intra16 modes and four UV
modes, using reconstructed error or a simple residual estimate. Add coefficient
bit-cost estimates next. Candidate trials must use scratch state: committing a
trial's borders, coefficient contexts, or arithmetic-coded header before a
winner is selected corrupts subsequent candidates. Intra4 requires sequential
reconstruction within the macroblock and more context bookkeeping; defer it
until Intra16 search works.

## Phase 5: residual transforms and quantization

**Input/output:** signed `source - predictor` samples → quantized coefficient
levels. Each 4×4 residual block receives an integer DCT-like transform.
For Intra16, the sixteen luma DC coefficients are collected into a second 4×4
block, transformed with a Walsh-Hadamard transform (Y2), and coded separately;
the corresponding Y blocks then code AC coefficients only. Intra4 codes each
luma block's DC directly and has no Y2 block. UV always codes DC and AC directly.

**C references:** `ReconstructIntra16`, `ReconstructIntra4`, and `ReconstructUV`
in [quant_enc.c](src/enc/quant_enc.c); `FTransform_C`, `FTransformWHT_C`, and
`QuantizeBlock_C` in [dsp/enc.c](src/dsp/enc.c). DSP function pointers such as
`VP8FTransform` select these scalar routines or architecture-specific versions.
`QuantizeBlock_C` applies thresholds/biases/sharpening, writes signed levels in
zigzag order, and replaces working coefficients with dequantized values.

`TrellisQuantizeBlock` optionally searches coefficient level choices using
distortion and token costs, including the cost of ending a block. Chroma
`CorrectDCValues`/`StoreDiffusionErrors` optionally diffuse DC quantization error
to neighboring blocks. Neither is required to produce a valid stream.

**Rust status:** [transform.rs](../image-webp/src/lossy/transform.rs) has
`dct4x4` and `wht4x4`. `get_luma_blocks_from_predicted_16x16` and
`get_chroma_blocks_from_predicted` subtract predictions and transform blocks.
The coefficient helpers and `encode_coefficients` quantize by signed integer
division, truncating toward zero. Quantization is calculated once for local
reconstruction and again for output tokens.

**Partial implementation:** keep simple scalar quantization first. To change
rounding or add C-like biases, calculate one canonical set of quantized levels
and use it for both reconstruction and tokens. Changing only the writer or only
the reconstruction path introduces prediction drift. Forward transforms can be
an encoder choice, but their scaling must suit VP8's prescribed inverse
operations. Matching C's rounding is useful for differential tests; reproducing
every C forward coefficient is not a format requirement.

## Phase 6: local reconstruction and boundary feedback

**Input/output:** chosen quantized levels plus predictor → the samples the
decoder will reconstruct, before deblocking. Multiply levels by the decoder's
steps, inverse-WHT Y2 when present, inverse-transform residual blocks, add the
prediction, and clip to byte range. Save bottom and right edges for subsequent
predictions. Within Intra4, each subblock's reconstruction feeds later subblocks.

**C references:** the three `Reconstruct*` routines above;
`VP8TransformWHT` in [dsp/dec.c](src/dsp/dec.c), `ITransform_C` in
[dsp/enc.c](src/dsp/enc.c), and `VP8IteratorSaveBoundary`,
`VP8IteratorStartI4`, and `VP8IteratorRotateI4` in
[iterator_enc.c](src/enc/iterator_enc.c).

**Rust references:** `get_dequantized_blocks_from_coeffs_luma_16x16`,
`get_dequantized_blocks_from_coeffs_chroma`, `transform_luma_block`,
`transform_luma_blocks_4x4`, and `transform_chroma_blocks` in the encoder;
`idct4x4`/`iwht4x4` in the transform module; `add_residue` in the prediction
module. These reuse inverse operations from the Rust decoder.

**Partial implementation:** this feedback cannot be omitted when predicting
from neighboring decoded pixels. Source borders or floating-point approximate
inverse transforms do not provide the same feedback as the decoder. Even a
macroblock with all-zero residuals must reconstruct its predictor and update
borders. Only the chosen candidate's state is committed.

For this keyframe path, prediction uses **unfiltered** samples. The Rust decoder's
`Vp8Decoder::decode_frame_` in [lossy/mod.rs](../image-webp/src/lossy/mod.rs)
reconstructs the image before its separate filtering sweep. Filtering those
saved encoder borders would therefore be incorrect.

## Phase 7: coefficient statistics, probability fitting, and rate control

**Input/output:** chosen modes and levels → coefficient branch counts, optional
updated probabilities, skip decisions, and possibly a revised quality setting.

**C references:** [frame_enc.c](src/enc/frame_enc.c): `StatLoop`, `OneStatPass`,
`RecordResiduals`, `FinalizeTokenProbas`, `FinalizeSkipProba`, `InitPassStats`,
and `ComputeNextQ`. Updates are retained only when estimated token savings
exceed the cost of signaling the new probabilities. `VP8CalculateLevelCosts`
in [cost_enc.c](src/enc/cost_enc.c) updates rate estimates used by mode search.
Target byte size or PSNR can trigger repeated passes with different quality.

The direct writer path, `VP8EncLoop`, collects statistics and then encodes
residuals. The alternative `VP8EncTokenLoop` records symbolic tokens, fits
probabilities, and emits them afterward using `VP8EmitTokens` in
[token_enc.c](src/enc/token_enc.c). This avoids repeating all transforms solely
to apply the final probability table, at the expense of token-buffer memory.

**Rust status:** `token_probs` uses `COEFF_PROBS`, and
`encode_updated_token_probabilities` writes every update flag as false.
`macroblock_no_skip_coeff` is `None`, so residuals are always encoded, including
EOB tokens for zero blocks. There is no size or PSNR search.

**Partial implementation:** default probabilities and explicit zero-block
tokens are valid. Probability fitting is a useful isolated size optimization:
retain levels/tokens or replay the macroblock pass, write chosen updates into
partition zero, and encode all tokens with those same final probabilities.
Updating probabilities silently during output is invalid.

Coefficient skipping can be added independently, but it needs the skip header,
per-macroblock bits, correct Y2/context resets, and reconstruction of zero-residual
blocks. The Rust skipped-block branch currently clears contexts without updating
prediction borders; enabling skip requires completing that path first.

For an archive byte budget, an outer loop can encode several quantizers and keep
the best acceptable result under the budget, including container/alpha/metadata
bytes. C's internal rate controller is optional. Image-dependent mode changes
mean file size should be measured rather than assumed strictly monotonic.

## Phase 8: select deblocking parameters

**Input/output:** quantization and optional reconstructed-image statistics →
filter type, strength, sharpness, and optional segment/mode adjustments. These
parameters tell the decoder how to smooth block edges; the encoder does not
write filtered pixel values into the stream.

**C references:** `SetupFilterStrength` in [quant_enc.c](src/enc/quant_enc.c);
`VP8InitFilter`, `VP8StoreFilterStats`, `DoFilter`, and
`VP8AdjustFilterStrength` in [filter_enc.c](src/enc/filter_enc.c).
Automatic filtering trials score candidate levels using SSIM on reconstructed
blocks. The simpler path derives strength from quantization and recorded edges.
`PutFilterHeader` in [syntax_enc.c](src/enc/syntax_enc.c) signals the result.

**Rust status:** `setup_encoding` fixes normal filtering at level 63 and
sharpness 7, irrespective of quality. `encode_loop_filter_adjustments` is a
disabled `todo!()` branch. Decoder filtering already exists in
[loop_filter.rs](../image-webp/src/lossy/loop_filter.rs), with orchestration in
`Vp8Decoder::loop_filter` and `calculate_filter_parameters`.

**Partial implementation:** filter level zero is valid and avoids needing
encoder-side filter simulation. A quantizer-dependent heuristic is another small
step. Evaluate decoded pixels when measuring displayed quality: distortion on
unfiltered reconstruction misses the signaled filter's effect. Port automatic
filter trials later if they improve the intended image corpus.

## Phase 9: entropy coding and VP8 partitions

**Input/output:** chosen modes and quantized levels → binary-coded header and
coefficient partitions. Coefficients use zigzag ordering, frequency bands,
block type (Y2, Y AC, Y with DC, UV), and neighboring nonzero-block contexts.
Tokens distinguish EOB, zero, small magnitudes, larger magnitude categories,
extra magnitude bits, and sign. Their binary decisions go through VP8's boolean
arithmetic coder, rather than VP8L Huffman coding.

**C references:** `PutCoeffs`, `CodeResiduals`, `RecordResiduals`, and
`ResetAfterSkip` in [frame_enc.c](src/enc/frame_enc.c);
`VP8RecordCoeffTokens`/`VP8EmitTokens` in [token_enc.c](src/enc/token_enc.c);
`VP8CodeIntraModes`/`VP8WriteProbas` in [tree_enc.c](src/enc/tree_enc.c).
`VP8PutBit`, `Flush`, and `VP8BitWriterFinish` in
[bit_writer_utils.c](src/utils/bit_writer_utils.c) implement probability splits,
renormalization, carry propagation, and termination.

**Rust references:** `write_macroblock_header`, `encode_residual_data`, and
`encode_coefficients`, plus `ArithmeticEncoder::write_bool`,
`write_with_tree_start_index`, and `flush_and_get_buffer` in
[arithmetic_encoder.rs](../image-webp/src/lossy/arithmetic_encoder.rs).
The decoder's corresponding routines are `read_macroblock_header`,
`read_residual_data`, and `read_coefficients`.

Partition zero contains the compressed frame header and macroblock modes. One,
two, four, or eight other partitions carry coefficients, with macroblock rows
assigned modulo the coefficient partition count. The Rust encoder always uses
one coefficient partition. Thus “one partition” in encoder configuration still
means a header partition **plus** a coefficient partition.

**Partial implementation:** retain the existing one-coefficient-partition coder.
Arithmetic coding, EOB handling, context updates, and carry termination are
mandatory even for DC-only prediction. Larger partition counts are optional.

## Phase 10: alpha and final WebP framing

**Input/output:** finished VP8 payload and optional alpha/metadata → RIFF/WebP
file. Alpha does not pass through YUV conversion or VP8's coefficient quantizer.
It has an independent full-resolution `ALPH` payload.

**C references:** [alpha_enc.c](src/enc/alpha_enc.c): `VP8EncStartAlpha`,
`EncodeAlpha`, `ApplyFiltersAndEncode`, `EncodeAlphaInternal`, `EncodeLossless`,
and `VP8EncFinishAlpha`. The lossless alpha helper places alpha in the green
channel and calls `VP8LEncodeStream`. It can try reversible alpha predictors and
fall back to raw bytes; optional `alpha_quality < 100` preprocessing reduces
alpha levels. Preserve exact alpha for archiving unless explicitly choosing
otherwise.

[syntax_enc.c](src/enc/syntax_enc.c): `VP8EncWrite`, `GeneratePartition0`,
`PutQuant`, `PutSegmentHeader`, `PutVP8FrameHeader`, `EmitPartitionsSize`, and
`PutWebPHeaders` assemble the result. Frame syntax includes the keyframe tag,
signature, dimensions, header-partition length, and coefficient partitions.
WebP syntax adds `RIFF`/`WEBP`, `VP8 `, optional `VP8X`/`ALPH`, and padding.

**Rust status:** the VP8 encoder has `encode_compressed_frame_header`,
`write_uncompressed_frame_header`, and `write_partitions`. The outer
[encoder.rs](../image-webp/src/encoder.rs) has `write_chunk` and
`encode_alpha_lossless`. The alpha helper calls `encode_frame_lossless` with
`implicit_dimensions = true`: an ALPH compressed stream omits the normal VP8L
image header. Its decoded green samples provide alpha; this is a concrete place
to reuse the existing Rust lossless encoder.

**Partial implementation:** reuse the Rust container and alpha path. Opaque
images need only basic RIFF and `VP8 ` framing. Lossy alpha optimization and alpha
filter search can be deferred. Metadata handling remains container work, separate
from the codec algorithm.

## Existing Rust issues to resolve before treating the baseline as usable

These observations constrain a partial port; they are not a complete correctness
or security audit. Unless stated otherwise, they come from inspecting the code,
not end-to-end encoder tests.

| Observation | Code evidence and implication |
| --- | --- |
| Odd RGB/RGBA edges are dropped | `convert_image_yuv` in [yuv.rs](../image-webp/src/lossy/yuv.rs) processes only complete pairs of rows and pairs of pixels via `chunks_exact`. The final odd row/column remains zero. A standalone call to this function with all-white 1×1, 3×2, and 2×3 RGB images confirmed that the last visible Y sample is zero; 2×2 produces Y=235. |
| Padded RGB planes contain zeros | The same function zero-initializes planes and never extends edges. The 2×2 probe also confirmed zero Y immediately beyond the visible row. Padding can influence visible transformed pixels; compare C's `ImportBlock` replication. |
| Grayscale conversion uses different range conventions | `convert_image_y` copies grayscale directly into Y and fills UV with 127. RGB conversion maps into limited-range Y and neutral chroma 128. Verify grayscale output against equal-channel RGB and both decoders. |
| Low-quality UV DC steps disagree with decoding | `setup_encoding` assigns `uvdc = DC_QUANT[q]` without a cap. `Vp8Decoder::read_quantization_indices` in [lossy/mod.rs](../image-webp/src/lossy/mod.rs) caps UV DC at 132; C `SetupMatrices` caps its UV DC table index at 117. For the coarse quantizers whose DC table exceeds 132, encoder reconstruction and decoder reconstruction therefore use different steps. |
| Dimensions are narrowed, not validated for VP8 | `encode_frame_lossy` checks conversion to `u16`; `write_uncompressed_frame_header` masks each dimension with `0x3fff`. Unlike the C limit of 16383, this admits zero and oversized VP8 dimensions. Validate before conversion/allocation and before writing. VP8L's separate maximum of 16384 must not be reused here. |
| Header-partition overflow is unchecked | `write_uncompressed_frame_header` shifts the partition length into a three-byte tag without checking the 19-bit limit. C's `PutVP8FrameHeader` rejects lengths ≥2^19. Check serialized sizes, including RIFF arithmetic, before narrowing. |
| Arithmetic-coder carry drops saturated bytes | `ArithmeticEncoder::add_one_to_output` in [arithmetic_encoder.rs](../image-webp/src/lossy/arithmetic_encoder.rs) pops trailing `0xff` bytes instead of retaining them as zeros. A standalone probe of the actual helper confirmed `[0, 10, 255, 255, 255]` becomes `[0, 11]` instead of `[0, 11, 0, 0, 0]`. Upstream [PR #192](https://github.com/image-rs/image-webp/pull/192) proposes the isolated fix. This demonstrates the helper defect, not reachability from a particular image. |
| Nonzero signed header values reverse sign | `write_optional_signed_value` sets the sign bit when `value >= 0`; `ArithmeticDecoder::read_optional_signed_value` in [arithmetic_decoder.rs](../image-webp/src/lossy/arithmetic_decoder.rs) treats that bit as negative. A standalone component roundtrip using these actual modules decoded `Some(1)` as −1 and `Some(-1)` as +1. This is dormant while quantizer deltas stay absent and filter adjustments stay disabled; fix it before adding either feature. |
| Additional coefficient partitions are unfinished | `write_partitions` interleaves each size with its partition's data; C's `EmitPartitionsSize` emits the entire size table before any coefficient data. The Rust path is currently one coefficient partition, so this branch is dormant. |
| Enabling optional features requires completing more than selection | Segmentation and filter-adjustment syntax contain `todo!()`. Skip handling lacks reconstruction/border updates. Several reconstruction helpers hardcode segment zero. These branches are harmless only while those features remain disabled. |
| Current interoperability tests do not establish lossy correctness | The `roundtrip_libwebp` cases in [encoder.rs](../image-webp/src/encoder.rs) use default lossless parameters or toggle the lossless predictor. Existing arithmetic/transform tests verify components, not complete lossy images. |

The Rust forward-transform formulas also differ from C's scalar rounding in
places. That alone is not proof of an invalid encoder; forward transforms and
level selection need not be bit-identical to C. Decoder-consistent inverse
reconstruction and independent decoded output are the relevant checks.

## What the upstream issues and PRs add

The tracker supplies both implementation candidates and evidence about review
and test gaps. The commands used were `gh issue list --state all --limit 200`,
`gh pr list --state all --limit 200`, selected `gh issue view`/`gh pr view`
calls including comments, `gh pr diff`, and read-only `gh api` queries for
`main`, tags, and commit comparisons. No branch was checked out or merged.

### Merged baseline versus proposed extensions

| Upstream item | Status at inspection | Relevance to this document |
| --- | --- | --- |
| [PR #155: original lossy encoder](https://github.com/image-rs/image-webp/pull/155) | Closed, unmerged | Original larger implementation included prediction mode choice. Maintainers asked for smaller independent changes. It is another historical Rust reference, not the implementation currently on `main`. |
| [PR #161: basic lossy encoder](https://github.com/image-rs/image-webp/pull/161) | Merged 2025-10-19 | Its description explicitly says DC/DC prediction was chosen to simplify the initial merge. The present fixed-mode behavior is a deliberate restricted baseline. |
| [PR #164: lossy alpha](https://github.com/image-rs/image-webp/pull/164), [PR #170: compressed alpha](https://github.com/image-rs/image-webp/pull/170) | Merged October and December 2025 | Explain why the current Rust container/VP8L alpha path is reusable. Alpha compression is already implemented, not missing lossy-encoder work. |
| [PR #187: integration fixes](https://github.com/image-rs/image-webp/pull/187) | Closed, unmerged | Fixes carry handling, odd edges/padding, header-partition overflow, invalid quality handling, and other encoder/decoder issues; adds targeted tests. Several overlap the source findings above. |
| [PR #188: mode selection](https://github.com/image-rs/image-webp/pull/188) | Closed, unmerged; stacked on #187 | Adds Intra16/UV candidate search and then greedy Intra4 search. A concrete Rust candidate for phases 4–6. |
| [PR #189: coefficient skipping](https://github.com/image-rs/image-webp/pull/189) | Closed, unmerged; stacked on #188 | Adds emptiness detection, skip probability/cost decisions, context handling, and header emission after whole-frame decisions. A candidate for phase 7. |
| [PR #190: quantization/quality/filter tuning](https://github.com/image-rs/image-webp/pull/190) | Closed, unmerged; stacked on #189 | Adds shared quantization rounding, a nonlinear quality table, and quantizer-dependent filtering. Candidates for phases 3, 5, and 8. |
| [PR #192: carry propagation](https://github.com/image-rs/image-webp/pull/192) | Open, unmerged; based on `f4d80bd` | A small independent alternative to the carry-fix portion of #187. |

The public review discussion on [#187](https://github.com/image-rs/image-webp/pull/187)
and [#188](https://github.com/image-rs/image-webp/pull/188) cites submission policy,
human-written descriptions, and the scope of the large stacked changes. These
closures do **not** establish that the encoding algorithms were disproved or
that the submitted tuning has been technically validated by maintainers. They
favor small independent fixes and feature slices for any eventual upstream work.

The lossy encoder was merged after [PR #157, the v0.2.4
release](https://github.com/image-rs/image-webp/pull/157). [Issue #183, the bugfix
release discussion](https://github.com/image-rs/image-webp/issues/183), remains
open and discusses shipping a minimal bugfix branch separately from `main`'s
new features. Do not equate the checkout's unchanged Cargo version with the
feature set of the tagged release, or assume a future bugfix release includes
the lossy path. Pin the revision when assessing this encoder.

### Concrete Rust code available in the closed feature stack

The following links isolate each PR's own changes; its normal Files tab includes
earlier stacked commits. Names in this subsection belong to those proposed
revisions, not necessarily the checked-out `main`.

- **#187, fixes:** [f4d80bd → 754c5cd](https://github.com/taucad/image-webp/compare/f4d80bd965df2c81e65b6f43c1f70e0750bd4b0f...754c5cd5e247b013c7d068e3664cfd61f1496e71).
  The diff replaces paired `chunks_exact` RGB conversion with clamped sampling
  over the padded planes, extends grayscale padding, and repairs carry handling.
  It also makes `ArithmeticEncoder::default()` match `new()` and propagates the
  lossless bit writer's final write error instead of unwrapping it. Derived
  `Default` currently creates a zero-range coder; this is a latent reuse hazard,
  not evidence that the current final `mem::take` call itself fails, since that
  replacement coder is not subsequently used.
- **#188, search:** [754c5cd → fc7945c](https://github.com/taucad/image-webp/compare/754c5cd5e247b013c7d068e3664cfd61f1496e71...fc7945cd3682702cdc085c477ae234b2d0ab7475).
  `choose_luma_mode`, `choose_chroma_mode`, `choose_bpred_modes`, and
  `score_subblock` reuse prediction/reconstruction helpers to compare
  candidates. `squared_error` supplies distortion; `lambda_for` scales the rate
  weight with the quantizer. `coefficient_rate` is a magnitude/zero-run proxy,
  **not** C's contextual coefficient bit-cost model. Luma/submode tree costs use
  `tree_rate`/`bool_rate`; the inspected UV chooser sums distortion and coefficient
  cost without a UV mode-header cost. Treat this as a starting point for a
  restricted search, with measurable approximation gaps. Its first commit
  `d459605` implements whole-block mode choice; its second `fc7945c` adds Intra4.
  These can be assessed separately rather than importing the entire stack.
- **#189, skip:** [fc7945c → b3dfa10](https://github.com/taucad/image-webp/compare/fc7945cd3682702cdc085c477ae234b2d0ab7475...b3dfa105a5a2f0b95ad83a8109c5b4b9151cc890).
  `macroblock_is_empty` checks Y2/Y/UV after quantization, `skipped_macroblock_cost`
  estimates the EOB costs that can be saved, and `skip_probability` compares
  estimated savings against skip signaling. `advance_bpred_context` shares mode
  context updates between decision and header passes. Importantly, it always
  reconstructs blocks before deciding to skip. `rewrite_coefficient_partitions`
  resets and replays coefficient coding when frame-level skip signaling is not
  worthwhile. Assess the resulting replay cost and retained macroblock data on
  large archival inputs; this is not a free size optimization.
- **#190, tuning:** [b3dfa10 → a95103f](https://github.com/taucad/image-webp/compare/b3dfa105a5a2f0b95ad83a8109c5b4b9151cc890...a95103fc10ab49bc32ecb967b98e6c6486deec58).
  `quantize` shares DC/AC rounding rules across reconstruction, emptiness
  detection, and output coding. `QUALITY_TO_QUANTIZER_INDEX` provides a lookup
  curve; `loop_filter_level` replaces fixed maximum filtering. The proposed
  thresholds, curve, and filter formula are encoder tuning choices, not syntax
  requirements. Their benefit needs measurement on this project's corpus.

The PR descriptions report large improvements on CAD renders and agreement
between Rust and libwebp decoding. Those are submitter reports, not benchmarks
run for this document. They often measure the **whole stack** against lossless or
DC-only output, so they cannot establish the isolated benefit of one phase or
performance against this project's JPEG fallback.

There is also a concrete discrepancy between description and tests: #190's
description claims an all-quality decoding sweep, but the inspected
`every_quality_decodes_in_both_decoders` test iterates only
`[0, 10, 25, 50, 75, 90, 99, 100]`. A claimed external sweep may still have been
run, but the committed test does not provide that coverage. Keep quantizer
thresholds and filter tuning separate enough to evaluate each on its own.

### Corruption reports change the validation priority

[Issue #191](https://github.com/image-rs/image-webp/issues/191) reports a decoded
quality anomaly on Kodak `kodim02` at quality 85, with both decoders returning
the same wrong-looking pixels. The follow-up attributes it to the carry helper,
and [PR #192](https://github.com/image-rs/image-webp/pull/192) proposes the fix.
There is some ambiguity in the reproduction scope: the issue names stock `main`,
while the PR description discusses discovery in a fork with mode selection and
skip signaling. The image-level reproduction and causal claim have not been
independently verified here. The helper's byte loss **has** been reproduced in a
standalone probe, and warrants an isolated fix before expanding encoder traffic.

Decoder agreement is necessary for interoperability but does not prove encoder
fidelity: both decoders can agree on a badly encoded image. Extend validation to
quality sweeps, spatial error maps, and distortion versus the source, especially
when the archiver chooses quality dynamically. A synthetic carry-helper test
also needs an eventual public-encoder regression fixture to establish the
affected reachable path.

Historical decoder issues are useful but must not be carried forward as current
gaps. [Issue #146](https://github.com/image-rs/image-webp/issues/146) attributes
remaining divergences to filter thresholds, zero-coefficient filtering, and
bottom-block geometry; it was closed after [#148](https://github.com/image-rs/image-webp/pull/148)
and [#149](https://github.com/image-rs/image-webp/pull/149). [Issue #142](https://github.com/image-rs/image-webp/issues/142)
was resolved by [#147](https://github.com/image-rs/image-webp/pull/147), which added
default bilinear chroma upsampling. The current
[tests/decode.rs](../image-webp/tests/decode.rs) checks exact reference bytes for
lossy fixtures; the earlier percentage tolerance described in #146 is historical.
These improvements strengthen the Rust decoder as a component oracle, while
reinforcing the need to match upsampling and filter settings in comparisons.

### Gaps the inspected proposals do not close

The #187–#190 diffs do not implement adaptive coefficient probability fitting,
segmentation/SNS, trellis quantization, SharpYUV, or completed multiple-coefficient
partition output. They also do not repair the missing VP8 dimension validation,
the low-quality UV DC cap, or the reversed optional signed-value bit identified
above. Those source findings remain distinct from the improvements in the stack.

Other tracker items matter chiefly to the surrounding validation and archive
pipeline:

- [#21, decoder memory limits](https://github.com/image-rs/image-webp/issues/21),
  is open. `WebPDecoder::set_memory_limit` still documents allocations that
  ignore it. Bound pixel allocations and retained encoder state independently;
  this setting is not an encoder memory budget.
- [#182, malformed animation/ALPH dimensions](https://github.com/image-rs/image-webp/issues/182),
  and open [PR #185](https://github.com/image-rs/image-webp/pull/185)/
  [PR #186](https://github.com/image-rs/image-webp/pull/186) concern decoder checks
  when embedded VP8 dimensions differ from animation frame dimensions. They do
  not supply missing still-image encoding phases, and their fixes are unmerged.
- [#136, loop-filter performance](https://github.com/image-rs/image-webp/issues/136),
  is a decoder bottleneck report. [#120, parallelization](https://github.com/image-rs/image-webp/issues/120),
  mainly concerns animation decoding/compositing. Neither establishes that
  parallelizing a raster-order intra encoder is straightforward or necessary.
- [PR #175](https://github.com/image-rs/image-webp/pull/175) proposes deprecating
  public raw `vp8::{Frame, Vp8Decoder}`. Internal reuse is possible, but avoid
  making the archive integration depend on that API remaining public.
- [#184, an automated audit report](https://github.com/image-rs/image-webp/issues/184),
  was closed with maintainer doubts about its findings, including a documented
  caller-contract panic. Its existence is not evidence that every listed item
  is a bug. The ordinary repository fuzz targets currently cover decoding, not
  encoder roundtrips or dynamic quality selection.

## Suggested implementation slices

“Partial” can mean a restricted **encoder search**, while still emitting complete
VP8 streams that ordinary WebP decoders accept. The following order keeps each
addition testable and avoids implementing all of libwebp at once.

| Slice | Scope | C reference / Rust work | Dependency and expected purpose |
| --- | --- | --- | --- |
| 0. Establish the baseline | Fix carry handling, geometry, range/step consistency, length validation, and dormant signed-value coding; choose deliberate filter settings | `ImportBlock`, `SetupMatrices`, `PutVP8FrameHeader`; Rust arithmetic/conversion/setup/framing; parts of #187 or isolated #192 | Existing DC-only encoder becomes a foundation for measured comparisons |
| 1. Search Intra16 and UV modes | Four luma and four shared chroma candidates; no Intra4 or segments | `PickBestIntra16`, `PickBestUV`, `RefineUsingDistortion`; replace fixed Rust mode choice with scratch candidate evaluation | Uses existing predictors and transforms; can reduce avoidable residuals |
| 2. Improve scalar quantization and quality mapping | One canonical levels buffer, deliberate rounding/bias, optional nonlinear quality mapping | `ExpandMatrix`, `QuantizeBlock_C`, `QualityToCompression` | Reuse levels for output and reconstruction; controls quality/size behavior |
| 3. Fit coefficient probabilities | Record final symbols/counts; signal only worthwhile updates | `FinalizeTokenProbas`, `VP8WriteProbas`, optionally token buffering | Changes entropy size without changing reconstructed samples if levels stay fixed |
| 4. Add basic rate-distortion scoring | Estimate mode and coefficient bits and trade them against reconstructed error | `SetRDScore`, `VP8GetCostLuma16`, `VP8GetCostUV` | Improves candidate decisions; costs must use the active probability tables |
| 5. Add Intra4 search | Compare sixteen subblock choices with Intra16, including mode costs and header-size budget | `PickBestIntra4`, iterator I4 helpers; activate existing Rust B-mode machinery after validation | More useful candidate shapes for edges/textures; larger search and header cost |
| 6. Add independent refinements | Skip syntax, quantizer-aware filter, then segments/SNS, trellis, SharpYUV, alpha predictor trials | Phases above | Choose based on corpus measurements; none is required for the restricted format subset |

Upstream candidates now make several slices concrete: evaluate #188's first
commit for whole-block search, its second for Intra4, #189 for skipping, and
#190's individual commits for quantization/filter tuning. Probability fitting
remains separate work. The submitted stacks should be evaluated rather than
treated as already integrated or benchmarked here.

The first useful subset can keep: one quantizer, Intra16 only, one coefficient
partition, default token probabilities, scalar levels, filtering disabled, and
exact separately encoded alpha. It still needs conversion, transform, correct
local reconstruction, coefficient/context coding, and all required headers.
For this checkout, those mandatory pieces largely exist already.

For a byte-size fallback, try that subset after lossless encoding exceeds the
budget, then select among a small number of lossy quantizers using decoded
quality and actual file bytes. If “too big” instead means pixel dimensions,
lossy WebP does not remove VP8's 16383-per-axis limit; resizing or an alternative
container is a separate decision.

There is also a separate, smaller experiment: preprocess RGB pixels with bounded
precision reduction or a near-lossless algorithm and feed the current VP8L
encoder. C examples are `VP8ApplyNearLossless` in
[near_lossless_enc.c](src/enc/near_lossless_enc.c) and predictor-integrated
near-lossless processing in [predictor_enc.c](src/enc/predictor_enc.c), called
from [vp8l_enc.c](src/enc/vp8l_enc.c). This produces a VP8L file that losslessly
stores the modified pixels. It avoids a VP8 encoder but has different quality and
size behavior; it is an alternative experiment, not a phase of lossy VP8.

## Evidence needed when selecting a slice

The research above was source and tracker based, with standalone Rust probes
for color-conversion edges/padding, the actual carry helper, and signed-value
encoder/decoder roundtrips. No repository encoder was changed, no upstream patch
was applied, and no complete lossy interoperability or rate-distortion benchmark
was run for this document.

For implementation, verify emitted images with the Rust decoder and an
independent VP8/WebP decoder, which can remain a test-only tool. Existing
`webp` dev-dependency use is one possible test harness; it need not enter the
archiver's runtime. Compare unfiltered YUV reconstruction exactly with filtering
disabled, then measure final decoded RGB using consistent chroma upsampling.
Lossy roundtrips should not assert equality with the original RGB input.

Useful cases include 1×1 and odd dimensions, incomplete macroblocks, multiple
macroblock rows, smooth ramps, colored edges, text, all-zero residuals, coarse
quantizers, extreme quality values, and RGBA images with partially transparent
boundaries. Check alpha equality separately. Test header-size limits especially
when enabling Intra4, and propagate output-write failures.

Use a representative archive corpus to compare bytes, encode time, peak memory,
and decoded distortion/perceptual quality against the current lossless result
and JPEG fallback. For RGB input, padded YUV planes alone use roughly 1.5 bytes
per padded pixel; alpha, retained levels/tokens, and output buffers add to this.
Choose features based on those measurements. This document does not claim that
a particular slice will beat the existing JPEG fallback.
