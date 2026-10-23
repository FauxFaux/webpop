//! Errors for the checked, owned-output lossless encoder.

use crate::enc::{picture_csp_enc::RgbaInputError, vp8l_enc::ImageWriteError};
use std::collections::TryReserveError;

#[derive(Debug, thiserror::Error)]
pub enum LosslessEncodeError {
    #[error(transparent)]
    Input(#[from] RgbaInputError),
    #[error("quality must be in 0..=100")]
    InvalidQuality,
    #[error("method must be in 0..=6")]
    InvalidMethod,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
    #[error(transparent)]
    Container(#[from] ImageWriteError),
}
