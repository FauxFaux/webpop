//! Checked full-canvas RGBA sequences, using explicit full-frame or optimized
//! encoding policies. Both own their encoded output until final assembly.

use super::*;
use crate::enc::config_enc::{LossyConfig, WebPValidateConfig};
use crate::enc::picture_csp_enc::checked_rgba_geometry;
use anim_encode::{AnimationCache, AnimationCacheOptions, MixedEncodingConfig};

pub use anim_encode::{
    AnimationCandidateCodec as AnimationFrameCodec, KeyframePolicy as AnimationKeyframePolicy,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationEncodingPolicy {
    /// Encode every submitted canvas, including repeats and zero durations.
    #[default]
    FullFrame,
    /// Select rectangles, blending/disposal, repeats and keyframes using the
    /// pinned optimizer, before its decode-dependent single-frame reconstruction.
    Optimized,
}

/// Global sequence settings. Canvas dimensions are supplied to
/// [`AnimationEncoder::new`]. Container settings own supplied metadata bytes.
#[derive(Debug, Default)]
pub struct AnimationOptions {
    pub policy: AnimationEncodingPolicy,
    pub container: AnimationContainerOptions,
    /// Requires Optimized when any setting differs from its default.
    pub optimizer: AnimationOptimizerOptions,
}

/// Optimizer controls, with insertion/mixed/minimize-size disabled by default.
/// Keyframe intervals are checked and normalized using the pinned ordering;
/// minimize-size disables insertion. Output remains buffered in memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationOptimizerOptions {
    pub minimize_size: bool,
    pub allow_mixed: bool,
    pub keyframes: AnimationKeyframePolicy,
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
    /// Requires Optimized and allow_mixed. Quality/method apply to both codecs;
    /// VP8 retains f32 quality, while VP8L truncates effort. Alpha/SharpYUV apply
    /// to VP8, including reversed trials. Primary affects geometry, not trial
    /// order or the winner. Both codecs and dormant fields are always validated.
    Mixed {
        primary: AnimationFrameCodec,
        options: LossyOptions,
        near_lossless: i32,
        exact: bool,
    },
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
            Self::Mixed {
                primary,
                options,
                near_lossless,
                exact,
            } => {
                MixedEncodingConfig {
                    primary: *primary,
                    codec: *options,
                    near_lossless: *near_lossless,
                    exact: *exact,
                }
                .validate()?;
                Ok(())
            }
        }
    }

    fn optimized_config(&self) -> Result<MixedEncodingConfig, WebPEncodingError> {
        self.validate()?;
        let lossless = |options: &LosslessOptions, strength| MixedEncodingConfig {
            primary: AnimationFrameCodec::Vp8l,
            codec: LossyOptions {
                quality: options.quality as f32,
                method: options.method,
                ..Default::default()
            },
            near_lossless: strength,
            exact: options.exact,
        };
        Ok(match self {
            Self::Lossless(options) => lossless(options, 100),
            Self::NearLossless { strength, options } => lossless(options, *strength),
            Self::Lossy(options) => MixedEncodingConfig {
                primary: AnimationFrameCodec::Vp8,
                codec: *options,
                near_lossless: 100,
                exact: false,
            },
            Self::Mixed {
                primary,
                options,
                near_lossless,
                exact,
            } => MixedEncodingConfig {
                primary: *primary,
                codec: *options,
                near_lossless: *near_lossless,
                exact: *exact,
            },
        })
    }
}

/// Frame indexes are zero-based. A finish error refers to the final frame.
#[derive(Debug, thiserror::Error)]
pub enum AnimationError {
    #[error("raw canvas axes must be in 1..=16383")]
    InvalidCanvas,
    #[error("nondefault optimizer settings require the Optimized policy")]
    UnsupportedOptimizerOptions,
    #[error("frame {frame_index}: mixed settings require Optimized with allow_mixed")]
    UnsupportedFrameOptions { frame_index: usize },
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
    Candidate {
        frame_index: usize,
        #[source]
        source: anim_encode::AnimationCandidateError,
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

/// Encode immutable straight-alpha RGBA canvases with explicit final timing.
/// FullFrame (the default) writes every input as a replacement and matches
/// WebPMux. Optimized owns original/carryover canvases, merges unchanged inputs,
/// selects rectangles/blending/disposal/codecs and supports deferred keyframes.
/// Both policies buffer encoded output; flushing does not stream or bound it.
///
/// Optimized matches pinned WebPAnimEncoder before OptimizeSingleFrame. A lone
/// cropped ANMF, including one produced by merged inputs, remains animated;
/// the pin may decode/re-encode it as a smaller full-canvas still. Full-canvas
/// single-frame mux cleanup is supported and loses timing/loop settings.
///
/// Existing frame variants map VP8L effort/method/exact or VP8 fidelity/method/
/// alpha/SharpYUV. Dormant VP8 defaults for VP8L are alpha 100/SharpYUV off;
/// dormant VP8L defaults for VP8 are strength 100/exact false. Mixed settings
/// control these explicitly. Quality has different meanings across codecs.
/// Near-lossless may change RGB/intermediate alpha. Original words drive skips;
/// later strength 100 does not repair carried pixels. Codec exact cannot retain
/// invisible RGB discarded by optimizer blending.
///
/// ```
/// use libwebpop::internal::{AnimationEncoder, AnimationEncodingPolicy, AnimationOptions,
///     AnimationOptimizerOptions, AnimationKeyframePolicy, AnimationFrameOptions};
/// let mut encoder = AnimationEncoder::new(1, 1, AnimationOptions {
///     policy: AnimationEncodingPolicy::Optimized,
///     optimizer: AnimationOptimizerOptions {
///         keyframes: AnimationKeyframePolicy::Bounded { min_interval: 2, max_interval: 5 },
///         ..Default::default()
///     },
///     ..Default::default()
/// })?;
/// encoder.add_rgba(&[255, 0, 0, 255], 4, 500, &AnimationFrameOptions::default())?;
/// encoder.add_rgba(&[0, 0, 255, 128], 4, 600, &AnimationFrameOptions::default())?;
/// let mut output = Vec::new();
/// encoder.finish(640, &mut output)?;
/// # Ok::<(), libwebpop::internal::AnimationError>(())
/// ```
#[derive(Debug)]
pub struct AnimationEncoder {
    backend: Backend,
}

#[derive(Debug)]
enum Backend {
    FullFrame(FullFrameEncoder),
    Optimized {
        cache: AnimationCache,
        allow_mixed: bool,
    },
}

impl AnimationEncoder {
    pub fn new(width: u32, height: u32, options: AnimationOptions) -> Result<Self, AnimationError> {
        let backend = match options.policy {
            AnimationEncodingPolicy::FullFrame => {
                if options.optimizer != AnimationOptimizerOptions::default() {
                    return Err(AnimationError::UnsupportedOptimizerOptions);
                }
                Backend::FullFrame(FullFrameEncoder::new(width, height, options)?)
            }
            AnimationEncodingPolicy::Optimized => Backend::Optimized {
                allow_mixed: options.optimizer.allow_mixed,
                cache: AnimationCache::new_with_keyframes(
                    width,
                    height,
                    AnimationCacheOptions {
                        minimize_size: options.optimizer.minimize_size,
                        allow_mixed: options.optimizer.allow_mixed,
                        container: options.container,
                    },
                    options.optimizer.keyframes,
                )?,
            },
        };
        Ok(Self { backend })
    }

    /// Submit one complete display canvas. Stride is positive and at least
    /// width*4; final-row padding is optional and input is never changed or
    /// borrowed after this call. Timestamps are nondecreasing u64 values with
    /// arbitrary origin and intervals in 0..=0x00ff_ffff milliseconds.
    /// Validation failures allow retry, including on skipped inputs. Allocation
    /// or encoding failures after validation permanently fail this encoder.
    /// Error indexes count every submitted input, including skips.
    pub fn add_rgba(
        &mut self,
        rgba: &[u8],
        stride: usize,
        timestamp_ms: u64,
        options: &AnimationFrameOptions,
    ) -> Result<(), AnimationError> {
        match &mut self.backend {
            Backend::FullFrame(encoder) => encoder.add_rgba(rgba, stride, timestamp_ms, options),
            Backend::Optimized { cache, allow_mixed } => {
                cache.check_active()?;
                let frame_index = cache.status().submitted_frames;
                if matches!(options, AnimationFrameOptions::Mixed { .. }) && !*allow_mixed {
                    return Err(AnimationError::UnsupportedFrameOptions { frame_index });
                }
                let codec =
                    options
                        .optimized_config()
                        .map_err(|source| AnimationError::Encoding {
                            frame_index,
                            source,
                        })?;
                cache.add_rgba(rgba, stride, timestamp_ms, &codec)
            }
        }
    }

    /// Supply the final input's explicit end timestamp and assemble. Empty or
    /// invalid timing allows retry; later failures are terminal. Every error
    /// preserves output. Success releases buffers and permanently closes the
    /// encoder. All output remains buffered until this call.
    pub fn finish(
        &mut self,
        timestamp_ms: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), AnimationError> {
        match &mut self.backend {
            Backend::FullFrame(encoder) => encoder.finish(timestamp_ms, output),
            Backend::Optimized { cache, .. } => cache.finish(timestamp_ms, output),
        }
    }
}

#[derive(Debug)]
struct FullFrameEncoder {
    container: AnimationContainer,
    last_timestamp_ms: Option<u64>,
    state: State,
}

impl FullFrameEncoder {
    pub fn new(width: u32, height: u32, options: AnimationOptions) -> Result<Self, AnimationError> {
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
        if matches!(options, AnimationFrameOptions::Mixed { .. }) {
            return Err(AnimationError::UnsupportedFrameOptions { frame_index });
        }
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
            AnimationFrameOptions::Mixed { .. } => unreachable!("validated full-frame settings"),
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
