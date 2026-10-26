# Lossy photo q80 / method 4: C versus Rust profiles

The release Rust CLI takes **2.28× C's time** on the full-size
`photo-japan-p50.webp`. About **80% of the excess sampled cycles** are in the
4×4 and 16×16 luma mode searches. Quantization and inverse transforms together
account for about **44% of the excess**; buffer copying and initialization
account for another **12%**, mostly in the 4×4 search.

This is a 2026-10-26 follow-up to the [five-image matrix](../BENCHMAKRS.md).
[Raw timings, profiles, counters, hashes and classification rules](2026-10-26-lossy-photo-profile.json)
accompany this report. No encoder algorithms or DSP implementations changed.

## Matched work and timing check

Input is the same 5,492 × 3,672 packed RGBA8 PAM used by the matrix, prepared
from the checked-in WebP without resizing. Both CLIs use lossy q80, method 4,
ordinary YUV conversion, default alpha quality 100 and single-threaded encoding.
C is the existing GCC 14.2.0 SIMD-enabled `-O3 -march=x86-64-v3` artifact at
libwebp `1844042d1ca751e62e61df7617dba19c98588339`. The profiling run records
repository revision `fa125e0`, whose encoder sources and Cargo configuration
match the matrix baseline at `a04a157`. Rustc is
`1.101.0-nightly (76456a688 2026-10-25)` / LLVM 23.1.4.

Both Rust builds use optimization level 3, x86-64-v3, LTO off, 16 codegen units,
assertions/overflow checks off, incremental off and panic unwind. The profiling
build adds debug level 2 and `-Wl,--no-rosegment` for DWARF unwinding. Explicit
`RUSTFLAGS` replace the shell's inherited `target-cpu=native`, and
`CARGO_ENCODED_RUSTFLAGS` is unset. Effective compiler commands are archived.

Before profiling, eight balanced measured blocks plus one warmup compare the
C, archived release Rust and profiling Rust executables, pinned to logical
CPU 2 on the same i9-13900KF. No perf capture or build overlaps this timing.
The boundary is whole CLI process latency on prepared PAM, including input,
import, allocation, encoding and buffered output.

| Implementation | Median seconds | IQR | Encoded bytes |
| --- | ---: | ---: | ---: |
| C SIMD | 1.1146 | 0.35% | 2,346,540 |
| Rust release | 2.5434 | 0.35% | 2,346,540 |
| Rust profiling | 2.4740 | 0.14% | 2,346,540 |

All outputs are byte-identical, deterministic and independently decoded.
Decoded RGB PSNR is 35.881387 dB against prepared visible RGB; alpha is exact.
The profiling build uses about 2.7% less time than release by medians
(paired speedup 1.030×, 95% bootstrap interval 1.027–1.036). Its profile is
representative of the large gap, but it is a different executable; the debug
and linker changes were not tested separately.

## Capture quality and stage costs

Each capture records 24 fresh CLI processes, after a warmup, on CPU 2 using
`cpu_core/cycles/u`, 99 Hz and `--call-graph dwarf,64000`. C and Rust captures
are sequential and separate. These are period-weighted user-mode cycle
estimates divided by 24, not exact per-function hardware totals or paired
wall-time samples. No builds overlap the captures.

C has 2,583 samples and Rust 5,826, with **zero lost samples**. Stacks reach
`main` for 99.96% of C and 100% of Rust sampled periods. Unknown leaf addresses
account for 0.30% and 0.23% respectively. Inline frames are included.

Every sample belongs to exactly one stage: the first matching stack pattern
wins. Stage costs include callees, allowing comparison across different
inlining decisions. Billion estimated cycles per encode:

| Stage | C | Rust | Share of excess cycles |
| --- | ---: | ---: | ---: |
| 4×4 luma mode search | 2.837 | 7.129 | 57.6% |
| 16×16 luma mode search | 0.863 | 2.535 | 22.4% |
| Chroma mode search | 0.469 | 0.987 | 6.9% |
| Initial analysis | 0.446 | 0.688 | 3.2% |
| RGBA-to-YUV import | 0.153 | 0.327 | 2.3% |
| Token recording | 0.307 | 0.483 | 2.4% |
| Token emission | 0.551 | 0.691 | 1.9% |
| Other mode decision work | 0.009 | 0.166 | 2.1% |
| Other encode-loop work | 0.021 | 0.074 | 0.7% |
| Probability updates | 0.000 | 0.002 | 0.0% |
| Other process work | 0.125 | 0.155 | 0.4% |
| **Total** | **5.780** | **13.237** | **100%** |

The 4×4 search explains about 58% of the gap and the 16×16 search another
22%. RGBA import accounts for only about 2%; token recording and emission
together account for about 4%. Input conversion and bitstream writing are
not the dominant targets in this capture.

## Kernel costs and concrete leads

A second classification assigns each sample once to a kernel family, using
its full stack. This is another view of the same samples: **do not add this
table to the stage table**. In particular, nested `QuantizeBlock` /
`Quantize2Blocks` and `Disto4x4` / `Disto16x16` frames are counted once.

| Kernel family | C billion cycles | Rust billion cycles | Share of excess cycles |
| --- | ---: | ---: | ---: |
| Quantization | 0.540 | 2.383 | 24.7% |
| Inverse transforms | 0.707 | 2.135 | 19.1% |
| Forward transforms | 0.637 | 1.438 | 10.7% |
| Spectral distortion | 0.483 | 1.286 | 10.8% |
| Copying / initialization | 0.003 | 0.871 | 11.6% |
| Prediction | 0.116 | 0.736 | 8.3% |
| Residual costing | 0.764 | 1.187 | 5.7% |
| Pixel distortion | 0.437 | 0.391 | -0.6% |
| Other work | 2.093 | 2.809 | 9.6% |

Disassembly and source provide three concrete leads:

- **Quantization:** Rust `QuantizeBlock_C` retains a scalar zigzag-index loop,
  coefficient threshold branches and a per-iteration bounds check on the
  loaded index. C `QuantizeBlock_SSE41` uses packed operations on coefficients.
  Quantization is the largest kernel-family contribution, about 25% of the
  excess. This does not establish how much of its gap is specifically bounds
  checking versus scalar arithmetic, branches or dispatch.
- **Inverse transforms:** Rust `ITransformOne` remains a separate function
  with substantial scalar arithmetic, including widened multiplies. C uses
  `ITransform_SSE2`. This family contributes about 19% of the excess.
- **Copies and initialization:** Rust `Copy4x4_C` emits **four calls** to
  `core::slice::copy_from_slice_impl::<u8>` for four-byte rows; that helper
  dispatches to libc copying code. The 4×4 candidate path also constructs a
  temporary block and copies it to scratch/output for each trial. Other
  disassembly shows 512-byte whole-buffer copies around reconstruction and
  `VP8IteratorRotateI4`, plus coefficient-array initialization. About 0.727
  billion cycles per encode in copying/initialization have `PickBestIntra4`
  in their stack, roughly 10% of the total excess. This includes multiple
  call sites, not solely the row-copy helper.

Relevant Rust sources are [DSP kernels](../../libwebpop/src/dsp/enc.rs) and
[mode searches](../../libwebpop/src/enc/quant_enc.rs). The C searches in
[quant_enc.c](../../upstream/src/enc/quant_enc.c) reconstruct directly into
scratch buffers and swap pointers between candidates. Reworking the Rust
scratch ownership and fixed-size copies is a concrete scalar experiment;
quantization and inverse-transform code generation are larger subsequent
targets. Preserve candidate order, scratch contents and output-byte parity.

Pixel-distortion kernels are slightly cheaper in Rust in this capture;
disassembly shows AVX2 auto-vectorization in, for example, `SSE16x8_C`.
The cost is concentrated in particular kernels and buffer handling rather
than a uniform slowdown of all DSP work.

## Hardware counter check

Separate `perf stat` runs repeat each profiling executable five times on CPU 2,
without call-stack recording. Means per process, user-space events:

| Counter | C | Rust | Rust / C |
| --- | ---: | ---: | ---: |
| Cycles | 5.848 B | 13.274 B | 2.27× |
| Instructions | 13.875 B | 46.184 B | 3.33× |
| Branches | 0.964 B | 5.370 B | 5.57× |
| Branch misses | 49.016 M | 84.105 M | 1.72× |

Rust retires about 3.33× as many instructions, but at about 3.48 instructions
per cycle versus C's 2.37. Branch miss rates are about 1.57% in Rust and 5.08%
in C. The overall gap therefore comes with substantially more executed work,
not lower aggregate IPC. These counters do not isolate individual bounds
checks or eliminate local cache/latency effects.

## Reproduction and artifacts

After building the release baseline/helper using the matrix's instructions:

```sh
PERF_EVENT=cpu_core/cycles/u bash scripts/profile-lossy.sh \
  target/benchmarks/2026-10-26-matrix-remaining/inputs/1-photo-japan-p50.pam \
  target/profile-lossy-photo-next 2 24
```

`BASELINE_RUST` and `BENCHMARK_HELPER` can select archived executables. This
capture used `target/benchmarks/2026-10-26-matrix/bin/rust-v3` and
`target/benchmarks/2026-10-26-matrix/bin/benchmark_images`. The default paths are
`target/benchmark-v3/release/cwebpop` and its `examples/benchmark_images` helper.
Use a new output directory. The explicit core PMU and CPU number are specific
to this hybrid Intel host; use appropriate values on another machine.

Full local artifacts are in `target/profile-lossy-photo/run/`: both perf
captures, `c.svg` and `rust.svg` flamegraphs, inline stack exports, flat/inclusive
reports, annotated disassembly, counter output, unprofiled timings, archived
binaries, input/output hashes, source snapshots and build logs. The initial
verbose compilation is in `initial-rust-build.log`; `rust-build.log` records
the script's cached build. Preserve this directory before cleaning `target/`.

The script now collects the counter check as well. The original separate
counter invocations are recorded in `stat-commands.json`. Re-aggregate an
existing capture with:

```sh
python3 scripts/compare-profiles.py target/profile-lossy-photo/run --mode lossy
```
