// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! Mixed SetFrame/GenerateCandidates. Timestamp caching and keyframe policy
//! remain separate; this module never decodes or reconstructs codec output.

use super::*;
use crate::enc::config_enc::{LossyConfig, WebPValidateConfig};
use crate::enc::picture_csp_enc::RgbaInputError;
use crate::mux::{ContainerFrameOptions, EncodedFrame, FrameDispose};
use crate::{WebPEncodingError, internal::LosslessOptions, internal::LossyOptions};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationCandidateCodec {
    Vp8l,
    #[default]
    Vp8,
}

/// Shared settings for the two copies of WebPConfig in mixed mode. `codec`
/// reuses the still options: quality and method apply to BOTH codecs, while
/// alpha_quality/SharpYUV apply to VP8. Quality is VP8 fidelity or VP8L effort,
/// not a common visual-fidelity scale. VP8 retains its fractional value; VP8L
/// truncates it as the pinned stream encoder does. `primary` controls disposal
/// area selection and the saved config, not trial order or the winning codec.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixedEncodingConfig {
    pub primary: AnimationCandidateCodec,
    pub codec: LossyOptions,
    /// VP8L near-lossless strength, signed 0–100, including on reversed trials.
    /// Below 100, RGB and intermediate alpha may change.
    pub near_lossless: i32,
    /// Disable transparent RGB cleanup in both codecs, and predictor quantization
    /// in VP8L. Animation transparency and standalone near-lossless preprocessing
    /// still apply; lossy coding has no exact RGB guarantee.
    pub exact: bool,
}

impl Default for MixedEncodingConfig {
    fn default() -> Self {
        Self {
            primary: AnimationCandidateCodec::Vp8,
            codec: LossyOptions::default(),
            near_lossless: 100,
            exact: false,
        }
    }
}

impl MixedEncodingConfig {
    /// Flip only the codec selector, preserving every other setting. Cache
    /// integration must save both configurations even when SetFrame skips.
    pub fn reversed(self) -> Self {
        Self {
            primary: match self.primary {
                AnimationCandidateCodec::Vp8l => AnimationCandidateCodec::Vp8,
                AnimationCandidateCodec::Vp8 => AnimationCandidateCodec::Vp8l,
            },
            ..self
        }
    }

    pub(super) fn lossless_options(&self) -> LosslessOptions {
        LosslessOptions {
            quality: self.codec.quality as u32,
            method: self.codec.method,
            exact: self.exact,
        }
    }

    pub(super) fn validate(&self) -> Result<u32, WebPEncodingError> {
        // Both codecs must be valid before palette heuristics or an empty-frame
        // shortcut can bypass encoding. VP8L's config uses integer effort.
        LossyConfig::from_options(&self.codec)?;
        WebPValidateConfig(&self.lossless_options())?;
        super::lossless::checked_strength(self.near_lossless)
    }
}

/// Mixed-mode controls for one independently evaluated keyframe or subframe.
#[derive(Clone, Copy, Debug, Default)]
pub struct MixedCandidateOptions {
    pub codec: MixedEncodingConfig,
    pub is_key_frame: bool,
    pub is_first_frame: bool,
    pub previous_candidate_undecided: bool,
    pub minimize_size: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedCandidateEligibility {
    /// None when minimize_size bypasses palette extraction. 257 means >256.
    pub palette_size: Option<usize>,
    pub lossless: bool,
    pub lossy: bool,
}

/// Apply the pin's mixed heuristic to the ORIGINAL lossless subframe, before
/// transparency/cleanup or near-lossless preprocessing. Minimize-size always
/// tries both. Otherwise VP8L is eligible below 194 colors and VP8 from 31.
pub fn mixed_candidate_eligibility(
    sub_frame: ArgbView<'_>,
    minimize_size: bool,
) -> MixedCandidateEligibility {
    if minimize_size {
        return MixedCandidateEligibility {
            palette_size: None,
            lossless: true,
            lossy: true,
        };
    }
    let count = crate::utils::palette::GetColorPalette(
        sub_frame.pixels,
        sub_frame.width as usize,
        sub_frame.height as usize,
        sub_frame.stride,
        &mut [0; 256],
    );
    MixedCandidateEligibility {
        palette_size: Some(count),
        lossless: count < 194,
        lossy: count >= 31,
    }
}

/// Winning mixed rectangle, with its actual codec, preceding disposal and mask.
#[derive(Debug)]
pub struct MixedCandidate {
    pub frame: EncodedFrame,
    pub rect: FrameRectangle,
    pub control: ContainerFrameOptions,
    pub previous_dispose: Option<FrameDispose>,
    pub carryover_mask: Vec<u8>,
    pub codec: AnimationCandidateCodec,
}

impl MixedCandidate {
    pub fn encoded_size(&self) -> usize {
        let extended = if self.codec == AnimationCandidateCodec::Vp8 && self.frame.has_alpha() {
            18
        } else {
            0
        };
        (12 + extended
            + self
                .frame
                .payload
                .disk_size()
                .expect("validated mixed payload")) as usize
    }

    pub fn carries_over(&self) -> bool {
        !self.carryover_mask.is_empty()
    }

    /// Update original-word history with the winning mask and disposal. Restore
    /// exact repeats using CopyIdenticalPixels before the next selection. Skips
    /// leave previous original and rectangle unchanged; mixed mode may skip
    /// merely similar pixels even when primary is VP8L. No decoded-pixel or
    /// lossless canvas guarantee applies to mixed output.
    pub fn update_canvases(
        &self,
        current: ArgbView<'_>,
        previous_rect: FrameRectangle,
        previous: &mut ArgbViewMut<'_>,
        carryover: &mut ArgbViewMut<'_>,
    ) -> Result<(), AnimationLeafError> {
        update_candidate_canvases(
            self.rect,
            self.previous_dispose,
            &self.carryover_mask,
            current,
            previous_rect,
            previous,
            carryover,
        )
    }
}

impl From<LosslessCandidate> for MixedCandidate {
    fn from(c: LosslessCandidate) -> Self {
        Self {
            frame: c.frame,
            rect: c.rect,
            control: c.control,
            previous_dispose: c.previous_dispose,
            carryover_mask: c.carryover_mask,
            codec: AnimationCandidateCodec::Vp8l,
        }
    }
}

impl From<LossyCandidate> for MixedCandidate {
    fn from(c: LossyCandidate) -> Self {
        Self {
            frame: c.frame,
            rect: c.rect,
            control: c.control,
            previous_dispose: c.previous_dispose,
            carryover_mask: c.carryover_mask,
            codec: AnimationCandidateCodec::Vp8,
        }
    }
}

fn pick_best(best: &mut Option<MixedCandidate>, candidate: MixedCandidate) {
    if best
        .as_ref()
        .is_none_or(|old| candidate.encoded_size() < old.encoded_size())
    {
        *best = Some(candidate);
    }
}

/// Encode one mixed-mode trial with the supplied actual codec, preserving the
/// shared config's dormant fields during reversal. VP8's `exact` bypasses
/// transparent-area cleanup; blending still disables its filter. This does not
/// refine rectangles, mutate transparency or choose eligibility/disposal.
pub fn encode_mixed_candidate(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
    config: &MixedEncodingConfig,
    codec: AnimationCandidateCodec,
    use_blending: bool,
) -> Result<MixedCandidate, AnimationCandidateError> {
    config.validate()?;
    match codec {
        AnimationCandidateCodec::Vp8l => Ok(encode_near_lossless_candidate(
            sub_frame,
            rect,
            &config.lossless_options(),
            config.near_lossless,
            use_blending,
        )?
        .into()),
        AnimationCandidateCodec::Vp8 => Ok(super::lossy::encode_candidate_with_exact(
            sub_frame,
            rect,
            &config.codec,
            use_blending,
            config.exact,
        )?
        .into()),
    }
}

fn GenerateCandidates(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    params: &SubFrameParams<'_>,
    dispose: FrameDispose,
    options: &MixedCandidateOptions,
    near_lossless: u32,
    best: &mut Option<MixedCandidate>,
) -> Result<(), AnimationCandidateError> {
    let eligible = mixed_candidate_eligibility(
        params.sub_frame_ll.expect("nonempty mixed rectangle"),
        options.minimize_size,
    );
    // The primary codec never reverses this order. Fresh per-trial originals
    // isolate VP8L transparency/cleanup from VP8 flattening and later disposal.
    if eligible.lossless {
        let settings = LosslessCandidateOptions {
            codec: options.codec.lossless_options(),
            is_key_frame: options.is_key_frame,
            is_first_frame: options.is_first_frame,
            previous_candidate_undecided: options.previous_candidate_undecided,
            minimize_size: options.minimize_size,
        };
        pick_best(
            best,
            super::lossless::generate_candidate(
                previous,
                current,
                params.rect_ll,
                dispose,
                &settings,
                near_lossless,
            )?
            .into(),
        );
    }
    if eligible.lossy {
        let settings = LossyCandidateOptions {
            codec: options.codec.codec,
            is_key_frame: options.is_key_frame,
            is_first_frame: options.is_first_frame,
            previous_candidate_undecided: options.previous_candidate_undecided,
            minimize_size: options.minimize_size,
        };
        pick_best(
            best,
            super::lossy::generate_candidate(
                previous,
                current,
                params.rect_lossy,
                dispose,
                &settings,
                options.codec.exact,
            )?
            .into(),
        );
    }
    Ok(())
}

/// Mixed-only SetFrame. Evaluate chosen disposal alternatives in pinned order:
/// NONE VP8L, NONE VP8, BACKGROUND VP8L, BACKGROUND VP8, omitting ineligible
/// codecs/disposals. Ordinary mode uses each original lossless crop's palette;
/// minimize-size tries both codecs. Primary codec chooses which rectangle's
/// area drives disposal selection. If EITHER change rectangle is empty, skip
/// before trying codecs, regardless of palette or primary. Ties retain the
/// first encoded candidate, its preceding disposal and carryover mask.
/// Caller owns timing, saved original/reversed config, canvas updates and
/// keyframe insertion. Public RGBA Optimized remains unavailable.
pub fn SetFrameMixed(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &MixedCandidateOptions,
) -> Result<Option<MixedCandidate>, AnimationCandidateError> {
    let near_lossless = options.codec.validate()?;
    if options.is_first_frame && !options.is_key_frame {
        return Err(AnimationCandidateError::InvalidFirstFrame);
    }
    if (previous.width, previous.height) != (current.width, current.height) {
        return Err(AnimationLeafError::InvalidDimensions.into());
    }
    if current.width > 16383 || current.height > 16383 {
        return Err(WebPEncodingError::from(RgbaInputError::InvalidDimensions).into());
    }
    validate_rectangle(previous, &previous_rect)?;
    let mut none = SubFrameParams::new(true, !options.is_first_frame);
    GetSubRects(
        previous,
        current,
        options.is_key_frame,
        options.is_first_frame,
        options.codec.codec.quality,
        &mut none,
    )?;
    if IsEmptyRect(&none.rect_ll) || IsEmptyRect(&none.rect_lossy) {
        return Ok(None);
    }
    let mut disposed_picture = None;
    let mut background = SubFrameParams::new(false, false);
    if !options.is_key_frame && !options.previous_candidate_undecided {
        let mut picture = super::lossless::copy_picture(previous)?;
        DisposeFrameRectangle(
            FrameDispose::Background,
            &previous_rect,
            &mut ArgbViewMut::new(
                &mut picture.argb,
                previous.width,
                previous.height,
                previous.width as usize,
            )?,
        )?;
        disposed_picture = Some(picture);
    }
    let disposed_view = disposed_picture.as_ref().map(|p| {
        ArgbView::new(
            &p.argb,
            previous.width,
            previous.height,
            previous.width as usize,
        )
        .expect("owned canvas")
    });
    if let Some(disposed) = disposed_view {
        GetSubRects(
            disposed,
            current,
            options.is_key_frame,
            options.is_first_frame,
            options.codec.codec.quality,
            &mut background,
        )?;
        let (a, b) = match options.codec.primary {
            AnimationCandidateCodec::Vp8l => (background.rect_ll, none.rect_ll),
            AnimationCandidateCodec::Vp8 => (background.rect_lossy, none.rect_lossy),
        };
        if options.minimize_size {
            background.should_try = true;
        } else if a.width as u64 * (a.height as u64) < b.width as u64 * (b.height as u64) {
            background.should_try = true;
            none.should_try = false;
        }
    }
    let mut best = None;
    if none.should_try {
        GenerateCandidates(
            previous,
            current,
            &none,
            FrameDispose::None,
            options,
            near_lossless,
            &mut best,
        )?;
    }
    if background.should_try {
        GenerateCandidates(
            disposed_view.expect("background trial"),
            current,
            &background,
            FrameDispose::Background,
            options,
            near_lossless,
            &mut best,
        )?;
    }
    Ok(best)
}
