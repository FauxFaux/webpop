// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! VP8 subset of SetFrame/GenerateCandidates and original-word carryover.

use super::*;
use crate::enc::config_enc::LossyConfig;
use crate::enc::picture_csp_enc::RgbaInputError;
use crate::mux::{ContainerFrameOptions, EncodedFrame, FrameBlend, FrameDispose};
use crate::{WebPEncodingError, internal::LossyOptions};

/// VP8 candidate controls without mixed trials or keyframe insertion policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct LossyCandidateOptions {
    pub codec: LossyOptions,
    pub is_key_frame: bool,
    pub is_first_frame: bool,
    pub previous_candidate_undecided: bool,
    pub minimize_size: bool,
}

/// Encoded VP8 rectangle, selected preceding disposal and winning carryover mask.
#[derive(Debug)]
pub struct LossyCandidate {
    pub frame: EncodedFrame,
    pub rect: FrameRectangle,
    /// Duration zero and disposal None until sequence caching sets them.
    pub control: ContainerFrameOptions,
    /// Applied to the preceding frame. None identifies keyframe evaluation.
    pub previous_dispose: Option<FrameDispose>,
    /// Packed rectangle-relative mask, or empty when no block carries over.
    pub carryover_mask: Vec<u8>,
}

impl LossyCandidate {
    /// Complete still-file size used by the pin's candidate comparisons,
    /// including VP8X/ALPH for transparent VP8 output.
    pub fn encoded_size(&self) -> usize {
        let extended = if self.frame.has_alpha() { 18 } else { 0 };
        (12 + extended
            + self
                .frame
                .payload
                .disk_size()
                .expect("validated VP8 payload")) as usize
    }

    pub fn carries_over(&self) -> bool {
        !self.carryover_mask.is_empty()
    }

    /// Apply CacheFrame's canvas updates using original input words. Dispose the
    /// preceding rectangle before masked copying, preserving carried pixels and
    /// the rectangle exterior, then save the original input in `previous`.
    /// Keyframe evaluation copies the full original canvas into both targets.
    /// No decoded VP8 pixels or transparent average colors enter either canvas.
    /// Before subsequent selection, restore unchanged original words with
    /// CopyIdenticalPixels. Skipped candidates leave `previous` and its rectangle
    /// unchanged, even when lossy similarity skipped different original pixels.
    /// Direct encode_lossy_candidate callers must set previous_dispose for a
    /// subframe update. All validation precedes mutation of either destination.
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

/// Encode the lossy branch of upstream EncodeCandidate. Blended rectangles use
/// filter strength zero; replacement rectangles retain the default preset.
/// Both paths keep autofilter disabled and carry method 0–6, alpha quality and
/// SharpYUV through the existing codec. Origins must already be even and the
/// rectangle dimensions match the view. Source words and padding are preserved.
/// This does not apply transparency substitution or check blend eligibility.
pub fn encode_lossy_candidate(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
    options: &LossyOptions,
    use_blending: bool,
) -> Result<LossyCandidate, AnimationCandidateError> {
    encode_candidate_with_exact(sub_frame, rect, options, use_blending, false)
}

pub(super) fn encode_candidate_with_exact(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
    options: &LossyOptions,
    use_blending: bool,
    exact: bool,
) -> Result<LossyCandidate, AnimationCandidateError> {
    let mut config = LossyConfig::for_animation(options, use_blending)?;
    config.exact = exact;
    validate_candidate_rectangle(sub_frame, rect)?;
    if sub_frame.width > 16383 || sub_frame.height > 16383 {
        return Err(WebPEncodingError::from(RgbaInputError::InvalidDimensions).into());
    }
    let stride = (sub_frame.width as usize)
        .checked_mul(4)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    let size = stride
        .checked_mul(sub_frame.height as usize)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(size)?;
    for y in 0..sub_frame.height as usize {
        for &p in sub_frame.row(y).unwrap() {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
    }
    let frame = crate::encode::encode_lossy_rgba_with_config(
        &rgba,
        rect.width as u32,
        rect.height as u32,
        stride,
        &config,
        options.use_sharp_yuv,
        |image| image.into_frame(rect.width as u32, rect.height as u32),
    )?;
    Ok(LossyCandidate {
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

fn GenerateCandidates(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    rect: FrameRectangle,
    dispose: FrameDispose,
    options: &LossyCandidateOptions,
    best: &mut Option<LossyCandidate>,
    exact: bool,
) -> Result<(), AnimationCandidateError> {
    let candidate = generate_candidate(previous, current, rect, dispose, options, exact)?;
    // Equal byte sizes retain the first candidate's disposal and mask.
    if best
        .as_ref()
        .is_none_or(|old| candidate.encoded_size() < old.encoded_size())
    {
        *best = Some(candidate);
    }
    Ok(())
}

pub(super) fn generate_candidate(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    rect: FrameRectangle,
    dispose: FrameDispose,
    options: &LossyCandidateOptions,
    exact: bool,
) -> Result<LossyCandidate, AnimationCandidateError> {
    let blending = !options.is_key_frame
        && IsLossyBlendingPossible(previous, current, &rect, options.codec.quality);
    // FlattenSimilarBlocks uses the original canvas grid, not a crop-local grid.
    // Restore the original input for each disposal trial before flattening.
    let mut picture = super::lossless::copy_picture(current)?;
    let mut mask = super::lossless::zero_mask(if blending {
        (rect.width as usize)
            .checked_mul(rect.height as usize)
            .ok_or(AnimationLeafError::SizeOverflow)?
    } else {
        0
    })?;
    let carries = if blending {
        FlattenSimilarBlocks(
            previous,
            &rect,
            &mut ArgbViewMut::new(
                &mut picture.argb,
                current.width,
                current.height,
                current.width as usize,
            )?,
            options.codec.quality,
            &mut mask,
        )?
    } else {
        false
    };
    let view = ArgbView::new(
        &picture.argb,
        current.width,
        current.height,
        current.width as usize,
    )?;
    let mut candidate =
        encode_candidate_with_exact(view.sub_view(rect)?, rect, &options.codec, blending, exact)?;
    if carries {
        candidate.carryover_mask = mask;
    }
    if !options.is_key_frame {
        candidate.previous_dispose = Some(dispose);
    }
    Ok(candidate)
}

/// Lossy-only SetFrame. `previous` is the carryover canvas, restored with
/// CopyIdenticalPixels before selection. Geometry and blend eligibility use
/// original words and fractional quality. Each disposal alternative starts with
/// a fresh canvas copy, then flattens on the global 8×8 grid before cropping and
/// codec conversion. Returns None for lossy-similar changes; the caller owns
/// timing merging, canvas updates and preceding-frame control changes.
/// Timestamp caching and keyframes belong to the shared cache used by RGBA Optimized.
pub fn SetFrameLossy(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &LossyCandidateOptions,
) -> Result<Option<LossyCandidate>, AnimationCandidateError> {
    set_frame_with_exact(previous, current, previous_rect, options, false)
}

pub(super) fn set_frame_with_exact(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &LossyCandidateOptions,
    exact: bool,
) -> Result<Option<LossyCandidate>, AnimationCandidateError> {
    LossyConfig::from_options(&options.codec)?;
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
        options.codec.quality,
        &mut none,
    )?;
    if IsEmptyRect(&none.rect_lossy) {
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
            options.codec.quality,
            &mut background,
        )?;
        if options.minimize_size {
            background.should_try = true;
        } else if background.rect_lossy.width as u64 * (background.rect_lossy.height as u64)
            < none.rect_lossy.width as u64 * (none.rect_lossy.height as u64)
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
            none.rect_lossy,
            FrameDispose::None,
            options,
            &mut best,
            exact,
        )?;
    }
    if background.should_try {
        GenerateCandidates(
            disposed_view.expect("background alternative"),
            current,
            background.rect_lossy,
            FrameDispose::Background,
            options,
            &mut best,
            exact,
        )?;
    }
    Ok(best)
}
