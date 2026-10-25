// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! Keyframe option/storage foundation. Deferred selection is not implemented.

use super::AnimationCandidateError;
use crate::mux::{ContainerFrameOptions, EncodedFrame, FrameBlend};

pub(super) const DELTA_INFINITY: i64 = 1i64 << 32;

/// Low-level policy mapping, separate from the public RGBA encoder options.
/// Enabled policies can be normalized but are not accepted for encoding yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyframePolicy {
    #[default]
    Disabled,
    AllFrames,
    /// Values must fit signed upstream intervals and max_interval must be >=2.
    /// The pin adjusts min_interval to satisfy its window constraints.
    Bounded {
        min_interval: u32,
        max_interval: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("bounded keyframe intervals must fit i32 and max_interval must be at least 2")]
pub struct InvalidKeyframeIntervals;

/// Effective signed intervals after the pin's ordered adjustments. Raw signed
/// mapping can retain negative kmin for kmax=2. Capacity includes the preceding
/// frame: a difference of 30 therefore allocates 31 slots, not 30.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveKeyframeOptions {
    pub kmin: i32,
    pub kmax: i32,
    pub capacity: usize,
}

impl KeyframePolicy {
    pub fn normalize(
        self,
        minimize_size: bool,
    ) -> Result<EffectiveKeyframeOptions, InvalidKeyframeIntervals> {
        let (kmin, kmax) = match self {
            Self::Disabled => (0, 0),
            Self::AllFrames => (0, 1),
            Self::Bounded {
                min_interval,
                max_interval,
            } => {
                let min = i32::try_from(min_interval).map_err(|_| InvalidKeyframeIntervals)?;
                let max = i32::try_from(max_interval).map_err(|_| InvalidKeyframeIntervals)?;
                if max < 2 {
                    return Err(InvalidKeyframeIntervals);
                }
                (min, max)
            }
        };
        Ok(SanitizeEncoderOptions(kmin, kmax, minimize_size))
    }
}

/// Normalize raw upstream signed intervals without verbose warnings. In this
/// order: minimize-size disables, kmax=1 selects all frames, nonpositive kmax
/// disables, then kmin/half-window/window-cap adjustments apply. This leaf does
/// not enable keyframe insertion in AnimationCache or AnimationEncoder.
pub fn SanitizeEncoderOptions(
    mut kmin: i32,
    mut kmax: i32,
    minimize_size: bool,
) -> EffectiveKeyframeOptions {
    let disable = |min: &mut i32, max: &mut i32| {
        *max = i32::MAX;
        *min = *max - 1;
    };
    if minimize_size {
        disable(&mut kmin, &mut kmax);
    }
    if kmax == 1 {
        kmin = 0;
        kmax = 0;
    } else {
        if kmax <= 0 {
            disable(&mut kmin, &mut kmax);
        }
        if kmin >= kmax {
            kmin = kmax - 1;
        } else {
            let limit = kmax / 2 + 1;
            if kmin < limit && limit < kmax {
                kmin = limit;
            }
        }
        if kmin < kmax - 30 {
            kmin = kmax - 30;
        }
    }
    // Widen before subtraction: raw i32 extremes must never overflow. The
    // normalized difference is in 0..=30, representable even on 32-bit hosts.
    let capacity = usize::try_from(i64::from(kmax) - i64::from(kmin) + 1)
        .expect("normalized keyframe window fits usize")
        .max(2);
    EffectiveKeyframeOptions {
        kmin,
        kmax,
        capacity,
    }
}

#[derive(Debug)]
pub(super) struct FrameAlternative {
    pub frame: Option<EncodedFrame>,
    pub control: ContainerFrameOptions,
}

impl Default for FrameAlternative {
    fn default() -> Self {
        Self {
            frame: None,
            control: ContainerFrameOptions {
                blend: FrameBlend::Blend,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct CachedFrame {
    pub sub_frame: FrameAlternative,
    pub key_frame: FrameAlternative,
    pub is_key_frame: bool,
}

impl CachedFrame {
    pub fn new(frame: EncodedFrame, control: ContainerFrameOptions, is_key_frame: bool) -> Self {
        let mut result = Self {
            is_key_frame,
            ..Default::default()
        };
        *result.selected_mut() = FrameAlternative {
            frame: Some(frame),
            control,
        };
        result
    }
    pub fn selected(&self) -> &FrameAlternative {
        if self.is_key_frame {
            &self.key_frame
        } else {
            &self.sub_frame
        }
    }
    pub fn selected_mut(&mut self) -> &mut FrameAlternative {
        if self.is_key_frame {
            &mut self.key_frame
        } else {
            &mut self.sub_frame
        }
    }
    pub fn set_duration(&mut self, duration_ms: u32) {
        self.sub_frame.control.duration_ms = duration_ms;
        self.key_frame.control.duration_ms = duration_ms;
    }
    pub fn into_selected(mut self) -> (EncodedFrame, ContainerFrameOptions) {
        let selected = self.selected_mut();
        // Dropping self releases the unselected alternative's owned payload.
        (
            selected.frame.take().expect("selected frame is encoded"),
            selected.control,
        )
    }
    pub fn status(&self) -> CachedFrameStatus {
        CachedFrameStatus {
            sub_frame: self.sub_frame.status(),
            key_frame: self.key_frame.status(),
            is_key_frame: self.is_key_frame,
        }
    }
}

impl FrameAlternative {
    fn status(&self) -> CachedAlternativeStatus {
        let (width, height, encoded_size) = self.frame.as_ref().map_or((0, 0, 0), |f| {
            let extended = if f.payload.has_alpha_chunk() { 18 } else { 0 };
            (
                f.width,
                f.height,
                12 + extended + f.payload.disk_size().expect("validated cached payload"),
            )
        });
        CachedAlternativeStatus {
            control: self.control,
            width,
            height,
            encoded_size,
        }
    }
}

/// Read-only observations of owned alternatives. encoded_size=0 means absent;
/// otherwise it is the complete still-file byte size used for keyframe penalty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedAlternativeStatus {
    pub control: ContainerFrameOptions,
    pub width: u32,
    pub height: u32,
    pub encoded_size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedFrameStatus {
    pub sub_frame: CachedAlternativeStatus,
    pub key_frame: CachedAlternativeStatus,
    pub is_key_frame: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheWindowStatus {
    pub effective: EffectiveKeyframeOptions,
    pub selected_keyframe: Option<usize>,
    pub best_penalty: i64,
    pub previous_candidate_undecided: bool,
    pub flush_count: usize,
}

#[derive(Debug)]
pub(super) struct CandidateWindow {
    pub frames: Vec<CachedFrame>,
    pub state: CacheWindowStatus,
}

impl CandidateWindow {
    pub fn push(&mut self, frame: CachedFrame) -> Result<(), AnimationCandidateError> {
        if self.frames.len() >= self.state.effective.capacity {
            return Err(AnimationCandidateError::CacheCapacityExceeded);
        }
        self.frames.push(frame);
        Ok(())
    }

    pub fn new(effective: EffectiveKeyframeOptions) -> Result<Self, AnimationCandidateError> {
        let mut frames = Vec::new();
        frames.try_reserve_exact(effective.capacity)?;
        Ok(Self {
            frames,
            state: CacheWindowStatus {
                effective,
                selected_keyframe: None,
                best_penalty: DELTA_INFINITY,
                previous_candidate_undecided: false,
                flush_count: 0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mux::{AnimationContainer, FrameDispose};

    fn frame(value: u8) -> EncodedFrame {
        EncodedFrame::encode_lossless_rgba(&[value, 2, 3, 255], 1, 1, 4, &Default::default())
            .unwrap()
    }

    #[test]
    fn animation_keyframe_typed_mapping_checks_ranges_and_ordering() {
        assert_eq!(
            KeyframePolicy::Disabled.normalize(false).unwrap(),
            SanitizeEncoderOptions(0, 0, false)
        );
        assert_eq!(
            KeyframePolicy::AllFrames.normalize(false).unwrap(),
            EffectiveKeyframeOptions {
                kmin: 0,
                kmax: 0,
                capacity: 2
            }
        );
        assert_eq!(
            KeyframePolicy::AllFrames.normalize(true).unwrap(),
            KeyframePolicy::Disabled.normalize(false).unwrap()
        );
        for policy in [
            KeyframePolicy::Bounded {
                min_interval: 0,
                max_interval: 0,
            },
            KeyframePolicy::Bounded {
                min_interval: 0,
                max_interval: 1,
            },
            KeyframePolicy::Bounded {
                min_interval: u32::MAX,
                max_interval: 2,
            },
            KeyframePolicy::Bounded {
                min_interval: 0,
                max_interval: u32::MAX,
            },
        ] {
            for minimize in [false, true] {
                assert_eq!(policy.normalize(minimize), Err(InvalidKeyframeIntervals));
            }
        }
        let max = i32::MAX as u32;
        assert_eq!(
            KeyframePolicy::Bounded {
                min_interval: max,
                max_interval: max
            }
            .normalize(false)
            .unwrap(),
            SanitizeEncoderOptions(i32::MAX, i32::MAX, false)
        );
    }

    #[test]
    fn animation_keyframe_owned_alternatives_survive_moves_selection_and_release() {
        for is_key_frame in [false, true] {
            let mut slot = CachedFrame::new(
                frame(11),
                ContainerFrameOptions {
                    duration_ms: 7,
                    ..Default::default()
                },
                false,
            );
            slot.key_frame = FrameAlternative {
                frame: Some(frame(99)),
                control: ContainerFrameOptions {
                    duration_ms: 9,
                    dispose: FrameDispose::Background,
                    ..Default::default()
                },
            };
            slot.is_key_frame = is_key_frame;
            let before = slot.status();
            assert_eq!(before.sub_frame.control.duration_ms, 7);
            assert_eq!(before.key_frame.control.duration_ms, 9);
            assert!(before.sub_frame.encoded_size > 0 && before.key_frame.encoded_size > 0);
            let mut window = CandidateWindow::new(SanitizeEncoderOptions(0, 100, false)).unwrap();
            assert_eq!(window.state.effective.capacity, 31);
            assert!(window.frames.capacity() >= 31);
            assert_eq!(window.state.best_penalty, DELTA_INFINITY);
            assert_eq!(window.state.selected_keyframe, None);
            window.push(slot).unwrap();
            let mut moved = window.frames.remove(0);
            assert_eq!(moved.status(), before);
            moved.set_duration(13);
            assert_eq!(moved.sub_frame.control.duration_ms, 13);
            assert_eq!(moved.key_frame.control.duration_ms, 13);
            // Signed penalties and explicit no-selection are distinct; negative
            // penalties must not wrap when this storage gains selection in 4c.
            window.state.best_penalty = -7;
            window.state.selected_keyframe = Some(0);
            window.state.previous_candidate_undecided = true;
            window.state.flush_count = 1;
            assert_eq!(window.state.best_penalty, -7);
            let (selected, control) = moved.into_selected();
            assert_eq!(control.duration_ms, 13);
            assert_eq!(control.dispose == FrameDispose::Background, is_key_frame);
            let mut container = AnimationContainer::new(1, 1, Default::default()).unwrap();
            container.push_frame(selected, control).unwrap();
            let mut bytes = Vec::new();
            container.assemble(&mut bytes).unwrap();
            let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            assert_eq!(
                decoded.get_pixel(0, 0).0,
                [if is_key_frame { 99 } else { 11 }, 2, 3, 255]
            );
        }
        let mut bounded = CandidateWindow::new(SanitizeEncoderOptions(0, 1, false)).unwrap();
        for _ in 0..2 {
            bounded
                .push(CachedFrame::new(frame(11), Default::default(), false))
                .unwrap();
        }
        let capacity = bounded.frames.capacity();
        assert!(matches!(
            bounded.push(CachedFrame::new(frame(99), Default::default(), false)),
            Err(AnimationCandidateError::CacheCapacityExceeded)
        ));
        assert_eq!(bounded.frames.len(), 2);
        assert_eq!(bounded.frames.capacity(), capacity);
        // An impossible reservation reports an allocation error rather than
        // panicking or attempting an unchecked capacity multiplication.
        assert!(
            CandidateWindow::new(EffectiveKeyframeOptions {
                kmin: 0,
                kmax: 2,
                capacity: usize::MAX
            })
            .is_err()
        );
    }
}
