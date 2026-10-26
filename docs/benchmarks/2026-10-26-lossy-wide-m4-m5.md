# Widened Rust: lossy q80, methods 4 and 5

Fresh paired measurements compare SIMD C, the archived scalar Rust CLI and
Rust revision `a239a59` after the portable kernel and fixed-row copy changes.
All five sample images are full size. This measures the combined change;
it does not separate widening from copy cleanup.

On the photo, new Rust takes **1.623 s at m4 and 1.804 s at m5**, about
**1.57× faster** than scalar Rust in both cases. Its time relative to C falls
from 2.27× to 1.44× at m4 and from 2.20× to 1.41× at m5. Across the five images,
paired improvements range from 1.09× to 1.57×; new Rust still takes 1.38–1.58×
C's time. All timing IQRs are below 1% of their medians.

[Raw measurements, hashes and build/host records](2026-10-26-lossy-wide-m4-m5.json)
accompany this report. The [original matrix](../BENCHMAKRS.md) records the
preceding scalar implementation, and the [earlier kernel-port report](2026-10-26-lossy-wide.md)
covers the implementation and correctness tests.

## Results

Median whole-process seconds on prepared packed RGBA8 PAM. C is the normal
SIMD-enabled reference. Old/new Rust speedup is the geometric mean of paired
within-block time ratios, with its 95% bootstrap interval (4,000 draws).
Values above 1 favor the new Rust build. New Rust/C is the ratio of medians;
values above 1 mean Rust takes longer.

### q80 / method 4

| Image | C seconds | Old Rust seconds | New Rust seconds | Old/new [95% interval] | New Rust/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.0588 | 0.1003 | 0.0884 | 1.125 [1.100, 1.140] | 1.504× |
| graphic-invite | 2.7905 | 6.1025 | 4.3749 | 1.399 [1.395, 1.405] | 1.568× |
| graphic-map | 0.4550 | 0.9758 | 0.7101 | 1.375 [1.370, 1.380] | 1.561× |
| photo-japan-p50 | 1.1247 | 2.5491 | 1.6227 | 1.571 [1.566, 1.576] | 1.443× |
| screenshot-wikipedia | 0.1404 | 0.3138 | 0.2118 | 1.481 [1.476, 1.486] | 1.509× |

### q80 / method 5

| Image | C seconds | Old Rust seconds | New Rust seconds | Old/new [95% interval] | New Rust/C |
| --- | ---: | ---: | ---: | ---: | ---: |
| graphic-alpha-transparency | 0.0989 | 0.1493 | 0.1366 | 1.093 [1.089, 1.097] | 1.381× |
| graphic-invite | 2.9692 | 6.5768 | 4.6790 | 1.405 [1.393, 1.415] | 1.576× |
| graphic-map | 0.4851 | 1.0527 | 0.7603 | 1.386 [1.382, 1.391] | 1.567× |
| photo-japan-p50 | 1.2804 | 2.8225 | 1.8037 | 1.565 [1.561, 1.569] | 1.409× |
| screenshot-wikipedia | 0.1579 | 0.3505 | 0.2356 | 1.494 [1.487, 1.505] | 1.492× |

Every one of the **10 paired cases is byte-identical across all three
implementations**, with unchanged sizes and hashes versus the original matrix.
All 270 outputs (240 measured encodes and 30 warmups) are deterministic.
Independent decoding gives identical metrics, with exact alpha at the default
alpha quality 100. Sizes, RGB errors/PSNR, throughput and timing spreads are in
JSON. No outliers are dropped.

## Matched builds and measurement

- Rust toolchain: `1.101.0-nightly (76456a688 2026-10-25)`, LLVM 23.1.4.
  Both Rust executables use release optimization 3, x86-64-v3, LTO off,
  16 codegen units, debug/assertions/overflow checks and incremental off,
  panic unwind. The new build replaces inherited
  `RUSTFLAGS=-Ctarget-cpu=native` with `-C target-cpu=x86-64-v3` and unsets
  `CARGO_ENCODED_RUSTFLAGS`. Actual library and CLI compiler commands are archived.
- C is the same executable as the original matrix: pinned libwebp
  `1844042d1ca751e62e61df7617dba19c98588339`, GCC 14.2.0,
  `-O3 -g -DNDEBUG -march=x86-64-v3`, normal SIMD dispatch, no LTO.
- The scalar Rust executable is also the original archived matrix binary.
  All three executables are freshly timed together; this comparison does not
  divide new timings by historical timing samples.
- Intel Core i9-13900KF, CPU 2, SMT sibling 3, governor `powersave`, existing
  turbo/power policy retained. The core and sibling were essentially idle
  before building. One warmup and eight measured blocks per implementation
  and case; seed 20261026. Each random implementation order is reversed in
  the next block, balancing positions over eight blocks. Children run serially.
  No builds, tests or profiling overlap measurement.
- Inputs are decoded outside timing to the same RGBA8 PAM bytes as before,
  without resizing. Timed work includes startup, PAM reading/import,
  allocation, encoding, buffered output and exit. Hashing and independent
  decode checks are outside timing. This is warm-cache CLI latency on this
  workstation, not an isolated kernel benchmark or a general platform result.

## Reproduction

Use the explicit non-LTO v3 build command from the matrix, changing
`--target-dir` to `target/benchmark-wide-v3`, then run:

```sh
python3 scripts/benchmark.py \
  --rust scalar=target/benchmarks/2026-10-26-matrix/bin/rust-v3 \
  --rust wide=target/benchmark-wide-v3/release/cwebpop \
  --helper target/benchmark-wide-v3/release/examples/benchmark_images \
  --case lossy:80:4 --case lossy:80:5 \
  --runs 8 --warmups 1 --cpu 2 --seed 20261026 \
  --build-note 'Same v3, LTO-off flags as matrix; preserve verbose build log' \
  --output target/benchmarks/2026-10-26-wide-m4-m5-next
```

Use a new result directory and an appropriate CPU on other hosts. Full local
artifacts are under `target/benchmarks/2026-10-26-wide-m4-m5/`, including copied
executables, prepared inputs, outputs, ordered samples and provenance.
The verbose build log is preserved there in `provenance/rust-build.log`.
Preserve these artifacts before cleaning `target/`.
