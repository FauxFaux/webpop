//! Scalar Rust port of the pinned libwebp still-image encoders.
//!
//! [`WebPEncodeRGBA`] encodes lossy VP8 with exact alpha;
//! [`WebPEncodeRGBAWithAlphaQuality`] also permits alpha quantization.
//! [`encode_lossy_rgba`] adds configurable SharpYUV conversion.
//!
//! [`WebPEncodeLosslessRGBA`] produces complete WebP images using libwebp's
//! simple API defaults. [`encode_lossless_rgba`] also exposes effort and exact
//! transparent-RGB handling. [`WebPEncodeRGBAWithNearLossless`] permits bounded
//! pixel changes with the same simple API effort settings. All use the pinned
//! encoder's configuration trials.
//! [`encode_near_lossless_rgba`] also exposes compression effort and exact handling.
//!
//! All RGBA entry points take a final `output: &mut Vec<u8>` and return
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

pub mod dsp;
pub mod enc;
pub mod mux;
pub mod sharpyuv;
pub mod utils;
pub mod webp;

mod encode;
pub use encode::{
    LosslessEncodeError, LosslessOptions, LossyOptions, WebPEncodeLosslessRGBA, WebPEncodeRGBA,
    WebPEncodeRGBAWithAlphaQuality, WebPEncodeRGBAWithNearLossless, encode_lossless_rgba,
    encode_lossy_rgba, encode_near_lossless_rgba,
};

/// Error type returned by the upstream-named simple encoding API.
pub type WebPEncodingError = LosslessEncodeError;
