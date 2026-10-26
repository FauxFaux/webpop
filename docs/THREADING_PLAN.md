# Encoder threading plan

## Target and compatibility

Add parallel execution to the lossless VP8L and lossy VP8 encoders while
preserving **every output byte** of the pinned upstream implementation for
the same pixels and coding settings. Scheduling, pool size and task boundaries
must not become compression settings. Retain recognizable upstream functions,
loops, trial order and state transitions wherever possible.

Source audit: 2026-10-26, repository revision `a37f915`, clean upstream pin
`1844042d1ca751e62e61df7617dba19c98588339`. This expands the performance track
of the [encoder completion plan](ENCODER_COMPLETION_PLAN.md), building on the
[lossless](LOSSLESS_PORT_PLAN.md), [near-lossless](NEAR_LOSSLESS_PORT_PLAN.md)
and [lossy methods](LOSSY_METHODS_PLAN.md) plans. Keep the upstream pin.

This is a proposed implementation plan, not a record of implemented threading
or newly measured speedups. The research implementation at commit
`4e30c58d703121f609968868aa94afe568100781` on `tokenslop/perf-research` was read
from Git without changing branches. Its recorded tests and measurements are
prior evidence; they have not been rerun against today's encoder.

Animation is outside the work list. Its existing calls into the shared
encoders may benefit from the same execution context. Preserve frame order,
candidate selection and cache/carryover state; do not add frame scheduling or
animation-specific parallelism. Existing documented animation variations
remain governed by [VARIATIONS.md](../libwebpop/VARIATIONS.md).

## Recommendation

Use Rayon for scheduling, with `join` for paired jobs and indexed parallel
iterators over independent work. Keep the codec's selection and commit loops
explicit and ordered. This replaces the C worker infrastructure without
requiring a different compression algorithm.

Implement upstream's three parallel boundaries first. Then adapt the research
commit's independent lossless searches, with bounded scratch memory. Leave
its speculative hash-chain algorithm until last: that is a substantial
algorithmic departure even though its purpose is exact parallel execution.

Prefer one reusable pool for an encoding workload. Nested alpha/VP8L work
should use that pool, rather than create more threads or pools. A direct
serial path remains available and is the initial library default.

## What upstream actually parallelizes

The pinned encoder has exactly three uses of the `WebPWorker` job interface.
`thread_level` is a validated **0/1 switch**, not a thread count. Each boundary
uses the calling thread and at most one side worker; the analysis and
alpha/color stages happen at different times.

| Area and functions | Parallel work and join point | Compatibility-sensitive details |
| --- | --- | --- |
| VP8L configuration trials: [`vp8l_enc.c`](../upstream/src/enc/vp8l_enc.c), `VP8LEncodeStream`, `EncodeStreamHook` | Run `EncoderAnalyze` first. Give the final `floor(n / 2)` crunch configurations to a side encoder and the initial `ceil(n / 2)` to the main encoder. Each hook tries its configurations serially. Join before returning the selected stream. With one configuration there is no side job. | Both writers start from the same bit-writer prefix/state. Encoders have independent mutable buffers. Side picture/error/stats state is separate and its progress hook is disabled. A strict smaller-size comparison within each group and then between groups preserves the earliest configuration on ties. |
| VP8 susceptibility analysis: [`analysis_enc.c`](../upstream/src/enc/analysis_enc.c), `VP8EncAnalyze`, `InitSegmentJob`, `DoSegmentsJob`, `MergeJobs` | Divide macroblock rows at `split_row = (9 * mb_h + 15) >> 4`. Enable the worker when `thread_level > 0` and `split_row >= 2`. Each job imports source borders, analyzes its row range and collects susceptibility counts/sums. Join and merge before `AssignSegments`. | Writes to macroblock information and prediction modes belong to disjoint row ranges. Sum histograms and alpha/UV totals before dividing by the total macroblock count. Only the first job reports progress. Preserve the actual small-image condition, including cases with an empty side range. |
| ALPH alongside VP8 color: [`alpha_enc.c`](../upstream/src/enc/alpha_enc.c), `CompressAlphaJob`, `VP8EncStartAlpha`, `VP8EncFinishAlpha`, `VP8EncDeleteAlpha` | After analysis, start the alpha job and run `VP8EncLoop` or `VP8EncTokenLoop` on the main thread. Join alpha before output assembly; deletion also drains any unfinished work on an error path. Opaque images have no alpha job. | Color and alpha consume the prepared YUVA picture. Alpha owns its quantization/filter/VP8L buffers and result; final output and statistics are committed after joining. Conversion and transparent-area cleanup precede both jobs. |

The lossy analysis shortcut avoids the segment jobs when complexity/mode analysis is
unneeded: upstream tests JPEG-size emulation, multiple segments, or methods
0/1, otherwise it resets macroblock information. Current Rust supports the
multiple-segments/method conditions; adding threading does not add JPEG-size
emulation or other advanced coding settings.

Upstream's ALPH `EncodeLossless` creates a fresh default `WebPConfig` with
`thread_level = 0`. Its nested VP8L encode is consequently serial even when
the outer VP8 encoder uses threading. Allowing our nested VP8L jobs onto the
same pool would be an additional optimization, subject to the same byte gate.

There is **no upstream parallel encoder implementation** of the main VP8
macroblock coding loops, VP8L hash-chain search, LZ77-kind/cache searches,
histogram remapping, predictor search, cross-color search, near-lossless
preprocessing, ordinary color conversion or SharpYUV conversion. Some of these
contain independent work we can expose. Initialization locks in DSP/SharpYUV
are separate from parallel image processing. The threaded decoder in
`src/dec/frame_dec.c` is outside this encoder plan.

### C workers versus Rust tasks

[`thread_utils.c`](../upstream/src/utils/thread_utils.c) and
[`thread_utils.h`](../upstream/src/utils/thread_utils.h) implement a reusable
OS worker with mutex/condition-variable synchronization and a replaceable
global function table. `Reset` prepares a thread, `Launch` starts its hook,
`Execute` invokes a hook synchronously, `Sync` waits and checks errors, and
`End` joins/releases the worker. With threading compiled out, launch executes
the hook synchronously.

Retain those job boundaries and functions' codec roles, but do not reproduce
the opaque pointers, mutable global worker interface or manual lifecycle.
Borrowed inputs, owned job results and structured completion express the
encoder's requirements in safe Rust. C worker-interface replacement is not a
public compatibility requirement of this crate.

| Rust mechanism | Fit for this encoder | Tradeoff |
| --- | --- | --- |
| `std::thread::spawn` | Can own an entire independent job | Requires `'static` captures; borrowing image/writer state needs ownership transfer or extra shared ownership. Repeated spawning and nested budgets need explicit management. |
| `std::thread::scope` | Direct equivalent of main/side jobs borrowing input | Naturally joins on every scope exit. A good dependency-free implementation of the three upstream boundaries, but repeated stages create OS threads unless we build our own reusable scheduler. |
| `rayon::join` | Two VP8L groups, two analysis jobs, alpha versus color | Borrowed CPU work with typed return values; supports nested work in a fixed pool. Recommended for the initial ports. |
| Indexed parallel iterators and slice chunks | Many independent lossless trials, cache sizes, remap blocks, analysis row ranges | Less scheduling code than the research commit's custom `par_map`; keep stable job identities and ordered commits. Recommended for extensions. |
| `rayon::scope` | Work whose start/join lifetime cannot be expressed conveniently as `join` | Useful if a later alpha pipeline needs that shape. The current alpha/color pair fits `join`; there is no need to recreate a persistent worker object. |

Standard scoped threads can borrow non-`'static` data and automatically join
before returning. [Rust scoped-thread documentation](https://doc.rust-lang.org/stable/std/thread/fn.scope.html)
describes this lifetime guarantee. Rayon also supports borrowed fork/join
work; its [scope documentation](https://docs.rs/rayon/latest/rayon/fn.scope.html)
recommends `join` or parallel iterators where their shape suffices.

Rayon's [join documentation](https://docs.rs/rayon/latest/rayon/fn.join.html)
describes work stealing over a fixed pool. Calling `join` outside the pool
blocks the caller while its jobs run in the pool: it does not preserve C's
guarantee that the main hook runs on the API caller's thread. That distinction
matters for callbacks, thread-local state and arbitrary output closures, even
when bytes are identical.

## Lessons from the working research commit

The commit includes threading **and** single-thread kernel changes. Evaluate
them separately; do not attribute the combined benchmark gains to threading
alone or cherry-pick the whole change as a threading port.

Its `docs/FASTER-THAN-C.md` records 100 passing release tests, an explicit
parallel-versus-serial hash-chain test with forced small segments, and 117
byte-identical encodes at 1, 3 and 12 threads covering lossless, exact and
near-lossless settings. It explicitly says the native-oracle suite was not
run. On the recorded six-core/twelve-thread Haswell system, the combined
changes range from small gains to approximately 3.3x versus C on those inputs.
These measurements are useful leads, not current acceptance evidence.

Inspect the historical files with:

```sh
git show 4e30c58:docs/FASTER-THAN-C.md
git show 4e30c58:libwebpop/src/utils/thread_utils.rs
git show 4e30c58 -- libwebpop/src/enc/vp8l_enc.rs
git show 4e30c58 -- libwebpop/src/enc/backward_references_enc.rs
git show 4e30c58 -- libwebpop/src/enc/histogram_enc.rs
```

| Research change | What it establishes | Proposed treatment |
| --- | --- | --- |
| Custom scoped `par_map` and `split_budget` | Workers claim indexed jobs through an atomic counter; per-item mutex slots retain input/result order. Explicit budgets limit nested OS-thread spawning. | Replace scheduler mechanics with Rayon. Preserve explicit job identity and memory limits; pool size alone does not limit retained scratch. |
| One VP8L encoder/writer per crunch configuration | More than two configurations can run concurrently; results are scanned in configuration order with strict `<`. Duplicate palettes may be recomputed and lose equal-size ties. | Start with upstream's two contiguous groups. Then use bounded contiguous groups or individual trials after auditing candidate state and duplicate handling. Preserve writer prefixes, palette inputs, near-lossless state and selected statistics. |
| Independent Standard/RLE/Box trials in `VP8LGetBackwardReferences` | Each strategy uses its own references, cost histogram and optional box chain, then results are folded in kind order. | Strong parallel-iterator candidate. Keep the selected box chain alive for traceback; retain no-cache-before-cache processing and strict improvement tests. |
| Per-cache-size `CacheSizeEntropy` | Each cache size owns its cache and histogram; scan costs in ascending size order. | Good candidate, but a larger translation change than scheduling an existing loop: upstream's serial path updates multiple cache sizes in one reference traversal. Verify token/cache state equivalence and measure the extra traversals. |
| Parallel `HistogramRemap` searches | Best-cluster queries read an immutable final cluster set. Empty histograms inherit the preceding symbol afterward in order; cluster histograms are rebuilt afterward. | Good `par_chunks_mut`/indexed-map candidate. Retain the existing per-cluster threshold/comparison behavior exactly and keep the final rebuild in source order. This does not parallelize clustering itself. |
| Speculative hash-chain search | Builds links serially, searches segments independently while recording fresh-search positions, then propagates the true extension state across boundaries until it rejoins a speculative fresh search. | Later, isolated milestone. Port with an explanation of the resynchronization invariant, intermediate-array tests and memory measurements. A naive independent search per pixel or chunk is not equivalent. |

The historical hash-chain threshold is `1 << 18` pixels, with eight segments
per requested thread. Those are tuning choices from that implementation,
not upstream constants or defaults to adopt without new measurements. It
retains the original link buffer while allocating another full packed-result
array and a fresh-search flag buffer. Budget these costs per active encoder.

## Deterministic execution contract

1. **Keep serial decision order.** Enumerate jobs in upstream order before
   scheduling. A winner must be selected using the original comparison in
   that order. VP8L stream and LZ77-kind trials use strict improvement; equal
   sizes/costs retain the earlier winner. Do not generalize that rule to every
   helper: retain each helper's actual threshold and tie behavior.
2. **Keep decision state.** Share immutable source pixels and analysis, not
   mutable transformed pixels, writers, reference buffers, histograms, caches,
   error fields or predictor reconstruction. A task starts from the state the
   corresponding serial trial would observe. Retained scratch is allowed
   only where it cannot carry a losing trial's decisions into another trial.
3. **Keep numerical order.** Preserve per-candidate entropy/cost accumulation,
   integer truncation, fixed-point arithmetic and floating-point evaluation.
   Lossy analysis combines integer counts/totals before averages and segment
   assignment. Do not parallel-reduce floating-point costs or centroids.
4. **Commit after completion.** Jobs return owned results or write disjoint
   preassigned slices. Select streams/stats and assemble output after joining.
   Errors leave the caller's output unchanged, as today. No worker can outlive
   borrowed inputs, including on an error or panic path.
5. **Select errors deliberately.** Drain started jobs, then inspect results
   in the chosen serial order. For the current Rust lossy path this means
   alpha's result before color's result. Upstream concurrent shared-error
   timing is not a useful deterministic contract to reproduce. Do not return
   whichever worker failed first.

An indexed `par_iter().map(...).collect::<Vec<_>>()` is convenient for small,
infallible metadata; the [indexed-iterator API](https://docs.rs/rayon/latest/rayon/iter/trait.IndexedParallelIterator.html)
provides indexed operations and collection into a vector. For codec-owned
fallible work, prefer pre-reserved ordered result slots:

```text
allocate result slots with try_reserve; assign stable job indices
parallel zip(job inputs, mutable result slots): slot = Some(run_job(input))
serial scan of result slots: propagate errors and apply original selection
```

This avoids an unchecked allocation for the result vector and makes error
precedence explicit. Rayon's [ParallelIterator documentation](https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html#method.try_for_each)
states that when multiple tasks fail, `try_for_each` does not specify which
error is returned. Do not use parallel fallible collection/early exit as the
codec's error-selection policy. Likewise, preserve an explicit index when
using filtering that removes an iterator's indexed structure.

Scratch and result vectors must continue using the repository's fallible
allocation conventions. Rayon runtime allocations are not all represented by
those `Result` APIs; adding Rayon does not establish recoverability from every
process allocation failure. Handle pool creation failure explicitly and do
not turn worker panics into successful encodes or ordinary allocation errors.

## Pool, API and memory policy

Introduce a small execution context at the integration layer; keep source
translation modules concerned with jobs and data. The initial implementation
should offer direct serial execution and execution within a reusable Rayon
pool. Do not port the C global worker registry or install a process-global
Rayon configuration from library code.

[`ThreadPool::install`](https://docs.rs/rayon/latest/rayon/struct.ThreadPool.html#method.install)
runs nested Rayon operations in that pool. A workload context can therefore
reuse the same pool across encodes and across alpha's nested VP8L work.
[`ThreadPoolBuilder`](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html)
supports explicit thread counts and fallible construction. Build the pool
once per workload, rather than once per candidate or stage.

Proposed initial policy, to confirm before public integration:

- Keep existing convenience APIs serial by default. A serial request executes
  directly on the caller, bypassing Rayon; a one-worker pool is also useful
  for testing but does not have the same caller-thread semantics.
- Start with an optional `parallel` Cargo feature using Rayon. The CLI can
  enable it and wire its currently unused `--mt` flag. Consider `--threads N`
  with `1 = serial`, `0 = automatic`, matching the research convention.
  Keep CLI automatic-by-default as a separate measured policy decision.
- Resolve an automatic count once for the workload and record the effective
  count. Explicit counts configure a reusable pool. Applications already
  scheduling encodes in Rayon should be able to reuse their execution pool
  instead of constructing a pool for every image.
- Add execution policy without casually changing supported crate-root
  `OpConfig`/`encode` contracts. Decide whether to expose a context/builder or
  an options field after the internal boundaries work. `LosslessOptions` and
  `LossyOptions` are currently under `internal`; the research patch predates
  the present public facade and is not an API patch to copy verbatim.
- Preserve the default build and direct serial path on targets without native
  thread support, including ordinary Wasm. Decide and test how an explicit
  parallel request on an unsupported build is reported before exposing it.

Current integration functions accept an arbitrary `FnOnce` output closure
and generic return value, without `Send` bounds. Do not wrap that whole API
inside `pool.install`, which requires transferable captures/results. Move
only the computational jobs into the pool, return their owned encoded data,
and invoke the output closure on the caller after joining. The same principle
applies to future progress callbacks: thread affinity and `Send` requirements
need an explicit contract. No callback framework is needed for this plan.

One fixed pool avoids recursively creating OS threads. It does **not** bound
live VP8L encoders, full-image scratch or retained completed bitstreams: a
suspended parent can retain memory while its worker executes another job.
Use bounded waves/groups of full-image trials and release losing streams
promptly. Keep an explicit limit on concurrent expensive candidates, separate
from the pool's worker count. Avoid blocking all pool workers on memory-budget
semaphores while runnable child jobs are needed to release that budget.

## Rust integration and remaining opportunities

| Module | Initial change |
| --- | --- |
| [`enc/vp8l_enc.rs`](../libwebpop/src/enc/vp8l_enc.rs) | Construct independent encoder/writer groups from one `EncoderAnalyze`; retain `EncodeStreamHook` and its serial loop. Preserve headerless callers and cloned partial writer state, not just complete-file output. |
| [`enc/analysis_enc.rs`](../libwebpop/src/enc/analysis_enc.rs), [`enc/iterator_enc.rs`](../libwebpop/src/enc/iterator_enc.rs) | Introduce the equivalent of `SegmentJob` with a row range, private iterator and statistics. Split immutable setup from writable modes/MB information, or return compact job-owned row outputs for ordered copying. Avoid cloning a complete `VP8Encoder` per job. |
| [`enc/alpha_enc.rs`](../libwebpop/src/enc/alpha_enc.rs), [`enc/webp_enc.rs`](../libwebpop/src/enc/webp_enc.rs) | Join the existing owned alpha job with the existing color driver after analysis. Both borrow the prepared picture; only color mutates `VP8Encoder`. Collect alpha data/SSE separately and assemble the file on the caller. |
| [`enc/backward_references_enc.rs`](../libwebpop/src/enc/backward_references_enc.rs), [`enc/histogram_enc.rs`](../libwebpop/src/enc/histogram_enc.rs) | Add independent trial/remap jobs after the upstream boundaries pass parity; keep the original ordered selection and serial reference entry points. |
| `utils/thread_utils.rs` (new) | Small Rust execution/policy helpers only where shared code is useful; no opaque `WebPWorker` emulation or bespoke replacement thread pool. |

`VP8IteratorInit` currently takes `&mut VP8Encoder` and clears shared top/NZ/
diffusion storage. `MBAnalyze` and mode setters also take the whole mutable
encoder even though analysis writes much smaller regions. Those signatures
must be narrowed or adapted for job-owned output. Do not solve the borrowing
problem with a whole-encoder mutex. Run shared initialization once and retain
source-border imports at every range boundary, including a job starting below
row zero. Preserve upstream names and annotate ownership adaptations clearly.

After the three upstream boundaries, these extensions are plausible:

- **More analysis row ranges:** source-border analysis is independent across
  macroblocks; chunk it with private iterators/counts and disjoint mode/MB
  output. Join all integer statistics before the unchanged `AssignSegments`.
  Keep the two-job implementation as the initial translation/reference.
- **Alpha filter trials:** `ApplyFiltersAndEncode` can try independent filter
  candidates with private filtered planes/writers, then choose in existing
  filter order. Preserve raw fallback, effort mapping and quantization before
  trials. This is an extension beyond upstream's threading, not a prerequisite
  for overlapping alpha and color.
- **Cached/no-cache traceback:** the two refinements can sometimes be
  independent, but upstream has a clone-and-break shortcut when the chosen
  strategy is the same and cache bits are zero. Preserve that shortcut and
  buffer ownership before trying a `join`.
- **Predictor histogram preparation:** investigate computing candidate tile
  histograms independently, followed by the existing ordered predictor
  selection. Histogram memory is large, so use bounded preparation. Tile
  selection itself depends on accumulated histograms and neighbor-mode biases.
  Near-lossless residual generation additionally uses reconstructed context.

Keep these stages serial unless separately proven equivalent:

- Main VP8 `VP8EncLoop`/`VP8EncTokenLoop`: unfiltered reconstructed top/left
  samples, Intra4 subblock reconstruction, nonzero/mode contexts, chroma error
  diffusion, probability refresh and retry state carry through raster order.
  Multiple coefficient partitions do not remove those coding dependencies.
- VP8L predictor and cross-color **selection**: accumulated entropy and
  previous selected modes/transforms affect later choices. Cross-color's
  `prev_x` intentionally carries across tile rows. Merely giving each tile
  its own task would change bytes.
- Histogram clustering merges: stochastic sequence, greedy merge choices,
  queue maintenance and changing cluster costs are serial decision state.
  Parallel fixed-state cost evaluation is a possible later optimization;
  simultaneous merges are not interchangeable with it.
- Bitstream emission into one writer, near-lossless reconstructed feedback,
  stateful conversion/dithering and SharpYUV iterative updates. A future row
  kernel optimization must respect neighborhoods, iteration barriers and
  arithmetic order; none is required to deliver this plan.

## Implementation order and acceptance gates

### 0. Reference and scheduling contract

Keep the existing scalar, threading-off oracle unchanged as the byte baseline.
It currently forces `WEBP_USE_THREAD=OFF` in
[`tests/reference/CMakeLists.txt`](../libwebpop/tests/reference/CMakeLists.txt).
Add a separate threading-enabled scalar build/configuration and recorded
request setting for `thread_level=0/1`; do not silently change old fixture
provenance or rely on `-mt` against a build with threading disabled.

For each supported route/configuration, compare Rust serial, Rust parallel,
C thread-level 0 and C thread-level 1. Establish C 0/1 agreement explicitly.
If a mismatch exists, isolate its first differing intermediate before choosing
an implementation; do not relax the requirement to pixel equality or encode
different bytes according to Rust's worker count. Add intermediate observers
only where they help explain a mismatch. Update the reference README/provenance
for the new build, without regenerating unrelated fixtures.

### 1. Executor and upstream boundaries

Implement serial/Rayon execution, then separately land VP8L's two groups,
alpha/color overlap and two-job VP8 analysis. Keep original job sizes and
selection loops. Each change must pass its byte/intermediate gates before
combining them. Verify serial and one-worker execution, borrowed inputs,
unchanged output on error and completion of all started jobs.

For analysis, compare the entire mode/prediction map, MB information,
susceptibility histogram/totals and final segment parameters, not merely the
eventual file. For VP8L compare selected configuration, cache/histogram/
transform settings and stream bytes. For alpha compare filtered/compressed
payload and SSE, then the complete VP8/ALPH container.

### 2. Independent lossless work

Adapt and verify each research technique independently: bounded configuration
groups, LZ77 strategies, final histogram remap, and per-cache-size costs.
Prefer shared serial/parallel kernels; document and test exceptions such as
the cache search's fused serial traversal. Reprofile after each retained
change. The available speedup depends on image analysis and enabled trials;
method 0 and single-configuration images need inner work to scale.

### 3. Additional measured jobs

Measure whether more analysis ranges, nested VP8L inside alpha, alpha filters,
or independent traceback refinements justify their scratch and scheduling
costs. Extend the same execution context. Any scheduling threshold or memory
limit must change only performance, never candidate availability or output.

### 4. Speculative hash-chain search

Retain serial link construction and the original serial search as a reference.
Port the research resynchronization algorithm with forced segment boundaries
and direct full-array comparisons. Test long repeated runs, distances 1/row/
arbitrary, maximum-length extension resets, chain/window/iteration limits,
both effort paths and segments much smaller than matches. Include adversarial
inputs where repair traverses much of a segment. Measure repair work and
extra peak memory; omit this optimization if its maintenance/memory cost is
not justified. It is not required to complete the upstream threading port.

### 5. Public/CLI integration and benchmark record

Finalize execution-policy exposure and dependency/target support after the
internal behavior is established. Wire `--mt` and any agreed explicit thread
count, document defaults and pool reuse, and preserve convenience-API behavior.
Animation receives only shared encoder/context plumbing needed for reuse.

Use deterministic synthetic cases plus the existing larger image corpus:
opaque/transparent, palette sizes around packing thresholds and 256 colors,
noise, long runs, sparse/empty histogram tiles and distinct-stream equal-cost
ties. Cover methods 0–6, quality thresholds including VP8L method-6 quality
100, fractional lossy quality, exact, near-lossless strengths, alpha quality
and ordinary/SharpYUV conversion. Exercise tiny/odd/padded geometries and
analysis splits just below/at/above the threshold. Include headerless ALPH
streams and borrowed image subviews used by existing callers.

Repeat at pool sizes 1, 2, 3 and a larger count, with forced small job chunks,
repeated runs, a reused external pool and nested alpha jobs. Perturb task
completion order in tests. Bound the existing test harness's case-level
parallelism so multiplying it by encoder workers does not dominate results.
Independent decoding remains required alongside byte comparisons.

Record Rust serial/parallel and C without/with `-mt`, matching builds and
coding settings. Report cold pool startup separately from warmed repeated
encodes; include bytes, latency, throughput, effective workers and peak
memory. Separate scalar-C parity from normal SIMD-enabled C performance
comparisons. Include single-configuration VP8L, opaque VP8 and alpha-heavy
VP8 so the record exposes where upstream-style threading cannot help much.
Do not promise core-count scaling for the serial VP8 coding loop.

Planned verification commands, once implementation exists:

```sh
cargo build -p libwebpop --lib
cargo test
cargo test --features parallel
cargo test --features parallel,native-oracle -- --nocapture
cargo fmt --all -- --check
```

Also run the separate threaded-C matrix and supported-target builds, recording
the exact commands once that harness and feature exist. Completion of the
initial threading port requires the three upstream boundaries, a usable
execution policy, exact-byte evidence and a repeated time/memory record.
Further lossless parallel algorithms each have their own acceptance evidence.
