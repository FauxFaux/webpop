//! Scalar Rust port of the pinned libwebp still-image and animation encoders.
//!
//! [`encode()`] accepts borrowed pixels and an [`OpConfig`] and returns an
//! owned WebP file. [`OpQuality`] selects lossless, near-lossless, or lossy
//! encoding, including alpha quality and SharpYUV conversion for lossy images.
//! [`OpAnimEncoder`] accepts the same image/config types with per-frame durations.
//!
//! [`WebPEncodeRGBA`] and [`WebPEncodeLosslessRGBA`] follow libwebp's simple
//! API defaults for lossy and lossless encoding and write into a caller-owned
//! buffer. All entry points use the pinned encoder's configuration trials.
//!
//! During the 0.1.x release series, compatibility is guaranteed only for the
//! crate-root [`encode()`], [`OpImage`], [`OpConfig`], [`OpQuality`],
//! [`OpAnimEncoder`], [`OpAnimOptions`], [`OpAnimPolicy`], [`OpAnimKeyframes`],
//! [`WebPEncodeRGBA`], [`WebPEncodeLosslessRGBA`], and [`OpAnimError`]/[`WebPEncodingError`] as
//! errors implementing [`std::error::Error`]. Error variants and their payloads are
//! implementation details and may change in patch releases.
//!
//! Everything under [`internal`] is exposed for testing and integration, is
//! unsupported, and may change or disappear in any release, including patch
//! releases. This includes custom C-style wrappers and codec/mux helpers.
//!
//! The two C-style entry points take a final `output: &mut Vec<u8>` and return
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
pub use encode::{WebPEncodeLosslessRGBA, WebPEncodeRGBA};

mod api;
pub use api::{OpConfig, OpImage, OpQuality, encode};

mod animation;
pub use animation::{OpAnimEncoder, OpAnimError, OpAnimKeyframes, OpAnimOptions, OpAnimPolicy};

/// Error type returned by the supported encoding APIs.
///
/// Compatibility during 0.1.x covers using this type as a [`std::error::Error`],
/// including its [`std::fmt::Display`] implementation. Variants, payloads and
/// exact error messages are unsupported implementation details.
pub type WebPEncodingError = encode::LosslessEncodeError;
