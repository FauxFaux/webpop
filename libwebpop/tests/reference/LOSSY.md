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
