// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_csp_enc.c (RGBA import/transparency).
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

/// Import the default lossy RGBA path (no dithering or SharpYUV), retaining
/// hidden RGB and alpha exactly until transparent-area cleanup is requested.
pub fn import_rgba_to_yuva(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<super::picture_enc::WebPYUVAPicture, LosslessEncodeError> {
    use crate::dsp::yuv::{
        ConvertRGBToY_C, WebPAccumulateRGB, WebPAccumulateRGBA, WebPConvertRGBA32ToUV_C,
    };

    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    let (w, h) = (width as usize, height as usize);
    let (uw, uh) = (w.div_ceil(2), h.div_ceil(2));
    let size = w.checked_mul(h).ok_or(RgbaInputError::SizeOverflow)?;
    let uv_size = uw.checked_mul(uh).ok_or(RgbaInputError::SizeOverflow)?;
    let has_alpha = (0..h).any(|row| {
        rgba[row * stride..row * stride + 4 * w]
            .chunks_exact(4)
            .any(|p| p[3] != 255)
    });
    let allocate = |len| -> Result<Vec<u8>, std::collections::TryReserveError> {
        let mut v = Vec::new();
        v.try_reserve_exact(len)?;
        v.resize(len, 0);
        Ok(v)
    };
    let mut pic = super::picture_enc::WebPYUVAPicture {
        width,
        height,
        y: allocate(size)?,
        u: allocate(uv_size)?,
        v: allocate(uv_size)?,
        a: if has_alpha {
            Some(allocate(size)?)
        } else {
            None
        },
    };
    let tmp_size = uw.checked_mul(4).ok_or(RgbaInputError::SizeOverflow)?;
    let mut tmp_rgb = Vec::new();
    tmp_rgb.try_reserve_exact(tmp_size)?;
    tmp_rgb.resize(tmp_size, 0u16);
    // ImportYUVAFromRGBA_C processes pairs; ImportYUVAFromRGBALastLine_C
    // repeats the final row by using a zero source stride. This loop shares
    // those operations while retaining the per-pair opaque fast path.
    for row in (0..h).step_by(2) {
        let rows = (h - row).min(2);
        let src = &rgba[row * stride..];
        let rgb_stride = if rows == 2 { stride } else { 0 };
        let mut rows_have_alpha = false;
        for dy in 0..rows {
            ConvertRGBToY_C(&src[dy * stride..], &mut pic.y[(row + dy) * w..], w, 4);
            if let Some(alpha) = &mut pic.a {
                for x in 0..w {
                    let a = src[dy * stride + 4 * x + 3];
                    alpha[(row + dy) * w + x] = a;
                    rows_have_alpha |= a != 255;
                }
            }
        }
        if rows_have_alpha {
            WebPAccumulateRGBA(
                src,
                &src[1..],
                &src[2..],
                &src[3..],
                rgb_stride,
                &mut tmp_rgb,
                w,
            );
        } else {
            WebPAccumulateRGB(src, &src[1..], &src[2..], 4, rgb_stride, &mut tmp_rgb, w);
        }
        let uv_offset = (row / 2) * uw;
        WebPConvertRGBA32ToUV_C(
            &tmp_rgb,
            &mut pic.u[uv_offset..],
            &mut pic.v[uv_offset..],
            uw,
        );
    }
    Ok(pic)
}

pub fn has_yuva_transparency(picture: &super::picture_enc::WebPYUVAPicture) -> bool {
    picture
        .a
        .as_ref()
        .is_some_and(|a| a.iter().any(|&v| v != 255))
}

/// Import RGBA through upstream's iterative conversion route
/// (`PreprocessARGB` / `WebPPictureSharpARGBToYUVA`). The encoder owns this
/// adaptation; the standalone SharpYUV module has no picture/encoder dependency.
pub fn import_rgba_to_yuva_sharp(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<super::picture_enc::WebPYUVAPicture, LosslessEncodeError> {
    use crate::sharpyuv::{
        SharpYuvConvert, SharpYuvGetConversionMatrix, SharpYuvMatrixType, SharpYuvOutput,
        SharpYuvRgb, SharpYuvRgbView,
    };

    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    // kMinDimensionIterativeConversion in picture_csp_enc.c. This fallback
    // includes the ordinary importer's per-row-pair alpha weighting.
    if width < 4 || height < 4 {
        return import_rgba_to_yuva(rgba, width, height, stride);
    }
    let input = SharpYuvRgb::U8(SharpYuvRgbView {
        r: rgba,
        g: &rgba[1..],
        b: &rgba[2..],
        width,
        height,
        step: 4,
        stride,
    });
    let SharpYuvOutput::U8(planes) = SharpYuvConvert(
        input,
        8,
        8,
        SharpYuvGetConversionMatrix(SharpYuvMatrixType::kSharpYuvMatrixWebp),
    )?
    else {
        unreachable!("8-bit SharpYUV output uses u8 planes");
    };
    let (w, h) = (width as usize, height as usize);
    let has_alpha = (0..h).any(|y| {
        rgba[y * stride..y * stride + 4 * w]
            .chunks_exact(4)
            .any(|p| p[3] != 255)
    });
    let a = if has_alpha {
        let mut alpha = Vec::new();
        alpha.try_reserve_exact(w * h)?;
        for y in 0..h {
            alpha.extend(
                rgba[y * stride..y * stride + 4 * w]
                    .chunks_exact(4)
                    .map(|p| p[3]),
            );
        }
        Some(alpha)
    } else {
        None
    };
    Ok(super::picture_enc::WebPYUVAPicture {
        width,
        height,
        y: planes.y,
        u: planes.u,
        v: planes.v,
        a,
    })
}
