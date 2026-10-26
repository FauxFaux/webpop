# WebP CLI benchmarks

This comparison covers two implementations, five configurations and all five
full-size images in `samples/`: **25 paired cases, 50 implementation results**.
The historical lossless experiments are in
[LOSSLESS-BENCHMARKS.md](LOSSLESS-BENCHMARKS.md).

All 25 paired cases produce **identical compressed bytes**. Rust takes about
9–41% longer for lossless. Near-lossless favors Rust on the invitation and
photo, while the map and screenshot favor C. At lossy q80, Rust takes about
1.5–2.3× C's time at methods 4–5 and 1.2–1.9× at method 6.

| Configuration | Quality / effort | Method | Near-lossless strength |
| --- | ---: | ---: | ---: |
| Lossless | 75 | 4 | 100 (disabled) |
| Near-lossless | 75 | 4 | 40 |
| Lossy | 80 | 4 | n/a |
| Lossy | 80 | 5 | n/a |
| Lossy | 80 | 6 | n/a |

Lossless and near-lossless use the CLIs' default compression effort and method.
Their quality parameter controls compression effort; lossy q80 controls visual
quality. All cases use default presets, exact off, default lossy alpha quality
100, ordinary RGB-to-YUV conversion and single-threaded encoding.
Near-lossless strength 40 can change intermediate alpha as well as RGB; it is
not a percentage quality score comparable to lossy q80.

## Builds and host

Measured on 2026-10-26 at repository revision
`a04a157`, with the benchmark harness changes archived alongside the results.
Both executables target **amd64-v3**, spelled `x86-64-v3` by GCC and rustc,
and both have **LTO disabled**.

- **C:** supplied `reference-cwebp/artifacts/cwebp`, pinned libwebp
  `1844042d1ca751e62e61df7617dba19c98588339`, GCC 14.2.0,
  `-O3 -g -DNDEBUG -march=x86-64-v3`, normal SIMD dispatch enabled.
  The pinned upstream checkout was clean. The scalar native-test oracle and
  `-noasm` are not used.
- **Rust:** freshly built `cwebpop`, rustc
  `1.101.0-nightly (76456a688 2026-10-25)`, LLVM 23.1.4,
  release optimization level 3, `target-cpu=x86-64-v3`, `lto=off`,
  16 codegen units, no debug information, debug assertions and overflow checks
  off, incremental off, panic unwind. The verbose build log confirms the
  effective CPU and LTO arguments.
- **Host:** Intel Core i9-13900KF, Linux x86-64, logical CPU 2
  (performance core, SMT sibling 3). Governor `powersave`; existing turbo and
  power policy retained. Per-session loads and host records are in the JSON.
  No builds or profiling overlap the timed run.

The shell's inherited `RUSTFLAGS=-Ctarget-cpu=native` was explicitly replaced,
and `CARGO_ENCODED_RUSTFLAGS` was unset before building. Merely choosing a
Cargo release profile would not override that CPU setting. Explicit environment
flags also replace the repository's configured `--no-rosegment` linker flag.

## Inputs and measurement

| Image | Dimensions | Megapixels |
| --- | ---: | ---: |
| graphic-alpha-transparency | 800 × 600 | 0.48 |
| graphic-invite | 10,416 × 7,812 | 81.37 |
| graphic-map | 4,977 × 2,515 | 12.52 |
| photo-japan-p50 | 5,492 × 3,672 | 20.17 |
| screenshot-wikipedia | 2,560 × 1,388 | 3.55 |

Inputs are decoded once outside timing to packed RGBA8 PAM, with no resizing.
Both CLIs receive the same pixels. The photo's name is not its pixel count;
this checked-in version is 20.17 MP and its decoded, already lossy pixels are
the benchmark source.

Each reported implementation has one warmup and eight measured runs per case:
**400 retained measured encodes plus 50 warmups**. Children run sequentially on CPU 2.
A seeded random order is reversed in the next block, balancing C/Rust positions
over each pair of blocks; seed 20261026. No outliers are removed.

The user reported system activity during the invitation's initial lossy run.
Those invitation q80/method 4, 5 and partial 6 measurements are excluded.
Measurement restarted at the invitation's q80/method 4 case with the same
archived executables, then continued through the remaining cases and images.
Earlier completed cases were retained at the user's request. An overlapping
restart, caused by the original process surviving its terminal interruption,
is also excluded in full. Session identities and provenance are recorded with
every accepted result and raw sample.

Timing includes process startup, argument parsing, PAM input/import, encoding,
allocation, complete buffered file output and exit. It is CLI latency, not
isolated encoding time. File caches are warm and output is not fsynced.
Decoding, hashing and output verification are outside timing.

Lossless outputs are independently decoded and checked for exact alpha and all
visible RGB. Near-lossless and lossy outputs are independently decoded for
geometry and RGB/alpha error metrics; lossy alpha must remain exact at default
alpha quality 100. Every subsequent output must match its implementation's
first SHA-256. Cross-implementation compressed-byte parity is recorded separately.

## Results

Median process seconds. **C/Rust** is the paired geometric mean of C time
divided by Rust time, followed by its 95% block-bootstrap interval
(4,000 draws); values above 1 favor Rust. Encoded sizes are in bytes.
IQRs, min/max, throughput, decoded metrics and every raw sample are in the
[machine-readable record](benchmarks/2026-10-26-matrix.json). These are
workstation measurements; the intervals describe this session and do not
account for all thermal drift or multiple comparisons.

### Lossless, q75 / method 4

| Image | C seconds | Rust seconds | C/Rust [95% interval] | C bytes | Rust bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.1527 | 0.1672 | 0.914 [0.909, 0.919] | 158,204 | 158,204 |
| graphic-invite | 15.1692 | 16.5216 | 0.918 [0.917, 0.919] | 2,783,988 | 2,783,988 |
| graphic-map | 1.5657 | 2.2064 | 0.710 [0.709, 0.711] | 94,476 | 94,476 |
| photo-japan-p50 | 5.1366 | 6.1418 | 0.836 [0.834, 0.838] | 24,549,614 | 24,549,614 |
| screenshot-wikipedia | 0.1679 | 0.1915 | 0.882 [0.876, 0.888] | 399,986 | 399,986 |

### Near-lossless=40, q75 / method 4

| Image | C seconds | Rust seconds | C/Rust [95% interval] | C bytes | Rust bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.2148 | 0.2135 | 1.009 [1.002, 1.015] | 107,242 | 107,242 |
| graphic-invite | 26.2876 | 23.0036 | 1.143 [1.142, 1.143] | 3,058,326 | 3,058,326 |
| graphic-map | 1.5666 | 2.2128 | 0.709 [0.707, 0.710] | 94,476 | 94,476 |
| photo-japan-p50 | 14.8577 | 14.2114 | 1.046 [1.044, 1.047] | 14,195,110 | 14,195,110 |
| screenshot-wikipedia | 0.1901 | 0.2496 | 0.766 [0.752, 0.780] | 301,758 | 301,758 |

### Lossy, q80 / method 4

| Image | C seconds | Rust seconds | C/Rust [95% interval] | C bytes | Rust bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.0595 | 0.1009 | 0.587 [0.585, 0.590] | 68,748 | 68,748 |
| graphic-invite | 2.8083 | 6.0910 | 0.460 [0.458, 0.462] | 1,039,528 | 1,039,528 |
| graphic-map | 0.4543 | 0.9782 | 0.463 [0.462, 0.465] | 369,088 | 369,088 |
| photo-japan-p50 | 1.1241 | 2.5417 | 0.442 [0.441, 0.443] | 2,346,540 | 2,346,540 |
| screenshot-wikipedia | 0.1410 | 0.3147 | 0.448 [0.447, 0.449] | 273,316 | 273,316 |

### Lossy, q80 / method 5

| Image | C seconds | Rust seconds | C/Rust [95% interval] | C bytes | Rust bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.0986 | 0.1498 | 0.657 [0.655, 0.659] | 65,028 | 65,028 |
| graphic-invite | 2.9627 | 6.6021 | 0.449 [0.447, 0.452] | 1,030,730 | 1,030,730 |
| graphic-map | 0.4840 | 1.0596 | 0.457 [0.456, 0.459] | 367,280 | 367,280 |
| photo-japan-p50 | 1.2793 | 2.8284 | 0.453 [0.452, 0.454] | 2,330,920 | 2,330,920 |
| screenshot-wikipedia | 0.1586 | 0.3516 | 0.451 [0.449, 0.452] | 271,572 | 271,572 |

### Lossy, q80 / method 6

| Image | C seconds | Rust seconds | C/Rust [95% interval] | C bytes | Rust bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 1.4384 | 1.7659 | 0.814 [0.812, 0.816] | 64,104 | 64,104 |
| graphic-invite | 8.8959 | 16.7523 | 0.530 [0.529, 0.531] | 1,000,218 | 1,000,218 |
| graphic-map | 1.4031 | 2.5242 | 0.556 [0.554, 0.557] | 366,088 | 366,088 |
| photo-japan-p50 | 2.5648 | 3.6785 | 0.697 [0.696, 0.698] | 2,271,528 | 2,271,528 |
| screenshot-wikipedia | 0.2487 | 0.4123 | 0.603 [0.600, 0.605] | 269,592 | 269,592 |

**Compressed-byte parity: 25/25 paired cases.** All 450 encodes were
deterministic within their implementation and configuration.

Near-lossless=40 changes size differently across inputs: the photo is about
42% smaller than lossless, the map is unchanged, and the invitation is about
10% larger. On the invitation, Rust uses about 12.5% less time than C for
near-lossless; on the photo it uses about 4.4% less. Percentages here use the
medians, while the table's ratios use paired geometric means.

All 50 timing distributions have IQRs below 1.9% of their medians. This
comparison measures the current CLIs with normal C SIMD enabled; it does not
attribute the timing differences to individual kernels or compiler decisions.

## Photo lossy q80 / method 4 profiling

The [perf follow-up](benchmarks/2026-10-26-lossy-photo-profile.md) reproduces the
photo's gap at C 1.115 s versus Rust release 2.543 s. About 80% of the excess
sampled cycles are in 4×4 and 16×16 luma mode searches. Quantization and inverse
transforms account for about 44% of the excess, and copying/initialization
another 12%. The report includes stage/kernel attribution, hardware counters,
disassembly leads and reproduction commands.

## Reproduction and records

Build before starting any timing. From the repository root:

```sh
mkdir -p target/benchmark-v3
env -u CARGO_ENCODED_RUSTFLAGS \
  RUSTFLAGS='-C target-cpu=x86-64-v3' \
  CARGO_PROFILE_RELEASE_LTO=off \
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  CARGO_PROFILE_RELEASE_OPT_LEVEL=3 \
  CARGO_PROFILE_RELEASE_DEBUG=0 \
  CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false \
  CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=false \
  CARGO_PROFILE_RELEASE_INCREMENTAL=false \
  CARGO_PROFILE_RELEASE_PANIC=unwind \
  cargo build --locked --offline --release -p cwebpop \
    --bin cwebpop --example benchmark_images \
    --target-dir target/benchmark-v3 -vv \
    > target/benchmark-v3/build.log 2>&1

python3 scripts/benchmark.py \
  --rust rust-v3=target/benchmark-v3/release/cwebpop \
  --helper target/benchmark-v3/release/examples/benchmark_images \
  --case lossless:75:4 --case near-lossless:75:4:40 \
  --case lossy:80:4 --case lossy:80:5 --case lossy:80:6 \
  --runs 8 --warmups 1 --cpu 2 --seed 20261026 \
  --build-note 'Explicit x86-64-v3, LTO off; build command above; preserve build.log' \
  --output target/benchmarks/2026-10-26-matrix-next
```

The offline build assumes dependencies are cached. Use a new output directory
for each run. CPU 2 is specific to this host; check affinity and core topology
before repeating elsewhere. The C artifact's build instructions and metadata
are documented in [reference-cwebp/README.md](../reference-cwebp/README.md).

Full local artifacts are under `target/benchmarks/2026-10-26-matrix/`,
`target/benchmarks/2026-10-26-matrix-invite-lossy-quiet/` and
`target/benchmarks/2026-10-26-matrix-remaining/`, including
prepared inputs, validated final outputs, archived executables, raw ordered
samples including warmups, summaries, source snapshots, working-tree patch,
Cargo.lock, toolchain/host records, C build metadata and Rust verbose build log.
Copy these elsewhere before cleaning `target/`.
