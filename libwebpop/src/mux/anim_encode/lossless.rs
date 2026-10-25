// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339. See COPYING and PATENTS.

//! Lossless subset of SetFrame/GenerateCandidates. Sequence caching, keyframe
//! decisions and carryover canvas updates are deliberately separate.

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

/// Supported subset of the upstream candidate controls: exact lossless VP8L
/// (strength 100), without mixed/lossy trials. `exact` is codec-level only:
/// transparency substitution can still discard invisible RGB during blending.
#[derive(Clone, Copy, Debug, Default)]
pub struct LosslessCandidateOptions {
    pub codec: LosslessOptions,
    pub is_key_frame: bool,
    pub is_first_frame: bool,
    pub previous_candidate_undecided: bool,
    pub minimize_size: bool,
}

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
    encode_picture(picture, rect, options, use_blending)
}

fn encode_picture(
    mut picture: WebPPicture,
    rect: FrameRectangle,
    options: &LosslessOptions,
    use_blending: bool,
) -> Result<LosslessCandidate, AnimationCandidateError> {
    let config = EncoderConfig {
        options: *options,
        near_lossless: 100,
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
    let mut candidate = encode_picture(picture, rect, &options.codec, use_blending)?;
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
/// `None` for an unchanged frame; timing merging belongs to the future cache.
/// Does not update either canvas or enable the public Optimized policy.
pub fn SetFrameLossless(
    previous: ArgbView<'_>,
    current: ArgbView<'_>,
    previous_rect: FrameRectangle,
    options: &LosslessCandidateOptions,
) -> Result<Option<LosslessCandidate>, AnimationCandidateError> {
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
            &mut best,
        )?;
    }
    Ok(best)
}
