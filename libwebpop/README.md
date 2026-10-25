## libwebp(sl)op

<img src="https://img.shields.io/badge/WARN-LLM%20Generated-FF6347" alt="llm generated"/>

A "live port" (by agents) of [libwebp](https://chromium.googlesource.com/webm/libwebp) to rust.

During the 0.1.x release series, compatibility is guaranteed only for the
crate-root `encode`, `OpImage`, `OpConfig`, `OpQuality`,
`WebPEncodeLosslessRGBA`, and `WebPEncodeRGBA` APIs,
and `WebPEncodingError` as a `std::error::Error` with `Display`. Error variants,
payloads, and exact messages are implementation details. Everything under
`libwebpop::internal` is exposed for testing and integration, is unsupported,
and may change or disappear in any release, including patch releases.

```rust
use libwebpop::{OpConfig, OpImage, encode};

let pixels = [12, 34, 56, 255];
let output = encode(
    OpImage::Rgba { pixels: &pixels, width: 1, height: 1, stride: 4 },
    &OpConfig::default(),
)?;
```

The default is lossless at effort 70, method 4, with transparent RGB cleanup.
For near-lossless encoding, select `OpQuality::Lossless { near_lossless,
effort, exact }`. For lossy encoding, select `OpQuality::Lossy { quality,
alpha_quality, sharp_yuv }`. Construct `OpConfig::new(quality)` and set
`config.method` to change method. Image stride is measured in bytes.
