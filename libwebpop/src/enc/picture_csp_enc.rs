// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_csp_enc.c (ARGB import/transparency).
// See COPYING and PATENTS.

//! Checked positive-stride RGBA import and transparency detection.

use super::picture_enc::WebPPicture;
use crate::webp::encode::LosslessEncodeError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RgbaInputError {
    #[error("width and height must be in 1..=16383")]
    InvalidDimensions,
    #[error("stride is smaller than width * 4")]
    InvalidStride,
    #[error("RGBA input length arithmetic overflowed")]
    SizeOverflow,
    #[error("RGBA buffer is shorter than the last pixel")]
    InputTooShort,
}

/// Validate without allocating or reading input. Returns the minimum byte
/// length, which deliberately excludes padding after the last row.
pub fn checked_rgba_geometry(
    input_len: usize,
    width: u32,
    height: u32,
    stride: usize,
) -> Result<usize, RgbaInputError> {
    if !(1..=16383).contains(&width) || !(1..=16383).contains(&height) {
        return Err(RgbaInputError::InvalidDimensions);
    }
    let row_bytes = (width as usize)
        .checked_mul(4)
        .ok_or(RgbaInputError::SizeOverflow)?;
    if stride < row_bytes {
        return Err(RgbaInputError::InvalidStride);
    }
    let required = (height as usize - 1)
        .checked_mul(stride)
        .and_then(|offset| offset.checked_add(row_bytes))
        .ok_or(RgbaInputError::SizeOverflow)?;
    if input_len < required {
        return Err(RgbaInputError::InputTooShort);
    }
    Ok(required)
}

pub(crate) fn WebPPictureImportRGBA(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<WebPPicture, LosslessEncodeError> {
    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    let size = (width as usize)
        .checked_mul(height as usize)
        .ok_or(RgbaInputError::SizeOverflow)?;
    let mut argb = Vec::new();
    argb.try_reserve_exact(size)?;
    for y in 0..height as usize {
        for pixel in rgba[y * stride..y * stride + width as usize * 4].chunks_exact(4) {
            argb.push(u32::from_be_bytes([pixel[3], pixel[0], pixel[1], pixel[2]]));
        }
    }
    Ok(WebPPicture {
        width,
        height,
        argb,
    })
}

pub(crate) fn WebPPictureHasTransparency(picture: &WebPPicture) -> bool {
    picture.argb.iter().any(|pixel| pixel >> 24 != 255)
}
