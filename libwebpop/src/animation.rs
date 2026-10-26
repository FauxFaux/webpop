//! Supported duration-based facade for the buffered sequence encoder.

use crate::encode::{LosslessOptions, LossyOptions};
use crate::mux::{
    AnimationContainerOptions, AnimationEncoder, AnimationEncodingPolicy, AnimationError,
    AnimationFrameOptions, AnimationKeyframePolicy, AnimationOptimizerOptions, AnimationOptions,
    BackgroundColor,
};
use crate::{OpConfig, OpImage, OpQuality};

/// How complete display canvases are encoded.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpAnimPolicy {
    /// Select rectangles, blending, disposal and keyframes, and merge repeats.
    #[default]
    Optimized,
    /// Encode every input as a full-canvas replacement, including repeats.
    FullFrame,
}

/// Keyframe insertion policy. Intervals count accepted output candidates,
/// excluding skipped inputs. The optimizer adjusts bounded intervals using
/// upstream's half-window rule and caps its deferred window at 31 slots.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpAnimKeyframes {
    #[default]
    Disabled,
    AllFrames,
    /// Both intervals must fit i32 and `max_interval` must be at least 2.
    Bounded {
        min_interval: u32,
        max_interval: u32,
    },
}

/// Global animation options. Construct with [`Self::default`], then set fields.
/// Dimensions are supplied to [`OpAnimEncoder::new`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpAnimOptions {
    /// Optimized by default. FullFrame requires default optimizer controls.
    pub policy: OpAnimPolicy,
    /// Total plays; zero means infinite looping. Default: zero.
    pub loop_count: u16,
    /// RGBA background hint, default opaque white. This does not control the
    /// optimizer's internal clearing, which uses transparent black.
    pub background: [u8; 4],
    /// Spend additional effort reducing size; disables keyframe insertion.
    pub minimize_size: bool,
    /// Disabled by default.
    pub keyframes: OpAnimKeyframes,
    /// Consider either codec for each frame, potentially changing pixels even
    /// with a lossless primary config. Default: false.
    ///
    /// Lossless configs share their integer effort with reversed VP8 trials,
    /// which use alpha quality 100 and SharpYUV off. Lossy configs share their
    /// fractional quality with VP8L, which truncates effort and uses strength
    /// 100 and exact false. Primary selection affects geometry and need not win.
    pub allow_mixed: bool,
    /// Supplied ICC bytes. None omits the chunk; Some(empty) preserves it.
    pub icc_profile: Option<Vec<u8>>,
    /// Supplied EXIF bytes, with the same presence rules as `icc_profile`.
    pub exif: Option<Vec<u8>>,
    /// Supplied XMP bytes, with the same presence rules as `icc_profile`.
    pub xmp: Option<Vec<u8>>,
}

impl Default for OpAnimOptions {
    fn default() -> Self {
        Self {
            policy: OpAnimPolicy::Optimized,
            loop_count: 0,
            background: [255; 4],
            minimize_size: false,
            keyframes: OpAnimKeyframes::Disabled,
            allow_mixed: false,
            icc_profile: None,
            exif: None,
            xmp: None,
        }
    }
}

/// Animation encoding error. During 0.1.x, compatibility covers using this type
/// as a [`std::error::Error`] with [`std::fmt::Display`]. Its representation,
/// sources and exact messages are implementation details. Frame indexes in
/// diagnostics are zero-based and include successfully submitted repeats.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct OpAnimError(#[from] AnimationApiError);

#[derive(Debug, thiserror::Error)]
enum AnimationApiError {
    #[error(transparent)]
    Backend(#[from] AnimationError),
    #[error(
        "frame {frame_index}: dimensions {width}x{height} differ from canvas {canvas_width}x{canvas_height}"
    )]
    Dimensions {
        frame_index: usize,
        width: u32,
        height: u32,
        canvas_width: u32,
        canvas_height: u32,
    },
    #[error("frame {frame_index}: duration {duration_ms} does not fit 24 bits")]
    Duration {
        frame_index: usize,
        duration_ms: u32,
    },
    #[error("animation timeline or submitted frame count overflowed")]
    Overflow,
}

impl From<AnimationError> for OpAnimError {
    fn from(source: AnimationError) -> Self {
        Self(AnimationApiError::Backend(source))
    }
}

/// Encode complete straight-alpha RGBA display canvases with explicit durations.
/// Pixels are borrowed only during [`Self::add`] and are never modified.
///
/// Like upstream, this buffers all encoded frames, then allocates the assembled
/// file at finish. It does not stream output. The WebP file limit is 4 GiB minus
/// 2 bytes. Validation errors permit retry; later allocation or encoding errors
/// permanently fail the encoder. Dropping the encoder releases its buffers.
///
/// Optimized output follows pinned WebPAnimEncoder except for its omitted
/// decode/re-encode optimization of a lone cropped frame. A lone full-canvas
/// output frame becomes a still image and loses its timing and loop settings.
/// Near-lossless and lossy can change pixels. Codec exact cannot preserve
/// invisible RGB removed by animation blending.
///
/// ```
/// use libwebpop::{OpAnimEncoder, OpAnimOptions, OpConfig, OpImage};
///
/// let mut encoder = OpAnimEncoder::new(1, 1, OpAnimOptions::default())?;
/// for (pixels, duration) in [([255, 0, 0, 255], 100), ([0, 0, 255, 255], 40)] {
///     encoder.add(
///         OpImage::Rgba { pixels: &pixels, width: 1, height: 1, stride: 4 },
///         duration,
///         &OpConfig::default(),
///     )?;
/// }
/// let webp = encoder.finish()?;
/// assert_eq!(&webp[..4], b"RIFF");
/// # Ok::<(), libwebpop::OpAnimError>(())
/// ```
#[derive(Debug)]
pub struct OpAnimEncoder {
    encoder: AnimationEncoder,
    width: u32,
    height: u32,
    timestamp_ms: u64,
    submitted_frames: usize,
}

impl OpAnimEncoder {
    /// Create an encoder for a canvas with each axis in `1..=16383`.
    pub fn new(width: u32, height: u32, options: OpAnimOptions) -> Result<Self, OpAnimError> {
        let keyframes = match options.keyframes {
            OpAnimKeyframes::Disabled => AnimationKeyframePolicy::Disabled,
            OpAnimKeyframes::AllFrames => AnimationKeyframePolicy::AllFrames,
            OpAnimKeyframes::Bounded {
                min_interval,
                max_interval,
            } => AnimationKeyframePolicy::Bounded {
                min_interval,
                max_interval,
            },
        };
        let [red, green, blue, alpha] = options.background;
        let encoder = AnimationEncoder::new(
            width,
            height,
            AnimationOptions {
                policy: match options.policy {
                    OpAnimPolicy::Optimized => AnimationEncodingPolicy::Optimized,
                    OpAnimPolicy::FullFrame => AnimationEncodingPolicy::FullFrame,
                },
                optimizer: AnimationOptimizerOptions {
                    minimize_size: options.minimize_size,
                    allow_mixed: options.allow_mixed,
                    keyframes,
                },
                container: AnimationContainerOptions {
                    loop_count: options.loop_count,
                    background: BackgroundColor {
                        red,
                        green,
                        blue,
                        alpha,
                    },
                    icc_profile: options.icc_profile,
                    exif: options.exif,
                    xmp: options.xmp,
                },
            },
        )?;
        Ok(Self {
            encoder,
            width,
            height,
            timestamp_ms: 0,
            submitted_frames: 0,
        })
    }

    /// Add a complete canvas matching the dimensions passed to [`Self::new`].
    /// Duration is in `0..=0x00ff_ffff` milliseconds, including zero; no viewer
    /// minimum delay is applied. Stride and final-row padding follow [`OpImage`].
    /// Each input may use different [`OpConfig`] settings. Invalid input does
    /// not advance timing or the submitted frame index, including on repeats.
    pub fn add(
        &mut self,
        image: OpImage<'_>,
        duration_ms: u32,
        config: &OpConfig,
    ) -> Result<(), OpAnimError> {
        let frame_index = self.submitted_frames;
        if duration_ms >= 1 << 24 {
            return Err(AnimationApiError::Duration {
                frame_index,
                duration_ms,
            }
            .into());
        }
        let end_ms = self
            .timestamp_ms
            .checked_add(u64::from(duration_ms))
            .ok_or(AnimationApiError::Overflow)?;
        let submitted_frames = frame_index
            .checked_add(1)
            .ok_or(AnimationApiError::Overflow)?;
        let OpImage::Rgba {
            pixels,
            width,
            height,
            stride,
        } = image;
        if (width, height) != (self.width, self.height) {
            return Err(AnimationApiError::Dimensions {
                frame_index,
                width,
                height,
                canvas_width: self.width,
                canvas_height: self.height,
            }
            .into());
        }
        let options = match config.quality {
            OpQuality::Lossless {
                near_lossless,
                effort,
                exact,
            } => AnimationFrameOptions::NearLossless {
                strength: i32::from(near_lossless),
                options: LosslessOptions {
                    quality: u32::from(effort),
                    method: u32::from(config.method),
                    exact,
                },
            },
            OpQuality::Lossy {
                quality,
                alpha_quality,
                sharp_yuv,
            } => AnimationFrameOptions::Lossy(LossyOptions {
                quality,
                method: u32::from(config.method),
                alpha_quality: i32::from(alpha_quality),
                use_sharp_yuv: sharp_yuv,
            }),
        };
        self.encoder
            .add_rgba(pixels, stride, self.timestamp_ms, &options)?;
        self.timestamp_ms = end_ms;
        self.submitted_frames = submitted_frames;
        Ok(())
    }

    /// Consume the encoder and assemble a complete WebP file. At least one
    /// successful add is required. The last add supplies the final duration.
    pub fn finish(mut self) -> Result<Vec<u8>, OpAnimError> {
        let mut output = Vec::new();
        self.encoder.finish(self.timestamp_ms, &mut output)?;
        Ok(output)
    }
}
