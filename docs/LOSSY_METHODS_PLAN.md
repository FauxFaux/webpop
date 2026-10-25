# libwebpop: remaining scalar lossy methods

## Target and compatibility

Extend the completed [lossy port](LOSSY_PORT_PLAN.md) from method 4 to every
method `0..=6` in the pinned libwebp encoder. This implements the additional
methods item in the [encoder completion inventory](ENCODER_COMPLETION_PLAN.md),
including the direct frame loop and method-dependent alpha work needed for
complete opaque and transparent files. It does not complete the inventory's
other advanced encoder features.

Audited 2026-10-24 at repository revision `227d20e`, against the unchanged,
clean [`upstream/`](../upstream) pin
`1844042d1ca751e62e61df7617dba19c98588339`. Keep that pin. The scalar C build,
with token buffering enabled and SIMD/threading disabled, remains the exact
oracle under a recorded toolchain. The older Wasm artifact is not the algorithm
reference for these methods.

Implementation started at `faefdbc` on 2026-10-24. That revision also adds the
public `LossyOptions`/`encode_lossy_rgba` API and SharpYUV selection for method
4. Extend that existing API with method selection and retain `use_sharp_yuv`;
the initial audit below describes the earlier source snapshot.

The target is complete-file RGBA encoding with caller-selected color quality,
method and alpha quality, using the default preset's remaining settings:

| Setting | Scope |
| --- | --- |
| `quality` | Finite `f32` in `0..=100`, including fractional values |
| `method` | Integer `0..=6`, with 4 remaining the default |
| `alpha_quality` | Integer `0..=100`, with 100 remaining the default |
| `segments`, `sns_strength` | 4, 50 |
| `filter_strength`, `filter_type`, `filter_sharpness`, `autofilter` | 60, 1, 0, 0 |
| `partitions`, `low_memory` | 0, 0: one coefficient partition; normal method-dependent loop selection |
| `pass`, `target_size`, `target_PSNR`, `qmin`, `qmax` | 1, 0, 0, 0, 100 |
| `partition_limit` | 0; retain automatic partition-0 header-budget retries |
| `preprocessing`, `use_sharp_yuv`, `emulate_jpeg_size` | 0, 0, 0 |
| `exact`, `thread_level` | 0, 0: existing transparent-area cleanup and synchronous execution |
| `alpha_compression`, `alpha_filtering` | 1, 1: compressed ALPH trials with fast filtering and raw fallback |
| Statistics, callbacks, reconstruction export | Unset |

The direct loop is required by methods 0–2 even with `low_memory=0`. Port its
single-partition, one-configured-pass behavior here. Public low-memory selection
for methods 3–6, multiple coefficient partitions, repeated passes, rate control,
non-default presets/analysis/filter controls, lossy `exact`, SharpYUV routing,
threading, SIMD and reporting remain separate completion work. Keep internal
state extensible for those tasks without exposing unsupported settings.

## Current repository and reusable foundations

All milestones in the [lossless](LOSSLESS_PORT_PLAN.md),
[near-lossless](NEAR_LOSSLESS_PORT_PLAN.md), original
[lossy](LOSSY_PORT_PLAN.md), and standalone
[SharpYUV](SHARPYUV_PORT_PLAN.md) plans are complete for their stated scopes.
No new conversion library, VP8 bit writer, VP8L compressor or ALPH format is
needed. Preserve their behavior and reference fixtures.

| Rust location | Existing behavior | Additional work |
| --- | --- | --- |
| [`config_enc.rs`](../libwebpop/src/enc/config_enc.rs), [`encode.rs`](../libwebpop/src/encode.rs) | Public `LossyOptions` carries color/alpha quality and SharpYUV selection; private `LossyConfig` now validates and carries method too | Expose method in existing public options after the algorithms compose; finish method propagation into alpha effort |
| [`webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs), [`vp8i_enc.rs`](../libwebpop/src/enc/vp8i_enc.rs) | Checked method-aware construction maps all RD levels, token/direct selection and header budgets; integrated encoding dispatches direct/token frames from the mapped tools | Trellis methods remain guarded until their algorithms land |
| [`analysis_enc.rs`](../libwebpop/src/enc/analysis_enc.rs) | Histogram and fast luma probes, UV analysis, segmentation and synchronous source-border traversal, including one-segment fast prediction initialization | No additional analysis for this scope |
| [`quant_enc.rs`](../libwebpop/src/enc/quant_enc.rs) | Matrices/lambdas, reconstruction, diffusion, basic RD and distortion refinement; non-RD `VP8Decimate` | Selected-mode requantization, trellis and remaining RD dispatch |
| [`frame_enc.rs`](../libwebpop/src/enc/frame_enc.rs) | Direct residual coding/statistics, skip fitting/reset, token recording, refresh/retry loop and deterministic filter finalization | Statistics/direct drivers and method-3 tokens complete; integrate trellis RD levels |
| [`alpha_enc.rs`](../libwebpop/src/enc/alpha_enc.rs) | Alpha quantization, filters, VP8L and raw fallback | Replace fixed effort 4/q32 throughout with method-dependent effort and quality |
| [`syntax_enc.rs`](../libwebpop/src/enc/syntax_enc.rs), [`tree_enc.rs`](../libwebpop/src/enc/tree_enc.rs) | Single-partition containers and probability/mode headers, including composed direct residuals and fitted skip state | No additional syntax for this scope |
| [`cwebpop/src/main.rs`](../cwebpop/src/main.rs) | Parses methods 0–6 but rejects lossy methods other than 4 | Route every valid method to the checked configurable lossy API |

`VP8Decimate` now accepts non-RD and basic RD;
`VP8EncTokenLoopWithObserver` accepts methods 3/4;
`ReconstructIntra16` and `ReconstructIntra4` reject trellis. Removing assertions
alone would produce the wrong encoder. Matrix setup already contains the
method threshold for texture-distortion weighting and the trellis lambdas;
extend its coverage instead of porting it again. `GetFilterMap` already accepts
alpha effort, but its caller fixes it at 4.

## Method behavior at the pin

Derive routing from `MapConfigToTools` in
[`webp_enc.c`](../upstream/src/enc/webp_enc.c), analysis from
[`analysis_enc.c`](../upstream/src/enc/analysis_enc.c), and mode/quantization
behavior from [`quant_enc.c`](../upstream/src/enc/quant_enc.c).

| Method | Luma analysis | Final macroblock decisions | Normal frame loop |
| --- | --- | --- | --- |
| 0 | `FastMBAnalyze` | `RD_OPT_NONE`; retain analyzed Intra16/Intra4 family, refine luma submodes, retain analyzed UV mode | Direct; fast partial statistics probe |
| 1 | `FastMBAnalyze` | `RD_OPT_NONE`; retain analyzed family, refine luma submodes and UV mode | Direct; full statistics pass |
| 2 | Histogram probe | `RD_OPT_NONE`; distortion-based Intra16/Intra4 comparison and UV refinement | Direct; full statistics pass |
| 3 | Histogram probe | `RD_OPT_BASIC`; existing Intra16/Intra4/UV RD search with method-dependent matrices | Buffered tokens |
| 4 | Histogram probe | `RD_OPT_BASIC`; current default behavior | Buffered tokens |
| 5 | Histogram probe | `RD_OPT_TRELLIS`; basic RD mode selection followed by trellis requantization of the selected modes | Buffered tokens |
| 6 | Histogram probe | `RD_OPT_TRELLIS_ALL`; trellis participates in candidate reconstruction during RD search | Buffered tokens |

Important details to preserve:

- Methods 0/1 still refine luma prediction submodes. Their fast analysis fixes
  the initial Intra16/Intra4 family, subject to the pinned border/flat-source
  safeguard; it does not mean copying DC predictions straight to the output.
- `FastMBAnalyze` truncates color quality to an integer for its threshold and
  compares accumulated block means using wide arithmetic and a strict tie rule.
  Histogram-based UV analysis remains active for every method.
- Methods below 4 change the texture-lambda setup, not all spatial noise
  shaping. Segmentation and quantizer adaptation remain active. The pin clamps
  lambdas to at least 1, including the texture lambda; preserve the actual math.
- Normal method 3 uses tokens. `StatLoop`'s method-3 fast probe applies only
  when the direct loop is selected, such as future low-memory mode; it is not
  the normal method-3 frame path. Keep that routing distinction in tests.
- `DO_TRELLIS_I16=1`, `DO_TRELLIS_I4=1`, and `DO_TRELLIS_UV=0` in this pin.
  Intra16 trellis handles luma AC; its WHT/DC quantizer remains scalar. Do not
  enable chroma or WHT/DC trellis as part of methods 5/6.
- Method 5's `SimpleQuantize` rebuilds levels/nonzero masks and reconstructed
  pixels for selected modes after mode selection. Preserve the pin's score,
  context and diffusion mutations; do not rerun mode search or recompute fields
  that upstream leaves unchanged.
- Token buffering keeps skip-probability syntax disabled and records zero
  residuals too. The direct path fits skip probabilities and can omit residuals.
  Files from the two paths need not have identical bytes to each other.

## Public API and configuration boundary

Extend the existing configurable API alongside the convenience functions:

```rust
pub struct LossyOptions {
    pub quality: f32,
    pub method: u32,
    pub alpha_quality: i32,
    pub use_sharp_yuv: bool,
}

pub fn encode_lossy_rgba(
    rgba: &[u8], width: u32, height: u32, stride: usize,
    options: &LossyOptions,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError>;
```

Use defaults `quality=75.0`, `method=4`, `alpha_quality=100`, and
`use_sharp_yuv=false`, matching the default preset and current CLI. Retain the
existing options/function exports consistently
from `encode` and the crate root. Keep `LosslessOptions` separate: lossy color
quality is fractional fidelity, whereas its lossless field is integer effort.

`WebPEncodeRGBA` and `WebPEncodeRGBAWithAlphaQuality` retain their signatures
and use the shared implementation with method 4. The first remains identical
to the second at alpha quality 100. `LossyOptions::default()` must produce
those same bytes at color quality 75. No public C ABI or general config/picture
API is needed for this extension.

Validate color quality, alpha quality and method before allocation, then use
the existing checked positive-stride geometry: dimensions `1..=16383`,
`stride >= width * 4`, and minimum reachable input length
`(height - 1) * stride + width * 4`. Use checked arithmetic. Reuse the existing
invalid-method error and preserve `WebPEncodingError` compatibility. Inputs
remain unchanged; success replaces `output` with one complete WebP file and
reuses capacity, while errors leave its contents unchanged.

Carry one validated method through encoder setup, analysis, the frame loop,
and the alpha job. Represent tool mapping once, with an explicit RD level and
frame-loop selection; avoid scattered inconsistent method tests. Apply the
configuration before analysis and matrix setup, not after default construction
has already run those stages. With pass fixed to 1, diffusion allocation retains
the color-quality `<=98` boundary independently of method.

## Function dependency map

Paths below are under `upstream/src/enc/`. Existing functions are dependencies,
not new ports in their entirety.

```text
encode_lossy_rgba / existing simple wrappers
└── checked options → ordinary RGBA-to-YUVA import → transparent cleanup
    └── WebPEncodeLossy                                      webp_enc.c
        ├── InitVP8Encoder → MapConfigToTools
        ├── VP8EncAnalyze                                   analysis_enc.c
        │   └── MBAnalyze
        │       ├── FastMBAnalyze → VP8Mean16x4              methods 0/1
        │       ├── MBAnalyzeBestIntra16Mode                 methods 2–6
        │       └── MBAnalyzeBestUVMode → AssignSegments     existing
        ├── VP8EncStartAlpha → CompressAlphaJob              alpha_enc.c
        │   └── EncodeAlpha → ApplyFiltersAndEncode
        │       ├── GetFilterMap(..., effort_level)
        │       └── EncodeAlphaInternal → EncodeLossless
        │           └── VP8LEncodeStream(method, quality, exact=1)
        ├── VP8EncLoop                                      methods 0–2
        │   ├── StatLoop → OneStatPass                       frame_enc.c
        │   │   ├── SetLoopParams → existing matrices/probas
        │   │   ├── VP8Decimate(RD_OPT_NONE)
        │   │   ├── RecordResiduals → VP8RecordCoeffs
        │   │   └── FinalizeSkipProba / FinalizeTokenProbas
        │   ├── InitBitWriters
        │   ├── VP8Decimate(selected RD level)
        │   ├── CodeResiduals → PutCoeffs, or ResetAfterSkip
        │   └── PostLoopFinalize → VP8AdjustFilterStrength
        ├── VP8EncTokenLoop                                 methods 3–6
        │   ├── existing refresh / retries / RecordTokens
        │   ├── VP8Decimate(selected RD level)
        │   └── existing final token emission / filter adjustment
        └── existing VP8/ALPH/VP8X/RIFF assembly

VP8Decimate                                                 quant_enc.c
├── RD_OPT_NONE → RefineUsingDistortion
│   ├── prediction SSE / mode costs / IsFlatSource16
│   └── existing reconstruction / Intra4 rotation / UV reconstruction
└── RD_OPT_BASIC / TRELLIS / TRELLIS_ALL
    ├── existing PickBestIntra16 / PickBestIntra4 / PickBestUV
    ├── SimpleQuantize                                      TRELLIS finish
    └── ReconstructIntra16 / ReconstructIntra4
        └── TrellisQuantizeBlock when do_trellis is set
            ├── RDScoreTrellis / kWeightTrellis
            ├── existing band/context level costs and VP8BitCost
            └── Node / ScoreState traversal and traceback
```

The direct statistics/final coding passes and token-loop retries have different
state lifetimes. Share proven leaves, without replacing either driver with the
other. Preserve boundaries, probability fitting and context reset order.

## Implementation order and acceptance gates

Milestones 0–4 are complete; milestones 5–8 remain pending. Integrate callers after
their dependencies pass, retaining the original plan's method-4 coverage at
every stage. Leaf-specific oracle requests are added before the corresponding
Rust ports in subsequent milestones, using the reference foundation below.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 (complete) | Extend the existing oracle with configurable full files, tool mapping and new branch traces; record pre-change method-4 baseline | Existing oracle/pin | 70 portable fixtures, 3,556 independently decoded C files, 672 tool mappings and 336 compact loop traces; method-4 Rust comparisons and wrapper/toolchain provenance recorded |
| 1 (complete) | Validated internal method configuration, tool mapping and propagation into encoder setup; retain public method-4 entry points | 0 | 84 normal-resource setup states match C across all methods; 7 additional portable setup fixtures; invalid signed methods rejected before allocation; method-4 bytes unchanged |
| 2 (complete) | Fast analysis, distortion refinement and non-RD `VP8Decimate` | 1 | 61 portable fixtures and 5,973 live C requests match fast decisions, selected modes, reconstruction, levels, nonzero masks and context/diffusion state for methods 0–2 |
| 3 (complete) | Direct coefficient coding/statistics and skip-probability leaves | 0 | 37 portable fixtures and 4,811 live C requests; exact bytes, writer/counter/statistics state, skip costs and post-skip contexts; independent coefficient decoding and geometric context checks |
| 4 (complete) | Single-pass statistics driver, direct final loop and method-3 token integration | 1–3 | 27 portable fixtures, 1,335 exact C driver traces and 1,384 normal-public-C opaque files; probe boundaries, skip on/off, retries, borders, probabilities and final filter state match |
| 5 | Trellis score/state leaves and `TrellisQuantizeBlock` | 0, existing matrices/costs | Exact levels/dequantized coefficients/masks and traceback; independent bounded path enumeration including terminal/tie cases |
| 6 | Trellis candidate reconstruction, `SimpleQuantize`, all RD dispatch and methods 5/6 token integration | 1, 5 | Methods 5/6 opaque files match normal C; traces distinguish selected-mode versus candidate trellis; method 4 remains identical |
| 7 | Method-dependent alpha effort, quality and filter trials | 1, existing ALPH/VP8L; 4 and 6 for complete files | Exact headerless streams, filter maps, winning payloads and complete transparent files across all methods/alpha qualities |
| 8 | Public `LossyOptions`/`encode_lossy_rgba`, CLI routing, full regression and baseline record | 4, 6, 7 | Checked API and CLI support methods 0–6; portable/live full-file parity, independent decode, validation and benchmark record pass |

### 0. Oracle and baseline

Completed 2026-10-24. [`lossy_methods.rs`](../libwebpop/tests/lossy_methods.rs)
and its shared evaluator/cases extend the existing scalar harness with
`WebPEncodeLossyMethod`, `VP8MethodTools` and `VP8MethodLoopFrame`. Full files
call the normally built C library; setup and compact loop observations reuse
the included-source wrapper. The 70 committed fixtures cover every method,
opaque and transparent files, tool mapping and both frame routes. Other methods
are reference-only until their Rust ports land; method-4 files and loops compare
exact Rust bytes. No new method is exposed publicly yet.

The native sweep independently decodes 3,556 files, checks 672 setup mappings
and 336 frame traces, and compares 14 observed opaque frame encodes with normal
public C output. Each method reaches both macroblock families; each direct
method reaches skip enabled and disabled. Counts cover method-0 partial probes,
token refresh boundaries and post-decimation trellis flags. Candidate-level
trellis invocation and new leaf traces remain with their port milestones.
See the [reference record](../libwebpop/tests/reference/LOSSY.md#remaining-lossy-methods-milestone-0)
for layouts, provenance, method-4 baseline hashes and regeneration.

Extend the existing word protocol, shared evaluators and wrappers documented in
[`tests/reference/README.md`](../libwebpop/tests/reference/README.md) and
[`LOSSY.md`](../libwebpop/tests/reference/LOSSY.md). Reuse the included-source
wrappers in `lossy_analysis.c`, `lossy_quant.c`, `lossy_frame.c`, `lossy_loop.c`
and `lossy_alpha.c`. Rename exported symbols when including translation units;
leave `upstream/` untouched and avoid duplicate definitions.

Add a test adapter that initializes `WEBP_PRESET_DEFAULT` at the supplied f32
quality, sets method/alpha quality, validates config, imports RGBA with the
ordinary YUVA path and calls the normally built `WebPEncode`. Keep the copied
custom alpha-quality wrapper unchanged for its existing method-4 provenance.
Do not route full-file reference tests through instrumented replacement loops.

As the corresponding leaves are ported, add logical requests for fast analysis,
non-RD refinement, trellis, direct residual writing and skip fitting, supplementing
the compact direct/token frame observations. Version/extend layouts
without reinterpreting old fixtures. Serialize f32 bits, signed values and
wide scores using existing conventions, not pointers, padding or undefined C
scratch fields. Some fast-path score/mode fields stay initialized or unused;
compare only defined logical state. Capture new C fixtures before implementing
the corresponding Rust branches. Record observed branches, not just input names.

### 1. Method mapping and internal configuration

Completed 2026-10-24, continuing from `9a1b16c`. Private `LossyConfig` validates
and carries a signed method, and complete-file glue passes it to
`VP8Encoder::new_with_method` before analysis. The original constructor and all
public lossy routes still select method 4. `MapConfigToTools` sets the encoder's
RD level, token/direct selection and both header budgets for the fixed normal
resource policy. The token driver now uses the stored RD level. The unported
algorithms retain their existing guards; method selection is not public yet.

Seven new normal-resource setup fixtures are appended to the original 70
without changing their inputs or outputs. Portable comparisons and 84 live C
comparisons cover every method, geometry-dependent header budgets and the
quality-98 diffusion allocation boundary. The remaining 588 setup observations
still inspect C's low-memory/multi-partition routing only. Signed method
extremes are rejected before allocation, and private configuration preserves
fractional quality while checking quality/alpha ranges. Public alpha effort
remains method 4 until milestone 7 generalizes that job.

The mapping preserves the default header limits:
`max_i4_header_bits=256*16*16` at partition limit 0 and the existing
per-macroblock header budget. Normal routing is direct for 0–2 and tokens for
3–6. Method 3 is a useful early integration check for basic RD outside method 4.

### 2. Fast analysis and distortion decisions

Completed 2026-10-24, continuing from `87adc67`. `FastMBAnalyze`, the method
branch in `MBAnalyze`, and the methods-0/1 one-segment analysis condition are
ported. Internal VP8 config carries the original floating-point quality before
analysis; only the fast threshold truncates it to an integer.
`RefineUsingDistortion` and non-RD `VP8Decimate` now reconstruct methods 0–2.
The token frame loop and public entry points remain method 4 until integration.

The new fast-path suite compares 5,973 requests with pinned scalar C and stores
61 portable fixtures without modifying the original fixtures. Exact state checks
include both luma families, method-2 family switches, partial I4 early-outs,
strict score ties, the flat-border safeguard, coefficient masks, scratch buffers,
raster borders and diffusion side effects. Independent checks establish block
concentration thresholds, prediction initialization with one segment, coefficient
mask consistency, and non-RD score/mode/error preservation. The C-observed header
boundary case accepts I4 at budget 16,117 and rejects it at 16,116 after fifteen
partial reconstructions. See the [reference record](../libwebpop/tests/reference/LOSSY.md#remaining-lossy-methods-milestone-2).

Port `FastMBAnalyze`, connect `MBAnalyze`'s method branch and preserve the
`do_segments` condition that includes methods 0/1. Even a one-segment internal
test must initialize their predictions. Keep the existing source-border import,
UV analysis, centroid assignment, susceptibility mixing and integer averaging.

Port `RefineUsingDistortion` and the `RD_OPT_NONE` branch of `VP8Decimate`.
Preserve prediction-only SSE, fixed rate penalties, strict mode ties, Intra4
header early-outs, partial reconstruction and border rotations. Include the
flat-source border safeguard from upstream bug #432, with its left/top mode
choice and restriction of family switching. Commit only the selected output
and nonzero mask, respecting the pin's UV/diffusion side effects.

### 3. Direct residual coding, statistics and skip helpers

Completed 2026-10-24, continuing from `998c02a` (lossy milestone 2 at `5918c1e`).
`PutCoeffs`, `CodeResiduals`, `RecordResiduals`, `CalcSkipProba`,
`FinalizeSkipProba` and `ResetAfterSkip` are ported. Direct coding propagates
writer allocation failures and updates wide luma/UV and segment counters;
statistics record only adaptive events. Existing basic/non-RD raster observers
now use production `RecordResiduals` instead of their test-only implementation.
The production frame loop and public method selection remain unchanged.

The new direct suite stores 37 C fixtures and checks 4,811 live requests,
including 247,644 coefficient blocks and 1,682 raster macroblocks. Exact checks
include writer state/positions/bytes, nonzero contexts, 64-bit counters,
statistics rescaling, skip costs and DC preservation/reset across row boundaries.
Independent decoders recover every coded coefficient; syntax-event walks and
contexts derived from geometric neighbors verify statistics and accounting.
Skip fitting is independently checked for every skip count at totals 0–512,
including the strict 249/250 boundary. See the [reference record](../libwebpop/tests/reference/LOSSY.md#remaining-lossy-methods-milestone-3).

Port `PutCoeffs` and `CodeResiduals`, reusing the existing category tables,
boolean writer, residual setup and nonzero pack/unpack helpers. Port
`RecordResiduals` using the existing `VP8RecordCoeffs`. Check both against
independent coefficient-tree walks and compare writer positions, context
mutation and statistics with C. Include zero blocks, all categories/signs,
first coefficient 0/1 and end-of-block at coefficient 15.

Port `CalcSkipProba` / `FinalizeSkipProba`: use the pin's floor calculation,
the strict `skip_proba < 250` threshold and exact signaling costs.
`ResetAfterSkip` clears the packed contexts for Intra16, including the left DC
context, but preserves the packed DC context for Intra4. Exercise neighboring
skipped/coded macroblocks and row transitions; do not reset all state uniformly.

### 4. The two-pass frame path and method-3 tokens

Completed 2026-10-25, continuing from `1fa6593`. `OneStatPass`, `StatLoop`,
`InitBitWriters`, `VP8EncLoop` and shared `PostLoopFinalize` now implement the
fixed single-partition, one-configured-pass/no-target path. The token loop
accepts method 3 and uses the mapped basic RD level. Complete-file glue selects
the mapped driver, and the container boundary calls either result residuals.
Public APIs and CLI still select method 4; method-dependent alpha and trellis
remain pending.

Twenty-seven new portable fixtures were captured from C before driver porting.
The live corpus compares 1,335 driver traces and 1,384 opaque RGBA files against
normal public C, with independent decoding. Traces include fitted probabilities,
statistics/level costs, selected levels, reconstruction, packed contexts,
diffusion, mode headers, final residual bytes and filter state. Method-0 probes
cover totals 49/50/51 and 199/200/201/204; direct and token retries cover strict
header-cost boundaries, retained statistics and a token refresh before retry.
Independent context grids and syntax-event decoding check 10,255 statistics
macroblocks, 13,059 coded blocks, 93 omitted blocks, 3,016 I4 decisions, 20
retries and 159 row transitions. See the
[reference record](../libwebpop/tests/reference/LOSSY.md#remaining-lossy-methods-milestone-4).


Port the applicable `PassStats` initialization, `OneStatPass`, `StatLoop`,
`InitBitWriters`, `VP8EncLoop` and `PostLoopFinalize`. With no target and one
configured pass, quality stays fixed, but statistics collection and final coding
are separate reconstructed scans. Statistics record residual events even for
zero macroblocks and count skips; only the final coding loop may omit them.
Fit skip/token probabilities and refresh level costs in the pinned order.

Method 0's statistics probe requests 50 macroblocks at total counts `<=200`,
otherwise `total >> 2`; traversal still stops at the actual end of a small
frame. Methods 1/2 collect full-frame statistics. `RD_OPT_NONE` does not fill
the RD distortion/header/rate fields the same way as basic RD; preserve those
defined values instead of deriving replacement scores. Keep partition-0 retry
logic and test it at its reachable branches, including basic-RD token retries.

Reset iterator borders, prediction/nonzero state and diffusion as upstream does
between scans; retain persistent segment/probability changes. Call decimation
before deciding whether to emit residuals for a skipped macroblock. Initialize
the writer after statistics establish quantizers, finish it once, and apply the
existing deterministic filter adjustment. Use a shared single-partition result
at the container boundary rather than naming every partition a token buffer.

Do not implement target-quality convergence, multiple partitions, autofilter
statistics, reconstruction export or public reports merely because the C loop
contains their hooks. Retain the pinned behavior for the fixed settings above.

### 5–6. Trellis and methods 5/6

Port `kWeightTrellis`, `Node`, `ScoreState`, `RDScoreTrellis` and
`TrellisQuantizeBlock`. Use safe arrays and explicit indices instead of cost
pointers, matching the existing band-remapped cost accessor. Preserve wide
signed RD scores, neutral-bias quantization, sharpening, capped candidate
levels, last-interesting-coefficient pruning, EOB cost, dead nodes and strict
predecessor/terminal ties. Traceback returns levels in zigzag order and
dequantized coefficients in transform order. Intra16 AC preserves coefficient
zero in both arrays.

Use independent enumeration of the same bounded candidate graph for short
blocks to verify minimum score, EOB selection and tie order. Compare production
inputs and defined arithmetic boundaries with C; do not feed the oracle values
that cause undefined signed overflow just to exhaust an i16 domain.

Connect trellis to `ReconstructIntra16` and `ReconstructIntra4`, carrying
`it.do_trellis` and incoming nonzero contexts explicitly. Intra16 AC visits
blocks in raster order and updates local contexts as it reconstructs. Keep
WHT/DC quantization and UV reconstruction on the pinned scalar paths.

Port `SimpleQuantize`, then complete `VP8Decimate`'s RD dispatch. Method 5
chooses modes with trellis off and requantizes the chosen modes with it on;
method 6 enables trellis during candidate searches. Preserve context unpacking,
scratch swaps, partial Intra4 reconstruction, final borders, maximum-edge
tracking, diffusion commits and skip calculation. Generalize the token loop to
use the mapped RD level, retaining its refresh countdown, probability reset,
header-budget retries and no-skip syntax. Confirm trellis sees refreshed level
costs after a mid-frame probability update.

### 7. Alpha effort is part of method selection

Propagate `effort_level=method` through `VP8EncStartAlpha`, `EncodeAlpha`,
`ApplyFiltersAndEncode`, `EncodeAlphaInternal` and `EncodeLossless`. Distinguish
this effort from the ALPH compression-method bit, which remains 1 unless raw
fallback wins. Reuse the existing configurable VP8L stream and alpha filters.

At the pin, headerless VP8L uses `exact=1`, near-lossless 100, the selected
method, and quality `8 * method`, except **method 6 with alpha quality 100 uses
VP8L quality 100**, enabling its high-effort trials. The exception depends on
the alpha-quality request (`!reduce_levels`), not color quality or whether the
centroid operation happened to change samples.

Fast alpha filtering additionally tries NONE at effort >3 or more than 192
colors; with <=16 colors it chooses NONE directly. Preserve existing ascending
filter trial order, first equal-size winner, and raw fallback only when the
compressed payload is strictly larger than the raw plane. Alpha quantization
and the preprocessing header bit retain their existing semantics.

Test effort boundaries 3/4 and the method-6 quality exception independently
from color coding. Require composed transparent-file parity before exposing
new methods publicly; opaque-only method support is incomplete.

### 8. Public and CLI integration

Extend the checked API with method selection after the algorithms compose.
Keep both existing lossy convenience functions delegated through it, with
method 4, and preserve the SharpYUV option. Connect
`cwebpop --lossy -m 0..6`, update help/errors and carry method through
`EncoderOptions::Lossy`. Retain fractional `-q`, `--alpha_q`, files,
stdin/stdout, `--noalpha`, and the current lossless default.

Replace CLI tests that currently reject valid non-4 lossy methods with byte
comparisons against `encode_lossy_rgba`; retain rejection of out-of-range
methods, incompatible codec choices and unsupported advanced switches before
input file access. Do not silently accept `--low_memory`, `--pass` or other
unimplemented options because the new internal loop shares their C source.

## Reference corpus and independent checks

Extend the existing `lossy_analysis`, `lossy_quant`, `lossy_reconstruct`,
`lossy_search`, `lossy_loop`, `lossy_tokens`, `lossy_costs`, `lossy_syntax`,
`lossy_alpha`, `lossy_public` and CLI tests and shared evaluators. Add focused
trellis/direct-loop files where useful, within the same oracle transport.
Document protocol additions and fixture regeneration in the existing lossy
reference record. Preserve old fixture bytes unless a reviewed correctness
change requires an explicit explanation.

Compare leaves, subgraphs and complete files. At subgraph boundaries record
analysis families/modes, selected RD level, trellis invocation sites, levels,
scores where defined, nonzero contexts, diffusion errors, borders, skip
probabilities/counts, fitted coefficient probabilities, residual bytes,
refresh/retry events, filter strengths and alpha trial results. Report the
first differing logical intermediate and retain minimized reproducers.

Required coverage:

- Every method with color qualities `0, 1, 30, 50, 75, 98, 98.5, 99, 100`,
  plus fractional qualities around fast-analysis integer thresholds and
  quantizer transitions. Use broad seeded sweeps and a bounded representative
  cross-product for routine tests; do not require every corpus image at every
  option combination.
- `1×1`, skinny/odd dimensions, dimensions around 4/8/16/32 boundaries,
  partial chroma blocks, missing top/left borders, multiple macroblock rows,
  and legal maximum dimensions using skinny images.
- Solid/flat-border blocks, gradients, directional edges, checkerboards,
  text, noise, separated smooth/textured regions, and the existing committed
  real-image corpus. Force both fast-analysis families, each refinement flag
  combination, family changes for method 2 and Intra4 early-outs/ties.
- Trellis all-zero/skip winners, first coefficient 0/1, positive/negative
  coefficients, sharpened zero inputs, saturated levels, early/last EOB,
  all incoming contexts, dead nodes and equal-cost paths. Observe differing
  method-5/method-6 invocation order and cases where their decisions differ.
- Method-0 probes around totals 49/50/51 and 199/200/201/204, checking actual
  visited macroblocks; token frames across the 96-count refresh boundary and
  larger interval changes. Test forced header retries with the existing
  bounded observer approach where applicable, preserving public reference
  calls without injected state.
- Skip probability 249/250 boundaries, no/all/mixed skips, both macroblock
  families, and coded neighbors after skips and row transitions. Independently
  parse skip headers and decode direct residuals and contexts.
- Opaque, fully transparent, binary/sparse/smooth alpha, nonzero hidden RGB,
  and alpha qualities `0, 4, 5, 69, 70, 71, 99, 100`. Force filter color-count
  boundaries 16/17 and 192/193, effort 3/4, method-6 alpha quality 99/100,
  raw/compressed payloads and equal-size trials.
- Padded strides, unaligned slice starts, exact minimum buffers, short input,
  invalid dimensions/strides, checked overflow, out-of-range methods/qualities,
  NaN/infinities, source immutability and unchanged output on failure.

Require exact complete bytes against normal pinned C for matching settings,
then decode with the existing independent Rust decoder and verify dimensions
and decoded-reference RGB/alpha agreement. At alpha quality 100 require exact
source alpha for every method. Below 100, compare decoded reference alpha;
ALPH preprocessing can invoke decoder smoothing. Alpha quality must not alter
opaque output. Quality 100 still permits lossy RGB.

Do not assert monotonic size, PSNR or time across methods: their searches and
heuristics differ. Preserve every method-4 byte fixture, convenience API
identity and lossless/near-lossless/standalone SharpYUV regression.

## Rust translation and completion

Mirror upstream source basenames and function names. Keep algorithm order,
integer widths, rounding and tie rules recognizable. Carry upstream
`WEBP_INLINE`, including macro-expanded annotations, as plain `#[inline]`.
Retain copyright/license/patent references. Use safe owned state and explicit
borrows/indices; production code continues to forbid unsafe Rust and to build
without C, CMake, Wasm or access to the upstream checkout. Propagate allocation
errors through existing `Result` APIs; do not add recoverable input panics.

As each milestone lands, update its status and evidence here and the relevant
reference record. On completion, update the encoder inventory: mark all lossy
methods and method-dependent alpha complete, and describe the remaining direct
loop integrations (public low-memory mode, partitions, passes/search) accurately.
Link this extension from the original lossy plan without changing its historical
method-4 acceptance scope.

Run the affected test targets during implementation, and at completion run:

```sh
cargo fmt --all -- --check
cargo build -p libwebpop --lib
cargo test
cargo test --features native-oracle -- --nocapture
cargo test --release --features native-oracle -- --nocapture
```

Record a release baseline by method for representative opaque and alpha inputs,
including larger images: complete encoded bytes, decoded color/alpha metrics,
repeated encode timing, and memory with measurement boundaries stated. Compare
paired Rust/C encodes with identical settings where recording speed differences;
verify bytes outside timed regions. Keep whole-process RSS distinct from encoder
allocation/peak-memory measurements. These are observations, not scalar/SIMD
performance parity gates.

Completion requires checked library and CLI access to all seven methods,
pinned-reference leaf/subgraph/full-file parity, independently decoded output,
observed new branches, unchanged method-4 defaults, passing inherited suites,
and a recorded size/quality/time baseline. Remaining advanced encoder options
continue under the encoder completion inventory.
