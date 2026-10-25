// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/{config_enc,picture_enc}.c, encoder defaults.
// See COPYING and PATENTS.

use crate::webp::encode::LosslessEncodeError;

/// Check the signed VP8 method before allocation or conversion to an index.
pub(crate) fn validate_vp8_method(method: i32) -> Result<(), LosslessEncodeError> {
    if !(0..=6).contains(&method) {
        return Err(LosslessEncodeError::InvalidMethod);
    }
    Ok(())
}

/// VP8 setup parameters for the default lossy preset.
#[derive(Clone, Debug, PartialEq)]
pub struct VP8EncoderConfig {
    /// Source color quality. Fast analysis truncates this for its threshold.
    pub quality: f32,
    pub method: i32,
    pub sns_strength: i32,
    pub filter_strength: i32,
    pub filter_sharpness: i32,
    pub filter_type: i32,
}

impl Default for VP8EncoderConfig {
    fn default() -> Self {
        Self {
            quality: 75.0,
            method: 4,
            sns_strength: 50,
            filter_strength: 60,
            filter_sharpness: 0,
            filter_type: 1,
        }
    }
}

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

/// Supported lossy settings, using the default VP8 preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LossyOptions {
    /// Color fidelity, finite and in 0..=100; fractional values are retained.
    pub quality: f32,
    /// Encoder effort, 0..=6; default 4. Also controls alpha compression effort.
    pub method: u32,
    /// Alpha fidelity, 0..=100. At 100 alpha is preserved exactly.
    pub alpha_quality: i32,
    /// Use SharpYUV conversion. As upstream does, fall back to ordinary
    /// conversion when either dimension is below 4. Conversion ignores alpha
    /// on the sharp path; transparent-area cleanup still follows conversion.
    pub use_sharp_yuv: bool,
}

impl Default for LossyOptions {
    fn default() -> Self {
        Self {
            quality: 75.0,
            method: 4,
            alpha_quality: 100,
            use_sharp_yuv: false,
        }
    }
}

/// Checked simple lossy configuration. Color quality stays floating point and
/// is independent of VP8L's integer compression-effort setting.
pub(crate) struct LossyConfig {
    pub quality: f32,
    pub alpha_quality: i32,
    pub method: i32,
    pub filter_strength: i32,
    pub exact: bool,
}

impl LossyConfig {
    pub fn from_options(options: &LossyOptions) -> Result<Self, LosslessEncodeError> {
        let method =
            i32::try_from(options.method).map_err(|_| LosslessEncodeError::InvalidMethod)?;
        Self::new(options.quality, options.alpha_quality, method)
    }

    /// EncodeCandidate disables filtering for blended VP8 rectangles. Autofilter
    /// is already disabled by every supported configuration.
    pub fn for_animation(
        options: &LossyOptions,
        use_blending: bool,
    ) -> Result<Self, LosslessEncodeError> {
        let mut config = Self::from_options(options)?;
        if use_blending {
            config.filter_strength = 0;
        }
        Ok(config)
    }

    pub fn new(quality: f32, alpha_quality: i32, method: i32) -> Result<Self, LosslessEncodeError> {
        if !quality.is_finite() || !(0.0..=100.0).contains(&quality) {
            return Err(LosslessEncodeError::InvalidQuality);
        }
        if !(0..=100).contains(&alpha_quality) {
            return Err(LosslessEncodeError::InvalidAlphaQuality);
        }
        validate_vp8_method(method)?;
        Ok(Self {
            quality,
            alpha_quality,
            method,
            filter_strength: VP8EncoderConfig::default().filter_strength,
            exact: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_config_retains_fractional_quality_and_validates_signed_method() {
        for method in 0..=6 {
            let config = LossyConfig::new(98.5, 71, method).unwrap();
            assert_eq!(config.quality.to_bits(), 98.5f32.to_bits());
            assert_eq!(config.alpha_quality, 71);
            assert_eq!(config.method, method);
        }
        for method in [i32::MIN, -1, 7, i32::MAX] {
            assert!(matches!(
                LossyConfig::new(75., 100, method),
                Err(LosslessEncodeError::InvalidMethod)
            ));
        }
        for quality in [-1., 100.01, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
            assert!(matches!(
                LossyConfig::new(quality, 100, 4),
                Err(LosslessEncodeError::InvalidQuality)
            ));
        }
        for alpha in [i32::MIN, -1, 101, i32::MAX] {
            assert!(matches!(
                LossyConfig::new(75., alpha, 4),
                Err(LosslessEncodeError::InvalidAlphaQuality)
            ));
        }
    }
}
