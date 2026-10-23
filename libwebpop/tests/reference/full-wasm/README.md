The retained `webp-encode.wasm` is the **full** quad-image encoder, copied from
`web/assets/webp-encode.wasm` at quad-image revision
`1ee887300721ad754c5632b56349d5daad17c11e`. It is separate from the formerly
bundled lossless-only module and is used only by the opt-in `wasm-oracle` test.
Tests do not need that checkout. Size: 440,786 bytes. SHA-256:
`f6317d0cce217547b165a768f82647c2e923c489b5ab75b8dd0310722cffa477`.
The Node host verifies the hash before loading it.

The reviewed recipe is quad-image's `webp-wasm/Dockerfile` at that revision:
libwebp v1.6.0, Emscripten 6.0.10, Release static libraries, `-O3 -msimd128`
final link, `STANDALONE_WASM=1`, `STACK_OVERFLOW_CHECK=2`, memory growth and
emmalloc. Its `simd.patch` enables SSE2/SSE4.1 via Wasm SIMD and repairs CPU
dispatch; threads are disabled. The later lossless-only patch does not affect
this full artifact. Codec code is under libwebp's BSD license and patent grant
(see `libwebpop/COPYING` and `libwebpop/PATENTS`); custom wrapper provenance and
permission are recorded in `../near_lossless_wrapper.c` and `../NEAR_LOSSLESS.md`.

Run from the repository root with Node supporting Wasm SIMD (verified with
Node v24.20.0; SIMD is enabled by default):

```sh
cargo test -p libwebpop --features wasm-oracle --test near_lossless_wasm -- --nocapture
```

`near-lossless.mjs` follows the recorded Emscripten initialization: stack init,
stack limits from exported base/end, then `_initialize`. It copies output
before freeing guest allocations and recreates views after potential memory
growth. It checks the custom export, unchanged unaligned input with padded
strides and minimum input lengths, cleared error slots for -1/101, and
near-lossless-100/simple-lossless byte identity. Rust independently decodes all
196 files, verifies dimensions and alpha endpoints, bounds visible RGB and
alpha error, and proves lower strengths actually change all four channels.
Requests and responses are retained in `target/full-wasm-oracle/`.

This is an export/ownership/semantic regression for an older module, not the
algorithm oracle. No old-Wasm/new-pin byte identity is required. Normal library
builds and tests neither load Wasm nor invoke Node; no runtime crate is added.
