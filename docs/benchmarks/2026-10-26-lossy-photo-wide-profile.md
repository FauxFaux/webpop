# Widened Rust: lossy photo q80 / method 4 profile

Focus next on **candidate-state initialization and inverse transforms**.
Together these families account for about **40% of the remaining sampled
cycle gap**. The 4×4 and 16×16 luma searches still explain about **73%** of
that gap. Quantization has improved substantially and now contributes about
11%, alongside spectral distortion at about 11%.

This repeats the [earlier scalar experiment](2026-10-26-lossy-photo-profile.md)
on the same full-size `photo-japan-p50`, after the
[kernel and copy changes](2026-10-26-lossy-wide.md). Repository revision is
`8028ff5`; encoder sources and Cargo configuration
match `a239a59`. No encoder code changes were made during this experiment.
[Raw timings, counters, hashes and classifications](2026-10-26-lossy-photo-wide-profile.json) accompany
the report.

## Matched work and timing

Input is the identical 5,492 × 3,672 RGBA8 PAM from the original experiment.
Both CLIs use q80/m4, ordinary YUV conversion, default alpha quality 100 and
one encoding thread. C is the unchanged SIMD-enabled GCC 14.2.0 artifact,
`-O3 -march=x86-64-v3`, pinned libwebp
`1844042d1ca751e62e61df7617dba19c98588339`, without LTO.

Rust uses rustc 1.101.0-nightly (76456a688), LLVM 23.1.4, optimization 3,
x86-64-v3, LTO **off**, 16 codegen units, assertions/overflow checks and
incremental off, panic unwind. Explicit `RUSTFLAGS` replace inherited
`target-cpu=native`; `CARGO_ENCODED_RUSTFLAGS` is unset. Profiling adds debug
level 2 and `-Wl,--no-rosegment`. Effective library and CLI compiler commands
are retained in JSON and the local build log.

Eight balanced measured blocks plus one warmup compare C, the current release
and profiling Rust executables before capture. Processes run serially on
CPU 2 of the same i9-13900KF, SMT sibling 3, with existing power policy.
Whole CLI latency includes prepared PAM reading/import, allocation, encoding,
buffered output and exit. Decoding and validation are outside timing.

| Implementation | Median seconds | IQR / median |
| --- | ---: | ---: |
| C SIMD | 1.1228 | 0.09% |
| Wide Rust release | 1.6210 | 0.32% |
| Wide Rust profiling | 1.6191 | 0.12% |

Release Rust takes **1.44× C's time**.
The profiling/release medians differ by only
0.11%;
paired release/profiling speedup is 0.998×
(95% bootstrap interval 0.991–1.002).
No timing outliers are discarded. The profiling build's maximum was
1.6627 s.

All 27 validation encodes are deterministic. The final capture and counter
outputs also retain the original **2,346,540-byte** output and SHA-256
`995b3727df8f8eb8227fb7fe88b610246f4ee7c9106f1c86fe9d02e6d4922bce`.
Independent decode gives unchanged RGB PSNR 35.881387 dB and exact alpha.

## Remaining stage and kernel costs

As before, each capture contains 24 fresh encodes after a warmup, pinned to
CPU 2, with `cpu_core/cycles/u`, 99 Hz and DWARF stack size 64,000.
The C and Rust captures, timing checks and counter runs are sequential;
no builds or tests overlap them. There are 2,578 C samples and
3,777 Rust samples, **zero lost samples**, and `main` appears in
100% of sampled periods for both. Unknown leaves account for
0.80% of C and 0.50% of Rust periods.

Values below are **billion estimated user cycles per encode**, obtained by
summing sample periods and dividing by 24. They are sampling estimates,
not exact per-function counter totals. Before/after Rust columns come from
separate captures; current C provides the denominator for remaining excess.
The earlier C total was 5.780 B versus 5.772 B now.
Close rankings should be treated as similar-sized targets.

Each sample is assigned once to a stage, including callees:

| Family | C now | Scalar Rust before | Wide Rust now | Share of remaining excess |
| --- | ---: | ---: | ---: | ---: |
| 4×4 luma search | 2.933 | 7.129 | 4.233 | 47.2% |
| 16×16 luma search | 0.846 | 2.535 | 1.550 | 25.5% |
| Chroma search | 0.400 | 0.987 | 0.657 | 9.3% |
| RGBA-to-YUV import | 0.143 | 0.327 | 0.334 | 7.0% |
| Initial analysis | 0.415 | 0.688 | 0.447 | 1.1% |
| Token recording | 0.288 | 0.483 | 0.370 | 3.0% |
| Token emission | 0.550 | 0.691 | 0.661 | 4.0% |
| Other mode decision | 0.011 | 0.166 | 0.034 | 0.8% |
| Other encode-loop work | 0.023 | 0.074 | 0.060 | 1.3% |
| Other process work | 0.162 | 0.155 | 0.180 | 0.6% |
| **Total** | **5.772** | **13.237** | **8.526** | **100%** |

The following is a separate partition of the same samples by kernel family.
Do not add its costs to the stage table. Nested block/pair quantizers and
4×4/16×16 distortion calls are counted once.

| Family | C now | Scalar Rust before | Wide Rust now | Share of remaining excess |
| --- | ---: | ---: | ---: | ---: |
| Inverse transforms | 0.647 | 2.135 | 1.224 | 20.9% |
| Copying / initialization | 0.048 | 0.871 | 0.562 | 18.7% |
| Quantization | 0.600 | 2.383 | 0.909 | 11.2% |
| Spectral distortion | 0.506 | 1.286 | 0.806 | 10.9% |
| Residual costing | 0.752 | 1.187 | 0.987 | 8.5% |
| Forward transforms | 0.638 | 1.438 | 0.843 | 7.4% |
| Pixel distortion | 0.439 | 0.391 | 0.597 | 5.7% |
| Prediction | 0.082 | 0.736 | 0.152 | 2.5% |
| Other work | 2.060 | 2.809 | 2.447 | 14.1% |

The classifier now recognizes `inverse_one`: the single-block
`ITransform_wide` entry point tail-calls it, leaving no `ITransform` frame
in those stacks. Both inline and fully qualified names are covered. Without
this correction, most inverse-transform work would be mislabeled “Other”.
The previous scalar capture has no `inverse_one` symbol, so its classification
is unchanged. The exact updated patterns are in JSON and
[`compare-profiles.py`](../../scripts/compare-profiles.py).

## Concrete next experiments

1. **Reduce candidate initialization and scratch movement.** Copy/init costs
   fell from 0.871 to 0.562 B cycles, but still explain about 19% of the gap.
   Of the current Rust total, 0.360 B is in `default` stacks; 0.333 B is
   specifically default initialization under `PickBestIntra4`, about 12% of
   the entire remaining gap. Assembly confirms repeated out-of-line default
   calls for the 512-byte luma and 256-byte chroma coefficient arrays inside
   the candidate loop. The 4×4 search constructs a complete `VP8ModeScore`
   for each trial even though coefficients are tracked separately in
   `levels`. Start by using only the score fields needed by each trial, or
   otherwise making initialization explicit enough to eliminate dead array
   initialization. Preserve initialized state and existing observer behavior.
   Then examine the remaining 512-byte destination copies in the 16×16
   search and iterator rotation. `Copy4x4_C` itself is now four direct
   32-bit loads/stores plus length checks; its old four libc/helper calls
   are gone. Large-buffer ownership and state initialization are the more
   substantial remaining copy leads.
2. **Improve the inverse-transform technique.** Rust now spends 1.224 B
   versus C's 0.647 B, about 21% of the gap. It is vectorized, but
   `inverse_one` still emits 12 `vpmulld` instructions across its two passes,
   plus high/low splitting and shuffles. C uses packed 16-bit multiply-high
   arithmetic and a dedicated paired-block layout. Rust preserves a broader
   full-i16 input contract, so directly adopting C's narrow arithmetic needs
   range evidence. A concrete experiment that preserves that contract is
   to specialize the first pass: its inputs are sign-extended i16 values,
   so multiplication by 20,091 or 35,468 fits i32 and can use a direct
   product/shift. Retain exact wide arithmetic where the second pass needs
   it. Also measure paired-block processing against today's two serial
   `inverse_one` calls. These are hypotheses to benchmark, not measured wins.
3. **Then revisit quantization, spectral distortion and residual costs.**
   Their excesses are 0.309, 0.300 and 0.236 B cycles respectively. The
   quantizer is now straight-line packed code without the old scalar zigzag
   loop; widening it again is not an automatic next win (the earlier
   eight-lane experiment gave no end-to-end benefit). Inspect packing,
   matrix loads and paired-block reuse. Spectral distortion uses 32-bit
   arithmetic for its full weight domain. Residual costing retains a
   dependent table walk with index checks, making table layout and bounds
   exposure concrete follow-ups. Keep parity tests for all broader domains.

These percentages are attribution, not recoverable speedup estimates.
The two leading families' combined excess is about 13% of current Rust
sampled cycles; experiments can recover only part of it. Pixel distortion
also merits checking: Rust's current widened family costs 0.597 B versus
0.391 B in the earlier auto-vectorized scalar capture, and C is essentially
unchanged at 0.439 B. This cross-capture result suggests an A/B experiment;
it does not isolate the kernel change. Import is now about 7% of the remaining
gap, while analysis is close to C; luma work remains the first focus.

## Hardware counters

Separate `perf stat -r 5` runs use the profiling executables on CPU 2,
without stack recording. Mean counts per process:

| Counter | C now | Scalar Rust before | Wide Rust now | Wide / C |
| --- | ---: | ---: | ---: | ---: |
| Cycles | 5.807 B | 13.274 B | 8.543 B | 1.47× |
| Instructions | 13.877 B | 46.184 B | 25.328 B | 1.83× |
| Branches | 0.965 B | 5.370 B | 2.902 B | 3.01× |
| Branch misses | 48.977 M | 84.105 M | 39.465 M | 0.81× |

Versus scalar Rust's earlier capture, instructions fall about **45%**,
branches **46%**, branch misses **53%**, and cycles **36%**. Current Rust IPC
is 2.96 versus C's 2.39; branch miss rates are 1.36% versus 5.08%.
There is still substantially more executed work, but the prior instruction
and branch explosion has narrowed. Counter runtime coverage is 99% for all
four events; cycle variation is below 0.1% in both five-repeat runs.

## Reproduction and artifacts

```sh
BASELINE_RUST=target/benchmark-wide-v3/release/cwebpop \
BENCHMARK_HELPER=target/benchmark-wide-v3/release/examples/benchmark_images \
  bash scripts/profile-lossy.sh \
    target/benchmarks/2026-10-26-matrix-remaining/inputs/1-photo-japan-p50.pam \
    target/profile-lossy-photo-wide-next 2 24
```

Use a fresh directory and the v3, non-LTO release baseline described in the
[timing report](2026-10-26-lossy-wide-m4-m5.md). CPU and PMU event selection
are host-specific. Local artifacts in `target/profile-lossy-photo-wide/`
include both perf captures, `c.svg` / `rust.svg` flamegraphs, inline stacks,
flat/inclusive reports, annotated kernels, disassembly, counters, raw timing
blocks, copied executables and input/output hashes. `commands.log` records
the full harness invocation sequence. The updated classifier and report
generator are preserved alongside the capture; preserve this directory before
cleaning `target/`. Re-aggregate with:

```sh
python3 scripts/compare-profiles.py target/profile-lossy-photo-wide --mode lossy
```
