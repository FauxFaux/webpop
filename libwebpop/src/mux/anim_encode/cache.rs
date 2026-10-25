// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! Lossless CacheFrame/FlushFrames with keyframe insertion disabled. This is
//! separate from the public RGBA policy until the remaining optimizer gates land.

use super::*;
use crate::LosslessOptions;
use crate::enc::config_enc::WebPValidateConfig;
use crate::mux::{
    AnimationContainer, AnimationContainerOptions, AnimationError, ContainerFrameOptions,
    EncodedFrame, FrameBlend,
};

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

/// Low-level lossless animation cache using checked ARGB word views. Only the
/// first accepted frame is a keyframe; later inputs use SetFrameLossless.
/// Repeats merge, changed frames can have zero duration, and overflow inserts the pin's
/// fixed transparent hold payload. Methods 0–6 and codec-level `exact` work;
/// near-lossless, lossy, mixed mode and keyframe intervals are not accepted here.
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
    container: AnimationContainer,
    cached: Vec<(EncodedFrame, ContainerFrameOptions)>,
    previous: Vec<u32>,
    carryover: Vec<u32>,
    minimize_size: bool,
    last_timestamp_ms: Option<u64>,
    status: LosslessCacheStatus,
    active: bool,
}

impl LosslessAnimationCache {
    pub fn new(
        width: u32,
        height: u32,
        options: LosslessAnimationCacheOptions,
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

    /// Validate dimensions, codec options and the preceding interval before
    /// mutation. Their errors permit retry. Allocation/encoding/container errors
    /// after validation are terminal. Timestamps have arbitrary u64 origin.
    pub fn add_argb(
        &mut self,
        current: ArgbView<'_>,
        timestamp_ms: u64,
        codec: &LosslessOptions,
    ) -> Result<(), AnimationError> {
        self.check_active()?;
        let frame_index = self.status.submitted_frames;
        let duration = self.interval(timestamp_ms)?;
        if (current.width, current.height)
            != (self.container.width as i32, self.container.height as i32)
        {
            return Err(candidate_error(
                frame_index,
                AnimationLeafError::InvalidDimensions.into(),
            ));
        }
        WebPValidateConfig(codec).map_err(|source| AnimationError::Encoding {
            frame_index,
            source,
        })?;
        let submitted_frames = frame_index
            .checked_add(1)
            .ok_or_else(|| candidate_error(frame_index, AnimationLeafError::SizeOverflow.into()))?;
        self.active = false;
        if let Some(duration) = duration {
            self.IncreasePreviousDuration(duration)?;
            // In particular, a hold insertion filled both cache slots.
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
            // The pin supplies this VP8L payload verbatim. Preparing typed
            // bytes here avoids parsing RIFF or re-encoding with current options.
            const HOLD: &[u8] = &[0x2f, 0, 0, 0, 0x10, 0x88, 0x88, 0x08];
            let mut payload = Vec::new();
            payload.try_reserve_exact(HOLD.len()).map_err(|source| {
                candidate_error(
                    self.status.submitted_frames.saturating_sub(1),
                    source.into(),
                )
            })?;
            payload.extend_from_slice(HOLD);
            self.cached.push((
                EncodedFrame {
                    width: 1,
                    height: 1,
                    has_alpha: true,
                    payload: crate::mux::muxinternal::FramePayload::Vp8l(payload),
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
        codec: &LosslessOptions,
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
            let candidate = SetFrameLossless(
                ArgbView::new(&self.carryover, w, h, stride)?,
                current,
                self.status.previous_rect,
                &LosslessCandidateOptions {
                    codec: *codec,
                    is_key_frame: first,
                    is_first_frame: first,
                    minimize_size: self.minimize_size,
                    ..Default::default()
                },
            )?;
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
