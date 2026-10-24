// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/mux/anim_encode.c at
// 1844042d1ca751e62e61df7617dba19c98588339.
// Use of this source code is governed by the BSD-style license in COPYING.
// See PATENTS for the additional intellectual property rights grant and
// AUTHORS for the contributing project authors.

//! Pixel comparisons, change rectangles and blend eligibility from the animation
//! optimizer. Pixels are straight-alpha **ARGB words**, not RGBA byte buffers.
//! Lossless comparisons include invisible RGB. Lossy similarity requires equal
//! alpha and weights RGB differences by that alpha.

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
    #[error("rectangle width and height must be positive")]
    InvalidRectangle,
    #[error("quality must be finite and in 0..=100")]
    InvalidQuality,
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

/// Animation's alpha-weighted lossy blend heuristic. This does not enable
/// blended lossy encoding, which also requires the internal filter override.
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
