// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/{config_enc,picture_enc}.c (lossless defaults).
// See COPYING and PATENTS.

use crate::webp::encode::LosslessEncodeError;

/// Lossless compression effort and transparent-pixel policy.
/// With `encode_lossless_rgba`, near-lossless strength is 100. These options can
/// also control effort and cleanup in `encode_near_lossless_rgba`, whose strength
/// is supplied separately. The image hint is default and encoding is synchronous.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LosslessOptions {
    /// Compression effort, 0..=100. Near-lossless strength is supplied separately.
    pub quality: u32,
    /// Effort, 0..=6. Zero uses the low-effort path; higher values refine
    /// transforms, histogram sampling and reference/cache trials.
    pub method: u32,
    /// Disable transparent RGB cleanup. Lossless encoding preserves all channels;
    /// near-lossless encoding also bypasses predictor quantization, but standalone
    /// preprocessing may still change pixels.
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

/// Private VP8L configuration. The lossless API selects strength 100; the custom
/// near-lossless wrapper keeps the simple API's effort defaults, while the
/// configurable entry point supplies effort and strength independently.
pub(crate) struct EncoderConfig {
    pub options: LosslessOptions,
    pub near_lossless: u32,
}
