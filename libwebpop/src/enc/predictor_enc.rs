// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/predictor_enc.c.
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

const ARGB_BLACK: u32 = 0xff000000;

use crate::dsp::lossless::VP8LPredictors;
use crate::dsp::lossless_common::{DivRound, LOG_2_PRECISION_BITS, VP8LSubPixels};
use crate::dsp::lossless_enc::VP8LCombinedShannonEntropy;

/// Find the largest uniform power-of-two tiles and compact their top-left
/// samples in place. The untouched tail is retained, matching upstream.
pub fn VP8LOptimizeSampling(
    image: &mut [u32],
    full_width: usize,
    full_height: usize,
    bits: u32,
    max_bits: u32,
) -> u32 {
    assert!(full_width > 0 && full_height > 0 && bits <= max_bits && max_bits < usize::BITS);
    let width = full_width.div_ceil(1 << bits);
    let height = full_height.div_ceil(1 << bits);
    assert!(image.len() >= width.checked_mul(height).expect("image geometry"));
    let mut best_bits = bits;
    // Check rows before columns; partial groups at the edges are allowed.
    while best_bits < max_bits {
        let square = 1 << (best_bits - bits);
        let good = (0..height)
            .step_by(square * 2)
            .take_while(|&y| y + square < height)
            .all(|y| {
                image[y * width..(y + 1) * width]
                    == image[(y + square) * width..(y + square + 1) * width]
            });
        if !good {
            break;
        }
        best_bits += 1;
    }
    while best_bits > bits {
        let square = 1 << (best_bits - bits);
        let good = (0..height).all(|y| {
            (0..width).step_by(square).all(|x| {
                (x + 1..(x + square).min(width))
                    .all(|i| image[y * width + i] == image[y * width + x])
            })
        });
        if good {
            break;
        }
        best_bits -= 1;
    }
    if best_bits > bits {
        let square = 1 << (best_bits - bits);
        let new_width = full_width.div_ceil(1 << best_bits);
        let new_height = full_height.div_ceil(1 << best_bits);
        for y in 0..new_height {
            for x in 0..new_width {
                image[y * new_width + x] = image[square * (y * width + x)];
            }
        }
    }
    best_bits
}

/// Upstream prediction-entropy bias, in the fixed-point entropy scale.
pub fn PredictionCostBias(counts: &[u32; 256], weight_0: u64, mut exp_val: u64) -> i64 {
    let mut bits = weight_0.wrapping_mul(u64::from(counts[0])) << LOG_2_PRECISION_BITS;
    exp_val <<= LOG_2_PRECISION_BITS;
    for i in 1..16 {
        let count = counts[i].wrapping_add(counts[256 - i]);
        bits =
            bits.wrapping_add(DivRound(exp_val.wrapping_mul(u64::from(count)) as i64, 100) as u64);
        exp_val = DivRound((6 * exp_val) as i64, 10) as u64;
    }
    -DivRound(bits as i64, 10)
}

pub fn PredictionCostSpatialHistogram(
    accumulated: &[u32; 1024],
    tile: &[u32; 1024],
    mode: u32,
    left_mode: u32,
    above_mode: u32,
    diff_mask: u32,
) -> i64 {
    let mut cost = 0;
    for plane in 0..4 {
        if diff_mask & (1 << plane) != 0 {
            let counts = tile[plane * 256..(plane + 1) * 256].try_into().unwrap();
            let global = accumulated[plane * 256..(plane + 1) * 256]
                .try_into()
                .unwrap();
            cost += PredictionCostBias(counts, 1, 94);
            cost += VP8LCombinedShannonEntropy(counts, global) as i64;
        }
    }
    if mode == left_mode {
        cost -= 15i64 << LOG_2_PRECISION_BITS;
    }
    if mode == above_mode {
        cost -= 15i64 << LOG_2_PRECISION_BITS;
    }
    cost
}

/// Return (active ARGB plane mask, first eligible predictor mode). Constant
/// planes are omitted only when they stay constant after transparent cleanup.
/// The quantization guard is retained for later near-lossless integration.
pub fn GetPlaneDiffMask(argb: &[u32], max_quantization: u32, exact: bool) -> (u32, u32) {
    let first = argb[0];
    if max_quantization > 1 {
        return (15, 0);
    }
    let mut has_diff = argb[1..].iter().fold(0, |diff, &p| diff | (p ^ first));
    if !exact {
        if has_diff & 0xff000000 == 0 {
            if first & 0xff000000 == 0 {
                return (0, 1);
            }
        } else {
            has_diff |= first & 0x00ffffff;
        }
    }
    let mut diff_mask = 0;
    let mut min_mode = 0;
    for i in 0..4 {
        let mask = 0xff000000 >> (8 * i);
        if has_diff & mask != 0 {
            diff_mask |= 1 << i;
        } else if first & mask != ARGB_BLACK & mask {
            min_mode = 1;
        }
    }
    (diff_mask, min_mode)
}

/// Accumulate only active planes of a residual histogram (ARGB plane order).
pub fn UpdateHisto(histo: &mut [u32; 1024], residuals: &[u32], diff_mask: u32) {
    for plane in 0..4 {
        if diff_mask & (1 << plane) != 0 {
            for &pixel in residuals {
                let index = plane * 256 + ((pixel >> ((3 - plane) * 8)) & 255) as usize;
                histo[index] = histo[index].wrapping_add(1);
            }
        }
    }
}

fn PredictPixel(mode: usize, x: usize, y: usize, current: &[u32], upper: &[u32]) -> u32 {
    if y == 0 {
        if x == 0 { ARGB_BLACK } else { current[x - 1] }
    } else if x == 0 {
        upper[0]
    } else {
        VP8LPredictors[mode](current[x - 1], &[upper[x - 1], upper[x], upper[x + 1]])
    }
}

/// Subtract predictions with the image-edge rules, without modifying source
/// pixels. Rows have an extra context word for right-edge wraparound.
pub fn PredictBatch(
    mode: usize,
    x_start: usize,
    y: usize,
    current: &[u32],
    upper: &[u32],
    out: &mut [u32],
) {
    assert!(mode < 14 && x_start + out.len() <= current.len());
    for (i, residual) in out.iter_mut().enumerate() {
        let x = x_start + i;
        *residual = VP8LSubPixels(current[x], PredictPixel(mode, x, y, current, upper));
    }
}

/// Lossless (max_quantization=1) GetResidual. Transparent RGB cleanup mutates
/// row context in the same order as C, including the right-edge wrap word.
#[allow(clippy::too_many_arguments)]
pub fn GetResidual(
    width: usize,
    upper: &mut [u32],
    current: &mut [u32],
    mode: usize,
    x_start: usize,
    y: usize,
    exact: bool,
    out: &mut [u32],
) {
    assert!(mode < 14 && x_start + out.len() <= width);
    assert!(current.len() > width && upper.len() > width);
    if exact {
        PredictBatch(mode, x_start, y, current, upper, out);
        return;
    }
    for (i, residual) in out.iter_mut().enumerate() {
        let x = x_start + i;
        let predict = PredictPixel(mode, x, y, current, upper);
        *residual = VP8LSubPixels(current[x], predict);
        if current[x] & 0xff000000 == 0 {
            *residual &= 0xff000000;
            current[x] = predict & 0x00ffffff;
            if x == 0 && y != 0 {
                upper[width] = current[0];
            }
        }
    }
}

/// Convert a prepared image in place using a supplied predictor map. This
/// ports the lossless path only; low effort always uses predictor 11 and skips
/// transparent cleanup, as in C. Scratch rows are owned and allocation failures
/// propagate. The first-row/column and right-edge wrap rules apply to all modes.
#[allow(clippy::too_many_arguments)]
pub fn CopyImageWithPrediction(
    width: usize,
    height: usize,
    bits: u32,
    modes: &[u32],
    argb: &mut [u32],
    low_effort: bool,
    exact: bool,
) -> Result<(), std::collections::TryReserveError> {
    assert!(width > 0 && height > 0 && bits <= 6);
    assert_eq!(
        argb.len(),
        width.checked_mul(height).expect("image geometry")
    );
    let tiles_per_row = width.div_ceil(1 << bits);
    assert!(low_effort || modes.len() >= tiles_per_row * height.div_ceil(1 << bits));
    let row_size = width.checked_add(1).expect("row size");
    let mut scratch = Vec::new();
    scratch.try_reserve_exact(row_size.checked_mul(2).expect("scratch size"))?;
    scratch.resize(2 * row_size, 0);
    let (mut upper, mut current) = scratch.split_at_mut(row_size);
    for y in 0..height {
        std::mem::swap(&mut upper, &mut current);
        let count = width + usize::from(y + 1 < height);
        current[..count].copy_from_slice(&argb[y * width..y * width + count]);
        if low_effort {
            PredictBatch(
                11,
                0,
                y,
                current,
                upper,
                &mut argb[y * width..(y + 1) * width],
            );
        } else {
            for x in (0..width).step_by(1 << bits) {
                let mode = ((modes[(y >> bits) * tiles_per_row + (x >> bits)] >> 8) & 255) as usize;
                let end = (x + (1 << bits)).min(width);
                GetResidual(
                    width,
                    upper,
                    current,
                    mode,
                    x,
                    y,
                    exact,
                    &mut argb[y * width + x..y * width + end],
                );
            }
        }
    }
    Ok(())
}
