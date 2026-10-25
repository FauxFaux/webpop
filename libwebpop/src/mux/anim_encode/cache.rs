// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! Shared CacheFrame/FlushFrames with keyframe insertion disabled. This is
//! separate from the public RGBA policy until the remaining optimizer gates land.

use super::*;
use crate::enc::config_enc::WebPValidateConfig;
use crate::mux::{
    AnimationContainer, AnimationContainerOptions, AnimationError, ContainerFrameOptions,
    EncodedFrame, FrameBlend,
};
use crate::{LosslessOptions, LossyOptions};

const MAX_DURATION: u32 = 1 << 24;

#[derive(Debug, Default)]
pub struct LosslessAnimationCacheOptions {
    pub minimize_size: bool,
    pub container: AnimationContainerOptions,
}

/// Logical cache counters after an add or finish. Emitted payloads remain
/// buffered in the output container; flushing does not stream bytes to a sink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LosslessCacheStatus {
    pub submitted_frames: usize,
    pub emitted_frames: usize,
    pub cached_frames: usize,
    pub count_since_key_frame: usize,
    pub previous_rect: FrameRectangle,
    pub pending_duration_ms: Option<u32>,
}

/// Low-level VP8L animation cache using checked ARGB word views. Only the first
/// accepted frame is a keyframe. `add_argb` selects strength 100;
/// `add_near_lossless_argb` accepts per-frame strength with the still codec's
/// RGB/intermediate-alpha caveat.
/// Repeats merge, changed frames can have zero duration, and overflow inserts the
/// pin's fixed transparent hold. Methods 0–6 and codec-level `exact` work;
/// lossy, mixed mode and keyframe intervals are not accepted here.
/// Switching to strength 100 does not re-encode unchanged pixels carried from
/// earlier near-lossless inputs. A wholly lossless sequence uses 100 throughout.
///
/// Owns two raw canvases and buffered encoded output. Input is borrowed only
/// during `add_argb`. Explicit final timing, validation recovery and terminal
/// failures follow AnimationEncoder. A lone cropped frame retains ANMF: the
/// decode-dependent OptimizeSingleFrame step is not performed. This does not
/// enable AnimationEncodingPolicy::Optimized or promise complete pinned parity.
///
/// ```
/// use libwebpop::mux::anim_encode::{ArgbView, LosslessAnimationCache};
/// let mut cache = LosslessAnimationCache::new(1, 1, Default::default())?;
/// for (timestamp, pixel) in [(500, 0xffff0000), (600, 0xff0000ff)] {
///     cache.add_argb(ArgbView::new(&[pixel], 1, 1, 1)?, timestamp, &Default::default())?;
/// }
/// let mut webp = Vec::new();
/// cache.finish(640, &mut webp)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct LosslessAnimationCache {
    inner: AnimationCache,
}

impl LosslessAnimationCache {
    pub fn new(
        width: u32,
        height: u32,
        options: LosslessAnimationCacheOptions,
    ) -> Result<Self, AnimationError> {
        Ok(Self {
            inner: AnimationCache::new(
                width,
                height,
                AnimationCacheOptions {
                    minimize_size: options.minimize_size,
                    allow_mixed: false,
                    container: options.container,
                },
            )?,
        })
    }
    pub fn status(&self) -> LosslessCacheStatus {
        self.inner.status()
    }
    /// Submit strength-100 VP8L. Validation errors permit retry; failures after
    /// validation are terminal. Unchanged near-lossless pixels remain carried.
    pub fn add_argb(
        &mut self,
        current: ArgbView<'_>,
        timestamp_ms: u64,
        codec: &LosslessOptions,
    ) -> Result<(), AnimationError> {
        self.add_near_lossless_argb(current, timestamp_ms, 100, codec)
    }
    /// Submit VP8L strength 0–100 after transparency substitution. RGB and
    /// intermediate alpha may change, including with `exact`. Original words
    /// drive repeats and carryover. All validation precedes cache mutation.
    pub fn add_near_lossless_argb(
        &mut self,
        current: ArgbView<'_>,
        timestamp_ms: u64,
        strength: i32,
        codec: &LosslessOptions,
    ) -> Result<(), AnimationError> {
        self.inner.check_active()?;
        let frame_index = self.inner.status.submitted_frames;
        self.inner.interval(timestamp_ms)?;
        lossless::checked_strength(strength).map_err(|source| AnimationError::Encoding {
            frame_index,
            source,
        })?;
        self.inner.validate_dimensions(current)?;
        WebPValidateConfig(codec).map_err(|source| AnimationError::Encoding {
            frame_index,
            source,
        })?;
        self.inner.add_argb(
            current,
            timestamp_ms,
            &MixedEncodingConfig {
                primary: AnimationCandidateCodec::Vp8l,
                codec: LossyOptions {
                    quality: codec.quality as f32,
                    method: codec.method,
                    ..Default::default()
                },
                near_lossless: strength,
                exact: codec.exact,
            },
        )
    }
    /// Supply explicit final timing. Errors preserve output; timing/empty errors
    /// allow retry. Success releases buffers and permanently closes the cache.
    pub fn finish(
        &mut self,
        end_timestamp_ms: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), AnimationError> {
        self.inner.finish(end_timestamp_ms, output)
    }
}

/// Options for a keyframe-disabled cache. Defaults disable mixed trials and
/// minimize-size; container defaults are white background and infinite looping.
#[derive(Debug, Default)]
pub struct AnimationCacheOptions {
    pub minimize_size: bool,
    pub allow_mixed: bool,
    pub container: AnimationContainerOptions,
}

/// Low-level, keyframe-disabled ARGB cache. Codec selection may change on every
/// input. Mixed mode tries eligible codecs with the supplied shared config.
/// Owns original-word history and buffered output; skips save settings and merge
/// timing. This does not enable the public RGBA Optimized policy or perform
/// decode-dependent single-frame reconstruction. Validation errors allow retry;
/// errors after validation are terminal. Explicit final timing is required.
#[derive(Debug)]
pub struct AnimationCache {
    container: AnimationContainer,
    cached: Vec<(EncodedFrame, ContainerFrameOptions)>,
    previous: Vec<u32>,
    carryover: Vec<u32>,
    minimize_size: bool,
    allow_mixed: bool,
    last_config: Option<MixedEncodingConfig>,
    last_config_reversed: Option<MixedEncodingConfig>,
    last_timestamp_ms: Option<u64>,
    status: LosslessCacheStatus,
    active: bool,
}

impl AnimationCache {
    pub fn new(
        width: u32,
        height: u32,
        options: AnimationCacheOptions,
    ) -> Result<Self, AnimationError> {
        if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
            return Err(AnimationError::InvalidCanvas);
        }
        let container =
            AnimationContainer::new(width, height, options.container).map_err(|source| {
                AnimationError::Container {
                    frame_index: 0,
                    source,
                }
            })?;
        let area = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| candidate_error(0, AnimationLeafError::SizeOverflow.into()))?;
        let allocate = || -> Result<Vec<u32>, AnimationError> {
            let mut canvas = Vec::new();
            canvas
                .try_reserve_exact(area)
                .map_err(|source| candidate_error(0, source.into()))?;
            canvas.resize(area, 0);
            Ok(canvas)
        };
        let mut cached = Vec::new();
        // Disabled keyframes allocate exactly two slots in the pin. A split
        // must flush one slot before CacheFrame can append another candidate.
        cached
            .try_reserve_exact(2)
            .map_err(|source| candidate_error(0, source.into()))?;
        Ok(Self {
            container,
            cached,
            previous: allocate()?,
            carryover: allocate()?,
            minimize_size: options.minimize_size,
            allow_mixed: options.allow_mixed,
            last_config: None,
            last_config_reversed: None,
            last_timestamp_ms: None,
            status: LosslessCacheStatus::default(),
            active: true,
        })
    }

    pub fn status(&self) -> LosslessCacheStatus {
        self.status
    }

    fn check_active(&self) -> Result<(), AnimationError> {
        if self.active {
            Ok(())
        } else {
            Err(AnimationError::InvalidState)
        }
    }

    fn interval(&self, timestamp_ms: u64) -> Result<Option<u32>, AnimationError> {
        let Some(previous_ms) = self.last_timestamp_ms else {
            return Ok(None);
        };
        let frame_index = self.status.submitted_frames - 1;
        let duration_ms =
            timestamp_ms
                .checked_sub(previous_ms)
                .ok_or(AnimationError::InvalidTimestamp {
                    frame_index,
                    previous_ms,
                    timestamp_ms,
                })?;
        if duration_ms >= u64::from(MAX_DURATION) {
            return Err(AnimationError::InvalidDuration {
                frame_index,
                duration_ms,
            });
        }
        Ok(Some(duration_ms as u32))
    }

    fn validate_dimensions(&self, current: ArgbView<'_>) -> Result<(), AnimationError> {
        if (current.width, current.height)
            != (self.container.width as i32, self.container.height as i32)
        {
            return Err(candidate_error(
                self.status.submitted_frames,
                AnimationLeafError::InvalidDimensions.into(),
            ));
        }
        Ok(())
    }

    /// Saved submitted and reversed settings, including settings on skipped
    /// inputs. The primary is independent of the selected candidate codec.
    pub fn saved_configs(&self) -> Option<(MixedEncodingConfig, MixedEncodingConfig)> {
        self.last_config.zip(self.last_config_reversed)
    }

    /// Accepted original canvas and carryover words, in packed row order.
    pub fn canvases(&self) -> (&[u32], &[u32]) {
        (&self.previous, &self.carryover)
    }

    /// Pending frame controls. Payloads and timing remain owned by this cache.
    pub fn pending_control(&self) -> Option<ContainerFrameOptions> {
        self.cached.last().map(|(_, c)| *c)
    }

    /// Validate all participating settings, dimensions and timing before mutation,
    /// including on exact repeats and similarity skips. Fractional VP8 quality
    /// is preserved; VP8L effort truncates only inside its codec adapter.
    pub fn add_argb(
        &mut self,
        current: ArgbView<'_>,
        timestamp_ms: u64,
        codec: &MixedEncodingConfig,
    ) -> Result<(), AnimationError> {
        self.check_active()?;
        let frame_index = self.status.submitted_frames;
        let duration = self.interval(timestamp_ms)?;
        codec
            .validate()
            .map_err(|source| AnimationError::Encoding {
                frame_index,
                source,
            })?;
        self.validate_dimensions(current)?;
        let submitted_frames = frame_index
            .checked_add(1)
            .ok_or_else(|| candidate_error(frame_index, AnimationLeafError::SizeOverflow.into()))?;
        self.active = false;
        if let Some(duration) = duration {
            // Use the preceding submitted config for a hold, before saving the
            // incoming config (even when the incoming candidate will skip).
            self.IncreasePreviousDuration(duration)?;
            self.FlushFrames(self.cached.len() - 1)?;
        }
        self.CacheFrame(current, codec)?;
        self.status.submitted_frames = submitted_frames;
        self.last_timestamp_ms = Some(timestamp_ms);
        self.active = true;
        Ok(())
    }

    fn IncreasePreviousDuration(&mut self, duration: u32) -> Result<(), AnimationError> {
        let previous = &mut self.cached.last_mut().expect("previous frame is cached").1;
        let new_duration = previous.duration_ms + duration;
        if new_duration < MAX_DURATION {
            previous.duration_ms = new_duration;
        } else {
            debug_assert_eq!(self.cached.len(), 1);
            // Typed payloads copied from the pin's fixed stills. These are
            // nested payload bytes, not stripped headers or a fresh encode.
            const VP8L: &[u8] = &[0x2f, 0, 0, 0, 0x10, 0x88, 0x88, 0x08];
            const VP8: &[u8] = &[
                0x30, 0x01, 0x00, 0x9d, 0x01, 0x2a, 0x01, 0x00, 0x01, 0x00, 0x02, 0x00, 0x34, 0x25,
                0xa4, 0x00, 0x03, 0x70, 0x00, 0xfe, 0xfb, 0xfd, 0x50, 0x00,
            ];
            let copy = |bytes: &[u8]| -> Result<Vec<u8>, AnimationError> {
                let mut result = Vec::new();
                result.try_reserve_exact(bytes.len()).map_err(|source| {
                    candidate_error(
                        self.status.submitted_frames.saturating_sub(1),
                        source.into(),
                    )
                })?;
                result.extend_from_slice(bytes);
                Ok(result)
            };
            use crate::mux::muxinternal::FramePayload;
            let payload = if self.allow_mixed
                || self.last_config.expect("saved previous config").primary
                    == AnimationCandidateCodec::Vp8l
            {
                FramePayload::Vp8l(copy(VP8L)?)
            } else {
                FramePayload::Vp8 {
                    image: copy(VP8)?,
                    alpha: Some(copy(&[0, 0])?),
                }
            };
            self.cached.push((
                EncodedFrame {
                    width: 1,
                    height: 1,
                    has_alpha: true,
                    payload,
                },
                ContainerFrameOptions {
                    duration_ms: duration,
                    blend: FrameBlend::Blend,
                    ..Default::default()
                },
            ));
            self.status.previous_rect = FrameRectangle {
                width: 1,
                height: 1,
                ..Default::default()
            };
            self.status.count_since_key_frame += 1;
        }
        self.refresh_pending_status();
        Ok(())
    }

    fn CacheFrame(
        &mut self,
        current: ArgbView<'_>,
        codec: &MixedEncodingConfig,
    ) -> Result<(), AnimationError> {
        debug_assert!(self.cached.len() < 2);
        let frame_index = self.status.submitted_frames;
        let first = frame_index == 0;
        let (w, h, stride) = (current.width, current.height, current.width as usize);
        let result = (|| -> Result<(), AnimationCandidateError> {
            if !first {
                CopyIdenticalPixels(
                    ArgbView::new(&self.previous, w, h, stride)?,
                    current,
                    &mut ArgbViewMut::new(&mut self.carryover, w, h, stride)?,
                )?;
            }
            self.last_config = Some(*codec);
            self.last_config_reversed = Some(codec.reversed());
            let previous = ArgbView::new(&self.carryover, w, h, stride)?;
            let candidate = if self.allow_mixed {
                SetFrameMixed(
                    previous,
                    current,
                    self.status.previous_rect,
                    &MixedCandidateOptions {
                        codec: *codec,
                        is_key_frame: first,
                        is_first_frame: first,
                        minimize_size: self.minimize_size,
                        ..Default::default()
                    },
                )?
            } else if codec.primary == AnimationCandidateCodec::Vp8l {
                set_frame_near_lossless(
                    previous,
                    current,
                    self.status.previous_rect,
                    &LosslessCandidateOptions {
                        codec: codec.lossless_options(),
                        is_key_frame: first,
                        is_first_frame: first,
                        minimize_size: self.minimize_size,
                        ..Default::default()
                    },
                    codec.near_lossless,
                )?
                .map(Into::into)
            } else {
                super::lossy::set_frame_with_exact(
                    previous,
                    current,
                    self.status.previous_rect,
                    &LossyCandidateOptions {
                        codec: codec.codec,
                        is_key_frame: first,
                        is_first_frame: first,
                        minimize_size: self.minimize_size,
                        ..Default::default()
                    },
                    codec.exact,
                )?
                .map(Into::into)
            };
            if let Some(candidate) = candidate {
                candidate.update_canvases(
                    current,
                    self.status.previous_rect,
                    &mut ArgbViewMut::new(&mut self.previous, w, h, stride)?,
                    &mut ArgbViewMut::new(&mut self.carryover, w, h, stride)?,
                )?;
                if let Some(dispose) = candidate.previous_dispose {
                    self.cached
                        .last_mut()
                        .expect("previous frame is cached")
                        .1
                        .dispose = dispose;
                }
                self.status.previous_rect = candidate.rect;
                self.status.count_since_key_frame += usize::from(!first);
                self.cached.push((candidate.frame, candidate.control));
            }
            Ok(())
        })();
        result.map_err(|source| candidate_error(frame_index, source))?;
        self.FlushFrames(self.cached.len() - 1)
    }

    fn FlushFrames(&mut self, count: usize) -> Result<(), AnimationError> {
        for _ in 0..count {
            let (frame, control) = self.cached.remove(0);
            self.container
                .push_frame(frame, control)
                .map_err(|source| AnimationError::Container {
                    frame_index: self.status.submitted_frames.saturating_sub(1),
                    source,
                })?;
        }
        self.status.emitted_frames = self.container.frames.len();
        self.refresh_pending_status();
        Ok(())
    }

    fn refresh_pending_status(&mut self) {
        self.status.cached_frames = self.cached.len();
        self.status.pending_duration_ms = self.cached.last().map(|(_, c)| c.duration_ms);
    }

    /// Supply final timing, flush the remaining frame and assemble. Every error
    /// preserves output; timing/empty errors permit retry. A successful finish
    /// releases canvases/payloads and permanently rejects further operations.
    pub fn finish(
        &mut self,
        end_timestamp_ms: u64,
        output: &mut Vec<u8>,
    ) -> Result<(), AnimationError> {
        self.check_active()?;
        let duration = self
            .interval(end_timestamp_ms)?
            .ok_or(AnimationError::NoFrames)?;
        let frame_index = self.status.submitted_frames - 1;
        self.active = false;
        self.IncreasePreviousDuration(duration)?;
        self.FlushFrames(self.cached.len())?;
        self.container
            .assemble(output)
            .map_err(|source| AnimationError::Container {
                frame_index,
                source,
            })?;
        self.container.frames.clear();
        self.previous = Vec::new();
        self.carryover = Vec::new();
        Ok(())
    }
}

fn candidate_error(frame_index: usize, source: AnimationCandidateError) -> AnimationError {
    AnimationError::Candidate {
        frame_index,
        source,
    }
}
