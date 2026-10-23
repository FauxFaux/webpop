// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/{config_enc,picture_enc}.c (lossless defaults).
// See COPYING and PATENTS.

use crate::webp::encode::LosslessEncodeError;

/// Lossless compression effort and transparent-pixel policy.
/// Near-lossless strength is 100, the image hint is default, and encoding
/// executes synchronously. Other upstream configuration fields are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LosslessOptions {
    /// Compression effort, 0..=100. Pixel quality remains lossless.
    pub quality: u32,
    /// Effort, 0..=6. Zero uses the low-effort path; higher values refine
    /// transforms, histogram sampling and reference/cache trials.
    pub method: u32,
    /// Preserve RGB beneath alpha zero as well as all visible channels.
    pub exact: bool,
}

impl Default for LosslessOptions {
    fn default() -> Self {
        Self {
            quality: 70,
            method: 4,
            exact: false,
        }
    }
}

/// Validate the exposed lossless subset of upstream WebPConfig.
pub(crate) fn WebPValidateConfig(config: &LosslessOptions) -> Result<(), LosslessEncodeError> {
    if config.quality > 100 {
        return Err(LosslessEncodeError::InvalidQuality);
    }
    if config.method > 6 {
        return Err(LosslessEncodeError::InvalidMethod);
    }
    Ok(())
}
