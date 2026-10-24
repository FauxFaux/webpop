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

### Milestone 2: VP8 boolean writer

Continued from `e96c3da` against the same clean upstream pin. Compiler and
configuration remain GCC 15.2.0-16ubuntu1, CMake 4.2.3, Release `-O3 -DNDEBUG`,
SIMD/threading off, and Rust 1.101.0-nightly (`7189d39d9`). The test-only
`boolean_writer.c` calls the normally linked upstream writer functions; it
contains no copied arithmetic-coding algorithm.

`VP8BitWriter` extends the existing Rust `utils/bit_writer_utils.rs` alongside
VP8L, preserving the VP8 range/value state, pending-bit count and deferred
`0xff` run, probability/uniform coding, MSB-first fields, zero/nonzero signed
flag, carry propagation and termination padding. Buffer capacity and allocation
growth are Rust-owned. Allocation errors propagate immediately through
`Result<T, TryReserveError>` instead of C's sticky error flag; callers must stop
on error. Append returns `Ok(false)` without changing state if pending bits
prevent appending. Initialization and wipe keep their distinct upstream states.
Finish terminates one coded stream, with append permitted afterward; it is not
an idempotent VP8L-style flush. Three upstream inline buffer/size/position
accessors retain `#[inline]`.

The `VP8BitWriter` request starts with `(expected_size, full_trace)`, followed
by `(opcode, value, bits)` triples:

- 0: `VP8PutBit(value, bits)`; bits is the probability in `0..=255`.
- 1: `VP8PutBitUniform(value)`; bits is unused.
- 2: `VP8PutBits(value, bits)`; widths `1..=31`, including values with
  ignored high bits.
- 3: `VP8PutSignedBits(value, bits)`; signed i32 as a u32 word, magnitude
  fits the requested width `0..=30` (zero emits only its nonzero flag).
- 4: finish; unused value/bits are zero.
- 5: append `value` byte words following this triple. Zero-size native
  appends use an allocated destination so C's memcpy receives a valid pointer.
- 6: wipe; may be repeated, then reinitialize before coding again.
- 7: wipe/reinitialize with reservation `value`.

A state record has nine words: operation return value, range, value, run,
pending-bit count (signed i32 words), approximate bit position low/high,
written-byte size, and FNV-1a hash of the current written prefix. Numeric bit
returns preserve noncanonical truthy arguments. The initial state is always
recorded, then either every operation or the final state, according to
`full_trace`. The response ends with byte count and all written bytes; no
implicit finish occurs. Capacity, addresses and allocator details are excluded.
The adapter validates field domains and bounds request/response sizes.

`support/vp8_boolean.rs` supplies 8,280 exact native requests. These cover every
probability, zero/one/alternating/grouped inputs around flush boundaries, all
4,096 twelve-bit uniform strings, every supported unsigned/signed field width,
noncanonical truthy inputs, 768 seeded mixed streams, long deferred-byte runs,
append sizes around 1,024-byte growth boundaries, rejected unaligned appends,
empty streams, wipe and reinitialization. A minimized 23-bit sequence explicitly
forces carry across a pending byte; noise alone rarely reaches this branch.
There are 128 portable C fixtures, with compact final-state records for large
requests. Existing fixtures and the source/toolchain pin are unchanged.

Independent portable tests use an interval-size streaming decoder, without the
encoder's normalization tables or carry/flush logic, to recover 1,278,333 bits
from 8,273 streams. They check unsigned field order, signed magnitude/sign and
zero flags, input truthiness, and terminated-stream bounds. Other checks assert
a maximum 511-byte pending run, both zero/`0xff` resolutions and mutation of
only the preceding byte on carry, append state preservation, reservation
failure, position/size and wipe/reuse behavior.

Review the pin and `target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_boolean boolean_writer_fixtures_match_pinned_reference
```

The boolean writer portion of milestone 2 is complete. Cost/probability tables,
residual costs and buffered tokens remain; public lossy encoding is still later
work in this plan.

Validation: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet` pass, as do all six boolean-writer
tests in release, `cargo fmt --all -- --check`, the normal library-only build,
and `git diff --check`. The upstream checkout remains clean at the recorded pin.

### Milestone 2: costs, residual statistics and probability headers

Continued from `3a6339f` against the unchanged, clean upstream pin and the same
recorded GCC 15.2.0-16ubuntu1/CMake 4.2.3/Release `-O3 -DNDEBUG` configuration,
SIMD/threading off, Rust 1.101.0-nightly (`7189d39d9`). `lossy_cost.c` calls
normally linked original functions and tables; no cost, recording or coding
algorithms are copied into this native adapter.

New production ports:

- `dsp/cost.rs`: `VP8EntropyCost`, `VP8LevelFixedCosts`, `VP8EncBands`,
  `SetResidualCoeffs_C` and `GetResidualCost_C`.
- `enc/cost_enc.rs`: fixed UV/I16/I4 costs, variable level-code paths,
  `VP8InitResidual`, inline `VP8RecordStats` / `VP8BitCost` / `VP8LevelCost`,
  `VP8CalculateLevelCosts` and `VP8RecordCoeffs`.
- `enc/tree_enc.rs`: default and update coefficient probabilities, Intra4
  mode probabilities, `VP8DefaultProbas` and `VP8WriteProbas`.
- `enc/vp8i_enc.rs`: probability dimensions/types and `VP8EncProba`.

The residual borrows its coefficient array; costing and recording receive the
probability state explicitly instead of retaining aliased C pointers. The
level-cost cache retains band/type/context layout and the dirty fast path.
Band remapping is resolved on access, without self references or duplicated
cached tables. Default probability setup preserves statistics, cost contents,
skip probability and skip count, matching C. `VP8RecordStats` uses explicit
wrapping for C's full-width unsigned `p + 1u`, including the `u32::MAX` edge.
The macroblock iterator wrappers `VP8GetCostLuma4/Luma16/UV` remain deferred
until that iterator exists. Buffered tokens and adaptive probability fitting
are the next units; mode syntax emission and the frame loop remain later work.

Independent syntax-tree walks exposed fixed estimates that differ from the
actual bits emitted by the pinned coder. Preserve these deliberately:

- Levels 9 and 10 each have `VP8LevelFixedCosts=640`; their fixed category/sign
  event costs are 851 and 952. All other levels through 2,047 equal the explicit
  fixed-event sums.
- UV search costs in DC/TM/VE/HE order are `[302, 984, 439, 642]`; the current
  `PutUVMode` tree costs `[218, 990, 601, 648]`.
- I16 and all 1,000 context-dependent I4 mode costs equal their explicit tree
  sums (I16 includes the block-type selection bit).

These are properties of the pinned cost model, not Rust/C divergence. Residual
independent checks use actual tree events for adaptive costs and statistics,
with the two fixed level estimates above retained when adding search costs.
The independent interval decoder used for boolean tests now lives in shared
support and also checks probability-header field order and values.

New requests extend the existing logical word protocol:

- `VP8CostTables`: no input; entropy costs (256), fixed level costs (2,048),
  coefficient bands including sentinel (17), UV (4), I16 (4), I4 (1,000),
  default coefficient probabilities (1,056), update probabilities (1,056).
- `VP8BitCost`: `(bit, probability)`; returns cost. Nonzero signed i32 bit
  words preserve upstream truthiness, probability is `0..=255`.
- `VP8RecordStats`: initial packed stats followed by zero/one observations;
  returns each bit's return value and resulting stats. Includes rescale edges,
  full-width unsigned states and a 131,072-observation stream.
- `VP8LevelCosts`: dirty flag followed by 1,056 probability bytes; initialized
  cache entries are 1,234. Returns dirty, then 6,528 cache entries. If the flag
  was set, also returns the logical target band of all 192 remapped pointers.
  A clear flag preserves the cache contents.
- `VP8Residual`: `(first, coefficient type, initial context, block count)`,
  then 1,056 probabilities, 1,056 packed stats and 16 signed i16 words per
  block. `first` is 0/1, type 0..3, context 0..2, magnitudes at most 2,047;
  AC-only blocks have zero DC. Returns `(last, cost, nonzero)` per block,
  followed by all updated stats, including untouched types/bands/contexts.
- `VP8WriteProbas`: `(use_skip, skip_probability, prefix_bits)`, followed by
  1,056 probabilities. Returns approximate bit position after the header,
  terminated byte count and bytes. Nonzero prefix widths write the low bits
  of `0x5ad`, and all headers are followed by an 11-bit `0x5ad` sentinel.
- `VP8DefaultProbas`: no input; checks setup on dirty state. Returns dirty,
  use-skip, skip-probability, skip count, segment probabilities, coefficient
  probabilities, stats and cached costs.

There are 4,363 exact native requests: 1,373 scalar/table/cache cases, 840
batched residual cases (247,644 blocks) and 2,150 probability-header streams.
The residual corpus covers every magnitude/sign at coefficient 15, every
coefficient position, all four types/three contexts/both starts, zero runs,
last-coefficient/EOB boundaries, default/extreme/asymmetric probabilities,
seeded dense coefficients and saturated counter rescaling. Header cases cover
every individual probability-update slot, dense updates, both skip branches,
probability byte extremes and prefixes through 31 bits. All match C exactly.

There are 55 portable C fixtures in `fixtures/lossy_costs.txt`. Independent
portable checks walk the coefficient coding tree for costs/statistics over all
247,644 blocks, validate fixed/mode tables and dirty-cache behavior, check
packed counters with wide and split-count arithmetic, and independently decode
all 2,150 probability headers. Earlier boolean fixtures remain unchanged.
Review the pin and `target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_costs cost_fixtures_match_pinned_reference
```

Validation passes: full workspace portable and native-oracle suites; all eight
cost/probability tests and six boolean-writer tests in release with
`native-oracle`; `cargo fmt --all -- --check`; normal library-only build; and
`git diff --check`. Existing manual timing/large-corpus tests remain ignored.
The source pin and previous fixtures are unchanged, and upstream remains clean.

### Milestone 2 complete: paginated tokens and adaptive probability fitting

Continued from `a0ca11e` with the same clean source pin and recorded compiler /
scalar configuration (GCC 15.2.0-16ubuntu1, CMake 4.2.3, Release `-O3 -DNDEBUG`,
SIMD/threading off, Rust 1.101.0-nightly `7189d39d9`). The test-only
`lossy_tokens.c` and `lossy_frame.c` include original pinned source files to
inspect private token pages and call static fitting/reset helpers. Their public
symbols are renamed to avoid collisions with the normally built library. No
recording, fitting or encoding algorithm is copied into these adapters.

`enc/token_enc.rs` ports `VP8TBufferInit/Clear`, `TBufferNewPage`, inline
`AddToken` / `AddConstantToken`, `VP8RecordCoeffTokens`, `VP8EmitTokens` and
`VP8EstimateTokenSize`. Pages retain the requested minimum of 8,192 tokens and
16-bit token packing: bit 15 is the observation, bit 14 selects fixed probability,
and the low bits are either probability index or fixed byte. Rust stores each
page in emission order instead of C's reverse-indexed slots. It resolves adaptive
probabilities at emission/estimation time and leaves statistics untouched there.
Trials retain all tokens; final emission releases pages. Rust also resets
cursors after a successful final emission, allowing immediate safe reuse where
C requires clear/reinitialization after its dangling cursors. Drop frees pages.
Allocation failures propagate immediately through `Result<T, TryReserveError>`;
callers must abandon a failed recording/emission. A capacity-overflow test checks
that a failed first-page allocation leaves token and counter state untouched.

`enc/frame_enc.rs` currently ports only probability foundations: category 3–6
constants, `CalcTokenProba`, `BranchCost`, `FinalizeTokenProbas`, `ResetStats`
and `ResetTokenStats`. Fitting compares each candidate against the default
probability, including its update flag and eight-bit signaling costs, and uses a
strict cheaper-than test. Dirty is relative to defaults, even when reverting a
previous nondefault state. `ResetStats` refreshes dirty cost tables and clears
only the skip count; `ResetTokenStats` clears only coefficient statistics. The
full frame loop, refresh countdown and retry scheduling remain in milestone 7.

The new logical protocol is:

- `VP8TokenProba`: `(ones, total, probability)` with `ones <= total <= 65535`;
  returns the fitted probability candidate and branch cost.
- `VP8ProbaState`: `(action, initial_dirty)`, 1,056 current probabilities,
  1,056 packed statistics. Actions: fit (0), fit/calculate level costs (1),
  reset token stats (2), reset stats (3). Other state is initialized to known
  sentinels. Returns signaling cost, dirty, skip count, skip probability,
  use-skip, three segment probabilities, all coefficient probabilities, all
  statistics, and a hash of the cost cache. No self-referential pointers cross
  the protocol.
- `VP8Tokens`: `(page_size, fit_period, fit_final, prefix_bits, block_count)`,
  1,056 initial probabilities, 1,056 initial packed stats, followed by each
  block's `(first, type, context, 16 signed coefficients)`. Page sizes are
  0..32768, coefficient magnitudes <=2047; AC-only blocks have zero DC.
  A nonzero period fits before each block whose nonzero index is divisible by
  it, then refreshes level costs. Optional final fitting follows the last block.
  This supplied schedule tests fitting/recording composition; it is not the
  frame loop's macroblock countdown.

Token responses contain block count and nonzero results; fit count and four-word
fit traces `(blocks_recorded, signaling_cost, dirty, probability_hash)`; then a
page snapshot `(page_size, page_count, free_slots_last_page, token_count)` and
all tokens in emission order. All stats, final probabilities and dirty follow.
Three stream records contain `(estimated_cost_low/high, bit_position_low/high,
byte_count, terminated_bytes)`: a trial with initial probabilities, a trial
with final probabilities, and a final emission with final probabilities.
Prefixes use the low requested bits of `0x5ad`; every stream ends with an
11-bit `0x5ad` sentinel before termination. The final response verifies empty
pages/zero estimated cost, clears twice, records an empty block at type 3 /
context 2, returns its page snapshot and all stats, then emits that reused buffer.
Input validation bounds worst-case response sizes. C's dangling post-final
cursor values are not observed.

There are 5,095 exact native requests: 2,608 scalar probability/branch cases,
1,591 fitting/reset states and 896 complete token workflows. Token cases cover
all coefficient types, both starts, all initial contexts, every magnitude/sign,
category boundaries, zero runs, saturated statistics, empty streams, fitting
between recording batches, 8,192/8,193/16,384-token pages, partial/full/multiple
pages and boundaries inside coefficient coding. Initial/final probabilities
include default, 0/255 and asymmetric byte values. There are 59 compact portable
C fixtures, including the exact fitting tie and adjacent decisions.

Independent portable tree walks construct expected tokens directly from supplied
coefficients, update packed counters with wide arithmetic, calculate candidate
probabilities via rounded complement ratios, and independently sum signaling
costs. They check 593,855 blocks / 7,515,965 token bits, with up to three pages.
The interval decoder checks emitted tree events, prefix and suffix sentinels;
788 trial streams change under later-fitted probabilities, and trial/final
emissions with the same probabilities are identical. Estimates use actual
emitted event costs: levels 9/10 differ from the pinned residual-search estimates
by 211/312 respectively, as recorded in the preceding batch.

The exact tie is flat probability slot 33 (type 0, band 1, context 0, node 0):
default 253, update probability 176, zero one-bits. At 780 total observations the
default wins; at 781 the costs tie and the default still wins; at 782 candidate
255 wins. Tests start with nondefault current probabilities and also exercise
reversion to defaults without manufacturing a dirty flag based on prior state.

Review the pin and `target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_tokens token_fixtures_match_pinned_reference
```

Milestone 2 is complete: 17,738 native requests and 242 portable fixtures across
boolean writer, cost/probability and token batches. Iterator-dependent
`VP8GetCostLuma4/Luma16/UV` wrappers still await the iterator/mode-search work;
they are not the residual cost leaves in this milestone. Public lossy encoding
remains later in the plan.

Validation passes: full workspace portable and native-oracle suites, all six
new token/fitting tests and eight cost/probability tests in release with
`native-oracle`, release library tests including allocation failure, formatting,
normal library-only build and `git diff --check`. Earlier fixtures and the
upstream pin remain unchanged; the upstream checkout is clean.

### Milestone 3 input: default YUVA import and transparent cleanup

Continued from `2237561` against the unchanged, clean pin and the same scalar
GCC 15.2.0-16ubuntu1 / CMake 4.2.3 / Release `-O3 -DNDEBUG` configuration,
SIMD/threading off, Rust 1.101.0-nightly `7189d39d9`. `lossy_yuv.c` includes
original `dsp/yuv.c` with renamed exports to inspect its private gamma helpers
and tables. Whole-image comparisons call the normally linked
`WebPPictureImportRGBA`, `WebPPictureHasTransparency` and
`WebPCleanupTransparentArea`. No conversion/cleanup formulas are copied into
the C adapter; the upstream checkout is unmodified.

`dsp/yuv.rs` ports `VP8ClipUV`, `VP8RGBToY/U/V`, `ConvertRGBToY_C`,
`WebPConvertRGBA32ToUV_C`, gamma table construction/interpolation and
`WebPAccumulateRGB/RGBA`. Gamma initialization uses a shared immutable
`OnceLock`, retaining C's double-precision evaluation order. Integer constant
evaluation generates the reciprocal-alpha table, with every entry compared to
C. Upstream inline helpers retain plain `#[inline]`.

`import_rgba_to_yuva` provides checked positive-stride import into
owned `WebPYUVAPicture` planes, distinct from the inherited ARGB picture.
It combines the paired-row and last-row DSP importer loops, replicating the
last row with zero source stride, while retaining per-pair opaque detection.
Luma ignores alpha; chroma uses gamma-compensated averaging and partial-alpha
weighting, with the original unweighted fallback for zero-alpha cells. Alpha
is optional for opaque input and otherwise copied exactly. Allocations return
errors through the existing `Result` contract. Shared public config/picture
routing remains milestone 9; no public lossy encoder is added here.

The YUVA branch of `WebPCleanupTransparentArea` smooths zero-alpha luma with
the floor mean of all nonzero-alpha luma in each 8×8 block. Fully transparent
complete blocks flatten Y/U/V to the first block's top-left values across each
horizontal transparent run. Visible blocks and row transitions reset the run.
Right/bottom partial blocks only smooth luma, and alpha is never changed.

The logical requests are:

- `YuvTables`: no input; 256 gamma-to-linear values, 33 linear-to-gamma values,
  then all 1,021 inverse-alpha entries.
- `YuvGamma`: `(base_value, shift)` pairs, shift `0..=2` and uplifted value
  at most 16,380; returns interpolation and gamma conversion for every pair.
- `YuvLuma`: `(r, g, b, rounding)`, byte RGB and rounding `0..=131072`;
  returns luma. `YuvUV` uses accumulated components `0..=1020` and returns U/V.
- `YuvAccumulate`: `(width, source_row_stride, weighted)`, then minimum-length
  RGBA byte words for two rows (stride zero repeats one row). Returns all four
  accumulator lanes, followed by U/V planes. Scratch starts at `0xa55a`; RGB
  accumulation must preserve its fourth lane.
- `YuvaImport`: `(width, height, byte_stride, cleanup)`, then minimum-length
  RGBA byte words. Returns transparency and alpha-plane-presence flags, tightly
  packed Y/U/V and optional alpha. C and Rust use unaligned source starts;
  source padding contains unrelated bytes and is excluded after the final row.

There are 6,574 exact native requests: 3,521 scalar/table/gamma requests,
2,619 accumulation requests and 434 whole imports (with/without cleanup).
Gamma inputs exhaust the full interpolation domain at each supported shift;
alpha inputs cover every sum `0..=1020`, both weighted color orders, zero and
opaque fallbacks, row/column replication and 1/254 alpha. Image cases include
boundaries around 8/16/32, transparent runs interrupted by visible blocks,
partial right/bottom blocks, locally opaque row pairs within transparent images,
both 16,383-pixel skinny geometries and the committed real-image sample.
All gamma table and plane comparisons are exact on the recorded toolchain.

An explicit reciprocal boundary has all RGB components equal to 5 and alpha
sum 3: the truncated reciprocal uplifts linear value 703 rather than exact
quotient 704. Gamma interpolation returns accumulator 19 rather than 20.
This preserves the pin's approximation instead of substituting division.
Independent portable checks cover monotonicity/endpoints, piecewise rational
interpolation, reciprocal inequalities, primary colors/clipping, destination
padding, equivalence to explicitly replicated even geometry, hidden-color
weighting, cleanup means/run resets/partial edges, alpha and visible-luma
preservation, input immutability and invalid/overflow geometry.

`fixtures/lossy_yuv.txt` contains 87 portable C fixtures. Earlier fixtures and
the source pin are unchanged. Review the pin, wrapper and recorded toolchain
in `target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_yuv yuv_fixtures_match_pinned_reference
```

Milestone 3's input/cleanup portion is complete. Iterator geometry, reconstructed
borders, contexts and Intra4 rotations remain next, followed by iterator-based
macroblock cost wrappers.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all nine YUV tests in debug and
release with `native-oracle`, `cargo fmt --all -- --check`, normal library-only
build and `git diff --check`. Existing manual tests remain ignored. The upstream
checkout is clean at the recorded pin.

### Milestone 3 complete: iterator and macroblock residual costs

Continued from `a41d65c` against the unchanged, clean pin and the same scalar
GCC 15.2.0-16ubuntu1 / CMake 4.2.3 / Release `-O3 -DNDEBUG` configuration,
SIMD/threading off, Rust 1.101.0-nightly `7189d39d9`. `lossy_iterator.c` calls
the normally linked original iterator and macroblock cost functions. It only
allocates and serializes their state; no iterator/cost algorithm is copied.
The constructor's private C layout is represented by logical geometry, owned
arrays and constant prediction/context borders. Native pointer alignment and
addresses are excluded from comparisons.

`enc/iterator_enc.rs` ports iterator initialization, row/countdown controls,
source block/line import and edge replication, reconstructed boundary saves,
advance, nonzero contexts, mode setters and Intra4 start/rotation. The scalar
37-byte Intra4 boundary is retained, with an explicit offset and 13-byte DSP
window. The rightmost subblocks carry the original external top-right samples
through successive rows; the last macroblock replicates its last top sample.
`MinSize` retains upstream's plain `#[inline]`.

`enc/vp8i_enc.rs` adds the iterator subset of `VP8Encoder`, `VP8EncIterator`,
`VP8MBInfo`, `DError` and `VP8ModeScore`, with the cache layout/offset constants.
The encoder constructor checks geometry, macroblock and allocation sizes and
propagates allocation errors. Frame arguments replace back-pointers; current
macroblock/prediction/context indices are derived from x/y. Left sample arrays
include their corner at index zero; chroma V starts at offset 16. Source-border
scratch is owned by the iterator and discarded on advance, then reimported by
analysis. Coding reads/writes persistent reconstructed top borders. No pointers,
aliased mutation or unsafe Rust are needed. Advanced config/partition routing,
callbacks/progress and `show_compressed` export remain outside the required
default path; the rest of VP8Encoder is added by subsequent milestones.

`cost_enc.rs` adds `VP8GetCostLuma4`, `VP8GetCostLuma16` and `VP8GetCostUV`.
Luma4 reads existing trial contexts without mutation. Luma16 and UV reimport
packed neighbors before each trial and update their own context grid. Luma16
DC costing preserves both DC contexts; `left_nz[8]` is propagated separately
by the caller, and packing always takes the DC bit from the top. Shared bottom/
right corner bits also take the top array's value. Unused bits are discarded.

The new logical requests are:

- `VP8IteratorNz`: `(action, top_packed, left_packed, left_dc)`, nine initial
  top bytes and nine left bytes. Bytes and DC are binary; action zero unpacks
  before packing, action one only packs. The explicit DC overrides the initial
  left array's last element. Returns both byte arrays, packed output and the
  unchanged left packed word.
- `VP8IteratorCosts`: `(top_packed, left_packed, left_dc, i4_index)`, nine top
  and nine left binary bytes, 1,056 probability bytes, then 16 DC, 256 luma AC,
  128 UV and 16 Intra4 signed coefficient words. AC DC slots are zero; levels
  are bounded by 2,047. Returns each of the Luma4/Luma16/UV costs followed by
  its complete top/left context arrays, then both unchanged packed words.
- `VP8IteratorScan`: `(width, height, source_borders, diffusion, start_row,
  count, trace_i4)`, tightly packed Y/U/V byte planes, then one 558-word packet
  per visited macroblock. Packets contain 512 reconstructed cache bytes,
  Intra16 flag, 16 modes, UV mode, skip, segment, nine top/nine left binary
  contexts, and four left/four top signed i8 diffusion errors. Intra16 uses
  the first mode; otherwise all sixteen Intra4 modes are stored. The supplied
  errors exercise persistence/reset behavior, not mode-search diffusion.

Scan output starts with snapshots after init and row/countdown selection.
Each 79-word snapshot contains position/countdowns/done, 17 left-Y and 9+9
left-U/V samples including corners, both nine-byte context arrays, four left
errors, trellis flag, twelve cleared bit counters and full 64-bit luma/UV bit
counts as low/high pairs. Init starts with dirty counters/errors/trellis and
sentinel bit counts so selective reset/preservation can be observed.

Each block returns its imported snapshot, all 512 input cache bytes including
padding, and 16+16 top-Y/UV bytes. Coding scans with `trace_i4` return the
starting `(i4_index, top_offset, 37 boundary bytes)`, then the rotation return
value and the same state after each of sixteen rotations. The trace next
contains the saved snapshot/top borders/packed context, advance return value
and advanced snapshot. Source analysis scans omit reconstructed border saves.
The response ends with the complete predictor map including constant borders,
all five logical fields per macroblock, the packed row including left sentinel,
both full top-sample arrays and optional top diffusion errors. Requests and
worst-case responses are bounded by the existing protocol limit. The native
predictor allocation also guards the terminal row pointer; unused guards and
C padding are not serialized.

There are 18,671 exact native requests: 17,484 nonzero-context cases, 275 scan
workflows and 912 macroblock cost cases. Context cases exhaust all 16,384
independent outgoing edge combinations, deliberately inconsistent shared
corners, every incoming bit and 1,036 seeded word pairs. Scans cover source and
reconstructed borders, both diffusion settings, empty/subrange/terminal scans,
partial dimensions around 8/16/32, multirow macroblocks, every Intra4 position,
both mode types/all UV modes/skip/segments and complete 1×16,383 / 16,383×1
scans. Cost cases cover all sixteen Intra4 positions, default, uniform extreme
and asymmetric probabilities, zero/sparse/dense blocks and signed/category
boundaries including the pinned level-9/10 estimates.

Independent portable tests check 418 macroblocks directly against source and
reconstructed pixel coordinates; 368 Intra4 windows against geometric
neighbors and external top-right samples; all outgoing context combinations
against a separate bit-position model; selective row/restart resets, cache
preservation, geometry errors and maximum skinny dimensions. Macroblock costs
are summed with independent coefficient syntax-tree walks and 4×4 / two 2×2
context grids, preserving trial reimports and the separate DC bit.

`fixtures/lossy_iterator.txt` contains 80 portable C fixtures. Existing fixtures
and source pin are unchanged. Review the pin, wrapper and toolchain record in
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_iterator iterator_fixtures_match_pinned_reference
```

Milestone 3 is complete: 25,245 native requests and 167 portable fixtures across
input/cleanup and iterator/cost batches. Analysis/segment assignment and
quantizer/filter foundations are next; the public lossy APIs remain later work.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all eight iterator/cost tests in
release with `native-oracle`, formatting, normal library-only build and
`git diff --check`. Existing manual tests remain ignored; upstream remains
clean at the recorded pin.

### Milestone 4 analysis: susceptibility probes and segment assignment

Continued from `1276a5b` against the unchanged, clean pin and the same scalar
GCC 15.2.0-16ubuntu1 / CMake 4.2.3 / Release `-O3 -DNDEBUG` configuration,
SIMD/threading off, Rust 1.101.0-nightly `7189d39d9`. `lossy_analysis.c` includes
original `analysis_enc.c` with its exported entry point renamed to expose the
private probes, centroid assignment and synchronous job. Complete-frame checks
also call the normally linked `VP8EncAnalyze`. Only allocation/serialization
and state seeding live in the adapter; no analysis algorithm is copied.

`enc/analysis_enc.rs` ports the required method-4/preprocessing-0 path:
`GetAlpha`, `FinalAlphaValue`, histogram initialization, `SetSegmentAlphas`,
`AssignSegments`, both best-mode probes, `MBAnalyze`, synchronous
`DoSegmentsJob`, default macroblock reset and `VP8EncAnalyze`.
`quant_enc.rs` adds `VP8MakeLuma16Preds` and `VP8MakeChroma8Preds`, copying only
sixteen top samples onto the stack so mutable prediction scratch stays separate
from the borrowed borders. There are no new production allocations in analysis.
The upstream inline clipping helper retains plain `#[inline]`.
`VP8EncSegmentHeader` and `VP8SegmentInfo` now retain the segment state; matrix
construction/quantizer and initial filter setup remain the next batch.

The two luma and two UV probe modes are DC and true motion. Luma chooses the
larger raw susceptibility with the earliest mode winning ties. UV chooses the
smaller raw susceptibility for its mode but contributes the larger score to
complexity analysis. Mixing uses `(3 * luma + uv + 2) >> 2`, then subtracts from
255 and clips. Macroblocks record this value before centroid assignment. Frame
averages truncate the accumulated raw sums. Rust uses i64 for the two aggregate
sums, avoiding signed overflow on very large pictures; block and centroid
arithmetic retains the pinned integers. A direct check crosses signed 32-bit
and unsigned 32-bit aggregate boundaries. Native corpus sums remain within C's
defined signed range.

The ordered centroid algorithm has two explicit compatibility cases:

- With uniform populations in bins 0..255 and four segments, initial centers
  `[31,95,159,223]` move to `[32,96,160,224]`. Total displacement four stops
  iteration. Bin 64 retains segment 1/center 96 from the preceding assignment,
  even though it now ties between centers 32 and 96. Do not reassign after the
  last center update.
- With populations one at bin 0 and seven at bin 2, four initial centers are
  `[0,0,1,1]`. The tied adjacent centers prevent advancing from segment zero,
  even for bin 2, which is closer to later centers. Both bins remain assigned
  to segment zero, whose center becomes 2; displacement two stops iteration.
  Do not replace the ordered adjacent-improvement search with a global nearest
  center search or deduplicate empty/tied centers.

The single-segment control path resets macroblock fields, the first segment's
alpha/beta and frame complexities, preserving predictor contents and unused
segments. Other methods' fast analysis, JPEG-size emulation, spatial smoothing,
worker threads and progress callbacks remain deferred as in the port plan.

Logical requests extend the existing word protocol:

- `VP8Susceptibility`: `(max_histogram_population, last_nonzero_bin,
  signed_raw_alpha)`, populations `0..=256`, bins `0..=31`; returns `GetAlpha`
  and independent `FinalAlphaValue(raw)`. Signed words use i32 as u32.
- `VP8SegmentAlphas`: `(segment_count, midpoint, four_centers)`; returns all
  four alpha/beta pairs, including untouched trailing segments. Centers are
  byte values and the midpoint lies within the adjusted min/max interval.
- `VP8AssignSegments`: segment count and 256 histogram populations. Weights
  must be nonempty, nonnegative and total at most 1,048,576. The adapter creates
  one representative macroblock per occupied bin; the supplied weights describe
  the full population. Returns four alpha/beta pairs and all five logical
  macroblock fields (type, UV mode, skip, segment, remapped alpha).
- `VP8AnalysisProbe`: border flags (left=1, top=2), 17 left-Y bytes, 25 packed
  left-U/V bytes including corners, sixteen top-Y/sixteen top-UV bytes and 512
  source cache bytes. Returns both raw best probe scores, 256 mixed-alpha counts
  and low/high pairs for the two sums, four logical macroblock records, the
  complete 1,792-byte prediction scratch and complete 9×9 predictor map. Input
  cache and unused prediction areas use explicit sentinel bytes.
- `VP8AnalysisFrame`: `(width, height, segment_count)` and tightly packed Y/U/V
  byte planes. Returns the job's histogram/sums and pre-assignment macroblock
  records; then normal full-entry-point frame complexities, segment header,
  four alpha/beta pairs, final macroblock records and full predictor map. Segment
  count one skips the collection job and exercises the normal reset branch.
  The picture planes remain unchanged. Request/response sizes are checked.

There are 17,004 exact native requests: 8,233 scalar susceptibility checks,
3,072 signed segment-scaling cases, 3,844 weighted assignments, 1,288 complete
probe checks and 567 image-analysis checks. The assignment corpus includes
constant/adjacent/extreme populations, up to the full macroblock population,
dense/sparse weighted distributions, unused centers and both explicit cases
above. Independent ordered replay reaches six iterations in six cases, the
exact displacement cutoff five in 136 decisions and 3,842 empty clusters.

Probe cases cover all border combinations, every constant byte and seeded
asymmetric borders/caches. Independent checks observe luma mode counts
`[1247,41]`, UV counts `[1056,232]`, 1,025 probe ties, 272 distinct UV score
pairs and 257 blocks clipped to mixed alpha zero. They build DC/TM predictions
from pixel averages and clipped neighbor arithmetic, then count transformed
coefficients using explicit spatial 4×4 tiles. Independent weighted iteration
uses exhaustive reachable-center selection and rational centroid rounding,
retaining adjacent tie barriers and the pre-update assignment map.

Frame cases cover dimensions around 8/16/32, all segment counts, constant,
gradient, checkerboard, noise, smooth/textured, blockwise and impulse planes,
partial edges, both 16,383-pixel skinny geometries and the committed real-image
sample converted by the previously validated YUVA importer. Independent checks
validate 7,798 analysis macroblocks, conserve histogram populations, recompute
frame averages, compare modes/predictors/remapped centers, and check source,
scratch and single-segment reset preservation.

`fixtures/lossy_analysis.txt` contains 82 portable C fixtures. Earlier fixtures
and source pin are unchanged. Review the pin, wrapper and toolchain record in
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_analysis analysis_fixtures_match_pinned_reference
```

Milestone 4's analysis/segment-assignment portion is complete. Segment-quality
mapping, quantizer tables/matrix expansion, lambdas, equivalent-segment merging
and initial filter strengths remain next; the public lossy APIs remain later.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all eight new analysis tests in
release with `native-oracle`, formatting, normal library-only build and
`git diff --check`. Existing manual tests remain ignored; upstream remains clean
at the recorded pin.

### Milestone 4 completion: quantizer matrices and initial filter strengths

`enc/quant_enc.rs` now ports `QualityToCompression`, `VP8SetSegmentParams`,
`ExpandMatrix`, `SetupMatrices`, `SetupFilterStrength` and `SimplifySegments`,
with the pinned DC/AC/AC2, bias and frequency-sharpening constants.
`enc/filter_enc.rs` adds `VP8FilterStrengthFromDelta`. The encoder owns the
required configuration subset, filter header, base quantizer and five deltas.
Defaults match the simple API's method 4, SNS 50, filtering strength 60/type 1/
sharpness 0. Method/SNS/filter variations here validate setup leaves; this does
not add alternate mode-search paths or public advanced configuration. JPEG
size emulation, automatic filtering and final edge-based filter adjustment are
outside this batch; the latter remains required in milestone 7.

`lossy_quant.c` includes pinned `quant_enc.c` with its external symbols renamed
so static leaves can be called without copying algorithms. Full parameter
setup calls the normally linked `VP8SetSegmentParams`. The composed-image
adapter in `lossy_analysis.c` calls normally linked `VP8EncAnalyze` followed by
`VP8SetSegmentParams`. Upstream remains unmodified.

Additional logical requests:

- `VP8FilterStrengthFromDelta`: sharpness `0..7`, nonnegative signed-int delta;
  returns one strength, clamping delta to 63.
- `VP8ExpandMatrix`: matrix type `0..2`, DC and AC steps `4..440`; returns the
  rounded average followed by 80 matrix words: 16 each of q, reciprocal q,
  bias, zero threshold and sharpening, all in coefficient order.
- `VP8QuantSetup`: 32 words: operation, segment count, method, SNS strength,
  filtering strength, configured sharpness, filter type, previous sharpness,
  raw UV susceptibility, quality f32 bits, five signed deltas (Y1 DC, Y2 DC,
  Y2 AC, UV DC, UV AC), four `(alpha,beta,quant,strength)` segment records,
  and second-quality f32 bits. Operations are matrices (0), filter strengths
  (1), equivalent-segment merging (2), full setup (3), and two consecutive
  full setups using the two qualities (4). Counts are `1..4`, methods `0..6`,
  strengths `0..100`, sharpness `0..7`, types `0..1`, UV susceptibility
  `0..7905`, qualities finite `0..100`, deltas `-15..15`, alpha `-127..127`,
  beta `0..255`, quant `0..127`, and segment strength `0..63`. Eight seeded
  macroblocks cycle over the active segment numbers. Matrices, lambdas and
  penalties start with distinct sentinels, including penalties above u32.
  The 1,077 output words are 13 frame/header values (segment count, update-map,
  segment-header size, base quantizer, five deltas, simple-filter flag, level,
  sharpness and Intra4 filter delta), four 256-word segments, and five metadata
  words per macroblock (type, UV mode, skip, segment, alpha). Each segment is
  three matrices in the 80-word layout above, then alpha, beta, quant, strength,
  maximum edge, minimum distortion, eight lambdas (I16, I4, UV, mode, texture,
  trellis I16, trellis I4, trellis UV) and low/high words of the Intra4 penalty.
- `VP8AnalysisQuant`: width, height, segment count, quality f32 bits and tightly
  packed byte-valued Y/U/V planes. Returns the same frame/segment/metadata
  state with the actual macroblock count, followed by average luma/UV
  susceptibility and the complete predictor map including constant borders.
  Output count is `1039 + 5*mb_w*mb_h + (4*mb_w+1)*(4*mb_h+1)`, checked against
  the existing logical-word limit. The picture/analysis state is seeded as in
  `VP8AnalysisFrame`, with the normal initial filter header.

Important retained ordering and rounding:

- Quality enters as f32 and is promoted before division by double `100.`.
  Mapping uses the original piecewise linear function, cube-root power and
  susceptibility-dependent second power, with truncation to integer quantizer.
  The smallest positive f32 qualities can still produce quantizer 127 because
  `1-c` rounds to exactly one. Chroma AC adaptation truncates twice, before
  and after multiplying by SNS; negative divisions truncate toward zero.
- Filter strengths are updated for all four slots using the *previous*
  header sharpness, before the configured sharpness is stored. Strengths below
  two are disabled, and strengths above 63 saturate.
- Segment equivalence compares only quantizer and filter strength. Stable
  first representatives retain their complete state; macroblocks are remapped
  and trailing slots within the old active range copy the last representative.
  Header `update_map` and size remain unchanged here. Setup expands matrices
  only after merging, leaving copied trailing matrices unexpanded; repeated
  trials preserve merges and the previous filter header.
- Reciprocals retain u16 widths; bias/thresholds use u32, matrix averages round
  with `+8`, UV DC table indices clip to 117, and all eight lambdas have minimum
  one. Sharpening applies only to Y1 with capped spatial-frequency weighting.

There are 14,153 exact new C requests:

- 1,040 filter lookups: every sharpness, deltas `0..128` and `INT_MAX`.
- 3,390 matrix expansions: each step `4..440`, each matrix type, endpoint
  pairings and 768 seeded asymmetric pairs.
- 1,536 matrix setups cover every quantizer index, signed/clipped deltas,
  segment counts, SNS 0/50/100 and methods 0/3/4/6.
- 2,048 initial-filter setups cover all quantizer indices/sharpness values,
  distinct previous/configured sharpness, beta extremes, both types and
  strengths 0/1/50/60/100.
- 340 segment-merging requests exhaust all assignments of up to four segments
  to four `(quant,strength)` pairs, with distinct non-equivalence fields.
- 5,599 full/repeated setups cover quarter-point qualities, every alpha,
  UV susceptibility `0..255` and extreme raw scores through 7905, SNS truncation
  boundaries, neighboring f32 values around every integer-quantizer transition,
  qualities around 75, subnormals and 129 repeated trials.
- 200 composed image requests cover missing borders, odd sizes, multiple
  macroblock rows, six skinny limit images and the real-image sample at
  qualities 0/30/75/98.5/100.

Independent portable tests enumerate the first nonzero integer coefficient
for each reciprocal/bias pair, check reciprocal bounds and rounded averages,
and run both sides of every threshold through the existing scalar quantizer.
They derive sharpening from 4×4 spatial coordinates, check lambda/filter
scaling with wider rational arithmetic, classify equivalence against immutable
first occurrences, and invert quality intervals in logarithmic space instead
of repeating nested powers. They also verify unused state, metadata and
predictor/context preservation, all cutoff/saturation branches and 1,672
quality trials that merge segments. Seven portable and two native tests pass.

`fixtures/lossy_quant.txt` contains 74 portable C fixtures. Source pin, clean
upstream checkout and recorded GCC 15.2.0-16ubuntu1/CMake 4.2.3/Release
`-O3 -DNDEBUG` scalar configuration were reviewed before regeneration; Rust is
1.101.0-nightly (`7189d39d9`, 2026-10-22) on x86_64 Linux, as in the preceding
batch. Earlier fixtures remain unchanged. Review the live provenance in
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_quant quantizer_fixtures_match_pinned_reference
```

Milestone 4 is complete: 31,157 native requests and 156 portable fixtures across
analysis/assignment and quantizer/filter setup. Candidate reconstruction and
mode search are next; public lossy Rust APIs remain unimplemented.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all nine quantizer tests in
release with `native-oracle`, `cargo fmt --all -- --check`, the normal
library-only build and `git diff --check`. Existing manual tests remain ignored;
upstream remains clean at the recorded pin.

### Milestone 5 candidate reconstruction and chroma DC diffusion

`enc/quant_enc.rs` now ports `MakeIntra4Preds`, `ReconstructIntra16`,
`ReconstructIntra4`, `ReconstructUV`, `QuantizeSingle`, `CorrectDCValues`,
`StoreDiffusionErrors` and `StoreMaxDelta`. The default method-4 path uses
scalar quantization without trellis. The two luma reconstruction helpers
explicitly require `do_trellis=false`; UV follows the pin's always-disabled
UV trellis branch. Scores, flatness penalties, winning-mode search and final
`VP8Decimate` integration remain in the next milestone-5 batch.

The existing `lossy_quant.c` adapter calls these original static helpers from
its renamed pinned translation unit. No reconstruction or diffusion algorithms
are copied into the wrapper. A seeded score exposes writes to coefficient
arrays and diffusion errors independently of untouched scores, modes and mask.
Candidate output starts with a sentinel; requests return complete prediction
scratch, iterator state and metadata, including padding and borders.

Additional logical requests:

- `VP8QuantizeSingle`: DC step `4..132` and signed i16 coefficient. Expands
  the chroma matrix using AC step 4, then returns the mutated coefficient and
  signed descaled error. The scalar helper has no sharpening or level cap;
  narrowing a reconstructed value outside i16 matches the pinned C toolchain.
- `VP8DCError`: DC step, four incoming top errors, four incoming left errors
  (channel-major pairs), and eight raster-order 16-coefficient blocks, totaling
  137 words. Errors are signed bytes, coefficients signed i16. Uses column 1
  of a two-column frame; column 0 holds distinct sentinels. Returns 128 mutated
  coefficients, a 435-word mode-score record, 12 incoming/left errors after
  correction, then 12 errors after explicit storage of the candidate. Score
  records contain five signed i64 values (D, SD, H, R, score; low/high words),
  16 Y DC levels, 256 Y AC levels, 128 UV levels, I16 mode, 16 Intra4 modes,
  UV mode, nonzero mask and six signed diffusion errors. Total output is 587
  words; all non-coefficient/non-diffusion score fields stay seeded.
- `VP8Reconstruct`: kind (I16=0, single I4=1, UV=2, sequential I4=3), base
  quantizer index, selected segment, border/diffusion flags, mode, I4 index,
  UV DC/AC deltas, four top errors, four left errors, 17 left-Y bytes (including
  corner), 25 left-UV bytes (including corners and the U/V gap), 32 top-Y bytes,
  32 top-UV bytes and the 512-byte source cache. This is 634 input words.
  Flags select column 1 (bit 0), row 1 (bit 1) and diffusion storage (bit 2)
  in a 2×2-macroblock frame. Segment quantizers are `(base + 17*segment)%128`,
  expanded by original `SetupMatrices`; all four macroblocks use the selected
  segment. UV deltas lie in `-15..15`; single-I4 indices cover `0..15`, luma/UV
  modes `0..3` and I4 modes `0..9`. Other inputs have byte or signed-byte ranges.
  Single-I4 setup rotates across sentinel previous blocks. Sequential I4
  reconstructs all sixteen blocks with mode `(initial_mode+block)%10`, updating
  boundaries from the actual preceding reconstruction.

  The 2,929 output words are the returned mask, the complete 435-word score,
  512 candidate bytes, 1,792 prediction-scratch bytes, 12 incoming/left errors,
  12 errors after optional UV commit, nine top and nine left nonzero-context
  bytes, I4 index/window offset and 37 boundary bytes, five metadata words for
  each of four macroblocks, the full 81-byte predictor map, three packed
  nonzero contexts (including sentinel) and four maximum-edge values. I16
  requests also call `StoreMaxDelta` on the resulting Y DC levels. As in C,
  reconstruction returns its nonzero mask separately and does not write the
  score's mask or selected modes. The adapter commits UV diffusion only after
  serializing the uncommitted incoming state.

There are 108,833 exact new C requests:

- 101,785 scalar DC quantizations: every step `4..132` over coefficients
  `-2*q..2*q`, eight signed extremes for each step, and the entire signed-i16
  domain at step 132. Negative odd errors distinguish floor shifts from
  truncation; coefficients above the block quantizer's level cap remain uncapped.
- 1,032 DC-grid corrections and commits: all 129 steps with zero, positive/
  negative extremes, alternating boundaries, per-block DC ramps and seeded
  coefficients/errors. AC coefficients and unrelated score/neighbor state are
  checked for preservation; coefficient additions retain signed narrowing.
- 6,016 reconstructions: all quantizer indices, four I16/UV modes, ten I4
  modes, four selected segment slots, all border/diffusion flags and varying
  intra-macroblock indices. Constant, alternating, ramp, impulse, clipped-edge
  and seeded inputs cover both zero and nonzero residuals. The 256 sequential
  I4 traces use flat and seeded pixels through all sixteen reconstructed blocks.

Independent portable checks use wider rational division instead of QUANTDIV,
construct zigzag order from diagonal coordinates, and derive both WHT directions
from explicit Hadamard matrix products. Single 4×4 transform/inverse leaves
compose the candidates independently of the paired-block kernels and scan
arrays. A spatial 2×2 diffusion grid verifies weights 7 and 8 with a three-bit
floor shift, candidate error outputs and the later 3/4–1/4 error split.
All 256 signed-byte values test conservation of that final split. Maximum-edge
checks exhaust signed-i16 values at each selected DC coefficient and prove that
other positions are ignored and stored maxima never decrease. Minimum strided
source/I4 and destination buffers and untouched output padding are checked.

Observed whole-candidate zero masks by kind I16/I4/UV/sequential-I4 are
`474/234/521/128`; nonzero masks are `806/2966/759/128`. Reconstructed active
pixels include 29,196 zeroes and 17,512 values clipped to 255. Every tested
candidate's levels, pixels and mask agree with the independent composition;
nonzero contexts, image metadata, source pixels and incoming diffusion storage
remain unchanged until the explicit commit. Six portable and two native tests
cover this batch.

`fixtures/lossy_reconstruct.txt` contains 45 portable C fixtures. Pin, clean
upstream checkout and toolchain were reviewed before generation: unchanged GCC
15.2.0-16ubuntu1, CMake 4.2.3, Release `-O3 -DNDEBUG`, scalar/threading-off
configuration and Rust 1.101.0-nightly (`7189d39d9`, 2026-10-22), x86_64 Linux.
Earlier fixtures remain unchanged. Review the current provenance in
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_reconstruct reconstruction_fixtures_match_pinned_reference
```

Milestone 5's candidate reconstruction and diffusion are complete. Default
rate-distortion mode selection, scoring and winning-candidate context commits
remain next; public lossy Rust APIs remain unimplemented.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all eight reconstruction tests
in release with `native-oracle`, formatting, normal library-only build and
`git diff --check`. Existing manual tests remain ignored; upstream is clean
at the recorded pin.

### Milestone 5 completion: default rate-distortion mode search

`dsp/quant.rs` ports inline `IsFlat`/`IsFlatSource16`. `enc/quant_enc.rs` adds
`InitScore`, `CopyScore`, `AddScore`, inline `SetRDScore`, contextual
`GetCostModeI4`, `PickBestIntra16`, `PickBestIntra4`, `PickBestUV` and the default
method-4/`RD_OPT_BASIC` `VP8Decimate`. The encoder now retains
`max_i4_header_bits`, initialized to the preset's 65,536 cost units. Other
methods/RD levels, trellis and distortion-only refinement remain deferred.
There are no new production allocations or unsafe code.

Owned arrays reproduce full Intra16 output-buffer swaps; booleans track the
Intra4 scratch/block and UV destination swaps. This retains writes by losing
trials and partial state on Intra4 rejection. Candidate scores are compared
strictly, so ties keep the earlier mode. Intra4 lower-bound pruning retains
`>=`; cumulative score rejection also uses `>=`, whereas the accumulated header
cost rejects only on `>`. Flat-source refinement is persistent across I16
candidates: once an AC-bearing candidate clears the initial flatness flag,
later candidates are not doubled. UV residual costing refreshes contexts on
every trial, so final scratch contexts can belong to the final evaluated mode;
residual recording subsequently imports packed contexts again. Only the
selected UV candidate's errors are committed.

The pinned `lossy_quant.c` translation-unit adapter calls original static
helpers for partial searches and normally linked `VP8Decimate` for complete
macroblock decisions. `lossy_analysis.c` composes normally linked analysis,
segment setup and decimation for a fixed-probability raster, recording contexts
through original `RecordResiduals` exposed by `lossy_frame.c`. These adapters
copy no search or context-recording algorithms. The raster is test composition,
not the production token loop: adaptive probability refresh, token emission,
header retries and final filtering remain in milestone 7.

Additional logical requests:

- `VP8ScoreOps`: operation (init=0, copy=1, add=2, score=3), lambda, then two
  eleven-word score records: five signed i64 fields (D, SD, H, R, score) in
  low/high order, followed by a mask. Inputs are bounded to ±2^40, lambda to
  1,000,000. Returns the existing 435-word score layout. Unrelated coefficient,
  mode and diffusion fields have distinct sentinels in source/destination to
  detect accidental whole-struct copying.
- `VP8Flat`: threshold `0..240`, block count `0..16`, 512 byte-valued source
  cache words and `16*count` signed-i16 levels. Returns coefficient flatness
  (DC omitted) and uniformity of the active 16×16 source (padding omitted).
- `VP8ModeCost`: Intra4 index `0..15`, macroblock-position flags `0..3`, complete
  81-byte predictor map and sixteen previous candidate modes, totaling 99
  words. All modes lie in `0..9`; returns ten fixed-mode costs selected from
  spatial top/left macroblock borders or prior modes inside the block.
- `VP8ModeSearch`: the 634-word `VP8Reconstruct` input, then stage, header limit,
  probability kind, incoming score low/high and 81 predictor modes: 720 words.
  Stages are I16-only (0), I16 then I4 (1), complete default decimation (2),
  UV-only (3) and forced I4 against the incoming score (4). Limits lie in
  `0..65536`; incoming scores are nonnegative through `MAX_COST`. Probability
  kinds are default (0), all-zero (1), all-255 (2) and asymmetric `(37*i+17)%256`
  (3). Synthetic tie kind 4 uses default probabilities and sets I16/I4/UV
  lambdas and texture lambda to zero, mode lambda to one; this tests equal
  candidate comparisons and is not a public configuration. Context scratch is
  initialized to valid binary values; packed contexts keep distinct sentinels.
  Both macroblock destinations and predictions start with distinct sentinels.

  Returns 3,429 words: result flag, normalized 435-word score, two 512-byte
  destinations in logical current/scratch order, complete 1,792-byte prediction
  scratch, 12 top/left diffusion bytes, nine top and nine left coefficient
  contexts, I4 index/window offset and 37 boundary bytes, five metadata words
  for four macroblocks, the 81 predictor modes, three packed contexts and four
  segment maximum edges. C leaves unrelated candidate fields uninitialized;
  normalization compares only defined luma fields, UV fields when searched,
  diffusion when enabled, and I4 modes when defined. Stage 1 explicitly seeds
  the mode array after I16 search to make rejected I4 prefixes observable.
  Undefined fields are zeroed for serialization; no C padding or uninitialized
  values enter fixtures. Rust-owned candidates are fully initialized.
- `VP8SearchFrame`: width, height, segment count, quality f32 bits, header limit
  and tight Y/U/V planes. Uses normal analysis and setup, default probability
  costs and a raster coding pass with reconstructed borders and original
  residual-statistics recording. Diffusion storage follows `quality<=98`.
  Returns segment count/base quantizer/four segment quantizers, 1,047 words per
  macroblock, then 1,056 statistics, the predictor map, packed column contexts,
  optional column diffusion and four edge maxima. Each macroblock contains
  skip result, normalized score, reconstructed cache, five metadata fields,
  top/left coefficient contexts, packed context, eight diffusion bytes and
  saved left/top borders. The word budget is checked before allocation:
  `1067 + 1047*mb_w*mb_h + (4*mb_w+1)*(4*mb_h+1) + mb_w`, plus `4*mb_w` with
  diffusion, must fit the existing logical limit.

There are 18,325 new exact native requests:

- 2,048 score operations, including signed values beyond u32 and wide weighted
  products, with preservation of non-score fields.
- 2,112 coefficient/source flatness checks over the required 1/8/16-block
  populations, DC/AC positions and all 512 source-cache positions.
- 6,400 contextual mode-cost requests: every top/left mode, all sixteen Intra4
  indices and all four macroblock positions.
- 7,561 partial/full macroblock searches: all 128 quantizer indices, segment
  slots, border/diffusion flags, default/extreme/asymmetric probabilities,
  ten pixel patterns, disabled/tight/full header budgets, five exact boundary
  cases and four synthetic candidate ties. A true-motion chroma field ensures
  UV TM wins; flat-source/asymmetric-border inputs exercise persistent flatness.
- 204 fixed-probability raster comparisons at qualities 0/30/75/98/98.5/100,
  including odd sizes, multiple macroblock rows and the real-image sample.

The independent portable search enumerates all candidates without production
lower-bound pruning. It computes SSE with wide pixel sums, texture costs using
explicit Hadamard matrix products, and residual rates through syntax-tree walks
shared with the iterator tests (including the pinned level-9/10 estimates).
It validates final scores, coefficients, selected pixels, predictor maps and
skip decisions. Required I16/UV modes and all ten I4 modes win. Counts of
independent decisions are I16/I4 `1132/533`; I16 modes `1126/39/322/178`; UV modes
`1682/64/678/522`; selected I4 block modes
`3389/1039/406/426/830/824/428/409/445/332`.

Independent branch counts include 192 disabled I4 searches, 1,757 cumulative
score rejections, 613 header rejections, 2,437 accepted I4 trials and 384 later
flat candidates after persistent flatness cleared. Exact C-derived inputs
retain an all-DC I4 header total of 640 and score 2,382,600: limits 639/640
reject/accept, and competitor scores 2,382,599/2,382,600/2,382,601 reject/reject/
accept. Four synthetic ties prove that the earliest candidate remains selected.

Raster checks independently validate 1,080 macroblocks against reconstructed
pixel coordinates, coefficient-derived masks, skip flags, outgoing context
edges and selected diffusion splits. They cover 205 preserved DC contexts
across I4 blocks and 1,188 nonzero diffusion commits. The packed DC context
persists through I4 and left DC resets at row boundaries. Native comparisons
also retain exact trial scratch, rejected prefixes, both output destinations,
final residual statistics and full predictor maps.

`fixtures/lossy_search.txt` contains 60 portable C fixtures. Reviewed provenance
before generation is unchanged: clean upstream pin, GCC 15.2.0-16ubuntu1,
CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off, Rust 1.101.0-nightly
(`7189d39d9`, 2026-10-22), x86_64 Linux. Earlier fixtures remain unchanged.
Review `target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_search search_fixtures_match_pinned_reference
```

Milestone 5 is complete: 127,158 native requests and 105 portable fixtures across
reconstruction/diffusion and default mode search. Partition-0 syntax and opaque
container assembly are next; public lossy Rust APIs remain unimplemented.

Validation passes: full workspace `cargo test --quiet` and
`cargo test --features native-oracle --quiet`, all eight search tests in release
with `native-oracle`, `cargo fmt --all -- --check`, the normal library-only build
and `git diff --check`. Existing manual tests remain ignored; upstream remains
clean at the recorded pin.

## Milestone 6: partition-0 syntax and opaque containers

`lossy_syntax.c` includes the pinned `syntax_enc.c` and `tree_enc.c` with
external definitions renamed. Static leaves are called directly; the complete
partition/container path calls the original syntax writer and normally linked
mode/probability coding. Public encoder references continue using the normally
built pinned library. No syntax or coefficient-coding algorithms are copied
into the C adapters. Production Rust adds `enc/syntax_enc.rs` and the mode
writers in `enc/tree_enc.rs`.

The default single coefficient partition is supported. `GeneratePartition0`
returns an owned finished boolean writer; `VP8EncWrite` takes stored encoder
state, dimensions/profile and finalized token bytes and returns an owned opaque
file. Mode serialization borrows the frame immutably and leaves reconstructed
borders, coefficient contexts, diffusion errors and decisions intact. It does
not need C's incidental iterator reset during serialization. Buffer allocation,
19-bit partition-0 limits, dimensions, profiles and RIFF size arithmetic return
explicit errors. Pinned C counts its final padding byte inside the VP8 chunk
length; Rust preserves that convention. Multiple token partitions, alpha
container extensions and the real token frame loop remain later work.

New logical requests:

- `VP8ModeSyntax`: `(kind, mode, top, left, segment_p0, segment_p1, segment_p2)`.
  Kinds 0/1/2/3 write Intra4/Intra16/UV/segment syntax. Intra4 probabilities
  come from the pinned `(top, left)` table. Returns pre-marker bit position
  as low/high u32 words, then finished bytes including an 11-bit `0x5ad` marker.
- `VP8Syntax`: a 31-word header, the full predictor grid including its top row
  and left column, then four words per raster macroblock
  `(type, uv_mode, skip, segment)`. Header fields are `(op, width, height,
  profile, num_segments, update_map, simple_filter, filter_level, sharpness,
  i4_filter_delta, base_quant)`, the five signed quantizer deltas, three segment
  probabilities, `(use_skip_proba, skip_proba, coeff_seed)`, four interleaved
  `(segment_quant, segment_filter)` pairs and `token_byte_count`. Signed fields
  travel as i32 encoded in u32. Coefficient seed 0 retains defaults, 1 sets
  zeros, 2 sets 255 and other values set `(slot * 37 + seed) & 255`.
  Operations 0/1/2/3 write segment/filter/quant/mode syntax and return the
  same position/marker trace as `VP8ModeSyntax`. Operation 4 returns finished
  partition-0 bytes without a marker. Operation 5 returns the complete opaque
  file with deterministic raw token bytes `(offset * 73 + 19) & 255`; these
  synthetic token bytes exercise assembly and are not meant to decode.
- `VP8FixedImage`: `(width, height, profile)` returns a complete opaque file.
  Both adapters store Intra16 DC and UV DC modes with zero residuals, record
  the Y2, sixteen Y and eight UV blocks through their existing token recorders,
  emit tokens with default probabilities and call their syntax writer. Skip
  flags are true but skip syntax is disabled, so all residual blocks are coded.
  These files independently decode to RGBA `(130,130,130,255)` everywhere.
  This test-only checkpoint does not implement a substitute public encoder.

All 4,164 native requests match: 1,024 mode leaves (all 1,000 Intra4
mode/top/left combinations, eight Intra16/UV leaves and sixteen segment leaves),
3,096 header/raster/partition/container cases and 44 fixed-mode files. Fields
include every base quantizer, every signed 4-bit quantizer delta, every signed
6-bit filter delta and segment-filter value, and every signed 7-bit segment
quantizer. Mixed header states cover coefficient updates, map/skip signaling,
filter type/sharpness and nonzero/zero deltas, all mode contexts across partial
and multiple macroblock rows, profiles and odd/even token payloads.

Independent portable tests decode syntax trees, all headers and raster modes,
check trailing sentinels for leaf streams, inspect frame tags, check both sides
of the partition-0/RIFF limits without huge allocations, and verify repeated
writes preserve frame scratch. The existing `image` decoder checks all 44
fixed-mode files across eleven geometries and four profiles, including both
16,383-pixel skinny limits. `fixtures/lossy_syntax.txt` retains 276 compact
C requests/responses. Existing fixtures and the upstream pin are unchanged.

Provenance reviewed before generating fixtures: clean upstream pin,
GCC 15.2.0-16ubuntu1, CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off,
Rust 1.101.0-nightly (`7189d39d9`, 2026-10-22), x86_64 Linux. Review
`target/native-oracle/provenance.txt` before regeneration:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_syntax syntax_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_syntax -- --nocapture
```

Milestone 6 is complete. The next checkpoint is the default token frame loop,
segment/probability feedback, header-budget retries and final filter adjustment.
Public lossy Rust entry points remain unimplemented.

Validation passes: full workspace `cargo test` and
`cargo test --features native-oracle`, all eight syntax tests in release with
`native-oracle`, `cargo fmt --all -- --check`, the normal library-only build
and `git diff --check`. Existing manual tests remain ignored; upstream remains
clean at the recorded pin.

## Milestone 7: default token frame loop and probability feedback

`enc/frame_enc.rs` adds `GetProba`, `ResetSegments`, `SetSegmentProbas`,
`SetLoopParams`, macroblock `RecordTokens` and the method-4 single-pass
`VP8EncTokenLoop`. It returns an owned finished coefficient partition for the
syntax writer. The default path has no skip syntax, target search, user-configured
multiple passes, autofilter or picture export. Those optional branches remain
deferred; required header-budget retries and periodic/final probability fitting
are implemented. `enc/filter_enc.rs::VP8AdjustFilterStrength` adjusts each
segment from its largest observed edge and sets the frame's maximum strength.

The refresh countdown is decremented before testing `< 0`. With minimum count
96, fitting happens before zero-based macroblocks 96, 193, 290, etc.; the
interval is `max(96, macroblocks / 8) + 1`. A retry halves the Intra4 budget
and reinitializes iterator borders, contexts, diffusion, quantizer matrices,
max-edge statistics, tokens and token statistics. It preserves fitted
coefficient probabilities, predictor decisions and persistent segment merges.
The next `SetLoopParams` recomputes costs before token statistics are reset.
Final fitting deliberately leaves the cost cache stale until another pass
needs it, matching C. All-zero macroblocks still record their residual tokens.

A read-only observer on the Rust driver supports subgraph comparisons. The
normal entry point uses a no-op observer and the pinned partition-0 estimate
limit, `(524288 - 2048) << 11`. Test calls can lower that limit to exercise
retries with small inputs. `lossy_loop.c` includes the original `webp_enc.c`
and `frame_enc.c`, renaming external definitions and wrapping only observation
points. It uses the original encoder initializer and original token loop.
For synthetic low-limit tests, the C adapter adds
`pinned_limit - test_limit` to the first macroblock's header-cost estimate on
each pass. This drives the original retry comparison without changing modes,
coefficients, tokens or any loop algorithm. Traces report the unmodified real
scores and estimates. Normal public C references receive no injection.

New requests use the existing logical transport:

- `VP8FrameSegments`: `(num_segments, macroblock_count, initial_p0, initial_p1,
  initial_p2)`, then raster segment IDs. Returns `(update_map, map_cost)`, the
  three resulting probabilities and all resulting IDs. Includes the rounded
  all-255 case where one rare segment-1 block among 1,024 is remapped to zero.
- `VP8AdjustFilter`: `(filter_strength, sharpness, initial_frame_level)`, then
  four `(max_edge, y2_ac_quant, initial_segment_strength)` triples. Returns the
  frame level and all four segment strengths. Includes disabled adjustment,
  exact WHT division boundaries and saturation of the existing lookup table.
- `VP8LoopFrame`: `(width, height, quality_f32_bits, num_segments,
  max_i4_header_bits, partition0_estimate_limit, source, trace, rgba_stride)`,
  followed by tightly packed Y/U/V planes for source 0 or opaque RGBA bytes
  for source 1. The RGBA importer uses a padded positive stride with no final-row
  padding and the C adapter starts at an unaligned byte. The normal public
  simple API is compared separately, using the existing `WebPEncodeRGBA` request.
  Trace 0 returns complete WebP bytes. Trace 1 adds events, final frame state,
  final predictor grid/macroblock information, coefficient-partition bytes and
  complete file bytes, with variable arrays prefixed by their length.

Event tags are pass start (0), periodic refresh (1), macroblock after token
recording and before boundary save (2), pass end (3), final fitting before token
emission/disposal (4) and final filter state (5). Pass/refresh events include the
pass/index and logical segment/header state, full coefficient probabilities and
statistics, and a level-cost cache hash. Macroblock events include scores,
coefficient/pixel hashes, selected modes/skip/segment, all outgoing nonzero
contexts and diffusion errors. Emission includes token count/hash, plus every
packed token for frames of at most 64 macroblocks. Larger traces use compact
summaries alongside exact probabilities, partition bytes and complete files.
C's uninitialized scratch/padding is excluded. Token inspection stays in
`lossy_tokens.c`, which already has access to the pinned page layout.

All 2,123 new native requests match: 1,057 segment/filter leaves, 410 composed
YUVA frame-loop traces (including forced retries) and 328 opaque RGBA files,
plus 328 separate requests to the normally linked public C encoder. The 328
RGBA outputs match public C byte for byte and decode independently. Geometry
includes partial/multiple macroblock rows and both 16,383-pixel skinny limits.
Qualities include 0, 1, 30, 50, 75, 98, 98.5, 99 and 100. Boundary frames have
95/96/97/98, 193/194/195, 767/768/775/776/783/784/792/800 macroblocks.
The pinned C first-pass header cost for the mixed 17×19 case at quality 75 is
20,912 units; tests and fixtures retain limits 20,911/20,912/20,913 to prove
strict greater-than retry behavior. A forced 97-block retry retains nondefault
probabilities from its periodic fit while clearing the other pass state.

Portable checks independently round segment probabilities and walk their tree
costs, verify monotonic filter adjustment, trace 219 probability refreshes and
199 retries, and check a retained nondefault probability state on restart.
Separate boolean context grids validate 301 macroblocks (137 I4, 48 skip flags),
rebuild every packed token from independent coefficient syntax trees, check
all adaptive statistics and independently decode 152,929 final token bits.
Another 151 complete files decode with correct dimensions and opaque alpha.
`fixtures/lossy_loop.txt` retains 31 C fixtures, including periodic boundaries,
header-cost ties and retries before/after probability refresh. Existing fixture
files and the upstream pin remain unchanged.

The existing real-image sample, decoded through `image`, records these RGB
PSNR/size baselines against its source. Rust and the pinned C encoder produce
identical bytes (and therefore identical decoded color):

| Color quality | File bytes | RGB PSNR |
| --- | ---: | ---: |
| 30 | 1,060 | 31.413 dB |
| 75 | 2,074 | 35.732 dB |
| 98.5 | 6,010 | 47.737 dB |
| 100 | 6,150 | 48.265 dB |

Provenance reviewed before fixture generation remains clean pin
`1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1, CMake 4.2.3,
Release `-O3 -DNDEBUG`, SIMD/threading off, Rust 1.101.0-nightly
(`7189d39d9`, 2026-10-22), x86_64 Linux. Review
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_loop frame_loop_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_loop -- --nocapture
```

Milestone 7 is complete. ALPH and shared public lossy integration are next;
public lossy Rust APIs remain unimplemented.

Validation passes: full workspace `cargo test` and
`cargo test --features native-oracle`, all nine frame-loop tests in release
with `native-oracle`, `cargo fmt --all -- --check`, the normal library-only
build and `git diff --check`. Existing manual tests remain ignored and the
upstream checkout remains clean at the recorded pin.

## Milestone 8: alpha planes, centroid quantization and ALPH

The Rust alpha path now consists of `dsp/alpha_processing.rs`, encoder-only
`dsp/filters.rs`, `utils/{filters_utils,quant_levels_utils}.rs` and
`enc/alpha_enc.rs`. It dispatches alpha to green with A/R/B zero, retains scalar
horizontal/vertical/clipped-gradient filtering and the sampled filter estimator,
quantizes levels with fixed source extrema, and runs the existing headerless
`VP8LEncodeStream` at method 4, quality 32, exact true. The implementation is
synchronous and owns its buffers; Rust drops replace C worker cleanup/free
operations. No decoder, SIMD, worker framework or new lossless encoder is added.

`QuantizeLevels` retains six centroid iterations, lower-class midpoint ties,
fixed minimum/maximum, the strict floating-point convergence criterion and
rounding after the final class assignment. Its returned SSE truncates the
floating-point centroid error before rounding representatives, which can differ
from the final integer-plane SSE. For example, `[0,4,5,6,7,10]` at three levels
becomes `[0,6,6,6,6,10]` with reported SSE 5, while integer-plane SSE is 6.
Frequency sums/products are widened to avoid signed C aggregate overflow on
large planes. Inputs with already-small palettes remain unchanged with SSE 0.

Alpha quality below 100 maps to `2 + quality / 5` levels through quality 70,
then `16 + (quality - 70) * 8`. The preprocessing bit is set whenever quality
is below 100, including unchanged small palettes. C's scalar filter values are
0/1/2/3; `LAST=4`, `BEST=5`, `FAST=6`. At default effort 4, FAST always includes
NONE and may also try the estimator's candidate. The estimator uses occupied
coarse-error bins, rather than their frequencies. Trial selection keeps the
first strictly smallest payload. A compressed payload falls back to raw only
when larger than the source, preserving compressed/raw equal-size ties and
retaining the filter bits if a filtered trial falls back.

`VP8EncStartAlpha` returns no payload for opaque pictures; otherwise it copies
and optionally quantizes the alpha plane and returns owned ALPH bytes plus
SSE. Color import/coding continues using original alpha, and quantization never
changes the source picture. `syntax_enc.rs` adds checked extended RIFF assembly:
`VP8X`, `ALPH` with its unpadded chunk size and optional zero padding, then VP8.
The existing opaque assembly delegates to the same writer with no alpha.
Partition-0, file/plane arithmetic and chunk lengths are checked before writing.
The pinned convention of counting VP8's final padding inside its chunk size is
preserved. Shared public lossy APIs/configuration remain milestone 9.

`lossy_alpha.c` includes the original `alpha_enc.c`, renaming external lifecycle
symbols. It directly invokes its static helpers, normally linked scalar DSP,
`QuantizeLevels` and the existing stream encoder. No alpha algorithms are copied
into the C wrapper. The logical `AlphaOps` request has a nine-word header
`(op, width, height, stride, param0, param1, param2, reserved, dst_stride)`, then
one word per source byte, excluding padding after the final row:

| Op | Original function | Parameters and logical result |
| --- | --- | --- |
| 0 | `WebPDispatchAlphaToGreen` | Destination stride in header; returns the full guarded u32 buffer, initialized to `5aa5a55a` |
| 1 | `WebPFilters` | param0 is scalar filter; returns bytes including preserved padding and five `a5` guard bytes |
| 2 | `WebPEstimateBestFilter` | Returns the filter index; tightly packed input |
| 3 | `QuantizeLevels` | param0 levels, param1 null-SSE flag, param2 null-data flag; returns success, SSE low/high and mutated input; SSE sentinel is `12345678` |
| 4 | `GetNumColors` | Returns the distinct color count, ignoring stride padding |
| 5 | `GetFilterMap` | param0 filter option, param1 effort; returns filter bitmap |
| 6 | `EncodeLossless` | Returns finished headerless method-4/q32/exact VP8L bytes |
| 7 | `EncodeAlphaInternal` | param0 compression, param1 scalar filter, param2 reduce-levels flag; returns control byte and trial payload |
| 8 | `ApplyFiltersAndEncode` | param0 compression, param1 filter option, param2 reduce-levels flag; returns selected trial |
| 9 | `EncodeAlpha` | param0 signed quality, param1 compression, param2 filter option; returns success, SSE low/high and final payload; accepts padded input stride |

`VP8AlphaFile` uses the existing custom-alpha public request layout
`(width, height, rgba_stride, color_quality_f32_bits, signed_alpha_quality)`, then
RGBA bytes. C routes this request to the normally linked custom public wrapper;
Rust composes its internal importer, cleanup, analysis, alpha job, token loop
and extended syntax writer. No public Rust lossy entry point is exposed yet.

All 8,614 native requests match, including 3,259 complete files. The corpus
covers every alpha quality 0 through 100, quantizer levels 0 through 257
(including rejection), both compression choices, all four scalar filters,
NONE/BEST/FAST trials, 16/17 and 192/193-color thresholds, odd/padded/minimum
planes, all four estimator choices, both skinny geometry limits and a real
image with synthetic smooth alpha. The complete public-file subset covers
alpha qualities 0/4/5/10/30/69/70/71/99/100 and color qualities
0/1/30/50/75/98/98.5/99/100, opaque/zero/binary/smooth/noisy/two-level alpha and
hidden RGB. All 3,259 files also decode independently: 1,098 raw and 1,620
compressed ALPH chunks and 316,376 changed decoded alpha samples are observed.
Quality 100 preserves original alpha everywhere. Lower qualities match C bytes
and therefore the same decoder's smoothing semantics; they are not assumed to
equal the unsmoothed centroid plane. Opaque output is independent of alpha quality.

Independent portable checks invert every filter, exhaust all 256^3 gradient
predictions, check untouched padding, centroid extrema/order/small palettes,
explicit midpoint/SSE examples, input/configuration failures, chunk layout and
synthetic RIFF size limits. They decode 87 headerless VP8L streams by adding
only ordinary image/container headers, checking exact green and zero A/R/B.
Trial checks invert 890 raw and 358 compressed payloads, retain 12 exact
compressed/raw size ties, and verify earliest selection in 102 equal-size
filter comparisons. Another 172 complete file decodes observe both raw and
compressed ALPH and changed quantized alpha. `fixtures/lossy_alpha.txt` retains
70 portable C fixtures, explicitly including centroid SSE rounding, compressed/
raw ties, earliest filter ties and padded alpha extraction. Earlier fixtures
and the source pin are unchanged.

Provenance reviewed before generation: clean pin
`1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1, CMake 4.2.3,
Release `-O3 -DNDEBUG`, SIMD/threading off, Rust 1.101.0-nightly
(`7189d39d9`, 2026-10-22), x86_64 Linux. Review
`target/native-oracle/provenance.txt` before regeneration:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_alpha alpha_fixtures_match_pinned_reference
cargo test -p libwebpop --features native-oracle --test lossy_alpha -- --nocapture
```

Milestone 8 is complete. Shared public lossy configuration/picture routing and
the public APIs are next.

Validation passes: full workspace `cargo test` and
`cargo test --features native-oracle`, all eleven alpha tests in release with
`native-oracle`, the portable alpha suite, `cargo fmt --all -- --check`, the
normal library-only build and `git diff --check`. Existing manual tests remain
ignored; upstream remains clean at the recorded pin.

## Milestone 9: public APIs and CLI

`WebPEncodeRGBA` and `WebPEncodeRGBAWithAlphaQuality` now return owned complete
WebP files through the production RGBA importer and synchronous default VP8
path. Standard encoding delegates to the custom-alpha API at alpha quality
100. The checked private lossy configuration retains `f32` color quality and
signed `i32` alpha quality; it rejects nonfinite/out-of-range settings before
allocation, including invalid alpha settings on opaque input. The importer
reuses the lossless checked geometry: dimensions 1–16383, positive stride at
least width*4, minimum input through the final pixel with checked arithmetic.

The shared `webp_enc.rs` glue uses separate typed ARGB/VP8L and YUVA/VP8
routes. It performs transparent cleanup, analysis, alpha compression, token
coding and final container writing in the upstream order. VP8L effort is never
used to store fractional color quality. `LosslessOptions` and the
`WebPEncodingError = LosslessEncodeError` alias remain; added variants report
invalid alpha quality, alpha encoder errors and VP8 container failures.
Allocation failures continue to propagate through `Result`.

The existing public logical requests now evaluate the exported Rust APIs.
Their layouts, native C implementations and all earlier text fixtures are
unchanged. `lossy.rs` compares all 3,566 original public outputs to C, then
independently decodes and checks alpha/container semantics. It observes 126,648
changed alpha samples, 1,215 raw ALPH chunks and 1,755 compressed ALPH chunks.
Cross-case checks establish standard/custom-100 identity and opaque invariance.
The 32 committed public reference files also compare exact Rust bytes now.

`lossy_public.rs` adds 310 exact public comparisons and independent decodes,
covering the real image with alpha, skinny dimension limits, 64 seeded images
and fractional qualities through both APIs, every integer color quality with
matching integer alpha quality, adjacent f32 values at quality boundaries,
frames across the probability refresh boundary and interval growth, and all
baseline input/quality combinations. Source buffers begin at an unaligned
slice offset and remain unchanged. These additional cases observe 785,297
changed decoded alpha samples. Explicit decoded RGB comparisons against the C
files have zero difference. Portable tests check padded/packed/surplus buffers,
unchanged input through transparent cleanup, invalid dimensions/strides/short
buffers/overflow, signed alpha extremes, NaN/infinities/out-of-range floats,
single-row `usize::MAX` stride and both maximum skinny shapes. They also prove
fractional quality changes encoded output and q100 still changes visible RGB.

Two new binary real-image fixtures supplement those corpus comparisons:

| File in `fixtures/corpus/` | Public C configuration | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `test_ref.lossy.webp` | Standard RGBA, color quality 75 | 2,074 | `69b51cca94353c41bf9ff01e3995c86d0d5f377c998e72b937af7818e1c66dfa` |
| `test_ref.alpha.webp` | Custom RGBA, color quality 75, alpha quality 70, synthetic alpha `(17*x+13*y)&255` | 2,540 | `6b6797d481f1921f8590c21209bee39d36001fc451d0e41490482fa279465956` |

Source/toolchain provenance was reviewed before generating these files: clean
pin `1844042d1ca751e62e61df7617dba19c98588339`, GCC 15.2.0-16ubuntu1,
CMake 4.2.3, Release `-O3 -DNDEBUG`, SIMD/threading off, x86_64 Linux,
Rust 1.101.0-nightly (`7189d39d9`, 2026-10-22). No existing fixtures or upstream
sources changed. Reproduce with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test lossy_public real_public_fixtures_match_pinned_c
cargo test -p libwebpop --features native-oracle --test lossy --test lossy_public -- --nocapture
```

`cwebpop --lossy` calls the custom public API, preserving fractional `-q`
(default 75), accepting `--alpha_q` (default 100) and supporting method 4.
The existing lossless default remains. Lossy mode conflicts with lossless,
near-lossless and exact settings; unsupported lossy controls and alpha quality
outside lossy mode fail before opening input/output. Executable tests verify
file and stdin/stdout output against the library, alpha settings, noalpha,
reporting and preservation of existing output on invalid settings.

Validation passes: full workspace `cargo test` and
`cargo test --features native-oracle`, release public suites with the native
oracle, final portable/new binary fixture tests, CLI unit/executable tests,
`cargo fmt --all -- --check`, `git diff --check` and
`cargo build -p libwebpop --lib`. Manual timing tests remain ignored by default.
The [initial lossy baseline](../../../docs/benchmarks/2026-10-24-lossy.md) records
28 size/PSNR/time/process-memory observations with reproduction instructions.
All scalar lossy milestones 0–9 are complete; deferred advanced configurations
and performance work remain outside this scope.

## Remaining lossy methods: milestone 0

The [methods extension](../../../docs/LOSSY_METHODS_PLAN.md) starts from
repository revision `faefdbc` on 2026-10-24. The upstream pin is unchanged and
clean. The existing `LossyOptions`/`encode_lossy_rgba` and SharpYUV routing are
preserved. This batch adds reference coverage before porting additional Rust
algorithms; public Rust encoding still selects method 4.

New requests share the existing u32-word transport:

- `WebPEncodeLossyMethod`: `(width, height, byte_stride, quality_f32_bits,
  alpha_quality_i32, method_i32, use_sharp_yuv)`, followed by RGBA bytes through
  the minimum reachable last pixel (no final-row padding). Returns complete
  file bytes, or an empty response for invalid quality/alpha/method settings.
  `lossy_methods.c` uses `WebPConfigPreset(WEBP_PRESET_DEFAULT, quality)`, applies
  those fields, validates config and calls the normally linked `WebPEncode`.
  Ordinary conversion occurs during YUVA import; sharp conversion retains ARGB
  until encoding. The input allocation begins at an unaligned offset and is
  checked unchanged. The copied custom alpha-quality wrapper is unchanged.
- `VP8MethodTools`: `(width, height, quality_f32_bits, method, low_memory,
  partitions_log2)`. Returns `(method, RD_level, use_tokens, num_parts,
  do_search, max_i4_header_bits, mb_header_limit_low, mb_header_limit_high,
  has_diffusion_storage, profile)`. `lossy_loop.c` calls the original static
  `InitVP8Encoder`/`MapConfigToTools`, then destroys the encoder. This request
  inspects low-memory/partition routing only; it adds no public support for
  those settings and emits no multi-partition files.
- `VP8MethodLoopFrame`: selected method followed by the **unchanged**
  `VP8LoopFrame` request. It requires default segments/header budgets and no
  old-style detailed trace; planar YUV and opaque RGBA inputs are supported.
  The included original frame loop is selected through `enc->use_tokens`.
  No retry bias is injected. Returns `(method, RD_level, use_tokens, num_parts,
  scan_count)`, then six words per scan: `(RD_level, visited_macroblocks,
  Intra16_count, Intra4_count, zero_residual_count, do_trellis_count)`. The tail
  is `(midframe_cost_refreshes, token_emissions, use_skip_proba, skip_proba,
  nb_skip, coefficient_probability_hash, final_filter_level,
  coefficient_partition_bytes, file_bytes)`, followed by the complete opaque
  WebP file. The probability hash uses the existing FNV-1a word fold over
  type/band/context/node order. `do_trellis_count` observes the flag **after
  decimation**, so both method 5's final requantization and method 6's full
  search count; candidate-level invocation traces belong to the trellis port.

The compact observations avoid inspecting unused coefficient/mode fields from
the non-RD route. Direct methods run statistics then final coding; method 0's
probe may visit only part of the frame. Token methods retain one coding scan
and no skip syntax. Existing detailed method-4 fixtures and request layouts
remain unchanged. The oracle now rejects builds with `DISABLE_TOKEN_BUFFER`
as well as SIMD/threading configuration macros.

There are 70 portable fixtures in `fixtures/lossy_methods.txt`: 21 setup
snapshots, 35 complete files and 14 compact loop traces. Portable tests validate
their generated inputs, setup rules, visit/skip/trellis/refresh invariants,
containers, independent decodes and exact source alpha at alpha quality 100.
Method-4 files and frame outputs compare exact Rust bytes. Other methods are
C reference artifacts until their ports are implemented, not Rust parity claims.

Live validation adds:

- 3,556 normal-library C files independently decoded, including all methods,
  fractional color quality, alpha qualities 0/99/100, SharpYUV fallback and
  active conversion, and the committed real-image sample. It observes 52,696
  changed alpha samples, 1,393 raw and 1,568 compressed ALPH chunks.
- 672 tool mappings over methods, quality/diffusion boundaries, skinny geometry,
  low-memory selection and requested partition counts. Buffered paths force
  one partition; direct setup retains the requested count.
- 336 compact traces over both macroblock families for every method, skip on
  and off for each direct method, method-0 probe boundaries at 49/50/51 and
  199/200/201/204 macroblocks, and token refresh boundaries/interval growth.
  Fourteen observed opaque RGBA encodes match the normally built public C
  library, checking that instrumentation preserves output.
- Invalid finite/non-finite color qualities, signed alpha qualities and method
  extremes; standard-simple method-4 identity; opaque alpha-quality invariance.

Pre-port method-4 fixture baselines (ordinary import; synthetic patterns from
`support::lossy::image`, seed `6d2b79f5`):

| Input | Color/alpha quality | Bytes | SHA-256 |
| --- | --- | --- | --- |
| 1×1 opaque ramp, pattern 0 | 75 / 100 | 44 | `1fbd6706e78d8b65bbfc2799c913b1f8cc303f56932305e05d5bfe5d3886316c` |
| 3×5 binary alpha, pattern 2 | 98.5 / 100 | 172 | `72509043fa9064aac818376f2d68f4e85de9ad403f3ea8db0a55a745442894b6` |
| 5×7 noisy RGBA, pattern 4 | 75 / 100 | 202 | `d8ee83d257f8f8d64228bc7c0ceb6e2214d22f2c04e401d12a3aa6dc613a6e93` |
| 5×7 noisy RGBA, pattern 4 | 98.5 / 71 | 272 | `ec448deab1b93da8593357753897aedb66cbfbb5a6334c7233124a904d88d47a` |
| 17×19 smooth alpha, pattern 3 | 100 / 0 | 590 | `ed202a8d66d2040666801e9143b5625f6024e510f1c8478f3b80e2f7f17b8768` |

Source/toolchain review before generating new fixtures: clean pin
`1844042d1ca751e62e61df7617dba19c98588339`, x86_64 Linux,
GCC 15.2.0-16ubuntu1 (`/usr/bin/cc`), CMake 4.2.3, Release `-O3 -DNDEBUG`,
SIMD/threading off, token buffering enabled, and Rust 1.101.0-nightly
(`7189d39d9`, 2026-10-22). The normal harness retains full build commands and
provenance in `target/native-oracle/`. No old fixtures, pin or upstream sources
were changed. Regenerate only after reviewing provenance:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test lossy_methods method_fixtures_match_pinned_c -- --nocapture
cargo test -p libwebpop --test lossy_methods
cargo test -p libwebpop --features native-oracle --test lossy_methods -- --nocapture
cargo test -p libwebpop --release --features native-oracle --test lossy_methods -- --nocapture
```

Validation passed: full workspace `cargo test`, full
`cargo test --features native-oracle`, the new native suite in release,
`cargo build -p libwebpop --lib`, `cargo fmt --all -- --check`, and
`git diff --check`. All inherited method-4 fixture comparisons remain unchanged.
The remaining-method algorithms and public method selection are subsequent
milestones, not implemented by this reference batch.
