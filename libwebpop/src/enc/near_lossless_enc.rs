// Copyright 2014 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/near_lossless_enc.c at
// 1844042d1ca751e62e61df7617dba19c98588339.
// Use of this source code is governed by the BSD-style license in COPYING.
// See PATENTS for the additional intellectual property rights grant and
// AUTHORS for the contributing project authors.

//! Standalone ARGB preprocessing for non-palette candidates without prediction.
//! Encoder candidate routing is deliberately left to the VP8L integration.

use std::collections::TryReserveError;

use crate::dsp::lossless_common::VP8LNearLosslessBits;

const MIN_DIM_FOR_NEAR_LOSSLESS: u32 = 64;
const MAX_LIMIT_BITS: u32 = 5;

#[derive(Debug, thiserror::Error)]
pub enum NearLosslessError {
    #[error("standalone near-lossless preprocessing requires quality in 0..=99")]
    InvalidQuality,
    #[error("width and height must be in 1..=16383")]
    InvalidDimensions,
    #[error("ARGB stride is smaller than width")]
    InvalidStride,
    #[error("ARGB buffer length arithmetic overflowed")]
    SizeOverflow,
    #[error("ARGB buffer is shorter than the last pixel")]
    InputTooShort,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
}

/// Round a byte to a multiple of 2^bits (or 255), resolving ties to even.
/// Corresponds to FindClosestDiscretized in near_lossless_enc.c.
pub fn FindClosestDiscretized(a: u32, bits: u32) -> u32 {
    assert!(a <= 255 && (1..=MAX_LIMIT_BITS).contains(&bits));
    let mask = (1 << bits) - 1;
    let biased = a + (mask >> 1) + ((a >> bits) & 1);
    if biased > 255 { 255 } else { biased & !mask }
}

pub fn ClosestDiscretizedArgb(a: u32, bits: u32) -> u32 {
    (FindClosestDiscretized(a >> 24, bits) << 24)
        | (FindClosestDiscretized((a >> 16) & 255, bits) << 16)
        | (FindClosestDiscretized((a >> 8) & 255, bits) << 8)
        | FindClosestDiscretized(a & 255, bits)
}

/// All four channel differences must be strictly less than limit.
pub fn IsNear(a: u32, b: u32, limit: u32) -> bool {
    (0..4).all(|k| ((a >> (k * 8)) & 255).abs_diff((b >> (k * 8)) & 255) < limit)
}

/// Check the four-connected neighborhood of an interior pixel.
pub fn IsSmooth(
    prev_row: &[u32],
    curr_row: &[u32],
    next_row: &[u32],
    ix: usize,
    limit: u32,
) -> bool {
    assert!(ix > 0 && ix + 1 < curr_row.len());
    IsNear(curr_row[ix], curr_row[ix - 1], limit)
        && IsNear(curr_row[ix], curr_row[ix + 1], limit)
        && IsNear(curr_row[ix], prev_row[ix], limit)
        && IsNear(curr_row[ix], next_row[ix], limit)
}

/// One pass on a tightly packed image, retaining the original neighborhood in
/// three rotating saved rows. Borders and smooth pixels remain unchanged.
///
/// Corresponds to static NearLossless in near_lossless_enc.c. The first C pass
/// also packs strided input; VP8ApplyNearLossless below packs it before calling
/// this in-place implementation, giving the same source neighborhoods.
pub fn NearLossless(
    width: usize,
    height: usize,
    limit_bits: u32,
    copy_buffer: &mut [u32],
    argb: &mut [u32],
) {
    assert!(width > 0 && height >= 2 && (1..=MAX_LIMIT_BITS).contains(&limit_bits));
    assert!(width.checked_mul(height).is_some_and(|n| n <= argb.len()));
    assert!(width.checked_mul(3).is_some_and(|n| n <= copy_buffer.len()));
    let limit = 1 << limit_bits;
    let (mut prev, mut curr, mut next) = (0, width, 2 * width);
    copy_buffer[curr..curr + width].copy_from_slice(&argb[..width]);
    copy_buffer[next..next + width].copy_from_slice(&argb[width..2 * width]);

    for y in 0..height {
        if y > 0 && y + 1 < height {
            let offset = y * width;
            copy_buffer[next..next + width]
                .copy_from_slice(&argb[offset + width..offset + 2 * width]);
            for x in 1..width.saturating_sub(1) {
                let value = copy_buffer[curr + x];
                argb[offset + x] = if IsSmooth(
                    &copy_buffer[prev..prev + width],
                    &copy_buffer[curr..curr + width],
                    &copy_buffer[next..next + width],
                    x,
                    limit,
                ) {
                    value
                } else {
                    ClosestDiscretizedArgb(value, limit_bits)
                };
            }
        }
        (prev, curr, next) = (curr, next, prev);
    }
}

fn allocate_pixels(len: usize) -> Result<Vec<u32>, TryReserveError> {
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(len)?;
    pixels.resize(len, 0);
    Ok(pixels)
}

/// Return tightly packed preprocessed ARGB, leaving the source unmodified.
/// Stride is in pixels; padding after the final row need not be present.
///
/// Quality 100 must bypass this function in the encoder, as in upstream. The
/// small-image shortcut is specific to this standalone route; it must not be
/// used to disable near-lossless predictor quantization.
pub fn VP8ApplyNearLossless(
    argb: &[u32],
    width: u32,
    height: u32,
    stride: usize,
    quality: u32,
) -> Result<Vec<u32>, NearLosslessError> {
    if quality >= 100 {
        return Err(NearLosslessError::InvalidQuality);
    }
    if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
        return Err(NearLosslessError::InvalidDimensions);
    }
    let (w, h) = (width as usize, height as usize);
    if stride < w {
        return Err(NearLosslessError::InvalidStride);
    }
    let required = (h - 1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(w))
        .ok_or(NearLosslessError::SizeOverflow)?;
    if argb.len() < required {
        return Err(NearLosslessError::InputTooShort);
    }
    let size = w.checked_mul(h).ok_or(NearLosslessError::SizeOverflow)?;
    let mut dst = allocate_pixels(size)?;
    for y in 0..h {
        dst[y * w..(y + 1) * w].copy_from_slice(&argb[y * stride..y * stride + w]);
    }
    if (width < MIN_DIM_FOR_NEAR_LOSSLESS && height < MIN_DIM_FOR_NEAR_LOSSLESS) || height < 3 {
        return Ok(dst);
    }
    let mut copy_buffer =
        allocate_pixels(w.checked_mul(3).ok_or(NearLosslessError::SizeOverflow)?)?;
    let limit_bits = VP8LNearLosslessBits(quality);
    for bits in (1..=limit_bits).rev() {
        NearLossless(w, h, bits, &mut copy_buffer, &mut dst);
    }
    Ok(dst)
}
