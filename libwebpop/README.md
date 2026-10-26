## libwebp(sl)op

<img src="https://img.shields.io/badge/WARN-LLM%20Generated-FF6347" alt="llm generated"/>

A "live port" (by agents) of [libwebp](https://chromium.googlesource.com/webm/libwebp) to rust.

Encoding only. The output is byte-identical to upstream for all known testcases.

There's an exposed rust-style API in the root. `libwebpop::internal` exposes a bunch of implementation details,
with lower API stability guarantees.


### Examples

Encode a still image:

```rust
use libwebpop::{OpConfig, OpImage, encode};

let pixels = [12, 34, 56, 255];
let output = encode(
    OpImage::Rgba { pixels: &pixels, width: 1, height: 1, stride: 4 },
    &OpConfig::default(),
)?;
```

Encode an animation:

```rust
use libwebpop::{OpAnimEncoder, OpAnimOptions, OpConfig, OpImage};

let mut encoder = OpAnimEncoder::new(1, 1, OpAnimOptions::default())?;
for (pixels, duration) in [([255, 0, 0, 255], 100), ([0, 0, 255, 255], 40)] {
    encoder.add(
        OpImage::Rgba { pixels: &pixels, width: 1, height: 1, stride: 4 },
        duration,
        &OpConfig::default(),
    )?;
}
let webp = encoder.finish()?;
```
