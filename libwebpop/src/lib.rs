//! Scalar Rust port of the pinned libwebp still-image encoders.
//!
//! [`WebPEncodeRGBA`] encodes lossy VP8 with exact alpha;
//! [`WebPEncodeRGBAWithAlphaQuality`] also permits alpha quantization.
//!
//! [`WebPEncodeLosslessRGBA`] produces complete WebP images using libwebp's
//! simple API defaults. [`encode_lossless_rgba`] also exposes effort and exact
//! transparent-RGB handling. [`WebPEncodeRGBAWithNearLossless`] permits bounded
//! pixel changes with the same simple API effort settings. All use the pinned
//! encoder's configuration trials.
//! [`encode_near_lossless_rgba`] also exposes compression effort and exact handling.
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

pub mod dsp;
pub mod enc;
pub mod sharpyuv;
pub mod utils;
pub mod webp;

mod encode;
pub use encode::{
    LosslessEncodeError, LosslessOptions, WebPEncodeLosslessRGBA, WebPEncodeRGBA,
    WebPEncodeRGBAWithAlphaQuality, WebPEncodeRGBAWithNearLossless, encode_lossless_rgba,
    encode_near_lossless_rgba,
};

/// Error type returned by the upstream-named simple encoding API.
pub type WebPEncodingError = LosslessEncodeError;
