// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339.
// Use of this source code is governed by the BSD-style license in COPYING.
// See PATENTS for the additional intellectual property rights grant and
// AUTHORS for the contributing project authors.

//! Pixel comparisons, subframe views, change rectangles and blend eligibility
//! from the animation optimizer. Pixels are straight-alpha **ARGB words**,
//! not RGBA byte buffers.
//! Lossless comparisons include invisible RGB. Lossy similarity requires equal
//! alpha and weights RGB differences by that alpha.

mod cache;
mod lossless;
mod lossy;
pub use cache::{LosslessAnimationCache, LosslessAnimationCacheOptions, LosslessCacheStatus};
pub use lossless::{
    AnimationCandidateError, EncodeCandidate, LosslessCandidate, LosslessCandidateOptions,
    PickBestCandidate, SetFrameLossless, encode_near_lossless_candidate, set_frame_near_lossless,
};
pub use lossy::{LossyCandidate, LossyCandidateOptions, SetFrameLossy, encode_lossy_candidate};

fn validate_candidate_rectangle(
    sub_frame: ArgbView<'_>,
    rect: FrameRectangle,
) -> Result<(), AnimationLeafError> {
    if rect.x_offset < 0
        || rect.y_offset < 0
        || rect.x_offset & 1 != 0
        || rect.y_offset & 1 != 0
        || rect.x_offset >= 1 << 24
        || rect.y_offset >= 1 << 24
        || rect.width != sub_frame.width
        || rect.height != sub_frame.height
    {
        return Err(AnimationLeafError::InvalidRectangle);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameRectangle {
    pub x_offset: i32,
    pub y_offset: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnimationLeafError {
    #[error("canvas dimensions must be positive and equal")]
    InvalidDimensions,
    #[error("ARGB stride is smaller than width")]
    InvalidStride,
    #[error("ARGB buffer length arithmetic overflowed")]
    SizeOverflow,
    #[error("ARGB buffer is shorter than the last pixel")]
    InputTooShort,
    #[error("rectangle has invalid extents or lies outside the canvas")]
    InvalidRectangle,
    #[error("quality must be finite and in 0..=100")]
    InvalidQuality,
    #[error("carryover mask is shorter than the rectangle area")]
    MaskTooShort,
}

/// Borrowed, checked ARGB canvas. Stride is in pixels; final-row padding is
/// optional. No pixel codec dimension limit is imposed by these helpers.
#[derive(Clone, Copy, Debug)]
pub struct ArgbView<'a> {
    pixels: &'a [u32],
    width: i32,
    height: i32,
    stride: usize,
}

impl<'a> ArgbView<'a> {
    pub fn new(
        pixels: &'a [u32],
        width: i32,
        height: i32,
        stride: usize,
    ) -> Result<Self, AnimationLeafError> {
        if width <= 0 || height <= 0 {
            return Err(AnimationLeafError::InvalidDimensions);
        }
        if stride < width as usize {
            return Err(AnimationLeafError::InvalidStride);
        }
        let len = (height as usize - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(width as usize))
            .ok_or(AnimationLeafError::SizeOverflow)?;
        if pixels.len() < len {
            return Err(AnimationLeafError::InputTooShort);
        }
        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    fn offset(self, x: i32, y: i32) -> usize {
        y as usize * self.stride + x as usize
    }

    pub fn width(self) -> i32 {
        self.width
    }

    pub fn height(self) -> i32 {
        self.height
    }

    /// Pixel stride inherited from the original canvas, including on subviews.
    pub fn stride(self) -> usize {
        self.stride
    }

    /// A visible row, excluding padding; out-of-range rows return `None`.
    pub fn row(self, y: usize) -> Option<&'a [u32]> {
        if y >= self.height as usize {
            return None;
        }
        let start = y * self.stride;
        Some(&self.pixels[start..start + self.width as usize])
    }

    /// Borrow a nonempty, contained ARGB rectangle without copying. Like the
    /// ARGB branch of WebPPictureView, odd origins are preserved. Nested views
    /// use coordinates relative to the parent and retain the canvas stride.
    pub fn sub_view(self, rect: FrameRectangle) -> Result<Self, AnimationLeafError> {
        validate_rectangle(self, &rect)?;
        if IsEmptyRect(&rect) {
            return Err(AnimationLeafError::InvalidRectangle);
        }
        let start = self.offset(rect.x_offset, rect.y_offset);
        let end = self.offset(rect.x_offset, rect.y_offset + rect.height - 1) + rect.width as usize;
        Self::new(
            &self.pixels[start..end],
            rect.width,
            rect.height,
            self.stride,
        )
    }
}

/// Checked mutable ARGB storage. Mutations preserve row padding and pixels
/// outside their rectangle; shared views borrow this storage temporarily.
#[derive(Debug)]
pub struct ArgbViewMut<'a> {
    pixels: &'a mut [u32],
    width: i32,
    height: i32,
    stride: usize,
}

impl<'a> ArgbViewMut<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: i32,
        height: i32,
        stride: usize,
    ) -> Result<Self, AnimationLeafError> {
        ArgbView::new(pixels, width, height, stride)?;
        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    pub fn as_view(&self) -> ArgbView<'_> {
        ArgbView {
            pixels: self.pixels,
            width: self.width,
            height: self.height,
            stride: self.stride,
        }
    }

    pub fn row_mut(&mut self, y: usize) -> Option<&mut [u32]> {
        if y >= self.height as usize {
            return None;
        }
        let start = y * self.stride;
        Some(&mut self.pixels[start..start + self.width as usize])
    }

    pub fn sub_view(
        &mut self,
        rect: FrameRectangle,
    ) -> Result<ArgbViewMut<'_>, AnimationLeafError> {
        let view = self.as_view().sub_view(rect)?;
        let len = view.pixels.len();
        let start = self.as_view().offset(rect.x_offset, rect.y_offset);
        ArgbViewMut::new(
            &mut self.pixels[start..start + len],
            rect.width,
            rect.height,
            self.stride,
        )
    }
}

/// Clear disposal to transparent black, independently of ANIM background
/// metadata. Validation precedes mutation, including for a no-disposal call.
pub fn DisposeFrameRectangle(
    dispose: super::FrameDispose,
    rect: &FrameRectangle,
    canvas: &mut ArgbViewMut<'_>,
) -> Result<(), AnimationLeafError> {
    validate_rectangle(canvas.as_view(), rect)?;
    if dispose == super::FrameDispose::Background {
        for y in rect.y_offset..rect.y_offset + rect.height {
            canvas.row_mut(y as usize).unwrap()[rect.x_offset as usize..][..rect.width as usize]
                .fill(0);
        }
    }
    Ok(())
}

/// Replace identical, nonzero ARGB words with transparent black and set their
/// rectangle-relative mask bytes to 1. Other mask bytes retain their values;
/// an already-zero word is not marked. Validate everything before mutation.
pub fn IncreaseTransparency(
    src: ArgbView<'_>,
    rect: &FrameRectangle,
    dst: &mut ArgbViewMut<'_>,
    carryover_mask: &mut [u8],
) -> Result<bool, AnimationLeafError> {
    if (src.width, src.height) != (dst.width, dst.height) {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    validate_rectangle(dst.as_view(), rect)?;
    let area = (rect.width as usize)
        .checked_mul(rect.height as usize)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    if carryover_mask.len() < area {
        return Err(AnimationLeafError::MaskTooShort);
    }
    let mut modified = false;
    for y in 0..rect.height as usize {
        let source = src.row(rect.y_offset as usize + y).unwrap();
        let target = dst.row_mut(rect.y_offset as usize + y).unwrap();
        for x in 0..rect.width as usize {
            let i = rect.x_offset as usize + x;
            if source[i] == target[i] && target[i] != 0 {
                target[i] = 0;
                carryover_mask[y * rect.width as usize + x] = 1;
                modified = true;
            }
        }
    }
    Ok(modified)
}

/// Replace fully opaque, similar 8×8 blocks with transparent source-average RGB
/// and mark all 64 pixels as carried. Blocks use canvas coordinates: the first
/// block starts strictly after the rectangle's left/top origin, even when the
/// origin is already aligned. Partial edge blocks are untouched. Mask bytes
/// outside replaced blocks, including any trailing bytes, retain their values.
/// Validate dimensions, quality, rectangle and mask before modifying anything.
pub fn FlattenSimilarBlocks(
    src: ArgbView<'_>,
    rect: &FrameRectangle,
    dst: &mut ArgbViewMut<'_>,
    quality: f32,
    carryover_mask: &mut [u8],
) -> Result<bool, AnimationLeafError> {
    if (src.width, src.height) != (dst.width, dst.height) {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    if !(0.0..=100.0).contains(&quality) {
        return Err(AnimationLeafError::InvalidQuality);
    }
    validate_rectangle(dst.as_view(), rect)?;
    let area = (rect.width as usize)
        .checked_mul(rect.height as usize)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    if carryover_mask.len() < area {
        return Err(AnimationLeafError::MaskTooShort);
    }
    let max_diff = QualityToMaxDiff(quality);
    // usize avoids signed overflow for valid leaf canvases near i32::MAX.
    let (left, top) = (rect.x_offset as usize, rect.y_offset as usize);
    let x_start = (left + 8) & !7;
    let y_start = (top + 8) & !7;
    let x_end = (left + rect.width as usize) & !7;
    let y_end = (top + rect.height as usize) & !7;
    let mut modified = false;
    for y in (y_start..y_end).step_by(8) {
        for x in (x_start..x_end).step_by(8) {
            let mut sums = [0u32; 3];
            let mut count = 0;
            for dy in 0..8 {
                let source = src.row(y + dy).unwrap();
                let target = dst.as_view().row(y + dy).unwrap();
                for dx in 0..8 {
                    let p = source[x + dx];
                    if p >> 24 == 255 && PixelsAreSimilar(p, target[x + dx], max_diff) {
                        count += 1;
                        for (sum, shift) in sums.iter_mut().zip([16, 8, 0]) {
                            *sum += (p >> shift) & 255;
                        }
                    }
                }
            }
            if count == 64 {
                let color = ((sums[0] / 64) << 16) | ((sums[1] / 64) << 8) | (sums[2] / 64);
                for dy in 0..8 {
                    dst.row_mut(y + dy).unwrap()[x..x + 8].fill(color);
                    let at = (y + dy - top) * rect.width as usize + x - left;
                    carryover_mask[at..at + 8].fill(1);
                }
                modified = true;
            }
        }
    }
    Ok(modified)
}

/// Copy complete ARGB words equal in both original inputs into the carryover
/// canvas before selecting a later frame. Other pixels and padding stay intact.
/// This includes invisible RGB and does not compare decoded codec output.
pub fn CopyIdenticalPixels(
    a: ArgbView<'_>,
    b: ArgbView<'_>,
    dst: &mut ArgbViewMut<'_>,
) -> Result<(), AnimationLeafError> {
    if (a.width, a.height) != (b.width, b.height) || (a.width, a.height) != (dst.width, dst.height)
    {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    for y in 0..a.height as usize {
        let row_a = a.row(y).unwrap();
        let row_b = b.row(y).unwrap();
        let row_dst = dst.row_mut(y).unwrap();
        for x in 0..a.width as usize {
            if row_a[x] == row_b[x] {
                row_dst[x] = row_a[x];
            }
        }
    }
    Ok(())
}

/// Copy pixels with a zero mask byte; every nonzero byte retains the old
/// destination pixel. The mask is packed with the view width, independently
/// of either pixel stride. Extra mask bytes and row padding are untouched.
pub fn CopyMaskedPixels(
    src: ArgbView<'_>,
    mask: &[u8],
    dst: &mut ArgbViewMut<'_>,
) -> Result<(), AnimationLeafError> {
    if (src.width, src.height) != (dst.width, dst.height) {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    let area = (src.width as usize)
        .checked_mul(src.height as usize)
        .ok_or(AnimationLeafError::SizeOverflow)?;
    if mask.len() < area {
        return Err(AnimationLeafError::MaskTooShort);
    }
    for y in 0..src.height as usize {
        let source = src.row(y).unwrap();
        let target = dst.row_mut(y).unwrap();
        for x in 0..src.width as usize {
            if mask[y * src.width as usize + x] == 0 {
                target[x] = source[x];
            }
        }
    }
    Ok(())
}

fn copy_pixels(src: ArgbView<'_>, dst: &mut ArgbViewMut<'_>) {
    debug_assert_eq!((src.width, src.height), (dst.width, dst.height));
    for y in 0..src.height as usize {
        dst.row_mut(y).unwrap().copy_from_slice(src.row(y).unwrap());
    }
}

// CacheFrame's canvas updates are identical for VP8L and VP8. Keep original
// ARGB words here; codec reconstruction and flattened colors are not history.
fn update_candidate_canvases(
    rect: FrameRectangle,
    previous_dispose: Option<super::FrameDispose>,
    mask: &[u8],
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
    validate_rectangle(current, &rect)?;
    if IsEmptyRect(&rect) {
        return Err(AnimationLeafError::InvalidRectangle);
    }
    if previous_dispose.is_some() {
        validate_rectangle(current, &previous_rect)?;
        let area = (rect.width as usize)
            .checked_mul(rect.height as usize)
            .ok_or(AnimationLeafError::SizeOverflow)?;
        if !mask.is_empty() && mask.len() < area {
            return Err(AnimationLeafError::MaskTooShort);
        }
    }
    if let Some(dispose) = previous_dispose {
        DisposeFrameRectangle(dispose, &previous_rect, carryover)?;
        let source = current.sub_view(rect)?;
        let mut target = carryover.sub_view(rect)?;
        if mask.is_empty() {
            copy_pixels(source, &mut target);
        } else {
            CopyMaskedPixels(source, mask, &mut target)?;
        }
    } else {
        copy_pixels(current, carryover);
    }
    copy_pixels(current, previous);
    Ok(())
}

fn validate_rectangle(
    canvas: ArgbView<'_>,
    rect: &FrameRectangle,
) -> Result<(), AnimationLeafError> {
    if rect.x_offset < 0
        || rect.y_offset < 0
        || rect.width < 0
        || rect.height < 0
        || i64::from(rect.x_offset) + i64::from(rect.width) > i64::from(canvas.width)
        || i64::from(rect.y_offset) + i64::from(rect.height) > i64::from(canvas.height)
    {
        return Err(AnimationLeafError::InvalidRectangle);
    }
    Ok(())
}

/// Parameters for one disposal alternative. Views borrow the current canvas;
/// `None` represents an allowed empty rectangle, never a zero-sized picture.
/// `should_try` is carried through rectangle routing for later candidate choice.
#[derive(Clone, Copy, Debug)]
pub struct SubFrameParams<'a> {
    pub should_try: bool,
    pub empty_rect_allowed: bool,
    pub rect_ll: FrameRectangle,
    pub sub_frame_ll: Option<ArgbView<'a>>,
    pub rect_lossy: FrameRectangle,
    pub sub_frame_lossy: Option<ArgbView<'a>>,
}

impl SubFrameParams<'_> {
    /// Safe counterpart of SubFrameParamsInit; no allocation or explicit free
    /// is needed for borrowed Rust views.
    pub fn new(should_try: bool, empty_rect_allowed: bool) -> Self {
        Self {
            should_try,
            empty_rect_allowed,
            rect_ll: FrameRectangle::default(),
            sub_frame_ll: None,
            rect_lossy: FrameRectangle::default(),
            sub_frame_lossy: None,
        }
    }
}

/// Refine a valid contained guess and borrow the resulting current-canvas view.
/// Later keyframes keep the guess; first frames are minimized against the
/// supplied previous canvas (transparent in the encoder). Empty changes may
/// be skipped or forced to 1x1 at (0,0). Nonempty origins expand to even values.
/// Errors leave `rect` unchanged. Unlike [`WebPAnimEncoderRefineRect`], this
/// does not clip guesses.
pub fn GetSubRect<'a>(
    prev_canvas: ArgbView<'_>,
    curr_canvas: ArgbView<'a>,
    is_key_frame: bool,
    is_first_frame: bool,
    empty_rect_allowed: bool,
    is_lossless: bool,
    quality: f32,
    rect: &mut FrameRectangle,
) -> Result<Option<ArgbView<'a>>, AnimationLeafError> {
    if (prev_canvas.width, prev_canvas.height) != (curr_canvas.width, curr_canvas.height) {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    if !(0.0..=100.0).contains(&quality) {
        return Err(AnimationLeafError::InvalidQuality);
    }
    validate_rectangle(curr_canvas, rect)?;
    let mut refined = *rect;
    if !is_key_frame || is_first_frame {
        MinimizeChangeRectangle(prev_canvas, curr_canvas, &mut refined, is_lossless, quality);
    }
    let view = if IsEmptyRect(&refined) && empty_rect_allowed {
        None
    } else {
        if IsEmptyRect(&refined) {
            // The pin requires a canonical empty rectangle for forced holds.
            if refined.x_offset != 0 || refined.y_offset != 0 {
                return Err(AnimationLeafError::InvalidRectangle);
            }
            refined.width = 1;
            refined.height = 1;
        }
        SnapToEvenOffsets(&mut refined);
        Some(curr_canvas.sub_view(refined)?)
    };
    *rect = refined;
    Ok(view)
}

/// Route both codec rectangles in pinned order: full canvas -> lossless ->
/// lossy seeded from the refined lossless rectangle. This only selects views;
/// it does not encode candidates or enable optimized sequence output.
/// Errors leave all parameters unchanged; successful empty results clear views.
pub fn GetSubRects<'a>(
    prev_canvas: ArgbView<'_>,
    curr_canvas: ArgbView<'a>,
    is_key_frame: bool,
    is_first_frame: bool,
    quality: f32,
    params: &mut SubFrameParams<'a>,
) -> Result<(), AnimationLeafError> {
    let mut rect_ll = FrameRectangle {
        width: curr_canvas.width,
        height: curr_canvas.height,
        ..Default::default()
    };
    let sub_frame_ll = GetSubRect(
        prev_canvas,
        curr_canvas,
        is_key_frame,
        is_first_frame,
        params.empty_rect_allowed,
        true,
        quality,
        &mut rect_ll,
    )?;
    let mut rect_lossy = rect_ll;
    let sub_frame_lossy = GetSubRect(
        prev_canvas,
        curr_canvas,
        is_key_frame,
        is_first_frame,
        params.empty_rect_allowed,
        false,
        quality,
        &mut rect_lossy,
    )?;
    params.rect_ll = rect_ll;
    params.sub_frame_ll = sub_frame_ll;
    params.rect_lossy = rect_lossy;
    params.sub_frame_lossy = sub_frame_lossy;
    Ok(())
}

/// Compare a nonempty row or column; steps are in pixels.
#[inline]
pub fn ComparePixelsLossless(
    src: &[u32],
    src_step: usize,
    dst: &[u32],
    dst_step: usize,
    length: usize,
    _max_allowed_diff: i32,
) -> bool {
    assert!(length > 0);
    (0..length).all(|i| src[i * src_step] == dst[i * dst_step])
}

#[inline]
pub fn PixelsAreSimilar(src: u32, dst: u32, max_allowed_diff: i32) -> bool {
    assert!((0..=255).contains(&max_allowed_diff));
    let alpha = dst >> 24;
    src >> 24 == alpha
        && [16, 8, 0].into_iter().all(|shift| {
            ((src >> shift) & 255).abs_diff((dst >> shift) & 255) * alpha
                <= max_allowed_diff as u32 * 255
        })
}

#[inline]
pub fn ComparePixelsLossy(
    src: &[u32],
    src_step: usize,
    dst: &[u32],
    dst_step: usize,
    length: usize,
    max_allowed_diff: i32,
) -> bool {
    assert!(length > 0);
    (0..length).all(|i| PixelsAreSimilar(src[i * src_step], dst[i * dst_step], max_allowed_diff))
}

pub fn IsEmptyRect(rect: &FrameRectangle) -> bool {
    rect.width == 0 || rect.height == 0
}

pub fn QualityToMaxDiff(quality: f32) -> i32 {
    assert!((0.0..=100.0).contains(&quality));
    let val = (f64::from(quality) / 100.0).sqrt();
    (31.0 * (1.0 - val) + val + 0.5) as i32
}

fn assert_compatible(src: ArgbView<'_>, dst: ArgbView<'_>, rect: &FrameRectangle) {
    assert_eq!((src.width, src.height), (dst.width, dst.height));
    assert!(rect.x_offset >= 0 && rect.y_offset >= 0 && rect.width >= 0 && rect.height >= 0);
    assert!(i64::from(rect.x_offset) + i64::from(rect.width) <= i64::from(dst.width));
    assert!(i64::from(rect.y_offset) + i64::from(rect.height) <= i64::from(dst.height));
}

/// Shrink a valid initial guess to the changed pixels. An unchanged or empty
/// rectangle becomes (0, 0, 0, 0). Even-offset expansion is a separate step.
pub fn MinimizeChangeRectangle(
    src: ArgbView<'_>,
    dst: ArgbView<'_>,
    rect: &mut FrameRectangle,
    is_lossless: bool,
    quality: f32,
) {
    assert_compatible(src, dst, rect);
    let max_diff = QualityToMaxDiff(quality);
    let compare = if is_lossless {
        ComparePixelsLossless
    } else {
        ComparePixelsLossy
    };
    if IsEmptyRect(rect) {
        *rect = FrameRectangle::default();
        return;
    }
    let column_matches = |x, rect: &FrameRectangle| {
        compare(
            &src.pixels[src.offset(x, rect.y_offset)..],
            src.stride,
            &dst.pixels[dst.offset(x, rect.y_offset)..],
            dst.stride,
            rect.height as usize,
            max_diff,
        )
    };
    while rect.width > 0 && column_matches(rect.x_offset, rect) {
        rect.x_offset += 1;
        rect.width -= 1;
    }
    while rect.width > 0 && column_matches(rect.x_offset + rect.width - 1, rect) {
        rect.width -= 1;
    }
    if rect.width == 0 {
        *rect = FrameRectangle::default();
        return;
    }
    let row_matches = |y, rect: &FrameRectangle| {
        compare(
            &src.pixels[src.offset(rect.x_offset, y)..],
            1,
            &dst.pixels[dst.offset(rect.x_offset, y)..],
            1,
            rect.width as usize,
            max_diff,
        )
    };
    while rect.height > 0 && row_matches(rect.y_offset, rect) {
        rect.y_offset += 1;
        rect.height -= 1;
    }
    while rect.height > 0 && row_matches(rect.y_offset + rect.height - 1, rect) {
        rect.height -= 1;
    }
    if IsEmptyRect(rect) {
        *rect = FrameRectangle::default();
    }
}

/// Expand left/up to even origins, preserving the right and bottom edges.
/// Requires nonnegative fields and extents that fit i32 after expansion.
#[inline]
pub fn SnapToEvenOffsets(rect: &mut FrameRectangle) {
    assert!(rect.x_offset >= 0 && rect.y_offset >= 0 && rect.width >= 0 && rect.height >= 0);
    let width = rect.width.checked_add(rect.x_offset & 1).unwrap();
    let height = rect.height.checked_add(rect.y_offset & 1).unwrap();
    rect.width = width;
    rect.height = height;
    rect.x_offset &= !1;
    rect.y_offset &= !1;
}

/// Clip a caller's signed rectangle, minimize it, then expand to even origins.
/// Validation errors leave `rect` unchanged. Like pinned C, this searches only
/// inside the supplied guess; it does not discover changes outside it.
pub fn WebPAnimEncoderRefineRect(
    prev_canvas: ArgbView<'_>,
    curr_canvas: ArgbView<'_>,
    is_lossless: bool,
    quality: f32,
    rect: &mut FrameRectangle,
) -> Result<(), AnimationLeafError> {
    if (prev_canvas.width, prev_canvas.height) != (curr_canvas.width, curr_canvas.height) {
        return Err(AnimationLeafError::InvalidDimensions);
    }
    if rect.width <= 0 || rect.height <= 0 {
        return Err(AnimationLeafError::InvalidRectangle);
    }
    if !(0.0..=100.0).contains(&quality) {
        return Err(AnimationLeafError::InvalidQuality);
    }
    // Upstream's inline clip uses i64 for the caller's potentially overflowing
    // signed offset + extent. clamp has the same semantics here.
    let right = (i64::from(rect.x_offset) + i64::from(rect.width))
        .clamp(0, i64::from(curr_canvas.width)) as i32;
    let bottom = (i64::from(rect.y_offset) + i64::from(rect.height))
        .clamp(0, i64::from(curr_canvas.height)) as i32;
    let left = rect.x_offset.clamp(0, curr_canvas.width);
    let top = rect.y_offset.clamp(0, curr_canvas.height);
    let mut clipped = FrameRectangle {
        x_offset: left,
        y_offset: top,
        width: (right - left).clamp(0, curr_canvas.width - left),
        height: (bottom - top).clamp(0, curr_canvas.height - top),
    };
    MinimizeChangeRectangle(prev_canvas, curr_canvas, &mut clipped, is_lossless, quality);
    SnapToEvenOffsets(&mut clipped);
    *rect = clipped;
    Ok(())
}

/// Whether a cropped lossless frame can use blending to reproduce `dst`.
pub fn IsLosslessBlendingPossible(
    src: ArgbView<'_>,
    dst: ArgbView<'_>,
    rect: &FrameRectangle,
) -> bool {
    assert_compatible(src, dst, rect);
    (rect.y_offset..rect.y_offset + rect.height).all(|y| {
        (rect.x_offset..rect.x_offset + rect.width).all(|x| {
            let a = src.pixels[src.offset(x, y)];
            let b = dst.pixels[dst.offset(x, y)];
            b >> 24 == 255 || a == b
        })
    })
}

/// Animation's alpha-weighted lossy blend heuristic. SetFrameLossy applies it
/// before flattening and the blended candidate's internal filter override.
pub fn IsLossyBlendingPossible(
    src: ArgbView<'_>,
    dst: ArgbView<'_>,
    rect: &FrameRectangle,
    quality: f32,
) -> bool {
    assert_compatible(src, dst, rect);
    let max_diff = QualityToMaxDiff(quality);
    (rect.y_offset..rect.y_offset + rect.height).all(|y| {
        (rect.x_offset..rect.x_offset + rect.width).all(|x| {
            let a = src.pixels[src.offset(x, y)];
            let b = dst.pixels[dst.offset(x, y)];
            b >> 24 == 255 || PixelsAreSimilar(a, b, max_diff)
        })
    })
}
