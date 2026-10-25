//! Full-canvas sequence API. This is an explicit encoding policy, separate from
//! the pinned WebPAnimEncoder's candidate selection and frame merging.

use super::*;
use crate::enc::config_enc::{LossyConfig, WebPValidateConfig};
use crate::enc::picture_csp_enc::checked_rgba_geometry;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationEncodingPolicy {
    /// Encode every submitted canvas, including repeats and zero durations.
    #[default]
    FullFrame,
    /// Reserved for the upstream optimizer; currently rejected at construction.
    Optimized,
}

/// Global sequence settings. Canvas dimensions are supplied to
/// [`AnimationEncoder::new`]. Container settings own supplied metadata bytes.
#[derive(Debug, Default)]
pub struct AnimationOptions {
    pub policy: AnimationEncodingPolicy,
    pub container: AnimationContainerOptions,
}

/// Codec settings for one complete display canvas. Lossless quality controls
/// effort; lossy quality controls fidelity. Near-lossless strength below 100
/// can change visible RGB and intermediate alpha, even with `exact` enabled.
/// Lossy encoding supports methods 0–6, including alpha quality and SharpYUV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimationFrameOptions {
    Lossless(LosslessOptions),
    NearLossless {
        strength: i32,
        options: LosslessOptions,
    },
    Lossy(LossyOptions),
}

impl Default for AnimationFrameOptions {
    fn default() -> Self {
        Self::Lossless(LosslessOptions::default())
    }
}

impl AnimationFrameOptions {
    fn validate(&self) -> Result<(), WebPEncodingError> {
        match self {
            Self::Lossless(options) => WebPValidateConfig(options),
            Self::NearLossless { strength, options } => {
                if !(0..=100).contains(strength) {
                    return Err(WebPEncodingError::InvalidNearLosslessStrength);
                }
                WebPValidateConfig(options)
            }
            Self::Lossy(options) => {
                LossyConfig::from_options(options)?;
                Ok(())
            }
        }
    }
}

/// Frame indexes are zero-based. A finish error refers to the final frame.
#[derive(Debug, thiserror::Error)]
pub enum AnimationError {
    #[error("raw canvas axes must be in 1..=16383")]
    InvalidCanvas,
    #[error("optimized animation encoding is not implemented")]
    UnsupportedPolicy,
    #[error("animation needs at least one frame")]
    NoFrames,
    #[error("animation has already finished or failed")]
    InvalidState,
    #[error("frame {frame_index}: timestamp {timestamp_ms} precedes {previous_ms}")]
    InvalidTimestamp {
        frame_index: usize,
        previous_ms: u64,
        timestamp_ms: u64,
    },
    #[error("frame {frame_index}: duration {duration_ms} does not fit 24 bits")]
    InvalidDuration {
        frame_index: usize,
        duration_ms: u64,
    },
    #[error("frame {frame_index}: {source}")]
    Encoding {
        frame_index: usize,
        #[source]
        source: WebPEncodingError,
    },
    #[error("frame {frame_index}: {source}")]
    Container {
        frame_index: usize,
        #[source]
        source: MuxError,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Active,
    Finished,
    Failed,
}

/// Encode immutable full-canvas, straight-alpha RGBA frames as replacements at
/// (0,0), without blending, disposal or merging. This policy matches WebPMux
/// given identical payloads and controls; it does not match WebPAnimEncoder's
/// optimized output. Frame codecs may vary within a sequence.
///
/// Input is borrowed only during `add_rgba`; no raw canvas is retained. Encoded
/// frames remain buffered until `finish`, so memory grows with encoded output.
/// This is not a streaming writer. A single frame becomes a still image and
/// loses playback timing and loop settings, following WebPMux cleanup.
///
/// ```
/// use libwebpop::{AnimationEncoder, AnimationFrameOptions, AnimationOptions};
/// let mut encoder = AnimationEncoder::new(1, 1, AnimationOptions::default())?;
/// encoder.add_rgba(&[255, 0, 0, 255], 4, 500, &AnimationFrameOptions::default())?;
/// encoder.add_rgba(&[0, 0, 255, 128], 4, 600, &AnimationFrameOptions::default())?;
/// let mut webp = Vec::new();
/// encoder.finish(640, &mut webp)?; // Stored durations: 100 ms and 40 ms.
/// # Ok::<(), libwebpop::AnimationError>(())
/// ```
#[derive(Debug)]
pub struct AnimationEncoder {
    container: AnimationContainer,
    last_timestamp_ms: Option<u64>,
    state: State,
}

impl AnimationEncoder {
    pub fn new(width: u32, height: u32, options: AnimationOptions) -> Result<Self, AnimationError> {
        if options.policy != AnimationEncodingPolicy::FullFrame {
            return Err(AnimationError::UnsupportedPolicy);
        }
        if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
            return Err(AnimationError::InvalidCanvas);
        }
        Ok(Self {
            container: AnimationContainer::new(width, height, options.container).map_err(
                |source| AnimationError::Container {
                    frame_index: 0,
                    source,
                },
            )?,
            last_timestamp_ms: None,
            state: State::Active,
        })
    }

    fn check_active(&self) -> Result<(), AnimationError> {
        if self.state != State::Active {
            return Err(AnimationError::InvalidState);
        }
        Ok(())
    }

    fn interval(&self, timestamp_ms: u64) -> Result<Option<u32>, AnimationError> {
        let Some(previous_ms) = self.last_timestamp_ms else {
            return Ok(None);
        };
        let frame_index = self.container.frames.len() - 1;
        let duration_ms =
            timestamp_ms
                .checked_sub(previous_ms)
                .ok_or(AnimationError::InvalidTimestamp {
                    frame_index,
                    previous_ms,
                    timestamp_ms,
                })?;
        if duration_ms >= 1 << 24 {
            return Err(AnimationError::InvalidDuration {
                frame_index,
                duration_ms,
            });
        }
        Ok(Some(duration_ms as u32))
    }

    /// Submit a complete canvas. Stride is positive and at least width * 4;
    /// final-row padding is optional. Source bytes are never changed. The first
    /// timestamp establishes the origin; subsequent timestamps may be equal,
    /// and each interval must fit 0..=0x00ff_ffff ms. No delay is invented.
    ///
    /// Layout, settings and timing errors leave the encoder usable. An encoding
    /// or allocation failure after validation marks it failed.
    pub fn add_rgba(
        &mut self,
        rgba: &[u8],
        stride: usize,
        timestamp_ms: u64,
        options: &AnimationFrameOptions,
    ) -> Result<(), AnimationError> {
        self.check_active()?;
        let duration = self.interval(timestamp_ms)?;
        let frame_index = self.container.frames.len();
        let (width, height) = (self.container.width, self.container.height);
        checked_rgba_geometry(rgba.len(), width, height, stride)
            .map_err(WebPEncodingError::from)
            .and_then(|_| options.validate())
            .map_err(|source| AnimationError::Encoding {
                frame_index,
                source,
            })?;

        self.state = State::Failed;
        let frame = match options {
            AnimationFrameOptions::Lossless(options) => {
                EncodedFrame::encode_lossless_rgba(rgba, width, height, stride, options)
            }
            AnimationFrameOptions::NearLossless { strength, options } => {
                EncodedFrame::encode_near_lossless_rgba(
                    rgba, width, height, stride, *strength, options,
                )
            }
            AnimationFrameOptions::Lossy(options) => {
                EncodedFrame::encode_lossy_rgba(rgba, width, height, stride, options)
            }
        }
        .map_err(|source| AnimationError::Encoding {
            frame_index,
            source,
        })?;
        self.container
            .push_frame(frame, ContainerFrameOptions::default())
            .map_err(|source| AnimationError::Container {
                frame_index,
                source,
            })?;
        if let Some(duration) = duration {
            self.container.frames[frame_index - 1].1.duration_ms = duration;
        }
        self.last_timestamp_ms = Some(timestamp_ms);
        self.state = State::Active;
        Ok(())
    }

    /// Set the final frame's end timestamp and assemble a complete file. Timing
    /// validation errors permit retry; assembly/allocation errors mark the
    /// encoder failed. All errors preserve `output`. Success releases buffered
    /// frame payloads and permanently rejects further adds or finishes.
    /// Unlike the C sequence API, final timing is always explicit.
    pub fn finish(
        &mut self,
        timestamp_ms: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), AnimationError> {
        self.check_active()?;
        let duration = self
            .interval(timestamp_ms)?
            .ok_or(AnimationError::NoFrames)?;
        let frame_index = self.container.frames.len() - 1;
        self.state = State::Failed;
        self.container.frames[frame_index].1.duration_ms = duration;
        self.container
            .assemble(output)
            .map_err(|source| AnimationError::Container {
                frame_index,
                source,
            })?;
        self.container.frames.clear();
        self.state = State::Finished;
        Ok(())
    }
}
