// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! VP8L subset of SetFrame/GenerateCandidates, with lossless and near-lossless
//! codec settings. Caching and keyframe decisions remain separate.

use super::*;
use crate::enc::config_enc::{EncoderConfig, WebPValidateConfig};
use crate::enc::picture_enc::WebPPicture;
use crate::mux::{ContainerFrameOptions, EncodedFrame, FrameBlend, FrameDispose, MuxError};
use crate::{LosslessOptions, WebPEncodingError};
use std::collections::TryReserveError;

#[derive(Debug, thiserror::Error)]
pub enum AnimationCandidateError {
    #[error(transparent)]
    Input(#[from] AnimationLeafError),
    #[error(transparent)]
    Encoding(#[from] WebPEncodingError),
    #[error(transparent)]
    Container(#[from] MuxError),
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
    #[error("the first animation frame must be evaluated as a keyframe")]
    InvalidFirstFrame,
}

/// VP8L candidate controls without mixed/lossy trials. The lossless functions
/// select strength 100; their near-lossless siblings accept strength separately.
/// `exact` is codec-level only:
/// transparency substitution can still discard invisible RGB during blending.
#[derive(Clone, Copy, Debug, Default)]
pub struct LosslessCandidateOptions {
    pub codec: LosslessOptions,
    pub is_key_frame: bool,
    pub is_first_frame: bool,
    pub previous_candidate_undecided: bool,
    pub minimize_size: bool,
}

/// Winning VP8L rectangle, from either lossless or near-lossless selection.
#[derive(Debug)]
pub struct LosslessCandidate {
    pub frame: EncodedFrame,
    pub rect: FrameRectangle,
    /// Duration zero and disposal None until sequence caching sets them.
    pub control: ContainerFrameOptions,
    /// Applied to the preceding frame, not this candidate. Keyframe evaluation
    /// leaves the previous disposal untouched and returns `None`.
    pub previous_dispose: Option<FrameDispose>,
    /// Packed rectangle-relative mask, or empty when no pixel carries over.
    pub carryover_mask: Vec<u8>,
}

impl LosslessCandidate {
    /// Complete still-file size used by the pin's strict candidate comparison.
    pub fn encoded_size(&self) -> usize {
        // These VP8L payload sizes were checked at the still-writer boundary.
        (12 + self
            .frame
            .payload
            .disk_size()
            .expect("validated VP8L payload")) as usize
    }

    pub fn carries_over(&self) -> bool {
        !self.carryover_mask.is_empty()
    }

    /// Apply CacheFrame's canvas updates after selecting this candidate.
    /// `current` is the original input, before codec cleanup/transparency.
    /// Keyframes replace the entire carryover canvas. Subframes first clear
    /// the preceding rectangle if selected, then update explicitly encoded
    /// pixels within this rectangle, preserving carried pixels and its exterior.
    /// Finally save the original input in `previous`, including invisible RGB.
    /// `previous_dispose == None` identifies the keyframe branch; callers using
    /// `EncodeCandidate` directly must set this field for a subframe update.
    ///
    /// Before selecting a later frame, call `CopyIdenticalPixels(previous,
    /// current, carryover)` and pass that carryover to `SetFrameLossless` or
    /// `set_frame_near_lossless`. Carryover always retains original input words,
    /// even when near-lossless encoding changes the decoded pixels.
    /// A skipped frame has no candidate and must not advance `previous` or
    /// the previous rectangle. Timing and preceding-frame controls belong to
    /// the caller. All validation precedes mutations of either destination.
    pub fn update_canvases(
        &self,
        current: ArgbView<'_>,
        previous_rect: FrameRectangle,
        previous: &mut ArgbViewMut<'_>,
        carryover: &mut ArgbViewMut<'_>,
    ) -> Result<(), AnimationLeafError> {
        if (current.width, current.height) != (previous.width, previous.height)
            || (current.width, current.height) != (carryover.width, carryover.height)
        {
            return Err(AnimationLeafError::InvalidDimensions);
        }
        validate_rectangle(current, &self.rect)?;
        if IsEmptyRect(&self.rect) {
            return Err(AnimationLeafError::InvalidRectangle);
        }
        if self.previous_dispose.is_some() {
            validate_rectangle(current, &previous_rect)?;
            let area = (self.rect.width as usize)
                .checked_mul(self.rect.height as usize)
                .ok_or(AnimationLeafError::SizeOverflow)?;
            if self.carries_over() && self.carryover_mask.len() < area {
                return Err(AnimationLeafError::MaskTooShort);
            }
        }
        if let Some(dispose) = self.previous_dispose {
            DisposeFrameRectangle(dispose, &previous_rect, carryover)?;
            let source = current.sub_view(self.rect)?;
            let mut target = carryover.sub_view(self.rect)?;
            if self.carries_over() {
                CopyMaskedPixels(source, &self.carryover_mask, &mut target)?;
            } else {
                copy_pixels(source, &mut target);
            }
        } else {
            copy_pixels(current, carryover);
        }
        copy_pixels(current, previous);
        Ok(())
    }
}

fn copy_picture(view: ArgbView<'_>) -> Result<WebPPicture, AnimationCandidateError> {
    if view.width > 16383 || view.height > 16383 {
        return Err(WebPEncodingError::from(
            crate::enc::picture_csp_enc::RgbaInputError::InvalidDimensions,
        )
        .into());
    }
    let area = (view.width as usize)
        .checked_mul(view.height as usize)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    let mut argb = Vec::new();
    argb.try_reserve_exact(area)?;
    for y in 0..view.height as usize {
        argb.extend_from_slice(view.row(y).unwrap());
    }
    Ok(WebPPicture {
        width: view.width as u32,
        height: view.height as u32,
        argb,
    })
}

fn zero_mask(len: usize) -> Result<Vec<u8>, TryReserveError> {
    let mut mask = Vec::new();
    mask.try_reserve_exact(len)?;
    mask.resize(len, 0);
    Ok(mask)
}

/// Encode the lossless subset of upstream EncodeCandidate from a subframe.
/// Origins must already be even and dimensions match the view. The source is
/// copied before codec cleanup. This does not apply transparency substitution.
pub fn EncodeCandidate(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
    options: &LosslessOptions,
    use_blending: bool,
) -> Result<LosslessCandidate, AnimationCandidateError> {
    encode_near_lossless_candidate(sub_frame, rect, options, 100, use_blending)
}

/// VP8L EncodeCandidate with explicit near-lossless strength. The checked
/// subframe is copied, then the existing codec chooses palette, predictor or
/// standalone preprocessing on that rectangle. Strength 100 is EncodeCandidate.
/// Lower strengths can change RGB and intermediate alpha; `exact` only disables
/// codec cleanup/predictor quantization, not standalone preprocessing.
pub fn encode_near_lossless_candidate(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
    options: &LosslessOptions,
    strength: i32,
    use_blending: bool,
) -> Result<LosslessCandidate, AnimationCandidateError> {
    let near_lossless = checked_strength(strength)?;
    WebPValidateConfig(options)?;
    if rect.x_offset < 0
        || rect.y_offset < 0
        || rect.x_offset & 1 != 0
        || rect.y_offset & 1 != 0
        || rect.x_offset >= 1 << 24
        || rect.y_offset >= 1 << 24
        || rect.width != sub_frame.width
        || rect.height != sub_frame.height
    {
        return Err(AnimationLeafError::InvalidRectangle.into());
    }
    let picture = copy_picture(sub_frame)?;
    encode_picture(picture, rect, options, near_lossless, use_blending)
}

pub(super) fn checked_strength(strength: i32) -> Result<u32, WebPEncodingError> {
    if !(0..=100).contains(&strength) {
        return Err(WebPEncodingError::InvalidNearLosslessStrength);
    }
    Ok(strength as u32)
}

fn encode_picture(
    mut picture: WebPPicture,
    rect: FrameRectangle,
    options: &LosslessOptions,
    near_lossless: u32,
    use_blending: bool,
) -> Result<LosslessCandidate, AnimationCandidateError> {
    let config = EncoderConfig {
        options: *options,
        near_lossless,
    };
    let frame = crate::enc::webp_enc::WebPEncode(&config, &mut picture, |image| {
        image.into_frame(rect.width as u32, rect.height as u32)
    })?;
    Ok(LosslessCandidate {
        frame,
        rect,
        control: ContainerFrameOptions {
            x_offset: rect.x_offset as u32,
            y_offset: rect.y_offset as u32,
            blend: if use_blending {
                FrameBlend::Blend
            } else {
                FrameBlend::Replace
            },
            ..Default::default()
        },
        previous_dispose: None,
        carryover_mask: Vec::new(),
    })
}

/// Strict comparison preserves the earlier candidate on an equal byte size.
/// Dropping the loser releases its payload and mask. Previous-frame disposal
/// and the winning carryover mask travel with the selected candidate.
pub fn PickBestCandidate(best: &mut Option<LosslessCandidate>, candidate: LosslessCandidate) {
    if best
        .as_ref()
        .is_none_or(|old| candidate.encoded_size() < old.encoded_size())
    {
        *best = Some(candidate);
    }
}

fn GenerateCandidates(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    rect: FrameRectangle,
    dispose: FrameDispose,
    options: &LosslessCandidateOptions,
    near_lossless: u32,
    best: &mut Option<LosslessCandidate>,
) -> Result<(), AnimationCandidateError> {
    let use_blending =
        !options.is_key_frame && IsLosslessBlendingPossible(previous, current, &rect);
    // A fresh copy for each alternative is the safe counterpart of
    // CopyCurrentCanvas after codec cleanup/transparency modified a candidate.
    let mut picture = copy_picture(current.sub_view(rect)?)?;
    let mut mask = zero_mask(if use_blending { picture.argb.len() } else { 0 })?;
    let carries_over = if use_blending {
        let source = previous.sub_view(rect)?;
        let mut target = ArgbViewMut::new(
            &mut picture.argb,
            rect.width,
            rect.height,
            rect.width as usize,
        )?;
        IncreaseTransparency(
            source,
            &FrameRectangle {
                width: rect.width,
                height: rect.height,
                ..Default::default()
            },
            &mut target,
            &mut mask,
        )?
    } else {
        false
    };
    let mut candidate = encode_picture(picture, rect, &options.codec, near_lossless, use_blending)?;
    if carries_over {
        candidate.carryover_mask = mask;
    }
    if !options.is_key_frame {
        candidate.previous_dispose = Some(dispose);
    }
    PickBestCandidate(best, candidate);
    Ok(())
}

/// Lossless-only port of SetFrame. `previous` is the optimizer's carryover
/// canvas, which can differ from the preceding original input. `previous_rect`
/// is the rectangle that would be cleared by background disposal. Returns
/// `None` for an unchanged frame; timing merging belongs to the cache.
/// Does not update either canvas or enable the public Optimized policy.
pub fn SetFrameLossless(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &LosslessCandidateOptions,
) -> Result<Option<LosslessCandidate>, AnimationCandidateError> {
    set_frame_near_lossless(previous, current, previous_rect, options, 100)
}

/// SetFrame's VP8L path with explicit near-lossless strength. Comparison,
/// rectangles, blend eligibility and masks still use original ARGB words.
/// Transparency substitution precedes codec routing. Carryover updates retain
/// original input words, not decoded pixels; playback below 100 is not lossless.
pub fn set_frame_near_lossless(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &LosslessCandidateOptions,
    strength: i32,
) -> Result<Option<LosslessCandidate>, AnimationCandidateError> {
    let near_lossless = checked_strength(strength)?;
    WebPValidateConfig(&options.codec)?;
    if options.is_first_frame && !options.is_key_frame {
        return Err(AnimationCandidateError::InvalidFirstFrame);
    }
    if (previous.width, previous.height) != (current.width, current.height) {
        return Err(AnimationLeafError::InvalidDimensions.into());
    }
    if current.width > 16383 || current.height > 16383 {
        return Err(WebPEncodingError::from(
            crate::enc::picture_csp_enc::RgbaInputError::InvalidDimensions,
        )
        .into());
    }
    validate_rectangle(previous, &previous_rect)?;
    let mut none = SubFrameParams::new(true, !options.is_first_frame);
    GetSubRects(
        previous,
        current,
        options.is_key_frame,
        options.is_first_frame,
        options.codec.quality as f32,
        &mut none,
    )?;
    if IsEmptyRect(&none.rect_ll) {
        return Ok(None);
    }
    let mut disposed_picture = None;
    let mut background = SubFrameParams::new(false, false);
    if !options.is_key_frame && !options.previous_candidate_undecided {
        let mut picture = copy_picture(previous)?;
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
            options.codec.quality as f32,
            &mut background,
        )?;
        if options.minimize_size {
            background.should_try = true;
        } else if (background.rect_ll.width as u64 * background.rect_ll.height as u64)
            < (none.rect_ll.width as u64 * none.rect_ll.height as u64)
        {
            background.should_try = true;
            none.should_try = false;
        }
    }
    let mut best = None;
    if none.should_try {
        GenerateCandidates(
            previous,
            current,
            none.rect_ll,
            FrameDispose::None,
            options,
            near_lossless,
            &mut best,
        )?;
    }
    if background.should_try {
        GenerateCandidates(
            disposed_view.expect("background alternative"),
            current,
            background.rect_ll,
            FrameDispose::Background,
            options,
            near_lossless,
            &mut best,
        )?;
    }
    Ok(best)
}
