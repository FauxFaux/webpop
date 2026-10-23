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
`versus_first_rust` results. `--case QUALITY:METHOD` is also repeatable. Without
it, the runner uses `75:4`, `75:0`, and `100:6`; the last can be expensive on
large images. Positional filenames select a subset of inputs; otherwise all
`samples/*.webp` are used. `--exact` preserves and checks RGB beneath zero alpha.
Quality is integer compression effort, not visual quality; method is 0–6.
There is no multithreading option in these comparisons. Near-lossless below
100, presets, and image hints are not implemented by the Rust CLI.

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
