The primary live oracle is the scalar **C** libwebp implementation in
`upstream/`, pinned to `1844042d1ca751e62e61df7617dba19c98588339`. No C++
reimplementation is involved. The wrappers call the original functions directly.
They are test-only and live outside the submodule.

Run from the repository root:

```sh
# Portable fixtures, exhaustive arithmetic checks, and the existing Wasm test:
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

This native backend was selected because it can expose internal/header leaves
directly from the pinned checkout. The bundled Wasm binary exports the public
encoder but no leaf wrappers, predates the pin, and cannot establish exact
intermediate parity. Emscripten is not installed in the initial environment.
The same seven image inputs run through both backends and an independent Rust
decoder; the old Wasm compressed bytes are not required to match the pin.

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
