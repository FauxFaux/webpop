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
