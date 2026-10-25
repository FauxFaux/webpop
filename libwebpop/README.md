## libwebp(sl)op

<img src="https://img.shields.io/badge/WARN-LLM%20Generated-FF6347" alt="llm generated"/>

A "live port" (by agents) of [libwebp](https://chromium.googlesource.com/webm/libwebp) to rust.

During the 0.1.x release series, compatibility is guaranteed only for the
crate-root `WebPEncodeLosslessRGBA`, `WebPEncodeRGBA`,
`WebPEncodeRGBAWithAlphaQuality`, and `WebPEncodeRGBAWithNearLossless` functions,
and `WebPEncodingError` as a `std::error::Error` with `Display`. Error variants,
payloads, and exact messages are implementation details. Everything under
`libwebpop::internal` is exposed for testing and integration, is unsupported,
and may change or disappear in any release, including patch releases.
