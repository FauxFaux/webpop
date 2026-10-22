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
cases. Header inline functions compile in the wrapper translation unit; current
transform functions are already exported upstream. Future static helpers should
get wrappers in a translation unit including their original `.c` file.

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

Differential cases exhaust byte arithmetic and signed color coefficient/channel
pairs, exercise all prefix entries and both lookup/slow paths, test prefix
boundaries, and run seeded arrays and cache traces for every legal cache size.
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

The Rust public encoder, predictors, entropy DSP, bit writer, and higher-level
subgraphs remain unimplemented. Checked RGBA geometry and the overflow-safe
extension of `VP8LSubSampleSize` have portable Rust boundary tests; the C oracle
is invoked only on its defined domain. In particular, zero-bit scans and NoLUT
prefix distances below 3 are invalid upstream inputs.
