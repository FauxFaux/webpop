Run the lossless WASM round-trip example with:

```sh
cargo test -p libwebpop --test integration
```

`webp-encode-lossless.wasm` is copied unchanged from
`~/code/quad-image/web/assets/webp-encode-lossless.wasm`. It is quad-image's
scalar, lossless-only libwebp v1.6.0 build using Emscripten 6.0.10.
The build recipe and patches live in that repository's `webp-wasm/` directory
(regenerate there with `npm run build:webp-wasm:lossless`).

The test initializes the Emscripten stack, allocates guest RGBA memory, calls
`WebPEncodeLosslessRGBA`, copies the encoded bytes to the host, and releases
the guest allocations. A Rust WebP decoder verifies the dimensions and exact
RGBA pixels after the Wasmtime store is dropped.
