//! Checked public RGBA entry points for the VP8L encoder.

pub use crate::enc::config_enc::LosslessOptions;
pub use crate::webp::encode::LosslessEncodeError;

use crate::enc::config_enc::{EncoderConfig, WebPValidateConfig};
use crate::enc::picture_csp_enc::{WebPPictureImportRGBA, checked_rgba_geometry};
use crate::enc::webp_enc::WebPEncode;

/// Encode positive-stride RGBA as a complete lossless still-image WebP using
/// the pinned libwebp simple API's defaults (quality 70, method 4, exact false).
/// Dimensions must be in 1..=16383. Final-row padding is optional; the source
/// is never modified. RGB beneath alpha zero may change.
pub fn WebPEncodeLosslessRGBA(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<Vec<u8>, LosslessEncodeError> {
    encode_lossless_rgba(rgba, width, height, stride, &LosslessOptions::default())
}

/// Encode positive-stride RGBA as a complete lossless still-image WebP.
/// Dimensions must be in 1..=16383; final-row padding is optional. Input and
/// options are validated before allocation, and the source is never modified.
/// RGB beneath alpha zero may change unless `options.exact` is set.
///
/// Uses the pinned encoder's entropy analysis and configuration trial loop.
/// Default options match [`WebPEncodeLosslessRGBA`]; other options retain
/// near_lossless=100, the default image hint and synchronous execution.
pub fn encode_lossless_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    options: &LosslessOptions,
) -> Result<Vec<u8>, LosslessEncodeError> {
    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    WebPValidateConfig(options)?;
    let mut picture = WebPPictureImportRGBA(rgba, width, height, stride)?;
    WebPEncode(
        &EncoderConfig {
            options: *options,
            near_lossless: 100,
        },
        &mut picture,
    )
}

/// Encode positive-stride RGBA as a near-lossless still-image WebP. Strength
/// must be in 0..=100; lower values permit stronger changes to RGB and
/// intermediate alpha. Alpha 0 and 255 stay unchanged. Strength 100 is byte
/// identical to [`WebPEncodeLosslessRGBA`]. Uses quality 70, method 4 and
/// transparent RGB cleanup. Input is never modified; geometry follows the
/// lossless API, including optional final-row padding.
pub fn WebPEncodeRGBAWithNearLossless(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    near_lossless: i32,
) -> Result<Vec<u8>, LosslessEncodeError> {
    if !(0..=100).contains(&near_lossless) {
        return Err(LosslessEncodeError::InvalidNearLosslessStrength);
    }
    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    let config = EncoderConfig {
        options: LosslessOptions::default(),
        near_lossless: near_lossless as u32,
    };
    let mut picture = WebPPictureImportRGBA(rgba, width, height, stride)?;
    WebPEncode(&config, &mut picture)
}
