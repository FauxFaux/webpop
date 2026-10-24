//! Errors for the checked encoders with caller-owned output. The historical lossless name
//! is retained for compatibility; the simple APIs also use WebPEncodingError.

use crate::enc::{
    alpha_enc::AlphaEncodeError, picture_csp_enc::RgbaInputError, syntax_enc::VP8WriteError,
    vp8l_enc::ImageWriteError,
};
use std::collections::TryReserveError;

#[derive(Debug, thiserror::Error)]
pub enum LosslessEncodeError {
    #[error(transparent)]
    Input(#[from] RgbaInputError),
    #[error("quality must be a finite number in 0..=100")]
    InvalidQuality,
    #[error("alpha quality must be in 0..=100")]
    InvalidAlphaQuality,
    #[error("near-lossless strength must be in 0..=100")]
    InvalidNearLosslessStrength,
    #[error("method must be in 0..=6")]
    InvalidMethod,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
    #[error(transparent)]
    Container(#[from] ImageWriteError),
    #[error(transparent)]
    Alpha(#[from] AlphaEncodeError),
    #[error(transparent)]
    VP8Container(#[from] VP8WriteError),
}
