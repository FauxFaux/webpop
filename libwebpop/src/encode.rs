//! Checked image entry point using the completed lossless encoding stages.

use std::collections::TryReserveError;

use crate::enc::backward_references_enc::{
    VP8LBackwardRefsInit, VP8LHashChainInit, kLZ77Box, kLZ77RLE, kLZ77Standard,
};
use crate::enc::picture_csp_enc::{RgbaInputError, checked_rgba_geometry};
use crate::enc::vp8l_enc::{
    ApplyCrossColorFilter, ApplyPredictFilter, ApplySubtractGreen, ClampBits, CrunchSubConfig,
    EncodeImageInternal, EncodePalette, ImageWriteError, MapImageFromPalette, WriteImage,
    WriteImageSize, WriteRealAlphaAndVersion,
};
use crate::utils::bit_writer_utils::{VP8LBitWriterInit, VP8LBitWriterNumBytes, VP8LPutBits};
use crate::utils::palette::{GetColorPalette, PaletteSortDefault};
use crate::utils::utils::BitsLog2Floor;

/// Compression effort and transparent-pixel policy for the current pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LosslessOptions {
    /// Compression effort, 0..=100. Pixel quality remains lossless.
    pub quality: u32,
    /// Effort, 0..=6. Zero uses the low-effort path; higher values refine
    /// prediction, histogram sampling and reference/cache trials.
    pub method: u32,
    /// Preserve RGB beneath alpha zero as well as all visible channels.
    pub exact: bool,
}

impl Default for LosslessOptions {
    fn default() -> Self {
        Self {
            quality: 70,
            method: 4,
            exact: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LosslessEncodeError {
    #[error(transparent)]
    Input(#[from] RgbaInputError),
    #[error("quality must be in 0..=100")]
    InvalidQuality,
    #[error("method must be in 0..=6")]
    InvalidMethod,
    #[error(transparent)]
    Allocation(#[from] TryReserveError),
    #[error(transparent)]
    Container(#[from] ImageWriteError),
}

/// Encode positive-stride RGBA bytes as a complete lossless still-image WebP.
/// Dimensions must be in 1..=16383; final-row padding is optional. Input is
/// validated before allocation and is never modified. RGB beneath alpha zero
/// may change unless `options.exact` is set.
///
/// This uses a fixed transform policy: a default-sorted palette when eligible,
/// otherwise subtract-green; prediction on both paths, then cross-color on
/// non-palette images at nonzero methods. Upstream's entropy analysis and
/// configuration trials are not integrated yet. This API produces valid WebP
/// but does not promise byte parity with `WebPEncodeLosslessRGBA` in libwebp.
pub fn encode_lossless_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    options: &LosslessOptions,
) -> Result<Vec<u8>, LosslessEncodeError> {
    checked_rgba_geometry(rgba.len(), width, height, stride)?;
    if options.quality > 100 {
        return Err(LosslessEncodeError::InvalidQuality);
    }
    if options.method > 6 {
        return Err(LosslessEncodeError::InvalidMethod);
    }
    let (w, h) = (width as usize, height as usize);
    let size = w.checked_mul(h).ok_or(RgbaInputError::SizeOverflow)?;
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(size)?;
    for y in 0..h {
        for p in rgba[y * stride..y * stride + w * 4].chunks_exact(4) {
            pixels.push(if !options.exact && p[3] == 0 {
                0
            } else {
                u32::from_be_bytes([p[3], p[0], p[1], p[2]])
            });
        }
    }
    let mut bw = VP8LBitWriterInit(0)?;
    WriteImageSize(&mut bw, width, height)?;
    WriteRealAlphaAndVersion(&mut bw, pixels.iter().any(|p| p >> 24 != 255))?;
    let mut chain = VP8LHashChainInit(size)?;
    let mut refs = std::array::from_fn::<_, 4, _>(|_| VP8LBackwardRefsInit(256));
    let low_effort = options.method == 0;
    let mut sorted = [0; 256];
    let palette_size = GetColorPalette(&pixels, w, h, w, &mut sorted);
    let use_palette = palette_size <= 256;
    let mut current_width = w;
    let mut cache_bits = 0;
    if use_palette {
        let mut palette = [0; 256];
        PaletteSortDefault(&sorted[..palette_size], &mut palette[..palette_size]);
        let palette = &palette[..palette_size];
        EncodePalette(&mut bw, palette, &mut chain, &mut refs[..2], low_effort)?;
        (current_width, pixels) = MapImageFromPalette(&pixels, w, palette, w, h)?;
        cache_bits = BitsLog2Floor(palette_size as u32) + 1;
    } else {
        ApplySubtractGreen(&mut bw, &mut pixels)?;
    }
    ApplyPredictFilter(
        &mut bw,
        &mut pixels,
        &mut chain,
        &mut refs[..2],
        current_width,
        h,
        6,
        options.method,
        options.quality,
        low_effort,
        options.exact,
    )?;
    if !use_palette && !low_effort {
        let bits = ClampBits(current_width, h, 6, 2, 9, 1 << 14);
        ApplyCrossColorFilter(
            &mut bw,
            &mut pixels,
            &mut chain,
            &mut refs[..2],
            current_width,
            h,
            bits,
            options.quality,
            low_effort,
        )?;
    }
    VP8LPutBits(&mut bw, 0, 1)?; // No more transforms.
    let initial = VP8LBitWriterNumBytes(&bw);
    // Upstream's histogram size cap avoids creating too many Huffman groups.
    let histogram_bits = ClampBits(
        current_width,
        h,
        (if use_palette { 9 } else { 7 }) - options.method as i32,
        2,
        9,
        2600,
    );
    let lz77 = kLZ77Standard | kLZ77RLE | if options.method >= 5 { kLZ77Box } else { 0 };
    EncodeImageInternal(
        &mut bw,
        &pixels,
        &mut chain,
        &mut refs,
        current_width,
        h,
        options.quality,
        low_effort,
        &[CrunchSubConfig {
            lz77,
            do_no_cache: options.method == 6,
        }],
        &mut cache_bits,
        histogram_bits,
        initial,
    )?;
    Ok(WriteImage(&mut bw)?)
}
