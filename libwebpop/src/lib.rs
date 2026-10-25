//! Scalar Rust port of the pinned libwebp still-image encoders.
//!
//! [`WebPEncodeRGBA`] encodes lossy VP8 with exact alpha;
//! [`WebPEncodeRGBAWithAlphaQuality`] also permits alpha quantization.
//!
//! [`WebPEncodeLosslessRGBA`] produces complete WebP images using libwebp's
//! simple API defaults. [`WebPEncodeRGBAWithNearLossless`] permits bounded
//! pixel changes with the same simple API effort settings. All use the pinned
//! encoder's configuration trials.
//!
//! During the 0.1.x release series, compatibility is guaranteed only for the
//! four crate-root `WebPEncode*` functions and [`WebPEncodingError`] as an error
//! implementing [`std::error::Error`]. Error variants and their payloads are
//! implementation details and may change in patch releases.
//!
//! Everything under [`internal`] is exposed for testing and integration, is
//! unsupported, and may change or disappear in any release, including patch
//! releases. This includes configurable encoders, animation, and codec helpers.
//!
//! The still RGBA entry points take a final `output: &mut Vec<u8>` and return
//! `Result<(), _>`. Success replaces the output with a complete WebP file,
//! reusing its capacity; errors leave its contents unchanged.
//!
//! ```
//! let rgba = [12, 34, 56, 255];
//! let mut output = Vec::new();
//! libwebpop::WebPEncodeLosslessRGBA(&rgba, 1, 1, 4, &mut output)?;
//! assert_eq!(&output[..4], b"RIFF");
//! # Ok::<(), libwebpop::WebPEncodingError>(())
//! ```
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

pub mod internal;

// Keep existing implementation paths without exposing them at the crate root.
use internal::{dsp, enc, mux, sharpyuv, utils, webp};

mod encode;
pub use encode::{
    WebPEncodeLosslessRGBA, WebPEncodeRGBA, WebPEncodeRGBAWithAlphaQuality,
    WebPEncodeRGBAWithNearLossless,
};

/// Error type returned by the upstream-named simple encoding API.
///
/// Compatibility during 0.1.x covers using this type as a [`std::error::Error`],
/// including its [`std::fmt::Display`] implementation. Variants, payloads and
/// exact error messages are unsupported implementation details.
pub type WebPEncodingError = encode::LosslessEncodeError;
