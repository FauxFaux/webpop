# Benchmarking Rust against C

Examined on 2026-10-22 against upstream revision
`1844042d1ca751e62e61df7617dba19c98588339`. Tool capabilities below are a dated
assessment; check the installed version before relying on particular options.
This is a measurement plan. The implemented CLI harness, reproduction commands
and initial results are in [BENCHMARKS.md](BENCHMARKS.md).

## Recommendation

Use two small, separately built executables and time them externally. At the
anticipated 2–8 seconds for a 50-megapixel image, one encoding per process is a
reasonable unit of measurement. Start with a command runner such as `hyperfine`;
add a small orchestration script when we need controlled execution order and
corpus summaries. No benchmarking crate is needed in `libwebpop`.

The useful property is that each executable performs a real encoding on
runtime input and exposes the result. This makes accidental constant folding,
unused-result elimination, and different benchmark compilation paths easier
to avoid and inspect. It does **not** make compiler flags, encoder settings,
I/O, or machine noise take care of themselves. Those remain explicit parts of
the experiment.

Keep the executables thin enough to represent library use. The Rust driver
calls the ordinary Rust API; the C driver calls the pinned upstream API.
Build once, save the artifacts and build records, then execute those artifacts
directly. Timing `cargo run`, CMake, or the test runner would measure additional
work. The public Rust lossless encoder and CLI now support complete-encoder
comparisons; matched internal stages remain useful for diagnosing differences.

## What a library would buy us

| Tool | Useful role here | Limits for the primary comparison |
| --- | --- | --- |
| [`hyperfine`](https://github.com/sharkdp/hyperfine) | External process timing, warmups, repeated samples, and JSON export across both implementations | Measures the entire command. Execution order needs attention; see the protocol below. It is a tool dependency, not a library dependency. |
| [`Criterion.rs`](https://bheisler.github.io/criterion.rs/book/user_guide/timing_loops.html) | Later measurements of individual Rust kernels, with control over per-iteration setup and destruction | Its timing loops and batching require a deliberate allocation/reset policy. Calling native C needs separate FFI infrastructure; spawning binaries still measures processes. |
| [`Gungraun` / Iai-Callgrind](https://github.com/gungraun/gungraun) | Library or binary regression measurements using Valgrind instruction/cache models; also offers Linux `perf` integration | Valgrind's instruction counts and estimated cycles answer different questions from elapsed encoding time. They cannot establish native SIMD throughput. |
| A small process runner | Balanced Rust/C ordering, input manifests, output validation, and storing samples in our own format | We must implement process timing and reporting correctly. Keep it small; do not recreate an adaptive microbenchmark framework. |

Criterion is a reasonable choice if we later need to distinguish two very
short implementations of a predictor or histogram kernel. It is not inherently
unfair to C, and a binary is not inherently immune to misleading optimization.
For second-scale operations, the main value of a library would be reporting
and statistics rather than timer resolution. Neither justifies making it the
initial cross-language measurement boundary.

## Define the operation before timing it

Use prepared raw RGBA input shared by both drivers. A manifest records its
hash, dimensions, byte stride, channel order, and encoder configuration. Specify
padding and the expected file length, and reject invalid geometry or truncated
input. Convert PNG/JPEG source images to this representation once, outside the
benchmark, so different decoders and color-management paths cannot dominate
the result. Raw 50-megapixel RGBA alone occupies about 200 MB; it is not a small
setup buffer.

There are two useful measurements, with different boundaries:

| Measurement | Timed work | Interpretation |
| --- | --- | --- |
| Process elapsed time, the initial default | Startup, argument parsing, reading RGBA, import, allocations, encoding, output consumption, cleanup and exit | Latency of the specified driver on prepared input |
| Library-call elapsed time, an optional second metric | The matched public encoding operation, including its import and allocation work | Encoding latency with input already resident in memory |

For the process measurement, both drivers should write a complete output file
to a designated local scratch directory. Read/compare/decode that file outside
the timed command. Use the same filesystem and write policy for both; ordinary
buffered writes measure page-cache writes, not durable storage. Do not add
`fsync` unless durable output is the workload being studied. Keep logging small.

If I/O materially obscures the comparison, add a separate library-call metric
with a monotonic timer inside each executable. Read and validate input before
starting it; include the same import, allocation, and output-finalization work
on both sides. State whether freeing the returned output is included. Save or
fully checksum the output after the timer stops. External process time remains
useful as a check on how much work this boundary excludes.

Do not estimate encoding time by subtracting a read-only or empty-program
baseline: cache, allocation, and memory-bandwidth costs need not be additive.
Measure setup separately if needed. A fresh process includes page faults and
fresh allocator state; an internal timer and preloaded input change that
workload. A resident service reusing buffers would be another explicit mode,
not something to mix into the initial results.

## Match the encoder's work

The first complete-encoder case should use the
[lossless port target](LOSSLESS_PORT_PLAN.md): `WebPEncodeLosslessRGBA` on both
sides. Its pinned C defaults are `quality=70`, `method=4`, `near_lossless=100`,
`exact=0`, `thread_level=0`, and the default image hint. These come from
[`picture_enc.c`](../upstream/src/enc/picture_enc.c) and
[`config_enc.c`](../upstream/src/enc/config_enc.c). Lossless `quality` controls
compression effort; changing it changes the amount of work.

Do not assume `cwebp -lossless` or a generic `WebPConfigInit` call selects this
same configuration. Use the simple API, or set and record the complete advanced
configuration. Later effort levels, near-lossless, and lossy modes get separate
cases once both implementations support them. Keep single-threaded execution
as the initial default; any parallel comparison must match thread policy.

Validate the exact binaries being timed before collecting results:

- Require successful encoding and a nonempty, complete output. Failures are
  failed cases, not fast samples.
- Compare compressed bytes when scalar parity is the target. Record output
  sizes and hashes along with the input and configuration.
- Independently decode and check dimensions, alpha, and RGB wherever alpha is
  nonzero. With `exact=0`, RGB under zero alpha may change. An `exact=1` case
  needs an API supporting it on both sides and checks every channel.
- Report any accepted compressed-byte divergence explicitly. Decoding the same
  pixels does not establish equal compression work: output size and selected
  algorithms can differ. A speed/size tradeoff is a different result from a
  faster equivalent port.

Preserve correctness checks while optimizing; a faster implementation that
skips a transform search is not an equivalent implementation of that search.

## Separate correctness and performance builds of C

The existing [native oracle](../libwebpop/tests/reference/README.md) is designed
for exact intermediate comparisons. Its
[CMake configuration](../libwebpop/tests/reference/CMakeLists.txt) forces
`WEBP_ENABLE_SIMD=OFF` and `WEBP_USE_THREAD=OFF`. Its hexadecimal text protocol
also serializes complete logical buffers. Reusing it unchanged would measure
protocol work and compare only against the deliberately restricted C backend.

Reuse its pin checks and provenance approach, but give performance drivers
separate build directories and a raw-input path. Keep wrappers outside
`upstream/`, and preserve the tooling-only C boundary and the Rust library's
`forbid(unsafe_code)` policy. No production C linkage is required.

Maintain two clearly labelled comparisons:

1. **Portable implementations:** portable Rust against C with explicit SIMD
   backends disabled, using a declared common CPU baseline. Leave ordinary
   compiler auto-vectorization enabled on both sides. “Scalar C” in the oracle
   means no hand-written SIMD backend; it does not guarantee scalar machine
   instructions. The same distinction applies to current Rust.
2. **Deployment implementations:** the Rust acceleration available to users
   against upstream with its normal supported SIMD dispatch enabled, under a
   declared target and threading policy. This is the practical performance
   comparison, including any backend coverage Rust has not implemented yet.

Host-specific `native` builds can be an additional labelled experiment. Do not
silently compare a native Rust build with portable C, or present a victory over
SIMD-disabled C as a victory over normal libwebp. A deployable C binary may
contain optional SIMD functions selected at runtime even when its general
compilation target is conservative. Record both target flags and dispatch
configuration. See [INTRINSICS.md](INTRINSICS.md) for the acceleration strategy.

## Build provenance is still necessary

Use an optimized Rust release profile and C Release build as the starting
point, with explicit CPU targets and no special floating-point relaxation.
Record actual commands rather than assuming what “Release” means. The current
workspace's development profile optimizes our crate at level 1 and dependencies
at level 3; ordinary `cargo test` is not a release performance comparison.

Cargo's `bench` profile inherits `release`; a benchmark crate does not inherently
force unoptimized code. Conversely, configuration files and environment
variables can override manifest profiles. Record optimization level, LTO,
codegen units, debug assertions, overflow checks, and panic strategy. See the
[Cargo profile documentation](https://doc.rust-lang.org/cargo/reference/profiles.html).

Identical-looking flags are not a proof of equivalent optimization across
compilers. Start with normal optimized builds, then label any tuning experiments
separately. If testing LTO or PGO, record the policy on both sides, including
training inputs. Do not disable Rust's safety checks just to resemble C; their
cost is part of the implementation. Avoid C `-Ofast`/`-ffast-math` as a casual
speed setting: relaxed floating-point semantics can change entropy estimates
and decisions. See [GCC's optimization options](https://gcc.gnu.org/onlinedocs/gcc/Optimize-Options.html).

Save the following with a result set:

- Rust revision and any working-tree patch, upstream pin and clean status,
  `Cargo.lock`, enabled features, and driver source revision.
- `rustc -vV`, Cargo and C compiler versions, target triples, linker versions,
  build commands, effective Rust flags, and CMake cache/compilation commands.
  Include relevant environment overrides such as `RUSTFLAGS` and
  `CARGO_ENCODED_RUSTFLAGS`; this environment has used `target-cpu=native`.
- Binary hashes, static/shared linkage, relevant runtime library and allocator
  choices, and the benchmark runner's version and arguments.
- CPU model and features, OS/kernel, core affinity, power policy, memory limits,
  and the corpus/configuration manifest.

Use separate CMake build directories for compiler/flag changes to avoid cached
configuration. Preserve the completed binaries so later rebuilds cannot change
what a saved command measures. Keep instrumentation and sanitizers out of the
primary timing build; use them separately for diagnosis.

## Preventing benchmark-only optimization

Load pixels at runtime and make the full encoded output observable. Writing
the complete encoded bytes is the simplest initial contract. Printing just
the buffer length is weaker: it does not require every output byte to have
been computed. If using a checksum instead, consume every byte, specify the
algorithm, and account for that extra pass in process time.

Do not embed a known image in the executable, discard the output, or repeatedly
call a pure leaf on constant inputs. Avoid benchmark-only implementations and
unnecessary `noinline` or volatile annotations that inhibit production
optimization. For later in-process kernel benchmarks, runtime inputs and
[`std::hint::black_box`](https://doc.rust-lang.org/std/hint/fn.black_box.html)
can help, but `black_box` is only a best-effort optimization barrier. Inspect
generated code when a result is surprising.

For mutable stages, restore equivalent input and scratch state before every
sample. Repeatedly transforming an already transformed image changes the work.
Specify whether cloning/resetting buffers belongs inside the measurement, and
make that decision consistently for C and Rust.

## Repetition, caches, and machine state

Start with one or two untimed warmups per implementation/input, then roughly
ten measured runs each. This is a starting budget, not evidence that ten runs
always resolve a small difference. Save every raw duration and execution order.
Run one process at a time, with the same affinity on a quiet machine under
stable power and thermal conditions. On hybrid CPUs, choose the same core
class; avoid contention from its SMT sibling. Watch for swapping and memory
pressure at these image sizes, and record peak resident memory separately.

Warm filesystem caches are a useful default for isolating codec work. A warmup
does not imply that a 200 MB image fits in the CPU's last-level cache, and an
earlier process's heap buffers do not survive into a fresh process. A cold-cache
experiment is a separate workload with an explicit preparation policy.

A custom runner should use a monotonic elapsed timer around spawning and
waiting for one child, check its exit status, and record the case and binary
identity with every sample. Avoid concurrent benchmark children or large
captured stdout buffers. Its timing boundary should match the process metric
above; do not combine its durations with an executable's internal timer.

Balance order over time: use pairs such as C/Rust followed by Rust/C, or
randomize pair order with a recorded seed. Running every C sample before every
Rust sample can turn thermal or background-load drift into an apparent speed
difference. Hyperfine's documented execution order groups commands in input
order, so a plain two-command invocation is useful for exploration but is not
an interleaved protocol. Use an outer runner for balanced individual samples,
or reverse command order in repeated sessions as a coarse check. Record the
policy; do not assume a runner randomizes samples.
See the [hyperfine usage documentation](https://github.com/sharkdp/hyperfine#usage).

Choose direct command execution explicitly with `--shell=none` when using
hyperfine. Warmups, a fixed run count, and JSON export are sufficient initially;
there is no need for shell pipelines or per-sample compilation. Keep timed
processes free of output verification and reference decoding.

Summarize each case with median elapsed time and spread, plus the raw samples.
Define speedup as `C time / Rust time`, so values above one favor Rust. Also
report megapixels/second using the actual pixel count and report encoded bytes.
For a formal comparison, estimate uncertainty from the balanced paired samples;
keep collecting data when the proposed gain is comparable to run-to-run noise.
Do not cherry-pick the fastest run or remove unexplained outliers. Record
interference and rerun an affected session when necessary.

## Corpus and interpretation

Include photographs, screenshots/text, palette art, gradients, repeated
patterns, noise, and images with transparent pixels and hidden RGB. Exercise
small and medium images alongside the 50-megapixel case, odd dimensions,
padded strides, and cases reaching different palette, predictor, cache, and
histogram paths. Keep real inputs or reproducible generators with hashes,
seeds, provenance, and redistribution permissions.

Report per-image results first. If summarizing a corpus, name the aggregation:
total C time divided by total Rust time answers a fixed-batch throughput
question, while a geometric mean of per-image speedups weights images equally.
Neither replaces the individual results or output-size comparison. One large
photo cannot establish performance for an image encoder in general.

Profile the same optimized executables separately to explain bottlenecks;
instruction counts, cache events, and allocation measurements supplement elapsed
time. Shared CI can run correctness and smoke measurements, but small wall-time
regression gates need a stable host and an observed noise baseline. Native and
Wasm results need separate target/runtime records; the bundled older C Wasm
regression is not a matched performance reference for the current pin.
