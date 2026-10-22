//! Bottom-up port of the pinned libwebp lossless encoder.
//!
//! [`encode_lossless_rgba`] produces complete WebP images using the ported
//! lossless pipeline. Automatic upstream configuration selection is still
//! being ported, so compressed bytes need not match libwebp's simple API.
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

pub mod dsp;
pub mod enc;
pub mod utils;
pub mod webp;

mod encode;
pub use encode::{LosslessEncodeError, LosslessOptions, encode_lossless_rgba};
