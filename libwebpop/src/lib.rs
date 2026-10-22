//! Bottom-up port of the pinned libwebp lossless encoder.
//!
//! These modules currently expose internal leaf operations for differential
//! testing. The public image encoder is not implemented yet.
#![forbid(unsafe_code)]
#![allow(non_snake_case)]

pub mod dsp;
pub mod enc;
pub mod utils;
pub mod webp;
