# Scalar near-lossless acceptance measurements

The accompanying [CSV](2026-10-23-near-lossless.csv) records complete-file parity
with pinned scalar C for the five full-size recorded samples, at strengths
0/20/40/60/80/100. The reference is libwebp
`1844042d1ca751e62e61df7617dba19c98588339`, configured as in
[the near-lossless reference record](../../libwebpop/tests/reference/NEAR_LOSSLESS.md).
All runs use the custom wrapper's quality 70, method 4 and exact false.

Run from the repository root:

```sh
cargo test -p libwebpop --release --features native-oracle \
  --test near_lossless_public real_image_corpus_parity_and_metrics -- --ignored --nocapture
```

Host: Intel Core i9-13900KF, Linux `7.0.0-38-generic`, x86_64. Rust:
`1.101.0-nightly (7189d39d9 2026-10-22)`, default release profile with repository
Cargo configuration. C: GCC `15.2.0-16ubuntu1`, CMake `4.2.3`, Release
`-O3 -DNDEBUG`, SIMD and threading disabled, near-lossless enabled. No explicit
CPU affinity was set. Measurements ran after the automated test suites finished.

Each source is decoded once to packed RGBA with the independent `image` WebP
decoder; both encoders receive those exact pixels. C encoding happens first,
through the normal pinned library and copied wrapper, outside Rust timing.
Complete encoded bytes must match. Both files are independently decoded and
compared. Alpha endpoints are checked, and strength 100 also matches the simple
Rust lossless API.

`rust_ms` is one timed Rust library call, excluding input decoding, C encoding,
validation and writing. These are exploratory single samples, without warmup,
randomized ordering or a comparative speed claim. `max_r/g/b/a` are maximum
absolute deviations from source RGBA; RGB excludes pixels with zero source or
decoded alpha. `process_peak_kib` is cumulative Linux process high-water RSS:
it includes source pixels, encoding, both compressed files, both independent
decodes, additional strength-100 identity encoding and test infrastructure.
It does not measure isolated encoder memory or C child memory.

Input provenance is in [samples/SOURCES.md](../../samples/SOURCES.md). Input
SHA-256 values for this run:

```text
1f50b3bf926709165c14c7dd0654d16cacd1c5b50a59560a52aa6c507bfdfa58  graphic-alpha-transparency.webp
8c705c674dbe319e505326ecd25acf6f24552abf3ac7861b8cebf8c6b7daacc2  graphic-invite.webp
ae08b45d6948f98dc7e665fd01363bcc59948f538f430b40959546240f44ce16  graphic-map.webp
a52ce587a9c5b1ee1ab60652ec7970db8c280bc8d4740e9b7497cbef36f85dd1  photo-japan-p50.webp
c4c533f1b4533927a057e38b0f517a776b3fdff183cc9d532e519f8c7da5e9f9  screenshot-wikipedia.webp
```

Prepared RGBA, C outputs, `metrics.csv` and native build provenance remain in
`target/native-oracle/`. Source fixtures and the upstream pin were unchanged.

All 30 cases passed complete-byte and decoded-reference comparisons. The
largest image is 10416×7812 (81,369,792 pixels); its Rust calls took about
15.6–22.5 seconds in this run. The maximum observed decoded error was 16 in
each RGBA channel at strength 0, reducing to zero at 100. The map was unchanged
at every strength. Encoded sizes are not monotonic across strength buckets.
Process high-water RSS reached 2,762,800 KiB with the cumulative measurement
scope described above.
