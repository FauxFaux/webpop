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
the guest allocations. Seven synthetic cases shared with the pinned native
oracle cover skinny images, odd dimensions, palettes, mixed/zero alpha, padded
strides, minimum buffer lengths, and unaligned guest inputs. A Rust WebP decoder
verifies dimensions, alpha everywhere, and RGB wherever alpha is nonzero after
the Wasmtime store is dropped. Fully transparent pixels may change hidden RGB
under the simple API's `exact=0` setting.

See [the native oracle guide](../reference/README.md) for live leaf comparisons
against the current upstream pin and portable committed fixtures. This older
Wasm build remains a public-API semantic regression, not a byte-exact oracle
for the pin.
