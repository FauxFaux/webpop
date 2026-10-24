# sharpyuv: bottom-up scalar RGB-to-YUV port

## Target and compatibility

Port the standalone [`upstream/sharpyuv/`](../upstream/sharpyuv/) library into
a self-contained `libwebpop::sharpyuv` module under
`libwebpop/src/sharpyuv/`. Keep the reference revision
`1844042d1ca751e62e61df7617dba19c98588339`, whose SharpYUV API version is
0.4.2. Preserve upstream function names, module boundaries, constants and
evaluation order. Port each function directly, validating its dependencies
before its callers. This is a small conversion library: four scalar algorithm
translation units, with no codec, bitstream or container milestones.

The result converts RGB to planar YUV420, with Y dimensions `width × height`
and U/V dimensions `ceil(width / 2) × ceil(height / 2)`. Cover the pinned
library's RGB depths 8/10/12/16, YUV depths 8/10/12, five predefined matrices,
computed/custom matrices, default sRGB transfer and every supported transfer
enum. The conversion assumes bilinear chroma reconstruction. It does not
process alpha; RGBA layouts can supply RGB channels while skipping alpha.

Start with **no explicit SIMD and no CPU dispatch**. Call the scalar DSP
functions directly, including the scalar sRGB fast paths. Defer SSE2, AVX2,
NEON, CPU probing and runtime function-pointer replacement. Rust compiler
autovectorization is compatible with this scope. Use the pinned scalar C build
as the exact sample oracle under a recorded toolchain; do not substitute the
existing ordinary WebP RGB-to-YUV converter or a generic color library.

The conversion module was completed before encoder integration. The
`use_sharp_yuv` adapter and CLI selector are now implemented as described
below; existing WebP defaults remain unchanged. Other preprocessing and SIMD
remain [encoder completion](ENCODER_COMPLETION_PLAN.md) work.

## Current repository and Rust boundary

**Milestones 0–4 are complete (2026-10-24).**
[`libwebpop::sharpyuv`](../libwebpop/src/sharpyuv/mod.rs) implements all scalar
conversion functions below, with its own types/errors and no encoder
dependencies. The lossy encoder retains its ordinary YUVA importer by default
and can select SharpYUV through `LossyOptions::use_sharp_yuv`. The
[reference record](../libwebpop/tests/reference/SHARPYUV.md) documents 153
portable C fixtures, exact debug/release comparisons and the first scalar
timing/buffer baseline.

Acceptance includes 3,317 leaf requests (over 6.6 million gamma/inverse
values), 976 DSP requests, 2,304 imported-row/iteration traces and 6,772 public
requests. All three iteration exits are observed. Independent tests cover
rounding, signed filtering, edge replication, layouts, clamping, alpha
independence, errors and repeatable concurrent calls. Workspace and existing
native encoder regressions pass. The oracle now prefers CMake's generated
scalar configuration over ignored in-source Autotools headers and rejects
SIMD/threading macros at compile time.

Implemented layout:

```text
libwebpop/
  src/sharpyuv/
    mod.rs                         # exports, safe views/results/errors
    sharpyuv.rs                    # sharpyuv.c / sharpyuv.h
    sharpyuv_csp.rs                 # sharpyuv_csp.c / .h
    sharpyuv_gamma.rs               # sharpyuv_gamma.c / .h
    sharpyuv_dsp.rs                 # scalar sharpyuv_dsp.c / .h
  tests/support/sharpyuv.rs         # cases/evaluator in the existing harness
  tests/fixtures/sharpyuv*.txt      # compact committed scalar C results
  tests/reference/sharpyuv*.c       # test-only C wrappers
  tests/reference/SHARPYUV.md       # protocol, provenance and regeneration
```

Expose the module from `libwebpop/src/lib.rs`; keep the existing workspace
members and reuse `libwebpop`'s `native-oracle` feature, build/transport,
request dispatch and fixture evaluators. The test framework is substantial
relative to this port, so a module avoids separate harness plumbing.
Production code forbids unsafe Rust and uses neither C nor the upstream
submodule at build/run time. Keep C interop in opt-in test infrastructure.

**Strong recommendation: keep SharpYUV separate from the encoder even while
it lives in the same crate.** Keep its algorithms, tables, public types,
validation and errors inside `src/sharpyuv/`, with dependencies limited to
the standard library and its own modules. Do not depend on `enc`, `dsp`,
`webp`, encoder errors or picture/config types, and do not reuse the encoder's
gamma implementation. Future integration should adapt encoder inputs and
outputs at the caller and depend on SharpYUV in one direction. Share the
test infrastructure, while keeping SharpYUV cases and wrappers identifiable.
This boundary should make later extraction into a standalone crate a move
of the module and its tests, without algorithm changes or encoder refactoring.

Retain public names `SharpYuvConvert`, `SharpYuvConvertWithOptions`,
`SharpYuvComputeConversionMatrix`, `SharpYuvGetConversionMatrix` and
`SharpYuvGetVersion`, with Rust ownership and errors replacing pointer/status
conventions. Use typed borrowed RGB channel views over `u8` or `u16` samples
and owned typed Y/U/V planes. Views carry positive pixel step and row stride
in **samples**, allowing planar RGB, interleaved RGB/BGR and RGBA/BGRA without
unsafe casts. Translate C's byte step/stride in test adapters. Return `u8`
planes for 8-bit output and `u16` planes for 10/12-bit output; values remain
logical samples, independent of host byte order. Output is tightly packed.

Use upstream-named matrix, color-space, range, transfer and options types.
Options initialization selects sRGB; the simple conversion must equal
conversion with default options. Preserve `SharpYuvOptionsInit` semantics as
an ordinary safe constructor/helper. Test the version rule in
`SharpYuvOptionsInitInternal`, without exposing C struct ABI negotiation as a
requirement for Rust callers. `SharpYuvGetVersion` reports the pinned upstream
compatibility version, distinct from the Cargo package version.

Validate positive geometry, legal depths, typed storage, nonzero step,
`stride >= width * step`, and each channel's minimum reachable length
`(height - 1) * stride + (width - 1) * step + 1`. Check all arithmetic,
even-rounded working dimensions, plane lengths and allocations, propagating
errors through `Result`. Retain C's dimension domain below `INT_MAX`, subject
to representable sizes; do not inherit WebP's 16,383-pixel limit. No final-row
padding is required in a Rust input slice. Negative steps/strides and raw C
ABI are deferred. Validate custom color-space/matrix inputs before arithmetic
that would be undefined in C; exact parity covers the defined C domain.

## Function dependency map

Paths below are relative to `upstream/sharpyuv/`. This is the complete scalar
conversion path; initialization and allocation adaptations are listed below.

```text
SharpYuvConvert                                      sharpyuv.c
└── SharpYuvConvertWithOptions                        validation / matrix scaling
    └── DoSharpArgbToYuv                              import, initialize, iterate
        ├── ImportOneRow → GetPrecisionShift / Shift
        ├── StoreGray → SharpYuvRGBToGray              sharpyuv_dsp.h
        ├── UpdateW / UpdateChroma
        │   ├── SharpYuvUpdateWSrgb_C                  sharpyuv_dsp.c
        │   ├── SharpYuvUpdateChromaSrgb_C → ScaleDownSrgb_C
        │   └── generic ScaleDown / RGBToGray
        │       └── SharpYuvGammaToLinear / SharpYuvLinearToGamma
        │           └── tables/interpolation or transfer helpers  sharpyuv_gamma.c
        ├── InterpolateTwoRows
        │   ├── Filter2 → clip_bit_depth
        │   └── SharpYuvFilterRow_C → SharpYuvClip16
        ├── SharpYuvUpdateY_C / SharpYuvUpdateRGB_C
        └── ConvertWRGBToYUV
            └── SharpYuvConvertRowY_C / SharpYuvConvertRowUV_C
                └── SharpYuvConvertComponent / SharpYuvClip16

SharpYuvComputeConversionMatrix → ToFixed16            sharpyuv_csp.c
SharpYuvGetConversionMatrix → five fixed matrices      sharpyuv_csp.c
```

## Implementation order and acceptance gates

All five milestones are complete. The function-level order below records the
port boundaries and acceptance requirements for future changes.

| ID | Work | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| 0 (complete) | Self-contained module, checked types/errors, existing oracle extension and initial fixtures | Existing pin/oracle conventions | Rust library builds without C; 153 portable fixtures and live C checks |
| 1 (complete) | Fixed-point leaves, matrices and gamma/transfer functions | 0 | 3,317 exact requests, full tables and over 6.6 million gamma/inverse samples; independent rounding/interpolation checks |
| 2 (complete) | All scalar DSP kernels, including sRGB paths | 1 | 976 exact requests; signed arithmetic, clipping, row boundaries and guards checked |
| 3 (complete) | Row helpers, final matrix conversion and iterative image conversion | 2 | 2,304 exact imported-row/iteration requests; all three loop exits observed |
| 4 (complete) | Public conversion/options/version functions and acceptance corpus | 3 | 6,772 public requests, portable regressions, real/skinny images, validation and recorded scalar timing/buffer baseline |

### 0. Module and scalar reference

Follow the existing oracle's pin/clean-checkout checks and recorded Release
configuration, with `WEBP_ENABLE_SIMD=OFF` and `WEBP_USE_THREAD=OFF`. Build the
pinned `sharpyuv` C target through the existing oracle build and add requests
to its test executable. Reuse the existing build rather than introducing a
separate SharpYUV harness. Put wrappers outside `upstream/`. Static helpers
require same-translation-unit wrappers that include the original source, with
renamed exported symbols where necessary to avoid duplicate definitions. Public
comparisons must also call the normally built C library.

Use logical samples, signed coefficients, u64 low/high words and exact f32
bit patterns in the existing request/response convention. Commit a few matrix,
gamma, DSP and complete-conversion C fixtures before implementing their Rust
counterparts. Record revision, compiler, libm/platform, flags and regeneration
commands in `libwebpop/tests/reference/SHARPYUV.md`, linked from the existing
reference README. Fixture generation stays opt-in.

### 1. Math, matrices and transfer leaves

Port in dependency order, retaining the two module-local `Shift` functions:

1. `sharpyuv_dsp.h`: `SHARPYUV_YUV_FIX`, `SharpYuvClip16`,
   `SharpYuvRGBToGray`, `SharpYuvConvertComponent`.
2. `sharpyuv_csp.c`: `ToFixed16`, the five exact matrix constants,
   `SharpYuvGetConversionMatrix`, then `SharpYuvComputeConversionMatrix`.
3. `sharpyuv_gamma.c`: `Shift`, `Roundf`, `Powf`, `Log10f`,
   `SharpYuvInitGammaTables`, `FixedPointInterpolation`, `ToLinearSrgb`,
   `FromLinearSrgb`.
4. Both `ToLinear` and `FromLinear` helpers for `709`, `470M`, `470Bg`,
   `Smpte240`, `Log100`, `Log100Sqrt10`, `Iec61966`, `Bt1361`, `Pq`,
   `Smpte428` and `Hlg`.
5. `SharpYuvGammaToLinear` and `SharpYuvLinearToGamma`, including all enum
   aliases and the special linear return path.

Preserve the 1,026-entry and 514-entry gamma tables, their repeated final
entries, integer interpolation and exact endpoints. The path named sRGB uses
the pinned formulas/tables; do not replace it with another sRGB formula.
Use immutable once-initialized tables instead of mutable globals and volatile
flags. Carry `WEBP_INLINE` and `SHARPYUV_INLINE` annotations to plain
`#[inline]`, including header helpers.

Use i64 for weighted gray/component products, as the pin does. Preserve f32
operations and f64 promotions: `Powf` calls double `pow` then casts to float;
`Roundf` performs its half adjustment in float before double floor/ceil.
`ToFixed16` uses floor after a float expression, including for negative
coefficients. Preserve branch inequalities, clamps, log midpoints, PQ's
`FLT_MIN` denominator and HLG's float exp/log/sqrt calls. Linear transfer
returns the input directly, rather than rescaling it to a uniform 16-bit range.

Compare every table entry, exhaustively sweep integer gamma domains at the
internal depths 10/12/14 and the inverse's reachable linear values, and probe
float helpers at each branch boundary. Check matrix coefficients at all
supported depths/ranges, including WebP versus Rec.601 limited differences.
Independently check clipping, rational weighted sums and interpolation.
Investigate any libm divergence at its first intermediate; a sample tolerance
does not satisfy exact parity.

### 2. Scalar DSP

Port `SharpYuvUpdateY_C`, `SharpYuvUpdateRGB_C`, `SharpYuvFilterRow_C`,
`SharpYuvConvertRowY_C` and `SharpYuvConvertRowUV_C` using milestone 1 leaves.
Then port `SharpYuvUpdateWSrgb_C`, `ScaleDownSrgb_C` and
`SharpYuvUpdateChromaSrgb_C`. Bind the corresponding non-`_C` DSP names
directly to these functions. Replace `SharpYuvInitDsp` with static scalar
selection; no function-pointer table or CPU module is needed.

Preserve target-minus-reconstructed update direction, u64 absolute-Y error
sums, signed UV storage/narrowing, arithmetic right shifts, filter rounding,
per-output-depth clipping and the separate RGB segments in row scratch.
Test lengths 0 where supported, 1 and odd/even lengths, positive/negative
residuals, full-range values and clipping thresholds. Use sentinel padding to
check exact writes and lookahead requirements. Independently expand the
9/3/3/1 filter and matrix dot products. Compare specialized sRGB W/chroma
results against the generic formulas as well as C.

### 3. Row helpers and image iteration

Port `sharpyuv.c` helpers in this order:

1. `GetPrecisionShift`, `clip_bit_depth`, `Shift`, `Filter2`.
2. `ScaleDown`, `UpdateW`, `UpdateChroma`, `StoreGray`, `ImportOneRow`.
3. `InterpolateTwoRows` and `ConvertWRGBToYUV`.
4. `DoSharpArgbToYuv`, after every called helper matches independently.

`SafeMalloc` becomes checked size calculations and fallible owned allocations.
Keep `fixed_y_t = u16` and `fixed_t = i16`; separate typed buffers replace
C's single allocation and signed/unsigned pointer reinterpretation. Preserve
the logical scratch/plane layout and mutation order.

Import adds two precision bits for 8/10/12-bit RGB and truncates 16-bit RGB
to 14 bits. Clamp out-of-range 10/12-bit samples before shifting. Replicate
odd right/bottom edges into even working dimensions. Initial `best_y` uses
gamma-domain gray while target W/chroma use the selected transfer path;
these initializations are intentionally different.

Preserve all four possible iterations, in-place row-pair updates and their
neighbor visibility. Previous/current/next chroma rows are views of the
mutating best-UV plane, not a snapshot of the whole previous iteration.
Stop only after an iteration, at `diff_y_sum < 3*w*h`, or after iteration 0
when error strictly increases over the preceding sum. Equality continues.
Final conversion uses reconstructed W plus subsampled RGB offsets and the
scaled matrix, cropping back to requested dimensions.

Compare imported rows, initial target/best planes, each iteration's updated
planes/error sum and final Y/U/V. Test replicated boundaries independently
and cover threshold exit, increasing-error exit and iteration-limit exit with
observed traces, retaining replay cases. Include 1×1, skinny and odd images,
sharp color edges, ramps and clipping-heavy patterns.

### 4. Public API and completion

Finish `SharpYuvGetVersion`, `SharpYuvOptionsInitInternal`,
`SharpYuvOptionsInit`, `SharpYuvConvertWithOptions`, then `SharpYuvConvert`.
Port the public wrapper's matrix coefficient scaling and precision-adjusted
offsets exactly, preserving integer rounding/truncation when RGB and YUV
depths differ. Replace `SharpYuvInit`'s lock/CPU-cache behavior with safe
one-time gamma initialization and direct scalar calls. The only unported C
functions should be the explicitly deferred CPU/SIMD implementations.

Compare public output samples exactly across all 12 RGB/YUV depth pairs,
all supported transfer enums, predefined and computed matrices for the
requested conversion, and representative valid custom matrices. Cover planar and
interleaved layouts, unrelated padding, minimum slices, unaligned u8 starts,
RGBA alpha independence, saturated colors, deterministic random images and
a recorded real-image sample. Require unchanged input, default/options
identity, output dimensions/ranges and consistent results on repeated and
concurrent calls. Invalid depths/enums, zero/overflow geometry, short buffers,
bad strides and rejected custom parameters must return errors without panic.

Record a small release timing/memory baseline against scalar C for ordinary
8-bit sRGB and a high-depth case. Timing is informational, not a test gate.
Do not require a WebP encoder/decoder or image-quality threshold to accept a
sample-exact RGB-to-YUV port.

Use `sharpyuv` in test names so focused runs cover the module. Run from the
repository root:

```sh
cargo build -p libwebpop --lib
cargo test -p libwebpop sharpyuv
cargo test -p libwebpop --features native-oracle sharpyuv -- --nocapture
cargo test -p libwebpop --release --features native-oracle sharpyuv -- --nocapture
cargo test --workspace
cargo fmt --all -- --check
```

Completion means all scalar functions above are ported or have a documented
Rust ownership/initialization replacement, portable fixtures and independent
checks pass without upstream tooling, the live scalar corpus matches exactly,
and the workspace regressions pass. Review the module's dependency boundary
to ensure it remains independent of encoder internals. Update this plan with
implementation and reference evidence as each milestone completes. SIMD
remains separate work.

## Completed encoder integration

`encode_lossy_rgba` exposes `LossyOptions { quality, alpha_quality,
use_sharp_yuv }` while retaining method 4, default preprocessing/cleanup and
the existing output-buffer guarantees. The simple lossy entry points select
ordinary conversion. The CLI accepts `--lossy --sharp_yuv` and equivalent
`--lossy --pre 4`; `--pre 0` selects ordinary conversion unless the explicit
sharp flag is also set. Other preprocessing values remain unsupported.

The encoder-side `import_rgba_to_yuva_sharp` adapts RGBA directly to the
standalone module's typed views and transfers its owned Y/U/V planes into
the encoder picture. It copies nonopaque alpha independently. If either
dimension is below upstream's `kMinDimensionIterativeConversion` (4), it
uses the existing ordinary alpha-aware importer. Transparent-area cleanup
and alpha compression run afterward, retaining upstream order.

There are 432 exact pinned-C YUVA import comparisons and 2,318 complete-file
comparisons with independent decoding, plus 60 portable C fixtures. They
cover both sides of the dimension threshold, odd boundaries, alpha policies,
cleanup, default/sharp/preprocessing routes, quality and alpha quality,
unaligned/padded/minimum inputs, skinny limits and real images. CLI tests
cover files, streams, `--noalpha`, route aliases and rejection before input
access. SharpYUV production code still depends only on its own modules and
the standard library. See the
[reference record](../libwebpop/tests/reference/SHARPYUV.md#encoder-integration).
