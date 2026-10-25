# Public lossy methods release baseline

Recorded 2026-10-25 after parent `8c26278`, with the methods milestone-8
working changes. The [42-row CSV](2026-10-25-lossy-methods.csv) records complete
file sizes/SHA-256, source-to-decoded RGB/alpha errors and PSNR, Rust/C median
encoding time, and fresh-process peak RSS. The [raw record](2026-10-25-lossy-methods.json)
preserves every repetition, commands, input hashes, binary hashes and provenance.
Every complete Rust output equals normally linked pinned C byte for byte;
independent Rust decoding also gives identical C/Rust color/alpha metrics.

Inputs are the committed 128×128 `test_ref.ppm` and
`samples/photo-japan-p50.webp`, independently decoded and reduced with Lanczos3
to 1024×685. Each has an opaque variant and an alpha variant with
`alpha=(17*x + 13*y) & 255`. Preparation, source decode and resizing occur
before measurement. All seven methods run at color quality 75; alpha inputs
use alpha qualities 70 and 100, while opaque inputs use 100. Exact source alpha
is verified at alpha quality 100. Input file and prepared packed RGBA hashes
are included in the raw record; sample provenance is in [SOURCES.md](../../samples/SOURCES.md).

The machine is an Intel Core i9-13900KF on x86_64 Ubuntu Linux
7.0.0-38-generic. Each benchmark child inherits affinity to logical CPU 0.
Rust uses Cargo release with `rustc 1.101.0-nightly (7189d39d9 2026-10-22)` /
LLVM 23.1.4 and the session's `RUSTFLAGS=-Ctarget-cpu=native`. C uses the clean
`1844042d1ca751e62e61df7617dba19c98588339` pin, GCC 15.2.0-16ubuntu1,
CMake 4.2.3 and Release `-O3 -DNDEBUG`, with SIMD dispatch/threading disabled
and token buffering enabled. Compiler auto-vectorization and libc remain
available. These build flags are recorded observations, not a tuned or
instruction-set-matched performance comparison.

Each implementation runs in a fresh child process for a given input/settings
pair, performs one warm-up, then five measured encodes. The table uses their
median; raw samples include all five timings. A seeded permutation chooses
whether Rust or C runs first for each row. Repetitions are consecutive inside
each implementation's process, rather than alternating C/Rust calls. The two
implementations use identical pixel/settings inputs and compare complete bytes
outside the timed regions. This is an exploratory baseline on a shared machine;
there are no confidence bounds or claims of monotonic size, quality or speed.

Both timers cover configuration checks, packed RGBA import, YUVA transparent
cleanup, analysis, alpha trials, frame coding, RIFF assembly and encoder/picture
cleanup. Output storage starts fresh per call; retaining/comparing previous
output, file reads/writes, hashing and independent decode are outside the timers.
Rust calls the public `encode_lossy_rgba`; C uses the ordinary default preset,
RGBA picture import and public `WebPEncode` with a memory writer. Neither uses
included-source/instrumented oracle encoders.

RGB PSNR uses the three original prepared channels where source alpha is
nonzero, without compositing or alpha weighting. Alpha PSNR uses every pixel.
Both use `10*log10(255^2 * sample_count / SSE)`. Empty PSNR cells in the CSV/JSON
mean infinite PSNR (zero SSE); exact integer SSE and sample counts are retained.
For the photo, these metrics refer to the prepared decoded/resized sample.

All rows below use the 1024×685 input, color quality 75 and alpha quality 100.
Alpha PSNR is infinite for every row.

| Method | Input | Bytes | RGB PSNR dB | Rust median ms | C median ms | Rust / C process peak KiB |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | Opaque | 175,198 | 30.964 | 20.476 | 21.177 | 7,600 / 7,264 |
| 1 | Opaque | 173,988 | 30.965 | 27.249 | 28.816 | 7,592 / 7,592 |
| 2 | Opaque | 159,866 | 30.450 | 32.352 | 35.734 | 7,664 / 7,456 |
| 3 | Opaque | 155,702 | 30.664 | 92.693 | 103.928 | 10,700 / 9,972 |
| 4 | Opaque | 156,050 | 30.672 | 92.431 | 104.040 | 10,564 / 9,860 |
| 5 | Opaque | 152,652 | 30.583 | 101.789 | 115.552 | 10,576 / 9,652 |
| 6 | Opaque | 149,532 | 30.585 | 152.360 | 176.339 | 10,432 / 9,688 |
| 0 | Alpha | 176,210 | 30.966 | 29.548 | 28.083 | 19,716 / 19,276 |
| 1 | Alpha | 174,908 | 30.964 | 75.285 | 65.757 | 19,616 / 19,708 |
| 2 | Alpha | 160,788 | 30.443 | 80.991 | 72.250 | 19,644 / 19,104 |
| 3 | Alpha | 156,738 | 30.666 | 141.257 | 140.743 | 20,012 / 20,012 |
| 4 | Alpha | 156,946 | 30.671 | 143.335 | 144.473 | 24,572 / 24,868 |
| 5 | Alpha | 153,744 | 30.588 | 191.815 | 184.188 | 24,584 / 24,340 |
| 6 | Alpha | 150,534 | 30.583 | 850.415 | 736.910 | 40,300 / 36,660 |

The method-6 alpha-quality-100 case includes its VP8L quality-100 trials.
Its alpha-quality-70 counterpart produces 152,972 bytes and measures
174.643 ms Rust / 193.652 ms C on the same prepared image. This describes the
specific settings, not a fidelity or effort recommendation.

RSS is GNU time's maximum resident set for the **whole benchmark child**:
runtime/shared libraries, loaded raw RGBA, one warm-up and five sequential
encodes, a retained complete first output, output comparison and file writing.
It excludes Cargo, Python orchestration, input preparation and independent
decoding, which run in separate processes. Across all rows, Rust process peaks
are 3,460–40,300 KiB and C peaks 2,992–36,660 KiB. Neither is an encoder-only
heap peak, allocation count or incremental per-call memory measurement.
Allocator/OS variation and these whole-process boundaries limit comparisons.

Reproduce with the checked-in scalar reference build and source samples:

```sh
python3 scripts/benchmark-lossy.py \
  --output target/benchmarks/lossy-methods-new --runs 5 --edge 1024 --cpu 0
```

The script builds both runners before timing, rejects a dirty/different upstream
pin, and retains raw inputs, complete WebP outputs, repeated timings, RSS,
source snapshots and CMake/Cargo provenance under its new output directory.
It refuses to overwrite an existing directory. Set `RUSTFLAGS` explicitly to
reproduce or change the recorded Rust target choice.
