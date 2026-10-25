// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/alpha_enc.c, synchronous methods 0–6.
// See COPYING and PATENTS.
use super::{picture_enc::WebPYUVAPicture, vp8l_enc::VP8LEncodeStream};
use crate::{
    dsp::{alpha_processing::DispatchAlphaToGreen_C, filters::*},
    utils::{
        bit_writer_utils::*, filters_utils::WebPEstimateBestFilter,
        quant_levels_utils::QuantizeLevels,
    },
};
use std::collections::TryReserveError;

#[derive(Debug, thiserror::Error)]
pub enum AlphaEncodeError {
    #[error("invalid alpha plane geometry or buffer")]
    InvalidInput,
    #[error("alpha quality must be in 0..=100")]
    InvalidQuality,
    #[error("invalid alpha compression, filter or effort option")]
    InvalidConfiguration,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
}
#[derive(Debug)]
pub struct AlphaEncodeResult {
    pub data: Vec<u8>,
    pub sse: u64,
}

/// Encode an exact green-only plane with method-dependent VP8L effort.
/// Only an unreduced alpha-quality request enables method 6's quality 100.
pub fn EncodeLossless(
    data: &[u8],
    width: usize,
    height: usize,
    effort_level: u32,
    use_quality_100: bool,
    bw: &mut VP8LBitWriter,
) -> Result<(), TryReserveError> {
    assert!((1..=16383).contains(&width) && (1..=16383).contains(&height));
    assert!(data.len() >= width * height);
    assert!(effort_level <= 6);
    let mut argb = Vec::new();
    argb.try_reserve_exact(width * height)?;
    argb.resize(width * height, 0);
    DispatchAlphaToGreen_C(data, width, width, height, &mut argb, width);
    let quality = if use_quality_100 && effort_level == 6 {
        100
    } else {
        8 * effort_level
    };
    VP8LEncodeStream(bw, &argb, width, height, width, effort_level, quality, true)?;
    Ok(())
}
pub fn GetNumColors(data: &[u8], width: usize, height: usize, stride: usize) -> usize {
    assert!(plane_len(width, height, stride).is_some_and(|n| data.len() >= n));
    let mut colors = [false; 256];
    for y in 0..height {
        for &v in &data[y * stride..y * stride + width] {
            colors[usize::from(v)] = true;
        }
    }
    colors.iter().filter(|&&v| v).count()
}
pub fn GetFilterMap(
    alpha: &[u8],
    width: usize,
    height: usize,
    filter: usize,
    effort_level: u32,
) -> u32 {
    if filter == WEBP_FILTER_FAST {
        let colors = GetNumColors(alpha, width, height, width);
        let chosen = if colors <= 16 {
            WEBP_FILTER_NONE
        } else {
            WebPEstimateBestFilter(alpha, width, height)
        };
        (1 << chosen)
            | if effort_level > 3 || colors > 192 {
                1
            } else {
                0
            }
    } else if filter == WEBP_FILTER_NONE {
        1
    } else {
        15
    }
}
pub fn EncodeAlphaInternal(
    data: &[u8],
    width: usize,
    height: usize,
    method: u8,
    filter: usize,
    reduce_levels: bool,
    effort_level: u32,
    tmp_alpha: &mut [u8],
) -> Result<Vec<u8>, TryReserveError> {
    assert!(method <= 1 && filter < WEBP_FILTER_LAST);
    let src = if filter != WEBP_FILTER_NONE {
        WebPFilter(data, width, height, width, filter, tmp_alpha);
        &tmp_alpha[..width * height]
    } else {
        &data[..width * height]
    };
    let mut bw = VP8LBitWriterInit(if method != 0 { width * height >> 3 } else { 0 })?;
    let mut chosen_method = method;
    let payload = if method != 0 {
        EncodeLossless(src, width, height, effort_level, !reduce_levels, &mut bw)?;
        let compressed = VP8LBitWriterFinish(&mut bw)?;
        if compressed.len() > width * height {
            chosen_method = 0;
            src
        } else {
            compressed
        }
    } else {
        src
    };
    let mut out = Vec::new();
    out.try_reserve_exact(1 + payload.len())?;
    out.push(chosen_method | ((filter as u8) << 2) | if reduce_levels { 16 } else { 0 });
    out.extend_from_slice(payload);
    Ok(out)
}
pub fn ApplyFiltersAndEncode(
    alpha: &[u8],
    width: usize,
    height: usize,
    method: u8,
    filter: usize,
    reduce_levels: bool,
    effort_level: u32,
) -> Result<Vec<u8>, TryReserveError> {
    let mut map = GetFilterMap(alpha, width, height, filter, effort_level);
    if map == 1 {
        return EncodeAlphaInternal(
            alpha,
            width,
            height,
            method,
            WEBP_FILTER_NONE,
            reduce_levels,
            effort_level,
            &mut [],
        );
    }
    let mut tmp = Vec::new();
    tmp.try_reserve_exact(width * height)?;
    tmp.resize(width * height, 0);
    let mut best: Option<Vec<u8>> = None;
    for filter in 0..WEBP_FILTER_LAST {
        if map & 1 != 0 {
            let trial = EncodeAlphaInternal(
                alpha,
                width,
                height,
                method,
                filter,
                reduce_levels,
                effort_level,
                &mut tmp,
            )?;
            if best.as_ref().is_none_or(|b| trial.len() < b.len()) {
                best = Some(trial);
            }
        }
        map >>= 1;
        if map == 0 {
            break;
        }
    }
    Ok(best.unwrap())
}
/// `method` is the ALPH compression bit (0/1); `effort_level` is encoder method
/// 0–6 and controls VP8L settings and fast-filter trials independently.
pub fn EncodeAlpha(
    alpha: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    quality: i32,
    method: u8,
    mut filter: usize,
    effort_level: u32,
) -> Result<AlphaEncodeResult, AlphaEncodeError> {
    if !(1..=16383).contains(&width)
        || !(1..=16383).contains(&height)
        || !plane_len(width, height, stride).is_some_and(|n| alpha.len() >= n)
    {
        return Err(AlphaEncodeError::InvalidInput);
    }
    if !(0..=100).contains(&quality) {
        return Err(AlphaEncodeError::InvalidQuality);
    }
    if effort_level > 6
        || method > 1
        || ![WEBP_FILTER_NONE, WEBP_FILTER_BEST, WEBP_FILTER_FAST].contains(&filter)
    {
        return Err(AlphaEncodeError::InvalidConfiguration);
    }
    if method == 0 {
        filter = WEBP_FILTER_NONE;
    }
    let mut quant_alpha = Vec::new();
    quant_alpha.try_reserve_exact(width * height)?;
    for y in 0..height {
        quant_alpha.extend_from_slice(&alpha[y * stride..y * stride + width]);
    }
    let reduce = quality < 100;
    let sse = if reduce {
        let levels = if quality <= 70 {
            2 + quality / 5
        } else {
            16 + (quality - 70) * 8
        };
        QuantizeLevels(&mut quant_alpha, width, height, levels as usize).unwrap()
    } else {
        0
    };
    Ok(AlphaEncodeResult {
        data: ApplyFiltersAndEncode(
            &quant_alpha,
            width,
            height,
            method,
            filter,
            reduce,
            effort_level,
        )?,
        sse,
    })
}
/// Owned synchronous alpha job; opaque pictures have no separate stream.
pub fn VP8EncStartAlpha(
    pic: &WebPYUVAPicture,
    quality: i32,
    effort_level: u32,
) -> Result<Option<AlphaEncodeResult>, AlphaEncodeError> {
    if !(0..=100).contains(&quality) {
        return Err(AlphaEncodeError::InvalidQuality);
    }
    if effort_level > 6 {
        return Err(AlphaEncodeError::InvalidConfiguration);
    }
    match &pic.a {
        Some(alpha) if alpha.iter().any(|&v| v != 255) => EncodeAlpha(
            alpha,
            pic.width as usize,
            pic.height as usize,
            pic.width as usize,
            quality,
            1,
            WEBP_FILTER_FAST,
            effort_level,
        )
        .map(Some),
        _ => Ok(None),
    }
}
