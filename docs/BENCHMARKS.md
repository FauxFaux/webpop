# Lossless CLI benchmarks

`scripts/benchmark.py` compares the current Rust CLI with the supplied normal
SIMD-enabled `reference-cwebp/artifacts/cwebp`. It requires Python 3.11+, Cargo,
and the existing C artifact. It does not build C or modify `upstream/`.

From the repository root:

```sh
bash scripts/build-benchmarks.sh
python3 scripts/benchmark.py \
  --rust v3=target/benchmark-bins/rust-v3 \
  --rust thin=target/benchmark-bins/rust-v3-thin \
  --rust native=target/benchmark-bins/rust-native \
  --case 75:0 --case 75:4 --runs 8 --cpu 2 \
  --build-note 'bash scripts/build-benchmarks.sh; see target/benchmark-bins/*.build.log' \
  --output target/bench-next
```

Use a new output directory on every invocation. `--cpu 2` selects a logical CPU
on this machine, not a universal choice: check `lscpu` and sibling topology on
another host. Omit it to leave scheduling unconstrained. Stop other work during
measurement, including builds. The build script saves each executable before
building the next, with verbose build logs. Preserve those logs with results.
It compares default release v3, v3 with **ThinLTO and one codegen unit together**,
and native with the default release policy. This does not isolate LTO from the
codegen-unit change, or test native plus LTO. No production profile is changed.
The script reproduces build settings rather than promising byte-for-byte
reproducible linking; the archived binaries and their hashes identify precisely
what was timed. Keep the same toolchain and Cargo configuration when repeating
the experiment. Verbose build logs expose effective compiler arguments.

For the already-built binary alone:

```sh
python3 scripts/benchmark.py --case 75:4 --output target/bench-current
```

`--rust LABEL=PATH` is repeatable; its first value is the Rust baseline in
`versus_first_rust` results. `--compare-c-noasm` adds the same C executable with
`-noasm` as a third implementation, interleaved with normal C and Rust.
`--case QUALITY:METHOD` is also repeatable. Without
it, the runner uses `75:4`, `75:0`, and `100:6`; the last can be expensive on
large images. Positional filenames select a subset of inputs; otherwise all
`samples/*.webp` are used. `--exact` preserves and checks RGB beneath zero alpha.
Quality is integer compression effort, not visual quality; method is 0–6.
There is no multithreading option in these comparisons. These benchmarks use
strength 100; the Rust CLI also supports lower near-lossless strengths.
Presets and image hints are not implemented by the Rust CLI.

## Measurement and validation

A small Rust example decodes each source **once**, outside timing, and writes
an uncompressed packed RGBA8 PAM. Both encoders read exactly those bytes. This
keeps PNG/WebP decoding and differing YUV conversion out of the comparison,
and preserves alpha. The photo source is already lossy WebP; its prepared
pixels, rather than an unavailable original photo, are the lossless target.

Timing uses a monotonic clock around a fresh child process: argument parsing,
PAM reading/import, allocation, encoding, complete buffered file output, and
exit. This measures CLI process latency, not an isolated encoder call. PAM
parsing/import still differs between the CLIs. Filesystem caches are warm;
writes are not `fsync`ed. Preparation and output verification are untimed.

Each implementation gets one warmup per input/configuration, then one sample
in each measured block. A seeded random permutation of implementations is
followed by its reverse in the next block, balancing execution positions for
even run counts. Odd counts have one unbalanced block. Children run sequentially
on the runner's inherited affinity. Every output must be deterministic, and
the first output for each case/implementation must decode to the prepared
pixels with the independent `image` WebP decoder. Every alpha value and every
visible RGB value is checked; `--exact` checks hidden RGB too. Later outputs
must match the first output's SHA-256. C/Rust compressed-byte divergence is
reported, not silently accepted as equivalent compression work.

The console and `summary.json` report median, IQR, encoded size, throughput,
and compressed-byte parity. Speedup is **C seconds / Rust seconds**, above one
favoring Rust. Its estimate is the geometric mean of within-block ratios;
the 95% percentile bootstrap interval resamples whole blocks (4,000 draws,
recorded seed). `versus_first_rust` uses the same procedure for build tuning.
These are exploratory intervals, not a significance test: eight samples,
serial machine drift, multiple comparisons, and changing thermal/power state
limit their interpretation. No outliers are dropped. Small gains need another
quiet session. There is deliberately no single aggregate corpus score.

The output directory retains input hashes/geometry, prepared inputs, final
outputs, raw ordered samples including warmups, copies/hashes of executables,
runner/helper sources, Cargo.lock, git state/patch, toolchain and host records,
affinity/governors/resource limits, relevant environment, dynamic linkage and
the C artifact's build metadata. Untracked source changes are listed by git
status but are not included in the patch; preserve them separately. Supply
actual Rust build commands using `--build-note`: a binary cannot reveal its
original RUSTFLAGS, and the runner's environment is not build provenance.
Copy the result directory elsewhere before cleaning `target/`.

## Initial measurements

The initial session uses an Intel Core i9-13900KF, Linux x86-64, logical CPU 2
(SMT sibling 3), and rustc `1.100.0-nightly (923c95cdf 2026-09-16)` / LLVM
23.1.1. The governor reports `powersave`; turbo/power policy was left as found.
There are eight measured samples and one warmup per implementation/case, using
seed 20261023. No builds overlap these measurements. The supplied C artifact is
GCC `-O3 -g -DNDEBUG -march=x86-64-v3`, normal SIMD dispatch, no LTO, single
threaded encoding, upstream `1844042d1ca751e62e61df7617dba19c98588339`.
The fresh default v3 Rust executable has the same SHA-256 as the original
user-provided release binary. Initial host load average was about 5, so this
is an exploratory workstation measurement rather than an isolated lab result.

Median process seconds; all rows use quality 75, exact off. All four
implementations produced **identical compressed bytes** in all eight cases.

| Input | Method | C | Rust v3 | Rust v3 ThinLTO + 1 CGU | Rust native | Encoded bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0 | 0.0120 | 0.0133 | 0.0137 | 0.0132 | 267,900 |
| graphic-alpha-transparency | 4 | 0.1501 | 0.1540 | 0.1604 | 0.1542 | 158,204 |
| graphic-map | 0 | 0.1184 | 0.1417 | 0.1468 | 0.1418 | 139,876 |
| graphic-map | 4 | 1.5635 | 2.0598 | 2.0777 | 2.0319 | 94,476 |
| photo-japan-p50 | 0 | 2.4928 | 2.4869 | 2.5260 | 2.4770 | 27,204,402 |
| photo-japan-p50 | 4 | 5.1598 | 5.8086 | 5.8529 | 5.7989 | 24,549,614 |
| screenshot-wikipedia | 0 | 0.0505 | 0.0578 | 0.0595 | 0.0568 | 633,476 |
| screenshot-wikipedia | 4 | 0.1601 | 0.1830 | 0.1873 | 0.1815 | 399,986 |

Compact [raw measured samples, execution order, input/binary hashes and build
records](benchmarks/2026-10-23-lossless.json) are committed alongside this report.
Full local artifacts, including warmups and bootstrap summaries, are under
`target/bench-lossless/`; the preliminary run in `target/bench-pilot/` overlaps
compilation and is excluded from these results.

There is a clear performance signal on the map at method 4: default v3 Rust
takes about **32% longer**, and its paired C/Rust speedup interval is
0.736–0.762. The photo takes about **13% longer** (interval 0.881–0.906), and
the screenshot about **14% longer** (0.867–0.882). Alpha is close at about 3%
longer (0.974–0.988). On the method-0 photo, Rust and C are effectively tied
(0.987–1.009). Ratios here are paired geometric means; quoted percentages use
the table's medians. This is an input/effort-dependent gap, not a uniform
language penalty.

Native improves paired timings versus v3 by roughly 0–2% on this host. The
map method-4 estimate is 1.020x with interval 1.012–1.035; the photo method-4
estimate is 1.004x with interval 0.999–1.011. These modest effects do not close
the larger C gaps. ThinLTO plus one codegen unit usually regresses by about
1–4%; its map/photo method-4 intervals include no change. It makes the binary
smaller (about 2.7 MiB versus 3.1 MiB), but this session provides no speed case
for enabling that combination globally. Test LTO and codegen units separately
before assigning a cause. Neither flag adds the upstream SIMD kernels that
the Rust port currently lacks.

Method 0 is a useful speed/size choice, not equivalent work to method 4. For
example, on the photo it roughly halves time but increases output from 24.55
MB to 27.20 MB. On the map it is much faster but increases output from 94,476
to 139,876 bytes. These are lossless outputs in both cases.

A separate follow-up on the alpha input, quality 100, method 6, `--exact`, four
measured samples and one warmup, also produced byte-identical results:
150,676 bytes, C median 2.0333s, Rust v3 2.1745s. It checks the high-effort and
hidden-RGB path; it is not part of the main eight-case comparison. Its full
local artifacts are under `target/bench-exact/`.

The next useful investigation is profiling the same optimized map/photo
method-4 encodes to attribute the difference to searches, allocations, bounds
checks, or DSP kernels. These timings alone cannot distinguish those causes.
Keep the normal SIMD-enabled C target and byte-parity checks while doing that.

## Large smooth-colour follow-up

`graphic-invite.webp` is 10,416 × 7,812 (81.37 MP). The prepared input contains
at least 257 distinct visible RGBA colours, ruling out the whole-image
256-colour palette path. Using the same archived v3 Rust and C binaries,
CPU 2, quality 75, method 4, exact off, one warmup and four balanced measured
repeats:

| Implementation | Median seconds | IQR | MP/s | Encoded bytes |
| --- | ---: | ---: | ---: | ---: |
| C | 15.2336 | 0.52% | 5.34 | 2,783,988 |
| Rust v3 | 16.1465 | 0.88% | 5.04 | 2,783,988 |

All outputs were byte-identical and decoded correctly. Rust takes about **6%
longer** by median; paired C/Rust speedup is 0.946x, bootstrap interval
0.942–0.950. This is larger than the observed noise, but substantially smaller
than the map's 32% gap. It strengthens the case for profiling the map's palette
path separately; input size alone does not explain that relative gap.

Reproduce with the archived baseline, or substitute a newly built v3 binary:

```sh
python3 scripts/benchmark.py --rust v3=target/bench-lossless/bin/v3 \
  --case 75:4 --runs 4 --cpu 2 --output target/bench-invite-next \
  samples/graphic-invite.webp
```

[Raw samples and hashes](benchmarks/2026-10-23-invite.json) accompany this report;
full local artifacts are under `target/bench-invite/`. No flag sweep was run.

## Map profiling follow-up

The `profiling` Cargo profile inherits release optimization and retains full
debug information, including inline frames. `.cargo/config.toml` adds
`-Clink-arg=-Wl,--no-rosegment` for Linux x86-64. An environment `RUSTFLAGS` or
`CARGO_ENCODED_RUSTFLAGS` overrides Cargo's configured rustflags; include the
linker argument explicitly in that case. The profiling script does this and
selects x86-64-v3 to match the C artifact and original Rust baseline.

Using a prepared RGBA PAM from the benchmark runner:

```sh
PERF_EVENT=cpu_core/cycles/u bash scripts/profile-map.sh \
  target/bench-lossless/inputs/1-graphic-map.pam target/profile-map-next 2 12
```

The arguments are input PAM, **new** output directory, logical CPU, and repeat
count. This builds Rust, archives both executables, warms each implementation,
then captures 12 quality-75/method-4 encodes per implementation, exact off.
The essential recording command is:

```sh
perf record -o OUTPUT.perf.data -e cpu_core/cycles/u -F99 \
  --call-graph dwarf,64000 -- taskset -c 2 ENCODER ARGS...
```

The explicit core PMU avoids mixing the Intel hybrid host's core and atom
events. Select an event and CPU appropriate to another host; the script's
default event is `cycles:u`. The 64,000-byte DWARF stack snapshot matters:
16,384-byte snapshots frequently lost outer encoder callers, particularly in
Rust. These are user-mode cycle profiles, not wall-time or allocation profiles.
Repeated child processes are recorded sequentially; captures for C and Rust
are separate, so sampled cycle totals are exploratory attribution, not a
paired benchmark. Neither encoder enables multithreading.

Artifacts include `c.perf.data`, `rust.perf.data`, flat self and inclusive
reports, build logs, hashes, source configuration, and, when `flamegraph` is
installed, `c.svg` and `rust.svg`. Inspect interactively with
`perf report -i target/profile-map-next/rust.perf.data`. The script checks output
byte parity. `scripts/compare-profiles.py` also exports inline call stacks and
`comparison.json`: each sample's period is assigned once to the first matching
encoder stage, with per-function inclusive shares recorded separately. This
avoids comparing only self time across different compiler inlining decisions.

The 2026-10-23 capture is under `target/profile-map-full/`; preliminary smaller
stack captures are under `target/profile-map/`. The Rust profiling build uses
rustc `1.101.0-nightly (7189d39d9 2026-10-22)` / LLVM 23.1.4, default release
LTO/codegen settings, and x86-64-v3. C is the original SIMD-enabled artifact.
An unprofiled eight-block run, one warmup, CPU 2, gives:

| Implementation | Median seconds | IQR |
| --- | ---: | ---: |
| C | 1.5613 | 0.49% |
| Rust profiling | 2.0422 | 0.06% |
| Archived Rust v3 release | 2.0409 | 0.17% |

All outputs are 94,476 bytes, byte-identical and independently decoded and
verified. C/Rust profiling paired speedup is 0.763x (95% bootstrap interval
0.761–0.765). The profiling binary and archived release binary are effectively
tied, despite the newer Rust compiler; this is not an isolated measurement of
the debug-info/linker flags. Full timing artifacts are under
`target/bench-map-profile/`.

The final captures contain 1,817 C samples and 2,387 Rust samples with zero
lost samples. Their estimated cycle totals are 95.51 billion and 127.07 billion
over 12 encodes. Call stacks reach `main` for 100% of C and 98% of Rust sampled
periods; the missing outer Rust callers are in cache-size estimation, whose
nearby encoder frames remain identifiable. Unknown leaf addresses account for
0.93% and 1.26% respectively.

Stage shares include callees and inline frames. Estimated billion cycles per
encode are the sum of sampled event periods divided by 12, not exact hardware
counter totals:

| Stage | C share | Rust share | C billion cycles | Rust billion cycles |
| --- | ---: | ---: | ---: | ---: |
| Predictor transform | 46.21% | 55.05% | 3.678 | 5.829 |
| Hash-chain construction | 12.66% | 9.68% | 1.008 | 1.025 |
| LZ77 box search | 22.06% | 14.27% | 1.756 | 1.511 |
| Backward-reference traceback | 8.16% | 7.99% | 0.650 | 0.846 |
| Palette mapping/sorting | 2.87% | 4.80% | 0.229 | 0.508 |
| All sampled work | 100% | 100% | 7.960 | 10.589 |

The predictor transform contributes about **82% of the excess estimated
cycles**. Inside it, `GetResidual` including callees costs about 1.56 billion
cycles in C versus 3.56 billion in Rust. Histogram updates are not the source
of that increase: inclusive `UpdateHisto` costs about 1.84 versus 1.74 billion
cycles. These subregions overlap the predictor stage and must not be added to
the table. Hash-chain construction is close in absolute cycles despite very
different percentage shares; LZ77 box search is actually cheaper in Rust in
this capture. Palette work and traceback are smaller secondary gaps.

Disassembly (`rust.PredictPixel.annotate.txt` and
`rust.GetResidual.annotate.txt` in the capture directory) provides a concrete
lead in `libwebpop/src/enc/predictor_enc.rs`: `GetResidual` calls the separate
`PredictPixel` helper for every pixel. The helper retains several bounds
checks, loads the predictor function, builds a three-pixel context on the
stack, and makes an indirect call. C inlines the edge-selection logic into
`GetResidual`/`ComputeResidualsForTile`, selects the function outside the pixel
loop, and passes the existing upper-row context directly. C also dispatches
to SSE2 predictors, whereas Rust uses the scalar implementations; Rust's
`VP8LPredictor10_C` alone has 5% self time. Sampling cannot apportion the gap
precisely among call overhead, context copies, checks, and missing SIMD.

The next optimization experiment should therefore target residual prediction:
allow the compiler to see loop invariants and row bounds, avoid reconstructing
context per pixel, and evaluate scalar code generation before deciding how
much SIMD is needed. Preserve transparent-pixel cleanup and image-edge rules,
then repeat byte-parity checks and unprofiled timings. This profiling change
does not alter encoder algorithms.

[Compact capture summaries, stage definitions, hashes, and raw unprofiled
timings](benchmarks/2026-10-23-map-profile.json) are committed. The full perf
data and SVGs remain local under `target/profile-map-full/`; copy that directory
elsewhere before cleaning `target/`.

## Map without C runtime SIMD dispatch

An unprofiled follow-up compares the same reference executable with and without
`-noasm` against the archived default release v3 Rust executable. CPU 2,
quality 75, method 4, exact off, the same prepared RGBA PAM, one warmup and
eight balanced measured blocks, with no concurrent builds:

| Implementation | Median seconds | IQR |
| --- | ---: | ---: |
| C, normal dispatch | 1.5565 | 0.10% |
| C, `-noasm` | 1.7376 | 0.28% |
| Rust v3 | 2.0372 | 0.34% |

Disabling C's runtime SIMD dispatch **narrows the gap but does not close it**.
Rust takes 30.9% longer than normal C and 17.2% longer than C with `-noasm`,
using medians. The C slowdown closes about 38% of the original absolute
seconds gap, leaving about 62%. Paired C-noasm/Rust speedup is 0.848x, with
95% bootstrap interval 0.836–0.855. One Rust sample took 2.149s; all samples
are retained. Every output is 94,476 bytes, byte-identical across all three
implementations and repeats, and independently decoded and verified.

`-noasm` sets `VP8GetCPUInfo` to null, bypassing libwebp's hand-written SIMD
dispatch, including the predictor and vector-mismatch kernels. It does not
disable compiler auto-vectorization in the existing `-O3 -march=x86-64-v3`
binary or SIMD inside libc. This supports investigating Rust's scalar
prediction overhead as well as missing SIMD; it does not isolate predictor
SIMD from other DSP kernels or input conversion.

```sh
python3 scripts/benchmark.py --compare-c-noasm \
  --rust v3=target/bench-map-profile/bin/baseline \
  --case 75:4 --runs 8 --cpu 2 --output target/bench-map-noasm-next \
  --build-note 'Archived default release x86-64-v3 Rust; same C binary with and without -noasm' \
  samples/graphic-map.webp
```

[Raw samples, hashes and paired comparisons](benchmarks/2026-10-23-map-noasm.json)
accompany this result. Full local artifacts are under `target/bench-map-noasm/`.

## Inlining the per-pixel predictor helper

Adding `#[inline(always)]` to `PredictPixel` in
`libwebpop/src/enc/predictor_enc.rs` removes its standalone function body from
the optimized executable (`nm -C` finds it in the before binary and not in the
after binary). This experiment changes only that annotation. Both Rust
executables use the same rustc 1.101.0-nightly compiler, `profiling` profile,
x86-64-v3 target CPU, and `--no-rosegment` linker flag from the profiling
follow-up. No profiler runs during timing. The predictor and public encoder
test suites pass: 13 tests passed, with one manual timing test ignored.

The unprofiled comparison uses the same map PAM, CPU 2, quality 75, method 4,
exact off, one warmup, and eight balanced measured blocks:

```sh
python3 scripts/benchmark.py --compare-c-noasm \
  --rust before=target/profile-map-full/rust \
  --rust inline=target/profiling/cwebpop \
  --case 75:4 --runs 8 --cpu 2 --output target/bench-map-inline-next \
  --build-note 'Same compiler and profiling profile; only PredictPixel inline(always) added' \
  samples/graphic-map.webp
```

| Implementation | Median seconds | IQR |
| --- | ---: | ---: |
| C, normal dispatch | 1.5656 | 0.63% |
| C, `-noasm` | 1.7510 | 0.35% |
| Rust before | 2.0541 | 0.33% |
| Rust `PredictPixel` inline | 1.9932 | 0.26% |

Inlining provides a **1.031x paired speedup**, bootstrap 95% interval
1.029–1.032, or about 3% less time by medians. The annotation is retained.
Rust remains 27.3% slower than normal C and 13.8% slower than C with `-noasm`.
All outputs remain byte-identical at 94,476 bytes and independently decode to
the prepared pixels. This is a useful improvement, but the full self time
previously attributed to `PredictPixel` was not removable call overhead:
inlining still performs prediction and context access in its callers.

[Raw samples, hashes, build settings and paired comparisons](benchmarks/2026-10-23-map-inline.json)
accompany this result. Full local artifacts, including before/after binaries
and build/test logs, are under `target/bench-map-inline/`.

## Lossy methods

The [2026-10-25 lossy methods baseline](benchmarks/2026-10-25-lossy-methods.md)
records paired scalar Rust/C bytes, quality, repeated encode-only timings and
whole-process RSS for all methods, including larger opaque/alpha inputs.
Reproduce it with `scripts/benchmark-lossy.py`; its measurement boundaries are
separate from the lossless CLI harness above.
