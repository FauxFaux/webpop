# Portable wide luma-search DSP

The completed portable DSP port takes **1.625 s**, versus **2.545 s**
for scalar Rust and **1.124 s** for SIMD C on the full-size photo at q80/m4.
That is **1.57x faster than scalar Rust**, reducing the time ratio to C
from 2.26x to **1.44x**. All output bytes are identical.

## Matched measurement

Follow-up to the [luma-search profile](2026-10-26-lossy-photo-profile.md).
The input is the same 5,492 x 3,672 RGBA PAM prepared from
`samples/photo-japan-p50.webp`, with ordinary YUV conversion, default alpha
quality 100, and single-threaded encoding. The boundary is whole CLI process
latency on prepared PAM. One warmup and eight balanced, randomized measured
blocks run pinned to CPU 2 on the same i9-13900KF, without overlapping builds,
tests, or perf captures.

C is the existing GCC 14.2.0 SIMD-enabled `-O3 -march=x86-64-v3` artifact at
libwebp `1844042d1ca751e62e61df7617dba19c98588339`. Scalar Rust is the archived
matrix binary. The new Rust build uses the same compiler and release settings:
opt level 3, x86-64-v3, LTO off, 16 codegen units, debug/assertions/overflow checks
and incremental off, panic unwind. `wide` is locked to 1.7.1.

| Implementation | Median seconds | IQR |
| --- | ---: | ---: |
| C SIMD | 1.1244 | 0.34% |
| Rust scalar DSP | 2.5453 | 0.43% |
| Rust wide DSP | 1.6247 | 0.37% |

The paired geometric speedup over scalar Rust is 1.570x
(95% bootstrap interval 1.565–1.574).
Each file has 2,346,540 bytes and SHA-256
`995b3727df8f8eb8227fb7fe88b610246f4ee7c9106f1c86fe9d02e6d4922bce`.
Independent decoding gives RGB PSNR 35.881387 dB and exact alpha.
[Raw timings, hashes, compiler/host information and width experiment](2026-10-26-lossy-wide.json)
are committed; full binaries, outputs, source snapshot and command provenance
remain in `target/benchmarks/2026-10-26-lossy-wide/`.

## Implementation and technique checks

The [intrinsics guide](../INTRINSICS.md#implemented-portable-encoder-kernels-2026-10-26)
records the `_wide` naming convention and upstream coverage. The encoder calls
portable kernels directly on every target, including wide's scalar fallback.
Coverage includes prediction, forward/inverse transforms and WHT,
quantization, pixel/texture distortion, histogram/mean statistics, residual
classification/costs, and NEON's AC-flatness helper. Scalar kernels remain
correctness references. The upstream source pin and fixtures are unchanged.

Release disassembly confirms that `Copy4x4_C` now uses four 32-bit loads and
stores after its length checks, with no helper/libc call on the normal path.
The first prototype retained out-of-line `array::map` helpers in tiny vector
casts, narrowing and fixed shuffles and measured 6.833 s. Whole-array casts,
explicit fixed shuffles and fixed-array stores removed those calls before the
measurements above. No LTO or unsafe-code policy change was needed.

A separate five-block comparison tried eight-lane AVX2 quantization while
holding the other DSP unchanged: four lanes took 1.665 s, eight took 1.680 s,
with matching bytes. The wider layout was not retained. This experiment
preceded the final `IsFlat` port. The final build uses the four-lane layout on
all targets. This host's result does not select the best technique for ARM;
additional techniques should be tested and measured before build-time selection.

## Validation

- `cargo test --offline --workspace`: 359 tests passed, two existing timing tests ignored.
- Native oracle tests across 16 relevant lossy suites: 132 passed, one existing timing test ignored. Includes reconstruction, searches, methods 0–6, trellis, complete files and existing scalar DSP fixtures.
- `RUSTFLAGS='-Ctarget-cpu=x86-64' cargo test -p libwebpop --offline --test lossy_wide --target-dir target/wide-baseline`: all six parity tests pass on baseline SSE2.
- All six parity tests also run through a temporary exported Wasm check under Wasmtime, compiled for `wasm32-unknown-unknown` separately with `-Ctarget-feature=-simd128` and `+simd128`. Both return success, exercising scalar fallback and Wasm SIMD. The harness source, manifests, build logs and modules are archived under this run's `provenance/wasm/`.
- `cargo fmt --all -- --check` and `git diff --check` pass.

The new corpus checks full signed coefficient ranges, arbitrary quantization
matrices, strict threshold ties, clamping/rounding, every prediction and border
case, unaligned/minimum transform buffers, padding, coefficient scatter, DC
exclusion and flatness thresholds. No ARM execution or new perf attribution was
performed; the end-to-end speedup is measured for this image and host.

## Reproduction

Build the release CLI with the settings above, then run:

```sh
python3 scripts/benchmark.py \
  --c reference-cwebp/artifacts/cwebp \
  --rust scalar=target/benchmarks/2026-10-26-matrix/bin/rust-v3 \
  --rust wide=target/benchmark-v3/release/cwebpop \
  --helper target/benchmarks/2026-10-26-matrix/bin/benchmark_images \
  --case lossy:80:4 --runs 8 --warmups 1 --cpu 2 --seed 20261026 \
  --output target/benchmarks/lossy-wide-next \
  target/benchmarks/2026-10-26-matrix/inputs/3-photo-japan-p50.pam
```
