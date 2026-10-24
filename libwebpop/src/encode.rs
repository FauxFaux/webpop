//! Checked public RGBA entry points for the VP8 and VP8L encoders.

pub use crate::enc::config_enc::{LosslessOptions, LossyOptions};
pub use crate::webp::encode::LosslessEncodeError;

use crate::WebPEncodingError;
use crate::enc::config_enc::{EncoderConfig, LossyConfig, WebPValidateConfig};
use crate::enc::picture_csp_enc::{
    WebPPictureImportRGBA, checked_rgba_geometry, import_rgba_to_yuva, import_rgba_to_yuva_sharp,
};
use crate::enc::webp_enc::{WebPEncode, WebPEncodeLossy};

/// Encode positive-stride RGBA as a complete lossy still-image WebP using
/// the pinned libwebp default preset (method 4). Color quality must be finite
/// and in 0..=100; fractional values are preserved. Quality 100 still uses
/// YUV420 and quantization, so RGB can change. Alpha is preserved exactly.
///
/// Dimensions must be in 1..=16383 and stride at least width * 4. The buffer
/// must contain (height - 1) * stride + width * 4 bytes; final-row padding is
/// optional. Inputs are validated before allocation and never modified.
///
/// On success, `output` contains one complete WebP file, replacing any previous
/// contents and reusing its capacity. On error, its contents are unchanged.
///
/// Upstream simple API (`upstream/doc/api.md`):
/// ```c
/// size_t WebPEncodeRGBA(const uint8_t* rgba, int width, int height, int stride,
///                       float quality_factor, uint8_t** output);
/// ```
/// Rust writes the encoded bytes into the caller-owned vector:
/// ```
/// use libwebpop::WebPEncodeRGBA;
///
/// let rgba = [255, 0, 0, 255]; // One opaque red pixel; stride is in bytes.
/// let mut webp = Vec::new();
/// WebPEncodeRGBA(&rgba, 1, 1, 4, 75.0, &mut webp)?;
/// # Ok::<(), libwebpop::WebPEncodingError>(())
/// ```
pub fn WebPEncodeRGBA(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    quality: f32,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError> {
    WebPEncodeRGBAWithAlphaQuality(rgba, width, height, stride, quality, 100, output)
}

/// Encode lossy WebP with independent alpha quality in 0..=100. At 100,
/// output is byte-identical to [`WebPEncodeRGBA`]. Lower values quantize alpha
/// and can change intermediate transparency. Alpha quality has no effect on
/// opaque images. Color quality, geometry and input guarantees follow
/// [`WebPEncodeRGBA`]. This extends the simple API with quad-image's custom
/// alpha-quality wrapper; the default preset and alpha compression are fixed.
/// Output replacement, capacity reuse and error guarantees follow [`WebPEncodeRGBA`].
pub fn WebPEncodeRGBAWithAlphaQuality(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    quality: f32,
    alpha_quality: i32,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError> {
    encode_lossy_rgba(
        rgba,
        width,
        height,
        stride,
        &LossyOptions {
            quality,
            alpha_quality,
            use_sharp_yuv: false,
        },
        output,
    )
}

/// Encode RGBA as method-4 VP8 with independent color/alpha quality and an
/// optional SharpYUV importer. Default options match [`WebPEncodeRGBA`] at 75.
/// Geometry, input immutability and output replacement/error guarantees follow
/// [`WebPEncodeRGBA`]. Other advanced VP8 configuration remains deferred.
///
/// SharpYUV uses the pinned sRGB/WebP matrix conversion and falls back to the
/// ordinary alpha-aware importer when width or height is below 4. Larger
/// images convert RGB without alpha weighting, then copy alpha exactly before
/// the encoder's usual transparent-area cleanup and alpha compression.
///
/// ```
/// use libwebpop::{LossyOptions, encode_lossy_rgba};
/// let rgba = [255u8; 4 * 4 * 4];
/// let mut output = Vec::new();
/// encode_lossy_rgba(&rgba, 4, 4, 16,
///     &LossyOptions { use_sharp_yuv: true, ..Default::default() }, &mut output)?;
/// # Ok::<(), libwebpop::WebPEncodingError>(())
/// ```
pub fn encode_lossy_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    options: &LossyOptions,
    output: &mut Vec<u8>,
) -> Result<(), WebPEncodingError> {
    let config = LossyConfig::new(options.quality, options.alpha_quality, 4)?;
    let mut picture = if options.use_sharp_yuv {
        import_rgba_to_yuva_sharp(rgba, width, height, stride)?
    } else {
        import_rgba_to_yuva(rgba, width, height, stride)?
    };
    WebPEncodeLossy(&config, &mut picture, output)
}

/// Encode positive-stride RGBA as a complete lossless still-image WebP using
/// the pinned libwebp simple API's defaults (quality 70, method 4, exact false).
/// Dimensions must be in 1..=16383. Final-row padding is optional; the source
/// is never modified. RGB beneath alpha zero may change.
/// Output replacement, capacity reuse and error guarantees follow [`WebPEncodeRGBA`].
///
/// Upstream lossless API (`upstream/doc/api.md`):
/// ```c
/// size_t WebPEncodeLosslessRGBA(const uint8_t* rgba, int width, int height,
///                               int stride, uint8_t** output);
/// ```
/// Rust:
/// ```
/// use libwebpop::WebPEncodeLosslessRGBA;
///
/// let rgba = [255, 0, 0, 255];
/// let mut webp = Vec::new();
/// WebPEncodeLosslessRGBA(&rgba, 1, 1, 4, &mut webp)?;
/// # Ok::<(), libwebpop::WebPEncodingError>(())
/// ```
pub fn WebPEncodeLosslessRGBA(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    encode_lossless_rgba(
        rgba,
        width,
        height,
        stride,
        &LosslessOptions::default(),
        output,
    )
}

/// Encode positive-stride RGBA as a complete lossless still-image WebP.
/// Dimensions must be in 1..=16383; final-row padding is optional. Input and
/// options are validated before allocation, and the source is never modified.
/// RGB beneath alpha zero may change unless `options.exact` is set.
///
/// Uses the pinned encoder's entropy analysis and configuration trial loop.
/// Default options match [`WebPEncodeLosslessRGBA`]; other options retain
/// near_lossless=100, the default image hint and synchronous execution.
/// Output replacement, capacity reuse and error guarantees follow [`WebPEncodeRGBA`].
///
/// `upstream/doc/api.md` recommends `WebPEncode()` with
/// `WebPConfig::exact = 1` to preserve RGB beneath fully transparent pixels.
/// The equivalent Rust configuration is:
/// ```
/// use libwebpop::{LosslessOptions, encode_lossless_rgba};
///
/// let rgba = [255, 0, 0, 0]; // Preserve this invisible red pixel too.
/// let options = LosslessOptions { exact: true, ..Default::default() };
/// let mut webp = Vec::new();
/// encode_lossless_rgba(&rgba, 1, 1, 4, &options, &mut webp)?;
/// # Ok::<(), libwebpop::WebPEncodingError>(())
/// ```
pub fn encode_lossless_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    options: &LosslessOptions,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    encode_near_lossless_rgba(rgba, width, height, stride, 100, options, output)
}

/// Encode positive-stride RGBA as a near-lossless still-image WebP. Strength
/// must be in 0..=100; lower values permit stronger changes to RGB and
/// intermediate alpha. Alpha 0 and 255 stay unchanged. Strength 100 is byte
/// identical to [`WebPEncodeLosslessRGBA`]. Uses quality 70, method 4 and
/// transparent RGB cleanup. Input is never modified; geometry follows the
/// lossless API, including optional final-row padding.
/// Output replacement, capacity reuse and error guarantees follow [`WebPEncodeRGBA`].
pub fn WebPEncodeRGBAWithNearLossless(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    near_lossless: i32,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    encode_near_lossless_rgba(
        rgba,
        width,
        height,
        stride,
        near_lossless,
        &LosslessOptions::default(),
        output,
    )
}

/// Encode positive-stride RGBA as VP8L with independent near-lossless strength
/// and compression options. `near_lossless` must be in 0..=100; `quality` and
/// `method` retain their compression-effort meanings. At strength 100 this is
/// byte-identical to [`encode_lossless_rgba`] with the same options.
///
/// Lower strengths can change visible RGB and intermediate alpha, while alpha
/// endpoints stay exact. `options.exact` disables transparent RGB cleanup and
/// predictor quantization; standalone preprocessing can still change pixels.
/// It does not guarantee exact alpha or RGB below strength 100.
/// Geometry, minimum input length and unchanged-input guarantees follow the
/// lossless API. Inputs and settings are validated before allocation.
/// Output replacement, capacity reuse and error guarantees follow [`WebPEncodeRGBA`].
pub fn encode_near_lossless_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    near_lossless: i32,
    options: &LosslessOptions,
    output: &mut Vec<u8>,
) -> Result<(), LosslessEncodeError> {
    if !(0..=100).contains(&near_lossless) {
        return Err(LosslessEncodeError::InvalidNearLosslessStrength);
    }
    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    WebPValidateConfig(options)?;
    let config = EncoderConfig {
        options: *options,
        near_lossless: near_lossless as u32,
    };
    let mut picture = WebPPictureImportRGBA(rgba, width, height, stride)?;
    WebPEncode(&config, &mut picture, output)
}
