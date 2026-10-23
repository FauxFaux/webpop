The primary live oracle is the scalar **C** libwebp implementation in
`upstream/`, pinned to `1844042d1ca751e62e61df7617dba19c98588339`. No C++
reimplementation is involved. The wrappers call the original functions directly.
They are test-only and live outside the submodule.

Run from the repository root:

```sh
# Portable fixtures, exhaustive arithmetic checks, and independent decodes:
cargo test -p libwebpop

# Also configure/build the pinned native oracle and run live comparisons:
cargo test -p libwebpop --features native-oracle -- --nocapture

# Production library build: no C compiler, CMake, upstream, or Wasm required:
cargo build -p libwebpop --lib
```

The live tests require Git, CMake >= 3.16, and a C99 compiler. They check the
upstream revision and reject modifications, then build only `webpop_oracle` and
its static-library dependencies under `target/native-oracle/`. Both SIMD and
threading are disabled; near-lossless support is compiled in to match upstream,
but the simple API uses `near_lossless=100`. CMake uses Release mode. Compiler
selection follows CMake (`CC` on the first configuration). To change compilers,
use a fresh build directory. There is no Cargo build script or production FFI.
Enabling the feature on a library build does not invoke the oracle.

The large public RGBA, encoder-stream, near-lossless public corpus and feedback
sweeps split their cases across up to eight workers each, bounded by available
CPU parallelism. Each worker runs a separate scalar oracle process and checks
its Rust results; no encoder-internal threading is enabled. Requests and
responses retain worker suffixes (for example `public-rgba-0.requests.txt`) in
`target/native-oracle/`. Case coverage and ordered cross-case checks are retained.

The [near-lossless reference record](NEAR_LOSSLESS.md) describes standalone
preprocessing, predictor quantization leaves, the copied custom public C
wrapper, integrated feedback/search and per-candidate routing, their request
layouts and fixture regeneration commands. The Rust near-lossless public
encoder matches pinned C. An independent older-full-Wasm regression is opt-in
through `wasm-oracle`; see [artifact and host provenance](full-wasm/README.md).

The [lossy reference record](LOSSY.md) describes the complete scalar VP8 DSP,
both lossy public C adapters, the copied alpha-quality wrapper, portable
VP8/ALPH files and independent decoding. The public lossy Rust encoder remains
unimplemented. This extends the same scalar backend and word protocol.

This native backend was selected because it can expose internal/header leaves
directly from the pinned checkout. The older lossless-only bundled Wasm regression has been
removed; its seven synthetic image cases now compare full public Rust output
with pinned C and independently decode. Normal tests use committed C fixtures.

`tests/support/mod.rs` defines logical requests and the Rust evaluator separately
from `tests/support/native.rs`, which handles builds and transport. Add a
function to the evaluator and to `oracle.c`, then add fixtures and deterministic
cases. Header inline functions compile in the wrapper translation unit. Exported
kernels and scalar DSP dispatch tables expose the current remaining leaves;
initialization selects the scalar implementation because SIMD is disabled.
Future static helpers without a dispatch entry should get wrappers in a
translation unit including their original `.c` file.

The protocol uses text lines:

```text
# request: function decimal-input-count hexadecimal-u32-inputs...
VP8LSubPixels 2 00000000 01010101
# response: decimal-output-count hexadecimal-u32-outputs...
1 ffffffff
```

Comments are documentation only; the executable expects request lines without
comments. Every output is a logical value, never a pointer, padding, or capacity.
Transforms return the complete mutated buffer. Prefixes return code, bit count,
and (except Bits variants) extra-bit value. Encoding returns one word per byte.
`VP8LColorCache` takes hash bits followed by `(op, key, pixel)` triples: insert
0, contains 1, get-index 2, lookup 3, set 4, copy-to-second-cache 5,
lookup-in-copy 6. Unused fields are zero. Contains uses `ffffffff` for absent.
Responses contain query results followed by the complete active and copied
tables, exposing collisions and copy independence.

64-bit values use two words, low word first, with signed values represented in
two's complement. `VP8LPredictors` takes `(mode, left, top-left, top, top-right)`.
`VP8LPredictorsSub` takes mode, the input including a preceding pixel, then the
upper row including top-left and a trailing top-right pixel (2*n+4 words total).
Entropy results are `(entropy-low, entropy-high, sum, nonzeros, max_val,
nonzero_code)`, followed by two counts and four row-major streak counters for
the streak functions. Combined entropy inputs concatenate equal-sized X/Y
arrays (256 each for combined Shannon entropy); combined Shannon reports the
sum of entropies of X and X+Y. `VP8LBundleColorMap_C` takes xbits then indices.
Color collectors take `(stride, width, height, coefficient0, coefficient1)`,
256 initial histogram bins, then pixels; stride is in pixels and the second
coefficient is unused for red. Vector operations concatenate equal-sized
arrays. These layouts expose logical fields and accumulated output buffers.

Milestone 2 adds these requests:

- `VP8LBitWriter`: initial capacity hint followed by `(op, value, n_bits)`
  triples. Operations are put bits (0), clone into a saved writer (1), reset to
  the saved cursor/accumulator (2), swap writers (3), finish (4), inspect (5),
  and wipe/reinitialize with `value` as the capacity hint (6). Reset preserves
  the destination's flushed prefix, following C. Each inspection, including an
  implicit final inspection, returns the pre-finish byte count, finished byte
  count, and finished bytes from a clone. Capacity and accumulator layout are
  deliberately excluded.
- `VP8LCreateHuffmanTree`: depth limit followed by histogram counts. Returns
  the mutated histogram, code lengths, and canonical reversed codes, in that
  order. This exposes histogram smoothing as well as tree construction.
- `VP8LCreateCompressedHuffmanTree`: code lengths; returns `(code, extra_bits)`
  pairs. The initial repeat value is 8, including across zero runs.
- `StoreHuffmanCode`: `(prefix_value, prefix_n_bits, depth_limit, alphabet_size)`,
  histogram counts, then optional `(symbol, extra_value, extra_n_bits)` triples.
  Stores the header, clears singleton codes, writes those symbols with extra
  bits, and appends an 11-bit `0x5ad` marker to expose the precise bit length.
  Returns a writer inspection followed by the cleared lengths and codes.
  `huffman.c` includes the pinned encoder translation unit to call its static
  serialization helpers directly; its external encoder entry points are renamed
  so public encoding tests still use the normally built library.

Milestone 3 adds these requests, with layouts shared by
`tests/support/references.rs` and the native backend:

- `VP8LBackwardRefs`: block size followed by `(op, value, length)` triples.
  Operations append a literal (0), cache index (1), or copy (2); recycle (3),
  clone to saved refs (4), swap (5), release (6), spatially map distances using
  width `value` (7), or inspect (8). Inspection returns token count, then
  `(mode, length, payload)` per token, with modes 0/1/2. Both active and saved
  refs are inspected at the end. No capacity or linked-block addresses are
  compared; cursor sequences span C's 256-entry minimum and larger blocks.
- `VP8LDistanceToPlaneCode`: width and raw distance; returns the spatial code.
- `PopulationCost`: population counts; returns refined entropy, population
  cost (each low/high word), trivial symbol, and used flag. Exhaustive short
  populations cover each entropy refinement branch and streak threshold;
  seeded larger populations exercise log fallbacks and unsigned cost wrapping.
- `VP8LHistogram`: cache bits, width (0 disables the distance modifier), then
  `(op, value, length)` triples. Append tokens (0–2), store buffered refs without
  reset (3), create from buffered refs (4, value 0 preserves cache bits, 1 sets
  them), init (5, value resets arrays), compute cached costs (6), inspect (7),
  set a flattened population bin (8, index/value in the remaining two words),
  set bin id (9), copy saved histogram (10), or save current histogram (11).
  Inspections, including one implicit final inspection, return cache bits,
  all five populations in literal/red/blue/alpha/distance order, five trivial
  symbols, cached total cost, five costs, five used flags, bin id, and estimated
  total cost. All costs use low/high word pairs. Cached costs exclude copy
  extra bits; estimates include them. Raw copy distances are bounded by
  `LZ77_WINDOW_SIZE = (1 << 20) - 120`, leaving room for spatial mapping.
- `VP8LHistogramSet`: size and cache bits. Dirty every histogram, shrink the
  active size, then clear; return size, maximum size, and every histogram in
  the inspection format above. Set clearing zeroes metadata, whereas individual
  histogram initialization uses nontrivial-symbol sentinels and used flags.

`references.c` and `histogram.c` include the pinned translation units to access
static helpers, renaming external definitions so the public encoder still uses
the library normally. C allocations are explicitly initialized before reading
their populations. Rust uses initialized owned vectors, retains storage across
recycled trials, and propagates allocation failures with `Result`.

Differential cases exhaust byte arithmetic and signed color coefficient/channel
pairs, exercise all prefix entries and both lookup/slow paths, test prefix
boundaries, and run seeded arrays and cache traces for every legal cache size.
The additional scalar DSP batch has 317,625 cases: every log input through
65536, large-value floating-point fallbacks, byte-pair averaging/half predictors,
boundary triples and selector ties across all modes, residual batches, short
exhaustive histograms, zero/nonzero streaks around lengths 3/4, all signed color
coefficients, padded tiles, accumulating/wrapping bins, vector mismatch positions,
and every palette packing depth with incomplete bundles. Rounding tests cover
signed operands and all legal right shifts, including unsigned wraparound.
The xorshift32 seed is `6d2b79f5`; failures report the function and first differing
output word. Full batches, responses, and `mismatch.request.txt` are retained in
the build directory. Replay a mismatch with:

```sh
target/native-oracle/webpop_oracle < target/native-oracle/mismatch.request.txt
```

Committed fixtures live in `tests/fixtures/leaves.txt`. They use
`function hex-inputs... | hex-outputs...`, with counts inferred by the test
parser. They cover each ported operation and two complete public encodings.
The portable tests compare Rust leaves directly against those C-produced
outputs and independently decode the public encoding fixtures. Regenerate only
after reviewing source/toolchain changes:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle \
  --test native_oracle committed_fixtures_match_reference
```

Initial fixture provenance (2026-10-22): Ubuntu x86_64 Linux, GCC
`15.2.0-16ubuntu1`, CMake `4.2.3`, Release `-O3 -DNDEBUG`, static libwebp,
SIMD/threading off, default near-lossless support on, no additional C flags.
Fixture inputs are synthetic edge cases listed in the fixture itself; no
external image corpus is used yet. Each live run records the current compiler
version, CMake cache, exact compilation commands, Rust host/toolchain, pin, and
seed in `target/native-oracle/provenance.txt`. Build and protocol failures fail
the test rather than silently skipping comparisons.

Milestones 1–4 are implemented, with 129 additional compact DSP fixtures and
100 bit-writer/Huffman fixtures generated on the same toolchain/configuration
as the initial fixtures. Milestone 2 adds 11,852 native comparisons: every
pending offset 0–63 and next-write width 0–32, C buffer growth, clone/reset/swap
trials, exhaustive short populations, seeded histograms up to the 2328-symbol
literal/length/cache alphabet, depth-limit retries, RLE boundaries, and both
trimmed-length encodings. Portable tests independently pack bits, expand RLE,
decode serialized headers and symbols, and check padding. No divergence was
observed. Milestone 3 adds 27,919 native comparisons, 40 portable C fixtures,
and six independent Rust tests covering hand-computed populations, storage and
cursor boundaries, histogram reset state, distance modifier equivalence,
allocation-size overflow, and token replay with overlapping copies and cache
updates. All comparisons pass in debug and release on the recorded toolchain.
Hash chains, LZ77 generation, cache selection, traceback and reference trial
selection are implemented. Histogram clustering and the Rust public encoder
remain unimplemented. Checked RGBA geometry and the
overflow-safe extension of `VP8LSubSampleSize` have portable Rust boundary tests; the C oracle
is invoked only on its defined domain. In particular, zero-bit scans and NoLUT
prefix distances below 3 are invalid upstream inputs.


Milestone 4 adds 32,212 native comparisons (15,180 matching/cache, 10,066
traceback and population-cost conversion, 6,966 complete reference selection)
with seed `6d2b79f5`, plus 79 portable C fixtures (including the two prefix-zero
fixtures). All run with the recorded scalar pin/configuration. Six independent
Rust tests cover saturated run boundaries, hash storage reuse, cache and
singleton costs, path packing, randomized interval splitting, and saturation of
the 500-slot interval pool against an eager minimum-cost expansion. Generated
reference sequences are independently replayed against the input pixels; final
spatial distances are inverted before replay. No numerical divergence was seen.

New protocol requests:

- `VP8LMatches`: `(width, height, quality, low_effort, strategy, cache_bits)`,
  then tightly packed ARGB pixels. Strategies are hash only (0), RLE (1),
  standard LZ77 (2), box LZ77 (3), or cache selection/rewrite after standard
  LZ77 (4). Returns the packed match array (the box array for strategy 3),
  selected cache bits for strategy 4, then a reference inspection.
- `VP8LCostEstimates`: population counts; returns fixed-point symbol costs.
- `VP8LTrace`: same inputs as `VP8LMatches`, with source strategy 1–3.
  Returns literal/red/blue/alpha/distance cost-model arrays, the complete
  traceback distance array, chosen-path count and lengths, then refined
  reference inspection. `cost.c` is included by `references.c` to expose
  static helpers from the pinned cost translation unit. Each logical stage is
  compared separately, and Rust's integrated traceback entry point is checked
  against the staged path.
- `VP8LGetBackwardReferences`: `(width, height, quality, low_effort,
  lz77_types, cache_bits_max, do_no_cache)`, then ARGB pixels. Returns selected
  cache bits and primary reference inspection, plus a no-cache inspection if
  requested. The low-effort branch only populates the primary result, matching
  upstream. Copy distances are spatial codes in this request.

Quality thresholds 24/25/26, 50/51, 75/76, all cache sizes, every strategy mask,
low effort, exhaustive eight-pixel binary inputs, repeated rows, random palettes,
and runs around length 4095 are covered. Progress hooks are omitted from the
Rust internal API. Prefix-table entry zero is defined upstream and used by the
traceback length-cost cache; NoLUT entry points still require inputs >= 3.

Milestone 5 auxiliary integration is complete. `auxiliary_streams_match_pinned_reference`
compares 720 complete `EncodeImageNoHuffman` streams (including an 11-bit
sentinel to expose trailing-bit differences), using seed `6d2b79f5` and the
same pinned scalar build. Cases cover five geometries, six patterns, four
qualities, both hash-chain effort settings, and three initial bit offsets.
Thirty C responses are retained in `tests/fixtures/auxiliary.txt`; each line
contains the existing request protocol, ` | `, then the response protocol.
These are cases with zero-based index `i % 24 == 16` from the live corpus.
`auxiliary_images` checks these without compiling C and independently decodes
75 test-only untransformed RIFF images with exact RGBA equality. Reusable
container headers and direct cache/multi-group serialization are covered below.

The `image_headers_match_pinned_reference` test compares 1,600 complete containers
against the original static `WriteImageSize`, `WriteRealAlphaAndVersion` and
`WriteImage` helpers. It crosses five widths/heights, both alpha flags and every
trailing bit count from 0 through 31. The adapter collects the actual C writer
callback output. Portable tests independently check packed header bits, RIFF and
chunk lengths, padding, invalid dimensions and size overflow. Auxiliary decode
tests now use the production Rust header/container helpers.

Direct `TokenStream` requests contain width, height, tile bits, cache bits,
group count, prefix bit count, tile count, packed tile symbols, then
(mode, payload, length) triples (literal=0, cache=1, copy=2). Distances are
already plane codes; synthetic references exercise serialization without
requiring a semantically replayable image. The adapter partitions tokens by
starting tile, builds each histogram with original `VP8LHistogramStoreRefs`,
then calls the original Huffman glue and token writer. Responses contain, for
each of five alphabets per group, its size and packed length | (code << 8),
followed by header/token bytes and the 11-bit sentinel. All 324 cases compare
exactly. Non-group bits of tile symbols are deliberately nonzero.

`fixtures/token_streams.txt` preserves 12 requests/responses from
`target/native-oracle/token-streams.{requests,responses}.txt`: zero-based cases
144..179 where i % 3 == (i / 3) % 3 (integer division). This selects geometry
17x33, tile bits 2, all four cache sizes and three offsets, alternating the three
token patterns. The seed remains `6d2b79f5`. The portable token test also checks
a hand-computed group transition after a copy crosses multiple rows.

Milestone 6 adds 4,388 exact clustering/sampling comparisons and 1,068 complete
main-image comparisons, with the same scalar pin/configuration and seed.
`clustering_and_sampling_match_pinned_reference` compares 1,308 transcripts
and 3,080 complete sampling buffers. It asserts that the corpus reaches empty
tiles, full/low-effort entropy-bin merges, stochastic merges/early stopping,
greedy merges and multiple final groups. The main-image test compares selected
cache bits, header/data byte counts and complete streams, including an 11-bit
sentinel. Configurations include all LZ77 strategies, one/two sub-configurations,
optional no-cache trials, preset/automatic cache bounds, both effort modes,
every legal histogram tile bit count and prefixes of 0/7/31/63/97 bits.

New requests:

- `VP8LOptimizeSampling`: `(full_width, full_height, bits, max_bits)`, then the
  complete subsampled input map. Returns selected bits and the entire mutated
  buffer, including the untouched tail after in-place compaction.
- `VP8LClustering`: `(width, height, quality, low_effort, tile_bits, cache_bits)`,
  then `(mode, payload, length)` tokens (literal=0, cache=1, copy=2). Distances
  are already plane codes; these synthetic tokens need not reconstruct pixels.
  Returns original/analyzed sets, the entropy-bin enabled flag, sets after bin
  assignment/combination, the greedy flag, sets after stochastic/greedy passes,
  the tile map, the remapped set, and the integrated entry point's set/map.
  Each set starts with its size. Each slot starts with a presence flag (0 for
  an empty original tile). Present histograms contain, for each of five
  populations, the number of nonzero bins then `(index, count)` pairs, followed
  by five trivial symbols, total cached cost, five population costs, five used
  flags and bin id. Costs are low/high word pairs. Every population value is
  preserved; there is no hash/checksum. Rust's unused original populations
  represent C's null histogram pointers.
- `EncodeImageInternal`: `(width, height, quality, low_effort, cache_bound,
  histogram_bits, prefix_bits, trial_count)`, `(lz77_mask, do_no_cache)` pairs,
  then ARGB pixels. Cache bound zero requests the automatic maximum (10).
  The adapter writes a fixed `0x05ad5ad5` prefix in chunks of at most 32 bits,
  masking partial chunks, and uses its rounded byte length as the initial byte
  position. Returns selected cache bits, header/data byte counts and the whole
  prefix/main stream with the 11-bit sentinel. `huffman.c` calls the original
  static encoder directly, including its original clustering implementation.

`fixtures/clustering.txt` contains the 20 cases from
`support::clustering::fixture_cases`; `fixtures/main_images.txt` contains the
11 cases from `support::main_images::fixture_cases`. Outputs come exclusively
from the pinned C wrappers. Review the source pin and
`target/native-oracle/provenance.txt` before regenerating these specific files:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle clustering_fixtures_match_pinned_reference
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle main_image_fixtures_match_pinned_reference
```

Portable independent tests exhaust uniform-square sampling, check copy/tile
assignment and remap ties, conserve histogram populations, test strict and
saturating thresholds, and compare combined trial selection to individually
encoded trials. `main_images` also independently decodes 168 untransformed
files with exact RGBA equality, including hidden RGB: 34 use caches, 50 use
multiple Huffman groups and 15 compact the group map to larger tiles.

### Milestone 7 palette foundations

`PaletteLeaves` compares the original public palette helpers in `palette.c`.
Its first word selects mapping (0), default ordering (1), delta ordering (2),
or extraction (3). Ordering/mapping inputs are the palette words; extraction
inputs are `(width, height, ARGB stride)` followed by pixels, excluding final-row
padding. Mapping returns sorted colors followed by the index map. Ordering
returns palette words. Extraction returns the count followed by sorted colors,
or just 257 for an ineligible image.

The 552 seeded cases cover palette boundaries, monotonic and random colors,
transparent relocation and padded rows. `fixtures/palette.txt` records every
13th case (43 fixtures). Review the pin and `target/native-oracle/provenance.txt`
before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle palette_fixtures_match_pinned_reference
```

`PaletteSpatial` inputs are `(method, width, height, ARGB stride, color count)`,
then sorted palette words and image pixels (excluding final-row padding).
Methods 0–4 correspond to `PaletteSorting` in the pinned header. The output is
exactly the reordered palette. The adapter calls the original `PaletteSort`;
no algorithms are duplicated in the C harness. There are 1,170 seeded cases
covering all five methods, 1–256 colors, skinny/padded images and six adjacency
patterns. `fixtures/spatial_palette.txt` records every 31st case (38 fixtures).
After reviewing the pin and provenance, regenerate with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle spatial_palette_fixtures_match_pinned_reference
```

Independent Rust checks compare spatial counts to a hand-computed graph and
MinLA's best insertion slots to brute-force complete arrangement costs, using
zero, seeded and maximum-u32 edge weights. Image tests directly sum adjacent
palette-index distances before/after refinement and check color preservation.

`PaletteTransform` inputs are `(palette size, width, height, source stride,
low effort, prefix bit count)`, then ordered palette words and ARGB rows without
final-row padding. The wrapper in `huffman.c` calls the original static
`MapImageFromPalette` and `EncodePalette`. Outputs are packed width, packed
pixels (packed width times height words), then the full palette bitstream with
the standard prefix and 11-bit sentinel. It covers the greedy small-palette,
three perfect-hash and sorted-search paths, all packing thresholds, and
trailing-zero omission at the 17/18-color boundary. There are 600 comparisons
and 27 fixtures (every 23rd case), regenerated after pin/provenance review with:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle palette_transform_fixtures_match_pinned_reference
```

Portable tests independently unpack mapped indices, check destination padding,
assert each lookup strategy is reached, and decode 200 complete palette files
using the main-image encoder. They require exact RGBA, including hidden RGB.

### Predictor transform

`PredictorFoundations` uses the original static functions included by
`predictor.c`, with external entry points renamed to retain the normal public
encoder. Its opcode determines the input and output:

- 0: `(max quantization, exact)`, pixels; returns plane mask and minimum mode.
- 1: `(mode, left mode, above mode, plane mask)`, 1024 tile counts, 1024
  accumulated counts; returns signed fixed-point cost as low/high u32 words.
- 2: `(width, height, tile bits, low effort, exact)`, predictor-map words,
  pixels; returns the converted image. Quantization is fixed to 1.
- 3: `(width, mode, start x, y, exact, pixel count)`, current and upper rows
  (width + 1 words each); returns residuals followed by both mutated rows.
- 4: plane mask, 1024 initial counts, residual words; returns updated counts.
- 5: `(zero weight, exponential value)`, 256 counts; returns bias as signed
  low/high words.
- 6: `(width, height, minimum bits, maximum bits, low effort, exact)`, pixels;
  returns selected bits, compact predictor map, and converted pixels. Uses
  `VP8LResidualImage` at near-lossless quality 100.
- 7: the same input as opcode 6 (low effort ignored); returns selected bits
  and compact map from `GetBestPredictorsAndSubSampling` without conversion.

There are 4,758 foundation comparisons and 5,290 automatic-selection comparisons.
Search cases cover all plane masks, eight sampling
levels (bits 2–9), both residual-histogram accumulation strategies, Z traversal,
partial right/bottom tiles, one-dimensional images, mixed predictor maps,
transparent cleanup, exact RGB, and low-effort mode 11. The maximum transform
size is 512 pixels (9 bits); uniform maps can optimize beyond the requested
search maximum.

`ClampBits` takes `(width, height, requested bits, minimum bits, maximum bits,
image size limit)` and returns clamped bits. Requested bits use signed i32
encoded as u32. There are 1,920 comparisons, including negative/out-of-range
requests, dimensions through 16,383, single-tile images and size-limit boundaries.

`PredictorTransform` takes `(width, height, requested bits, method, quality,
low effort, exact, prefix bit count)`, then prepared pixels. It returns selected
bits, all residual pixels, then finalized stream bytes. The stream contains the
requested 0/5/97-bit prefix, the original `ApplyPredictFilter` header and map,
and an 11-bit `0x5ad` sentinel. The wrapper in `huffman.c` calls the original
static function with near-lossless quality 100. All 4,040 comparisons match,
including the 16,384-entry map limit, partial tiles, methods 0/4/5/6, both effort
modes, transparent cleanup and exact RGB, and qualities 0/25/50/75/100.

The combined predictor fixture file has 102 cases. Review the pin and recorded
toolchain provenance before regeneration:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle predictor_fixtures_match_pinned_reference
```

Portable independent tests reconstruct 3,456 supplied-map and 5,280 selected-map
cases, check plane eligibility/histogram counts, fixed-point bias identities,
constant-image ties and uniform-map compaction, and decode 336 complete files
containing supplied or selected predictor maps. An exhaustive size search
independently checks clamping. Another 576 complete-file decodes exercise the
production writer alone and after subtract-green or packed 2/4/16/17-color
palette transforms, reusing hash/reference scratch. Near-lossless residual
quantization remains separate.

### Cross-color transform

`CrossColor` uses the original static helpers included by `predictor.c`, with
the same renamed external entry points as the predictor adapter. Opcodes are:

- 0: one color code; returns the three multiplier bytes (green-to-red,
  green-to-blue, red-to-blue) and the normalized color code.
- 1: 256 accumulated counts, 256 local counts; returns signed cost low/high.
- 2: `(stride, tile width, tile height, previous-x code, previous-y code,
  green-to-red)`, 256 accumulated counts, strided tile pixels; returns red cost.
- 3: as opcode 2 with `(green-to-blue, red-to-blue)` in place of green-to-red;
  returns blue cost. Coefficient candidates are signed i32 encoded as u32.
- 4: `(stride, tile width, tile height, previous-x code, previous-y code,
  quality)`, 256 accumulated red counts, 256 accumulated blue counts, strided
  tile pixels; returns the selected multiplier color code.
- 5: `(width, height, bits, quality)`, pixels; returns optimized bits, compact
  multiplier map and the complete transformed pixels.
- 6: `(width, height, pixel x, pixel y, tile size, multiplier color code)`,
  pixels; returns all pixels after transforming only the given tile.

There are 6,126 leaf comparisons and 6,480 complete searches. The corpus covers
signed-byte boundaries, negative/wrapped candidates, neighbor/zero biases,
strided tiles without final-row padding, partial tiles, 512-pixel tiles,
one-dimensional images, varying/transparent alpha, correlated colors, runs,
row repetitions and mixed maps. Qualities 0/24/25/36/37/50/51/73/74/100 straddle
every red/blue search iteration threshold. Original candidate order and
first-minimum ties, repeated-delta skipping and cross-row previous multipliers
are retained.

`CrossColorTransform` takes `(width, height, bits, quality, low effort, exact,
preceding transforms, prefix bit count)`, then prepared pixels. Preceding
transforms use bit 0 for subtract-green and bit 1 for prediction. The wrapper
in `huffman.c` calls the original `ApplySubtractGreen`, `ApplyPredictFilter`
(method 6, requested bits 6, near-lossless quality 100) and
`ApplyCrossColorFilter`. Outputs are selected cross-color bits, all transformed
pixels, and stream bytes including the 0/5/97-bit prefix and 11-bit `0x5ad`
sentinel. All 4,624 streams match exactly, including both effort/exact modes,
every preceding-transform combination and quality-dependent auxiliary coding.

Ninety-eight committed fixtures cover helpers, complete searches and streams.
Review the pinned source and recorded toolchain before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle cross_color_fixtures_match_pinned_reference
```

Portable independent checks rebuild byte histograms, check bias identities and
tile boundaries, and reproduce/invert all 6,480 selected maps with exact ARGB
(including hidden RGB). Uniform zero-coefficient maps must compact to 9 bits.
Another 768 complete-file decodes cover cross-color alone and after every
subtract-green/predictor combination, optionally preceded by packed palettes.
They exercise all four transforms in sequence and reused hash/reference scratch.

### Milestone 8 analysis and stream trials

`huffman.c` calls the original `AnalyzeEntropy`, `GetHistoBits`,
`GetTransformBits`, `EncoderAnalyze` and synchronous `VP8LEncodeStream`.
The prepared input is positive-stride ARGB, with no final-row padding required.
No analysis or trial-selection algorithms are copied into the C adapter.

- `AnalysisSizing`: `(width, height, method, use_palette)`; returns histogram
  bits and initial transform bits. Geometry ranges through 16,383 in both axes
  without allocating full-size images.
- `AnalyzeEntropy`: `(width, height, stride, transform_bits, palette_size)`,
  sorted palette words, then pixels. Zero palette size disables palette
  analysis. Returns selected entropy mode and the original/transformed
  red/blue-zero flags.
- `EncoderAnalyze`: `(width, height, stride, method, quality)`, then pixels.
  Returns palette size and active sorted entries, histogram/predictor/cross-color
  bits, transformed-zero flag, config count, then each config's entropy mode,
  palette sorting method (5 means unused), sub-config count and
  `(LZ77 mask, do_no_cache)` pairs. The transformed-zero flag starts at zero,
  matching the stream entry point's low-effort initialization.
- `VP8LEncodeStream`: `(width, height, stride, method, quality, exact,
  prefix_bits)`, then pixels. Returns selected features, histogram/predictor/
  cross-color bits, cache bits, palette size, total/header/data stream sizes,
  then the complete stream with the standard `0x05ad5ad5` prefix and 11-bit
  `0x5ad` sentinel. Near-lossless strength is 100, threading is off, and stats
  are collected from the original encoder. Transparent RGB is supplied as-is;
  public import/cleanup behavior belongs to milestone 9.

There are 13,534 exact analysis/entropy/sizing comparisons and 4,341 complete
stream/statistics comparisons with seed `6d2b79f5`. Assertions require every
entropy guess and trial mode, both zero flags, every palette sorting method,
exhaustive trials with/without subtract-green and all four selected transforms.
Inputs cross palette sizes 1/2/3/4/5/16/17/224/225/256/257, methods 0–6,
quality 49/50 and 99/100 boundaries, both exact settings and writer prefixes
through 97 bits. Padding contains unrelated colors. All results match C.

`fixtures/analysis.txt` contains the 64 `support::analysis::fixture_cases`;
`fixtures/streams.txt` contains the 21 `support::streams::fixture_cases`.
Provenance remains Ubuntu x86_64, GCC 15.2.0-16ubuntu1, Release `-O3 -DNDEBUG`,
SIMD/threading off, with the unchanged source pin. Review the pin and
`target/native-oracle/provenance.txt` before regenerating:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle analysis_fixtures_match_pinned_reference
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle encoder_stream_fixtures_match_pinned_reference
```

Independent tests check first-pixel zero flags, entropy ties, small-palette
packing/reference trials, padding and sampling limits. The combined stream is
compared to every individually encoded trial to check rounded byte selection,
different streams tied for size and later winning configurations. Another
1,296 complete-file decodes check dimensions, alpha, visible RGB and all RGB
in exact mode, and verify that prepared source pixels remain unchanged.

### Milestone 9 public RGBA encoding

`WebPEncodeLosslessRGBA` retains its original request layout `(width, height,
byte_stride)`, followed by one u32 word per RGBA byte, including row padding.
It now compares the Rust simple API with the original C simple API.
`WebPEncodeRGBAOptions` takes `(width, height, byte_stride, quality, method,
exact)`, followed by RGBA bytes, and returns complete RIFF/WebP bytes. The C
adapter initializes the normal lossless config, imports RGBA, calls the original
`WebPEncode` and collects its memory writer output. Near-lossless strength is
100, the image hint is default and threading is off. Both adapters deliberately
start input at an unaligned byte offset.

`public_rgba_encoders_match_pinned_reference` compares and independently decodes
2,584 full files, spanning all palette thresholds, methods 0–6, quality/cache
boundaries, exact/transparent cleanup, padded rows without final-row padding,
unaligned source slices, both 16,383-pixel skinny geometries, and 128 additional
seeded geometries/strides with random effort settings. The same cases run in
debug and release. There is no compressed-byte divergence on the recorded
scalar toolchain. The production library remains independent of C.

The real-image sample is copied verbatim from the pinned
`upstream/examples/test_ref.ppm` into `fixtures/corpus/test_ref.ppm` so normal
tests do not depend on the submodule. Its C-encoded `test_ref.webp` is 14,954
bytes and is compared directly in portable tests. The corpus README records
source provenance and checksum. `fixtures/public_rgba.txt` contains 43 compact
public requests/responses from `support::public_images::fixture_cases`.
Source pin, compiler and build options are unchanged from milestone 8.
Review the pin and `target/native-oracle/provenance.txt` before regenerating
the text fixtures and sample binary together:

```sh
WEBPOP_UPDATE_FIXTURES=1 cargo test -p libwebpop --features native-oracle --test native_oracle public_rgba_fixtures_match_pinned_reference
```

Portable tests also cover invalid geometry/effort errors, byte-count overflow,
input immutability, layout-independent default output and the equivalence of
simple/default-configured APIs. The CLI's default output is byte-compared with
library encoding at quality 75, matching upstream `cwebp -lossless`; the library
simple API retains quality 70. `public_encode_baseline` is an explicitly ignored
timing test;
its release invocation and first size/time measurements are recorded in the
lossless port plan. Timings do not affect test success.
