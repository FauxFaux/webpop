//! Bottom-up port of the pinned libwebp lossless encoder.
//!
//! [`WebPEncodeLosslessRGBA`] produces complete WebP images using libwebp's
//! simple API defaults. [`encode_lossless_rgba`] also exposes effort and exact
//! transparent-RGB handling. [`WebPEncodeRGBAWithNearLossless`] permits bounded
//! pixel changes with the same simple API effort settings. All use the pinned
//! encoder's configuration trials.
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

pub mod dsp;
pub mod enc;
pub mod utils;
pub mod webp;

mod encode;
pub use encode::{
    LosslessEncodeError, LosslessOptions, WebPEncodeLosslessRGBA, WebPEncodeRGBAWithNearLossless,
    encode_lossless_rgba,
};

/// Error type returned by the upstream-named simple encoding API.
pub type WebPEncodingError = LosslessEncodeError;
