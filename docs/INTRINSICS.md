# SIMD in libwebpop

Examined on 2026-10-22 against upstream revision
`1844042d1ca751e62e61df7617dba19c98588339`. External Rust API and crate status
below is a dated assessment; recheck it before selecting a dependency.

## Recommendation

Keep the scalar Rust implementation as the correctness reference, let LLVM
vectorize straightforward loops, and use a portable **integer SIMD** library
for measured bottlenecks that remain. Start with `wide` for a small, explicit
128-bit implementation. Evaluate `fearless_simd` if runtime selection of wider
x86 vectors is needed; `pulp` is another viable dispatch abstraction. Do not
translate every upstream SSE2 and NEON file into separate Rust backends.

Most upstream intrinsics implement familiar image-processing operations, and
their semantics can be expressed portably. That does not mean an existing
high-level image or numerics routine can replace the codec kernel. We still
need to write WebP's particular transform, rounding, clipping, predictor, and
packing logic. A SIMD library supplies the vector operations and architecture
implementations underneath that logic.

`std::arch` is the supported route for direct hardware intrinsics, including
SSE2, AVX2, and AArch64 NEON. It can reproduce upstream's instruction choices,
but it leaves us maintaining architecture-specific kernels and feature
dispatch. Reserve it for a demonstrated gap in the portable implementation.
This document proposes an optimization approach, not a dependency addition or
a measured speedup.

## What upstream actually accelerates

The implementations are concentrated in [`src/dsp`](../upstream/src/dsp) and
[`sharpyuv`](../upstream/sharpyuv). The checkout has 12 SSE2, 7 SSE4.1, 3 AVX2,
11 NEON, and 7 MSA `.c` files, plus shared headers. It also has 6 MIPS32 and
10 MIPS DSP R2 `.c` files; some of those use inline assembly and scalar DSP
instructions rather than vector intrinsics. These are alternative
implementations of selected functions, not separate codec algorithms.

Initialization generally installs C functions and then overwrites selected
function pointers according to compiled backends and CPU detection. See
[`cpu.c`](../upstream/src/dsp/cpu.c),
[`cpu.h`](../upstream/src/dsp/cpu.h), and
[`VP8LEncDspInit`](../upstream/src/dsp/lossless_enc.c).
Backend coverage differs: the lossless NEON encoder initializer replaces
subtract-green, cross-color, and combined entropy; SSE2 additionally replaces
histogram addition, mismatch search, palette packing, color collectors, and
all 14 predictor-subtraction modes. SSE4.1 and AVX2 provide further replacements.
There is no requirement to reproduce this historical coverage matrix in Rust.

| Family and representative sources | Actual processing pattern | Portable SIMD assessment |
| --- | --- | --- |
| [`lossless_enc_sse2.c`](../upstream/src/dsp/lossless_enc_sse2.c), [`lossless_enc_neon.c`](../upstream/src/dsp/lossless_enc_neon.c), [`lossless_enc_avx2.c`](../upstream/src/dsp/lossless_enc_avx2.c) | Subtract-green and signed fixed-point cross-color transforms; wrapping byte residuals; histogram-vector addition | Strong fit for byte/word arithmetic, masks, shifts, widening and narrowing. Histogram addition is particularly easy for the compiler. |
| Same lossless encoder files | Predictor averages, clamped predictors, per-pixel sums of absolute channel differences, comparison/select | Strong fit after choosing an appropriate layout and moving predictor selection outside the pixel loop. Reduction must stay within each pixel's four channels. |
| Same lossless encoder files | Transform candidate pixels, then increment indexed histogram bins | Only partly vectorizable. SSE2/AVX2 store transformed indices to a temporary array and increment the bins scalarly. Duplicate indices prevent a naive gather/add/scatter replacement. |
| Same files; [`lossless_enc_sse41.c`](../upstream/src/dsp/lossless_enc_sse41.c) | Combined entropy, weighted extra-bit costs, first mismatch in an LZ77 match | Mixed arithmetic and control flow. Explicit compare/mask and reduction operations help; table-based logarithms and early exits need algorithm-specific treatment. |
| [`lossless_sse2.c`](../upstream/src/dsp/lossless_sse2.c), [`lossless_neon.c`](../upstream/src/dsp/lossless_neon.c) | Inverse transforms, reconstruction, BGRA conversion and packing | Color conversion is a good fit for shuffles/interleave. Reconstruction using the previously reconstructed left pixel has a recurrence. |
| [`alpha_processing_sse2.c`](../upstream/src/dsp/alpha_processing_sse2.c), [`alpha_processing_neon.c`](../upstream/src/dsp/alpha_processing_neon.c) | Extract/insert channels, test opacity, replace transparent pixels, multiply alpha | Standard masks, widening multiply, fixed-point scale and narrowing; strong fit. Preserve exact alpha rounding. |
| [`filters_sse2.c`](../upstream/src/dsp/filters_sse2.c), [`filters_neon.c`](../upstream/src/dsp/filters_neon.c) | Alpha-plane differences, gradient predictors and inverse filters | Forward differences and vertical reconstruction are straightforward; horizontal reconstruction needs a prefix sum, and inverse gradient has a clipped left-pixel recurrence. |
| [`enc_sse2.c`](../upstream/src/dsp/enc_sse2.c), [`enc_neon.c`](../upstream/src/dsp/enc_neon.c), [`dec_sse2.c`](../upstream/src/dsp/dec_sse2.c) | Small integer transforms, transposes, intra prediction, quantization, masked deblocking | Portable building blocks fit, but careful shuffle and fixed-point design is required. These are codec-specific small kernels, not general matrix multiplication. |
| [`cost_sse2.c`](../upstream/src/dsp/cost_sse2.c), [`ssim_sse2.c`](../upstream/src/dsp/ssim_sse2.c) | Coefficient classification, nonzero masks, sums, squared errors and local statistics | Good fit for comparisons, widening multiply and reductions; entropy cost lookup/control flow remains. |
| [`yuv_sse2.c`](../upstream/src/dsp/yuv_sse2.c), [`upsampling_neon.c`](../upstream/src/dsp/upsampling_neon.c), [`rescaler_sse2.c`](../upstream/src/dsp/rescaler_sse2.c), [`sharpyuv_sse2.c`](../upstream/sharpyuv/sharpyuv_sse2.c) | Packed/planar channel conversion, integer color matrices, interpolation, fixed-point rescaling and iterative color correction | Typical image kernels, with layout, accumulator width, rounding, and boundary requirements. Generic filtering/resizing APIs are not automatically equivalent. |

For the current lossless RGBA encoder target, prioritize lossless encoder DSP,
RGBA import and alpha handling. Lossy transforms, YUV conversion, resizing and
decoder reconstruction are useful future context, not all dependencies of that
entry point. See the [lossless port plan](LOSSLESS_PORT_PLAN.md).

### Portable operations, with codec-specific semantics

Several details explain why replacing intrinsics mechanically, or using a
generic image operation, can produce incorrect output:

- `_mm_sub_epi8` subtracts independently modulo 256. Subtracting packed `u32`
  pixels directly allows borrowing between channels. Use byte vectors or
  preserve the masks in Rust's existing `VP8LSubPixels`.
- SSE2's unsigned byte average rounds upward. Lossless `Average2_m128i`
  corrects it to round downward. A portable byte expression is
  `(a & b) + ((a ^ b) >> 1)`, where the shift is within each byte lane.
- `TransformColor_SSE2` uses signed high-half multiplication with pre-shifted
  coefficients. The underlying operation is signed channel/coefficient
  multiplication followed by arithmetic `>> 5`, then byte wrapping. Widening
  to signed lanes expresses this portably without copying the SSE shuffle
  sequence. Blue uses the original red during the forward transform.
- Predictor 12 saturates its prediction to `[0, 255]`, then wraps the
  subtraction. Predictor 13 corrects negative values to implement division
  toward zero; arithmetic right shift alone would round negative values down.
  Predictor 11's tie-breaking rule must also survive vector comparisons.
- Palette packing groups 1/2/4/8 indices per output pixel. The SSE2 one-bit
  case uses `movemask`; a portable comparison/bitmask or shift/OR formulation
  can express the same grouping, but its cost varies by architecture.

The SSE and NEON implementations sometimes choose different strategies:
NEON color conversion uses interleaved `vld4`/`vst3`/`vst4` loads and stores,
where SSE uses unpack/shuffle networks. SSE2 combined entropy extracts masks
of occupied bins and iterates set bits; NEON accumulates vector sums and skips
all-zero chunks while still calling scalar `VP8LFastSLog2` for individual lanes.
A portable implementation should preserve the operation, not every instruction
or scheduling decision.

Encoding residuals can use several original input pixels at once, even when
their predictors read the left neighbor. Decoding is different: when that
neighbor is newly reconstructed output, it creates a dependency. Some modes
admit a vector scan; nonlinear clamped/selected recurrences do not become
independent lanes just by using a SIMD type.

## What the Rust compiler already does

LLVM has both loop vectorization and SLP vectorization, with legality and cost
models. Independent contiguous integer loops are good candidates. Irregular
updates, function calls, recurrences and early exits can prevent or limit
vectorization. This is a prediction to verify in generated code, not a promise
that idiomatic iterators or fixed-size arrays always vectorize.
See [LLVM's vectorizer documentation](https://llvm.org/docs/Vectorizers.html).

An assembly inspection of the current working tree used Rust
`1.100.0-nightly (923c95cdf 2026-09-16)`, LLVM 23.1.1, release optimization,
and `x86_64-unknown-linux-gnu`. The portable baseline was explicitly selected:

```sh
RUSTFLAGS='-Ctarget-cpu=x86-64' cargo rustc -p libwebpop --lib --release --offline -- --emit=asm
```

Inspect the emitted `.s` under `target/release/` (the exact output path depends
on Cargo's artifact layout). Repeat without `--offline` if dependencies are
not cached. This environment normally sets `RUSTFLAGS=-Ctarget-cpu=native`;
overriding that matters when assessing portable binaries.

| Current Rust function | Baseline x86-64 observation |
| --- | --- |
| `AddVector_C`, `AddVectorEq_C` | Vector loops using SSE2 `movdqu` and `paddd`, with scalar remainders. A library call is unnecessary just to obtain SIMD addition here. |
| `PredictorSub0_C`, `PredictorSub1_C` | Vector loops using SSE2. Mode 1 still implements masked packed-channel arithmetic rather than automatically recovering upstream's simple byte-subtract formulation. |
| `VectorMismatch_C` | Scalar compare/early-exit loop. Explicit chunk comparison is a useful experiment if match lengths justify its overhead. |
| `VP8LCollectColorBlueTransforms_C` | Scalar loop, including indexed histogram increments. Separating a vector transform from scalar bin updates would require an explicit rewrite. |
| `PredictorSub11_C` through `PredictorSub13_C` | Call the shared `PredictorSub`; that shared body retains per-pixel indirect predictor calls and is scalar. First try specialization/inlining that exposes a concrete predictor for the whole loop. |
| `CombinedShannonEntropy_C` | Scalar bin processing and scalar `log` calls in fallback paths. XMM-register usage here includes scalar floating point and is not evidence of packed SIMD entropy evaluation. |

With the environment's native CPU setting, the addition and modes 0/1 loops
used AVX2 instructions. This proves the compiler handles some relevant loops
without a SIMD dependency. It does not establish equivalent throughput to
upstream, the best possible instruction sequence, or behavior on AArch64/Wasm.
No timings or cross-architecture assembly comparisons were performed.

Before introducing explicit vectors, expose equal lengths and row bounds once,
use slice/chunk iteration, preserve explicit wrapping arithmetic, and move
mode/function selection outside hot loops. Keep scalar packed-pixel helpers
when they already perform well. Do not assume LLVM will invent conflict-safe
histogram updates, a prefix-scan algorithm, or an efficient channel transpose.

## `std::arch`, portable SIMD, and libraries

### Direct hardware intrinsics: `std::arch`

[`std::arch`](https://doc.rust-lang.org/std/arch/index.html) (also available as
`core::arch`) exposes vendor operations such as `_mm_sub_epi8` and AArch64
`vsubq_u8`. These are suitable for exact integer image kernels and broadly map
to the upstream C intrinsics. They are architecture-specific APIs, not a
portable SIMD layer: Rust does not turn x86 intrinsics into NEON on ARM.

Direct use means owning architecture `cfg`s, feature requirements, loads and
stores, tails, and any runtime dispatch. Compiling everything with
`target-cpu=native` or `+avx2` does not create a portable binary that chooses
SSE2 on older CPUs. For a general x86-64 build, SSE2 is a baseline; optional
SSE4.1/AVX2 acceleration needs separate compiled functions and dispatch.

The crate's [`#![forbid(unsafe_code)]`](../libwebpop/src/lib.rs) matters, with
a nuance: it does **not** mean all use of `std::arch` is forbidden. Rust 1.86
allowed safe `#[target_feature]` functions and safe calls between compatible
feature contexts. Arithmetic can therefore be safe inside a correctly
annotated function. A local compile check confirmed this for SSE2 set/add/
extract operations under `forbid(unsafe_code)`. Calling that function from an
ordinary context still needs the feature-safety boundary; a runtime feature
check alone does not make the call syntactically safe. See the
[Rust 1.86 announcement](https://blog.rust-lang.org/2025/04/03/Rust-1.86.0/).

Memory operations such as
[`_mm_loadu_si128`](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_loadu_si128.html)
remain unsafe pointer APIs. Thus a direct translation of upstream's
load/compute/store kernels does not fit the present policy. A dependency can
encapsulate loads and dispatch behind safe APIs without weakening our crate's
lint: `forbid(unsafe_code)` is not a ban on dependencies containing unsafe code.

### Portable vector types

[`std::simd` / `core::simd`](https://doc.rust-lang.org/std/simd/index.html)
is the portable API: vector integers, masks, shuffles, casts and reductions,
with scalar implementations where hardware lacks an operation. It is a good
semantic fit for much of this code, but remains nightly-only under
`#![feature(portable_simd)]` at the assessment date. It also does not supply
runtime CPU multiversioning by itself, or guarantee one instruction per
operation. Avoid making it a production requirement unless the project
deliberately chooses nightly. The local nightly toolchain is not itself a
reason to require nightly for library consumers.

`std::intrinsics::simd` is a separate compiler-internal, unstable interface;
it is not the user-facing alternative to `std::arch`. There is no reason to
build this port on it. See the
[intrinsics module documentation](https://doc.rust-lang.org/std/intrinsics/index.html).

| Option, assessed version | Fit and limitations for this project |
| --- | --- |
| [`wide` 1.7.1](https://docs.rs/wide/1.7.1/wide/) | First choice for a small stable-Rust prototype: fixed-width integer types, wrapping operators, masks, selection, shuffles and safe array conversions. Start with `u8x16`, `i16x8`, `u32x4` as appropriate. Backends include x86, AArch64 NEON, Wasm SIMD and scalar fallback; selection is at compile time, not runtime CPU dispatch. Wider types may be split into multiple registers. Check each needed conversion/shuffle's generated code. |
| [`fearless_simd` 1.1.0](https://docs.rs/crate/fearless_simd/1.1.0) | Strong alternative when dispatch is needed: safe APIs, portable integer vectors, x86 runtime levels, AArch64 NEON, Wasm SIMD and scalar fallback. Has a companion annotation macro to keep SIMD computation in the appropriate feature context. Published MSRV is 1.89. A new dependency should first be tried on representative byte/shuffle/reduction kernels, including compatibility with our unsafe-code lint and binary-size requirements. |
| [`pulp` 0.22.3](https://docs.rs/pulp/0.22.3/pulp/) | Safe generic SIMD plus runtime dispatch, including an auto-vectorization-only entry point. The [integer trait](https://docs.rs/pulp/0.22.3/pulp/trait.Simd.html) includes arithmetic, masks, first-true searches and reductions. Its [source](https://docs.rs/pulp/0.22.3/src/pulp/lib.rs.html) includes x86, AArch64 and Wasm backends. Assess byte shuffles, widening and saturating packs specifically; generic numerics support alone is insufficient evidence of efficient image kernels. |
| [`safe_arch` 1.2.0](https://docs.rs/safe_arch/1.2.0/safe_arch/) | Safe compile-time wrappers for x86 intrinsics, including memory access wrappers. Useful for a narrow x86 escape hatch, but does not solve NEON portability or runtime dispatch. |

A high-level numerics package such as a matrix/array or BLAS library is the
wrong primary abstraction. Most of this work is interleaved `u8`/`i16`/`u32`
processing, exact fixed-point transforms and small reductions, not large
floating-point linear algebra. A generic convolution or color-conversion
routine would need the exact pinned WebP coefficients, rounding, clipping and
edge behavior before it could replace a kernel. Avoid extra image-format
conversions and temporary whole-image allocations just to reach such an API.

## A practical optimization sequence

1. Finish scalar behavior and preserve differential tests against the pinned
   scalar C backend. Profile the completed encoder before ranking kernels;
   the presence or size of a C intrinsics file is not a hotspot measurement.
2. Keep auto-vectorized addition and similarly simple loops unless benchmarks
   show a reason to replace them. Try concrete predictor specialization before
   adding vectors to the current indirect-call loop.
3. Prototype one genuinely unmet operation with `wide`: compare/mask-based
   `VectorMismatch`, or an exact byte-lane predictor residual. Use subtract-green
   as a simple correctness/layout exercise, not an assumed performance win.
   Then assess signed cross-color and transform-plus-scalar-histogram kernels.
4. Retain library vector types inside DSP implementation functions. Accept
   ordinary slices, use safe array/chunk loads and stores, process complete
   chunks then scalar tails, and dispatch once per row/batch rather than pixel.
   Preserve logical `0xAARRGGBB` values across endianness; a native byte view of
   `u32` storage is not a portable channel-order definition.
5. If wider native x86 code provides worthwhile end-to-end gains, compare a
   dispatch library before maintaining separate `std::arch` files. Investigate
   direct intrinsics only for a specific, measured operation the abstraction
   cannot express efficiently. Keep the current unsafe-code policy unless a
   separate decision changes it.

Benchmark release builds for baseline x86-64, optional wider x86, AArch64, and
Wasm both without and with `simd128`. Baseline 128-bit vectors are a useful
common starting width, not a claim that one width is optimal everywhere.
For browser Wasm, SIMD is a module feature: a scalar branch inside a module
containing SIMD instructions does not let a non-SIMD engine load it. If those
engines must remain supported, distribute separate scalar/SIMD modules and
select before instantiation. A future Rust SIMD backend needs its own parity
checks against the scalar implementation.

Measure tiny rows/tiles as well as large images, mismatch-at-first versus long
matches, sparse/dense histograms, repeated bins, padded strides and vector-tail
lengths. Check assembly for spills and scalarized shuffles, and record throughput,
dispatch overhead and binary size. Require exact scalar/SIMD results on valid
inputs, including odd averages, signed coefficients/channels, clamping, predictor
ties, wrapping and every lane/tail boundary. Preserve fixed-point entropy
tables and arithmetic; a vector floating-point logarithm is not a compatible
replacement for `VP8LFastSLog2`. Use the existing
[oracle infrastructure](../libwebpop/tests/reference/README.md) plus independent
checks, then verify complete output once the Rust encoder exists.
