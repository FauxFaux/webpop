use crate::WebPEncodingError;
use crate::encode::{LosslessOptions, LossyOptions, encode_lossy_rgba, encode_near_lossless_rgba};

/// Borrowed pixels to encode. Encoding never modifies the input.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpImage<'a> {
    /// Interleaved red, green, blue, alpha bytes, with positive row stride.
    Rgba {
        /// Pixel bytes, including any padding between rows. Final-row padding
        /// is optional: at least `(height - 1) * stride + width * 4` bytes
        /// must be present.
        pixels: &'a [u8],
        /// Image width in pixels, in `1..=16383`.
        width: u32,
        /// Image height in pixels, in `1..=16383`.
        height: u32,
        /// Distance between row starts in bytes, at least `width * 4`.
        stride: usize,
    },
}

/// Codec, fidelity, and codec-specific settings.
///
/// The default is lossless at effort 70, near-lossless strength 100, and
/// `exact: false`, matching [`crate::WebPEncodeLosslessRGBA`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpQuality {
    /// Encode VP8L, optionally permitting bounded pixel changes.
    Lossless {
        /// Fidelity in `0..=100`. At 100 encoding is lossless; lower values
        /// permit changes to RGB and intermediate alpha. Alpha 0 and 255
        /// remain unchanged.
        near_lossless: u8,
        /// Compression effort in `0..=100`, independent of [`OpConfig::method`].
        /// This maps to libwebp's lossless quality setting, not pixel fidelity.
        effort: u8,
        /// Preserve RGB beneath fully transparent pixels and disable predictor
        /// quantization. Near-lossless preprocessing can still change pixels
        /// when `near_lossless < 100`; this does not make that encoding exact.
        exact: bool,
    },
    /// Encode lossy VP8 color with independently controlled alpha fidelity.
    Lossy {
        /// Finite color quality in `0..=100`; fractional values are retained.
        /// Quality 100 still uses YUV420 and quantization and can change RGB.
        quality: f32,
        /// Alpha quality in `0..=100`. At 100 alpha is preserved exactly;
        /// lower values can quantize intermediate transparency.
        alpha_quality: u8,
        /// Use SharpYUV color conversion. Falls back to ordinary conversion
        /// when either image dimension is below 4, following libwebp.
        sharp_yuv: bool,
    },
}

impl Default for OpQuality {
    fn default() -> Self {
        Self::Lossless {
            near_lossless: 100,
            effort: 70,
            exact: false,
        }
    }
}

/// Encoding configuration, validated by [`encode()`] and [`crate::OpAnimEncoder::add`].
///
/// Construct with [`Self::new`] or [`Self::default`], then set public fields.
/// The default matches [`crate::WebPEncodeLosslessRGBA`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpConfig {
    /// Encoder method in `0..=6`, default 4. Controls compression effort for
    /// both codecs, including alpha compression for lossy images.
    pub method: u8,
    /// Codec, fidelity, and codec-specific settings.
    pub quality: OpQuality,
}

impl OpConfig {
    /// Select a codec and quality settings, using encoder method 4.
    /// Settings are validated when passed to [`encode()`].
    pub fn new(quality: OpQuality) -> Self {
        Self { method: 4, quality }
    }
}

impl Default for OpConfig {
    fn default() -> Self {
        Self::new(OpQuality::default())
    }
}

/// Encode borrowed pixels into an owned, complete WebP file.
///
/// Input geometry and settings are checked before pixel conversion or encoder
/// allocation. The input is never modified. Errors implement
/// [`std::error::Error`]; their variants and exact messages are implementation
/// details.
///
/// ```
/// use libwebpop::{OpConfig, OpImage, OpQuality, encode};
///
/// let pixels = [255, 0, 0, 128];
/// let mut config = OpConfig::new(OpQuality::Lossy {
///     quality: 98.5,
///     alpha_quality: 70,
///     sharp_yuv: true,
/// });
/// config.method = 6;
/// let webp = encode(
///     OpImage::Rgba { pixels: &pixels, width: 1, height: 1, stride: 4 },
///     &config,
/// )?;
/// assert_eq!(&webp[..4], b"RIFF");
/// # Ok::<(), libwebpop::WebPEncodingError>(())
/// ```
pub fn encode(image: OpImage<'_>, config: &OpConfig) -> Result<Vec<u8>, WebPEncodingError> {
    let OpImage::Rgba {
        pixels,
        width,
        height,
        stride,
    } = image;
    let mut output = Vec::new();
    match config.quality {
        OpQuality::Lossless {
            near_lossless,
            effort,
            exact,
        } => encode_near_lossless_rgba(
            pixels,
            width,
            height,
            stride,
            i32::from(near_lossless),
            &LosslessOptions {
                quality: u32::from(effort),
                method: u32::from(config.method),
                exact,
            },
            &mut output,
        )?,
        OpQuality::Lossy {
            quality,
            alpha_quality,
            sharp_yuv,
        } => encode_lossy_rgba(
            pixels,
            width,
            height,
            stride,
            &LossyOptions {
                quality,
                method: u32::from(config.method),
                alpha_quality: i32::from(alpha_quality),
                use_sharp_yuv: sharp_yuv,
            },
            &mut output,
        )?,
    }
    Ok(output)
}
