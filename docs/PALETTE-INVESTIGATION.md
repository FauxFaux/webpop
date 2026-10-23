# Palette-path performance investigation

Experiments on 2026-10-23 follow the `graphic-map.webp`, quality-75/method-4
investigation in [BENCHMARKS.md](BENCHMARKS.md). They target residual prediction
in [`predictor_enc.rs`](../libwebpop/src/enc/predictor_enc.rs), which accounted
for most of the map's measured C/Rust gap. The results apply to this input and
configuration; broader corpus measurements remain outstanding.

## What the earlier experiments established

Disabling C's runtime SIMD dispatch narrowed the gap without closing it.
Adding `#[inline(always)]` to Rust's `PredictPixel` improved map encoding by
about 3%, leaving Rust 13.8% slower than C with `-noasm` in that comparison.
See the final two sections of [BENCHMARKS.md](BENCHMARKS.md) for their raw data
and methodology. `-noasm` does not disable compiler auto-vectorization or SIMD
inside libc.

Inlining eligibility alone was therefore insufficient to explain the gap.
The optimized Rust cleanup loop still contained bounds checks, a predictor
table lookup and indirect call per pixel, and a temporary three-word context.
Upstream selects the predictor function before its cleanup loop and passes
the existing upper-row context directly. Source-file proximity and ThinLTO
do not guarantee the optimizer will inline through an indirect call.

## Borrowing context avoids store-forwarding stalls

The inlined Rust assembly builds `[top_left, top, top_right]` on the stack
using a four-byte store followed by an eight-byte store. Predictor 10 begins
with an eight-byte `vmovq` that spans both stores. This creates a
store-to-load forwarding problem: the processor cannot directly satisfy that
load from either individual store.

The first experiment replaced the temporary array with a borrowed window:

```rust
VP8LPredictors[mode](
    current[x - 1],
    upper[x - 1..x + 2].try_into().unwrap(),
)
```

This conversion borrows an existing three-element array; it allocates and
copies no pixels. The exact source change is preserved in
[borrowed.patch](benchmarks/2026-10-23-palette-investigation/borrowed.patch).

A standalone probe uses the actual Rust predictors, selects the predictor
at runtime, and verifies copied/borrowed equality for all 14 modes. Each
timed sample evaluates 4,096 pixels 2,000 times. Six alternating-order blocks
initially measured predictor 10 at roughly 49 ms with copied context versus
7.9 ms with borrowed context, about a 6× difference. Predictor 7, a control
without that wide context load, measured roughly 9 ms versus 6 ms.

A subsequent counter run reproduced the predictor-10 result:

| Predictor-only process | User cycles | Instructions | Blocked store-forwarding events |
| --- | ---: | ---: | ---: |
| Copied context | 1.648 billion | 2.220 billion | 49,371,360 |
| Borrowed context | 0.259 billion | 1.541 billion | 4,216 |

Separate full-map counter runs showed the same direction:

| Encoder | Blocked store-forwarding events |
| --- | ---: |
| Archived Rust with `PredictPixel` inlined | 20,808,262 |
| Rust with borrowed context | 546,024 |
| C with `-noasm` | 1,005,361 |

The event is `cpu_core/ld_blocks.store_forward/u`. These are process-wide
counts from individual runs, not a precise attribution of every event to
predictor 10. The assembly and isolated test provide stronger evidence for
the mechanism than the whole-process counts alone.

The end-to-end improvement was much smaller than the isolated predictor
speedup: about 2% in the paired map comparison below. Borrowing fixes a real
cost but accounts for only part of the remaining gap.

## Specializing the residual loop helps further

The second experiment adds one `match mode` per span, dispatching to
`get_residual_for_mode::<0>` through `::<13>`. Each const-generic version
receives a compile-time predictor mode. This exposes the selected predictor
to the optimizer, permitting direct calls/inlining and removal of unused
context accesses.

This experiment includes borrowed context. The incremental change is in
[specialized.patch](benchmarks/2026-10-23-palette-investigation/specialized.patch).
Boundary handling and transparent-pixel cleanup are preserved. The `exact`
path still calls `PredictBatch`; its loop was not separately specialized.

| Paired experiment | Earlier variant median | Later variant median | Paired speedup |
| --- | ---: | ---: | ---: |
| Inlined baseline → borrowed context | 1.9622 s | 1.9230 s | 1.0215× |
| Borrowed context → borrowed plus specialization | 1.9017 s | 1.7898 s | 1.0683× |

These are two separate timing sessions. The different borrowed-context
medians must not be combined as though all variants were timed together.
Speedup is the geometric mean of within-block earlier/later time ratios.
No confidence intervals were calculated for these exploratory runs.

Specialization changes several compiler decisions together: call dispatch,
inlining, context loads, bounds checks, and register allocation can all
change. The result does not isolate their individual contributions.
`GetResidual` grows to about 8 KiB, so performance on other workloads could
differ. A separate specialized full-map counter run recorded 36.26 billion
instructions, versus 40.25 billion with borrowing alone.

## Method and validation

Both experiments used temporary source copies. Their baseline already
contained `#[inline(always)]` on `PredictPixel`; that is an experimental
baseline, not a statement about the current repository source. Compiler
arguments were replayed from
`target/bench-map-inline/provenance/inline-build.log`: rustc
`1.101.0-nightly (7189d39d9 2026-10-22)`, LLVM 23.1.4, release optimization,
debug information, `x86-64-v3`, and `--no-rosegment`. Existing compiled
external dependencies and unchanged CLI sources were reused.

Unprofiled timing used CPU 2 on the same i9-13900KF, the existing prepared map
RGBA PAM, quality 75, method 4, and exact off. Each executable received one
warmup followed by eight paired blocks with alternating execution order.
No builds or profiler runs overlapped measured timing. The workstation was
not otherwise isolated, and some samples show timing variation.

Every measured output was checked byte-for-byte against the existing known
output: 94,476 bytes, SHA-256
`ae08b45d6948f98dc7e665fd01363bcc59948f538f430b40959546240f44ce16`.
The specialized library passed all nine predictor tests and all four
non-ignored public encoder tests; one manual timing test remained ignored.
Those suites include independent decoding checks. They were built with
`rustc --test` using the experimental library and cached dependency flags.
The native oracle suite was not run for these experiments.

The first specialization timing attempt completed all eight blocks and
output checks but failed while formatting its summary. The retained timing
data comes from a complete subsequent repeat after correcting that reporting
error.

## Next experiments

1. Remove remaining loop bounds checks structurally. Handle the first row
   and column separately, validate spans once, then iterate over row windows.
   Check generated code before attributing any timing change to fewer checks.
2. Match upstream's selective histogram clearing. Rust clears all completed
   histograms, including inactive planes; C clears only active planes when
   some are constant. Palette-index images can have several constant planes,
   making the extra clearing and cache traffic worth measuring. See the reset
   loop in [upstream predictor_enc.c](../upstream/src/enc/predictor_enc.c).
3. Test a fast path when transformed pixels contain no zero alpha. Transparent
   cleanup then makes no changes, permitting a simpler residual loop without
   its mutation dependency. Check the transformed pixels rather than assuming
   opacity from the original image or the use of a palette.
4. Profile the improved variants against C with `-noasm` again. The old
   profile cannot establish where the remaining gap moves after these changes.
   Repeat map, photo, screenshot, alpha, and exact-path timings before
   retaining a production change.

## Preserved artifacts and reproduction

[results.json](benchmarks/2026-10-23-palette-investigation/results.json)
contains ordered samples, warmups, medians, paired speedups, counter reports,
test reports, source hashes, and executable hashes. The two patches preserve
the experimental changes without applying them to the encoder. Both expect
the inlining annotation in the baseline; apply the borrowed patch before the
specialization patch.

The standalone probe is preserved as
[predictor-probe.rs](benchmarks/2026-10-23-palette-investigation/predictor-probe.rs).
From the repository root:

```sh
rustc --edition=2024 -O -C target-cpu=x86-64-v3 \
  docs/benchmarks/2026-10-23-palette-investigation/predictor-probe.rs \
  -o /tmp/webpop-predictor-probe-bin
taskset -c 2 /tmp/webpop-predictor-probe-bin 10
perf stat -e cpu_core/cycles/u,cpu_core/instructions/u,cpu_core/ld_blocks.store_forward/u \
  -- taskset -c 2 /tmp/webpop-predictor-probe-bin 10 copied
```

Substitute `borrowed` for `copied` to compare counters, or use mode `7` for
the control. Adapt CPU and PMU selection on other machines. Run builds before
timing. Full-map repeats can use the repository benchmark runner with
separately archived variant executables, as described in
[BENCHMARKS.md](BENCHMARKS.md).

The original temporary build scripts, source copies, binaries, and full logs
remain under `/tmp/webpop-predictor-probe/`; that directory is disposable and
is not required to read the preserved results.
