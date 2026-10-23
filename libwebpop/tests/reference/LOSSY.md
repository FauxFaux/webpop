# Lossy VP8/ALPH reference record

Milestone 0 extends the existing scalar oracle; production Rust has no C or
Wasm dependency. The pin remains `1844042d1ca751e62e61df7617dba19c98588339`.
The upstream checkout is unmodified. The public lossy Rust APIs are not yet
implemented; public files in this batch are C reference outputs for subsequent
ports, independently decoded with the existing `image` WebP decoder.

`alpha_quality_wrapper.c` copies `webp-wasm/alpha-quality.c` verbatim, apart
from its provenance header, from quad-image revision
`1ee887300721ad754c5632b56349d5daad17c11e`. Copyright (c) 2017 Chris West;
relicensed with the author's permission under the BSD-3-Clause license in
`libwebpop/COPYING`, as for the near-lossless wrapper. The wrapper leaves the
default preset and YUVA import route intact and changes only `alpha_quality`.
Both it and the standard `WebPEncodeRGBA` call the normally built pinned
library. No sibling checkout or Wasm runtime is needed at test time.

`lossy.c` exposes the scalar `VP8FTransform` dispatch entry directly, without
including a duplicate encoder translation unit or copying its algorithm. The
Rust `dsp/enc.rs::FTransform_C` preserves the pinned fixed-point evaluation
order and signed shifts. In particular, a zero residual has coefficient 1
equal to 1 because of the first pass's rounding bias; it is not an all-zero
coefficient block.

## Logical requests

Requests use the existing u32-word transport and fixture format:

- `VP8FTransform`: 16 source bytes then 16 reference bytes, each logical block
  in raster order. Adapters expand them into encoder-stride (`BPS=32`) buffers
  with different padding. Returns 16 signed i16 coefficients, sign-extended
  through i32 to u32 in raster order.
- `WebPEncodeRGBA`: `(width, height, byte_stride, quality_f32_bits)`, then one
  word per RGBA byte, including row padding but excluding final-row padding.
- `WebPEncodeRGBAWithAlphaQuality`: the same header plus signed `alpha_quality`
  encoded as u32, then RGBA bytes. Both public requests accept checked positive
  geometry and valid input lengths. Color quality is reconstructed with
  `memcpy`, retaining fractional values, NaNs and infinities. Successful calls
  return complete files, one word per byte; rejected quality returns no words.
  Both import from an unaligned address and verify input immutability. The
  custom adapter also checks that failures clear a non-null output sentinel.
- `AlphaQualityWrapperErrors`: no inputs; returns 1 when the custom wrapper
  safely rejects a null output slot.

The native corpus has 1,727 exact transform comparisons: all constant signed
residuals -255 through 255, signed/edge impulses at every position, and 1,024
seeded block pairs. An independent portable check exhausts all 65,536 constant
byte pairs, checking DC normalization, the rounding bias and remaining zeros.

The public corpus independently decodes 3,566 files: six geometries (`1×1`,
`1×17`, `17×1`, `3×5`, `17×19`, `33×17`), six alpha/color patterns (opaque,
fully transparent with hidden RGB, binary, smooth, noise and two-level), nine
color qualities (`0, 1, 30, 50, 75, 98, 98.5, 99, 100`), and the standard API
plus custom alpha qualities `100, 0, 4, 5, 10, 30, 69, 70, 71, 99`. It includes
two quality-75 encodes of the existing real-image sample. All standard/custom
alpha-100 pairs match byte-for-byte; opaque output is independent of alpha
quality. Exact alpha is checked against the source at quality 100. Lower
qualities demonstrably change decoded alpha, with smoothing permitted.

Checks validate RIFF lengths, chunk order and padding, VP8 frame signatures,
VP8X dimensions/alpha flags, ALPH control fields, raw payload lengths and decoded
dimensions. The corpus records 1,215 raw and 1,755 compressed ALPH chunks,
with 126,648 changed decoded alpha samples across the quantized cases. These
are measured emitted paths, not claims about internal VP8 mode coverage.
Six invalid color-quality bit patterns (including both signs of NaN) are
rejected by both wrappers, as are four invalid signed alpha qualities. The
custom wrapper clears its output slot on those failures and rejects null output.

## Fixtures and validation

`fixtures/lossy.txt` has 48 compact cases: 16 transform outputs and 32 complete
public reference files. Portable tests compare the Rust leaf to C, decode the
files, check exact/quantized alpha and chunk structure, and retain default/custom
100 identity and opaque invariance. Review the pin, wrapper source and
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test lossy lossy_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test lossy
```

Initial provenance (2026-10-23): Ubuntu x86_64 Linux, GCC 15.2.0-16ubuntu1,
CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off, near-lossless on;
Rust 1.101.0-nightly (`7189d39d9`, 2026-10-22). Seed `6d2b79f5`. Every native
run records exact compiler/build commands, toolchain, inputs and responses in
`target/native-oracle/`; mismatches retain the existing one-request reproducer.

Validation for this batch passed: `cargo test --quiet`, the full workspace
`cargo test --features native-oracle -- --nocapture`, the six lossy tests in
release with `native-oracle`, `cargo fmt --all -- --check`, and
`cargo build -p libwebpop --lib`. Manual timing/large-corpus tests retain their
existing ignored status. The copied wrapper was also compared directly with
the recorded source revision, excluding only the added provenance header.

## Milestone 1: remaining transforms

`dsp/enc.rs` now also ports `FTransform2_C`, `ITransform_C` and its inline
`ITransformOne`/clipping helpers, plus `FTransformWHT_C`. `dsp/dec.rs` contains
only the encoder-required inverse `TransformWHT_C`. The AC3 multiplication
helpers retain the pinned constants and signed-shift order, using i64 products
to avoid overflow for larger i16 inputs. DSP dispatch calls in `lossy.c` still
use the normally built scalar library; no upstream files are changed.

Additional request layouts:

- `VP8FTransform2`: 32 source bytes then 32 reference bytes, each image in 8×4
  raster order. Returns the left block's sixteen signed coefficients followed
  by the right block's sixteen. Adapters use `BPS=32` and differing row padding.
- `VP8ITransform`: `(do_two)`, then 16 or 32 reference bytes in 4×4 or 8×4
  raster order, then 16 or 32 signed coefficients in consecutive block order.
  Returns all 128 destination bytes, including sentinel row/trailing padding.
  The oracle limits coefficients to -4095 through 4095, where its signed C
  products are safe. Rust uses exact-minimum reference and coefficient slices.
- `VP8FTransformWHT`: all 256 signed coefficients of sixteen consecutive luma
  blocks; only the sixteen DC slots (multiples of 16) contribute. Returns
  sixteen signed WHT coefficients. Oracle DC inputs cover -2048 through 2047;
  unrelated AC inputs span the full i16 range.
- `VP8TransformWHT`: sixteen signed WHT coefficients followed by 256 initial
  destination coefficients. Returns the complete mutated destination. Only DC
  slots may change. Oracle transform inputs cover -16384 through 16383, keeping
  results within i16; destination sentinels span the full i16 range.

Signed values use two's-complement i32 encoded as u32. The new corpus has
21,728 exact C comparisons: 1,024 paired forward blocks, 18,078 inverse block
cases (both single/paired forms), 1,296 forward WHT and 1,330 inverse WHT cases.
It covers every inverse DC value -4095 through 4095 in both forms, signed
impulses in every coefficient, clipping/rounding thresholds, constant Walsh
inputs, mixed coefficients, and seeded samples. `fixtures/lossy_transforms.txt`
retains 90 representative cases separately from milestone 0's unchanged files.

Independent portable tests multiply explicit Hadamard matrices to verify both
WHTs, normalization, negative rounding and gather/scatter layout; check exact
round trips for even DC inputs; vary unrelated AC inputs; verify paired forward
transforms against separate blocks; and check inverse DC rounding/saturation
and unchanged padding. Offset destination slices and exact-minimum buffers
exercise boundaries, including WHT's last DC slot at index 240.

The pin, compiler, CMake flags, Rust toolchain and seed are unchanged from
milestone 0. Regenerate only after reviewing them and the added adapters:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test lossy_transforms vp8_transform_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_transforms -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test lossy_transforms
```

Validation passed for this extension: the full workspace `cargo test --quiet`
and `cargo test --features native-oracle --quiet`, all eight transform tests in
release with `native-oracle`, `cargo fmt --all -- --check`,
`cargo build -p libwebpop --lib`, and `git diff --check`. Existing manual
timing/large-corpus tests remain ignored. At this checkpoint, milestone 1's
predictions, scans, quantization and remaining distortion/histogram DSP were
still pending.

## Milestone 1: scalar predictions

`dsp/enc.rs` ports `Intra16Preds_C`, `IntraChromaPreds_C`, `Intra4Preds_C`,
their five inline large-block helpers and all ten Intra4 modes. New
`enc/vp8i_enc.rs` retains prediction offsets and `PRED_SIZE_ENC=1792` from the
pinned internal header. The three oracle adapters use the normal scalar DSP
dispatch entries, including the library's clipping-table initialization.

Rust includes the corner in each left slice: luma's `[corner, left0..left15]`
has 17 bytes. Chroma's 25-byte window has U's corner at 0, U samples at 1–8,
V's corner at 16, and V samples at 17–24. Indices 9–15 are ignored; top holds
eight U followed by eight V samples. Missing borders use `None`. Intra4 takes
exactly `[L,K,J,I,X,A,B,C,D,E,F,G,H]`, mapping upstream `top[-5..7]` without
negative indexing. Its caller supplies substituted/replicated borders; iterator
rotation and actual reconstructed-border integration remain milestone 3 work.

Request layouts:

- `VP8EncPredLuma16`: `(border_flags, sentinel)`, 17 left-window bytes, then
  16 top bytes. Bit 0 enables left; bit 1 enables top. Bytes for absent borders
  are still present in the request and must not influence the result.
- `VP8EncPredChroma8`: the same header, 25 left-window bytes, 16 top bytes.
- `VP8EncPredLuma4`: `sentinel`, followed by the 13-byte boundary window.

Each returns the entire initialized 1,792-byte prediction scratch buffer,
including other block families, the unused Intra4 trial slot and padding. This
checks all prediction offsets and detects writes outside the intended modes.

There are 4,910 exact native comparisons: 1,752 each for luma16/chroma8 (all
four border-presence combinations, every constant byte, independent U/V
corners, clipping boundaries, and seeded mixed borders), and 1,406 for luma4
(all constants, impulses in all thirteen positions, edge triples, seeded
borders, and 128 replicated top-right windows). There are 51 portable fixtures
in `fixtures/lossy_predictions.txt`, taking every 97th case.

Independent portable checks compute rounded means from the available border
samples, verify large-block replication/defaults and clipped gradients, test
nine mode transpose identities with replicated edges, check constant
Intra4 predictions, and verify that DC excludes the corner and top-right.
Half/quarter rounding checks exhaust all byte pairs with third samples 0/255;
additional checks cover the LD/VL last-pixel averages and HU's repeated final
left sample. Exact-minimum destination/left/top slices and offset buffers
verify boundaries, and changing only the unused chroma left gap has no effect.

The pin, compiler, CMake flags, Rust toolchain and seed remain unchanged.
Review those and the new adapters before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test lossy_predictions vp8_prediction_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_predictions -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test lossy_predictions
```

Validation passed: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all eight predictor tests in
release with `native-oracle`, `cargo fmt --all -- --check`,
`cargo build -p libwebpop --lib`, and `git diff --check`. The upstream checkout
remains clean. At this checkpoint, scans, coefficient quantization and remaining
distortion/histogram DSP were still pending in milestone 1.

## Milestone 1 complete: quantization, scans and remaining DSP

`dsp/enc.rs` now ports `QuantizeBlock_C`, `Quantize2Blocks_C`, all four SSE
shapes, both texture distortion shapes and their `TTransform`, `Mean16x4_C`,
`VP8SetHistogramData`, `CollectHistogram_C`, block copies, `VP8DspScan` and
`kZigzag`. The WHT dispatch uses `QuantizeBlock_C`, matching scalar C.
`enc/vp8i_enc.rs` adds `VP8Matrix`, `MAX_LEVEL`, `QFIX`, `BIAS` and the inline
`QUANTDIV`; `enc/quant_enc.rs` adds scan/prediction offset tables only. Segment
parameters, quality mapping, matrix expansion and mode decisions remain later
milestones. No production C dependency is added.

`QUANTDIV` deliberately wraps its unsigned product and bias sum at 32 bits
before shifting by 17. Coefficient absolute values widen before negation to
handle i16 minimum safely. Quantization compares the sharpened magnitude
strictly against `zthresh`, saturates levels at 2047, writes raster dequantized
coefficients in place, and writes zigzag levels separately. The corpus keeps
dequantization products within i16, avoiding implementation-defined C narrowing.

New adapters in `lossy_dsp.c` call the original dispatch entries/exports. The
word protocol adds:

- `BIAS`: one u32 bias, returning the unsigned shifted result.
- `QUANTDIV`: `(magnitude, reciprocal, bias)`, returning the level before the
  2047 limit. Full-u32 inputs expose multiplication/addition wrapping.
- `VP8DspTables`: no inputs. Returns 24 DSP scan offsets, 16 luma scan offsets,
  four Intra16 offsets, four UV mode offsets, `QFIX`, then `MAX_LEVEL`.
  Static zigzag/UV tables are also checked through quantizer/histogram results
  and independent diagonal/tile reconstruction.
- `VP8Quantize`: `kind` (0 single, 1 paired, 2 WHT), then sixteen words each
  for `q`, `iq`, `bias`, `zthresh`, `sharpen`, followed by sixteen or thirty-two
  signed input coefficients. Returns nonzero flag/paired mask, the complete
  mutated coefficients, then zigzag levels. Signed values use sign-extended
  i32 encoded as u32. Narrow matrix fields are checked against u16 limits.
- `VP8Metric`: opcode, optional sixteen texture weights, packed source pixels,
  and (for SSE/texture) packed reference pixels. Opcodes 0–3 are SSE 4×4,
  8×8, 16×8, 16×16; 4–5 are texture distortion 4×4/16×16; 6 is mean16×4;
  7–8 copy 4×4/16×8. Adapters expand planes to BPS=32 with differing padding.
  Returns a scalar, four block sums, or all `height * BPS` destination bytes,
  including sentinel padding for copies.
- `VP8SetHistogramData`: 32 signed distribution counts; returns maximum
  positive count and last positive bin, with the empty sentinel (0, 1).
- `VP8CollectHistogram`: `(start_block, end_block)`, then two packed 16×16
  planes. Returns the two histogram fields after the original forward
  transform/bin collection. Empty and all valid half-open scan ranges are used.

There are 6,189 additional exact native requests: 3,394 quantizer/table/helper
cases (including 2,844 block operations), 648 metric/copy cases, and 2,147
histogram cases. Matrices include threshold ties, sharpening, zero reciprocals,
saturation, independently varied fields with wrapping biases, and 512 seeded
blocks with consistent reciprocal/threshold matrices. All paired masks and
both ordinary/WHT dispatches are exercised. Texture weights include zero,
the original Y weights, full-u16 and asymmetric matrices. Histogram cases cover
signed/empty distributions, all 325 scan ranges and every constant residual
-255 through 255 over luma, chroma and empty ranges.

Independent tests use wide fixed-point arithmetic modulo 2^32; hand-computed
threshold/sharpening/saturation cases; direct squared pixel differences and
block sums; explicit Hadamard matrix products with i64 energy accumulation;
per-tile distortion truncation; alternating-diagonal zigzag construction;
tile coverage; exact constant-residual histogram bins; and offset/minimum copy
buffers. Full-u16 weights stay within C's signed arithmetic bounds. The native
sweep uses the existing bounded parallel case helper, retaining worker traces.

`fixtures/lossy_dsp.txt` contains 133 cases, taking every 47th request separately
from the quantization, metric and histogram corpora. The pin/toolchain/build
configuration and seed are unchanged. Review them and the added wrapper before
regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test lossy_dsp vp8_dsp_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_dsp -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test lossy_dsp
```

Validation passed: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all nine new DSP tests in release
with `native-oracle`, `cargo fmt --all -- --check`,
`cargo build -p libwebpop --lib`, and `git diff --check`. Existing manual
timing/large-corpus tests remain ignored; the upstream checkout is clean.
Milestone 1 is complete, with 34,554 exact native requests and 290 portable DSP
fixtures across all batches. Public lossy integration remains later in the plan.
