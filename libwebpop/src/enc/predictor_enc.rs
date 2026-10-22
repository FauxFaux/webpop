// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/predictor_enc.c.
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

const ARGB_BLACK: u32 = 0xff000000;

use crate::dsp::lossless::VP8LPredictors;
use crate::dsp::lossless_common::{DivRound, LOG_2_PRECISION_BITS, VP8LSubPixels};
use crate::dsp::lossless_enc::{VP8LCombinedShannonEntropy, VP8LShannonEntropy};
use crate::webp::format_constants::{MIN_TRANSFORM_BITS, NUM_TRANSFORM_BITS};
use std::collections::TryReserveError;

// Defined in upstream/src/enc/vp8li_enc.h.
pub const MAX_TRANSFORM_BITS: u32 = MIN_TRANSFORM_BITS + (1 << NUM_TRANSFORM_BITS) - 1;

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
    assert!(width > 0 && height > 0 && bits <= MAX_TRANSFORM_BITS);
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

fn zeroed<T: Default>(size: usize) -> Result<Vec<T>, TryReserveError> {
    let mut values = Vec::new();
    values.try_reserve_exact(size)?;
    values.resize_with(size, T::default);
    Ok(values)
}

struct PredictorSearch {
    // One residual histogram per (sampling, predictor), followed separately
    // by the selected residual and predictor histograms for each sampling.
    histograms: Vec<[u32; 1024]>,
    accumulated: Vec<[u32; 1024]>,
    predictors: Vec<[u32; 14]>,
    modes: Vec<Vec<u32>>,
}

impl PredictorSearch {
    #[allow(clippy::too_many_arguments)]
    fn ComputeResidualsForTile(
        &mut self,
        width: usize,
        height: usize,
        tile_x: usize,
        tile_y: usize,
        min_bits: u32,
        update_up_to_index: usize,
        scratch: &mut [u32],
        argb: &[u32],
        exact: bool,
        min_mode: usize,
        diff_mask: u32,
    ) {
        let start_x = tile_x << min_bits;
        let start_y = tile_y << min_bits;
        let max_x = (1 << min_bits).min(width - start_x);
        let max_y = (1 << min_bits).min(height - start_y);
        let have_left = usize::from(start_x > 0);
        let context_start = start_x - have_left;
        let (mut upper, mut current) = scratch.split_at_mut(width + 1);
        let mut residuals = [0; 1 << MAX_TRANSFORM_BITS];
        for mode in min_mode..14 {
            if start_y > 0 {
                let count = max_x + have_left + 1;
                let offset = (start_y - 1) * width + context_start;
                current[context_start..context_start + count]
                    .copy_from_slice(&argb[offset..offset + count]);
            }
            for y in start_y..start_y + max_y {
                std::mem::swap(&mut upper, &mut current);
                let count = max_x + have_left + usize::from(y + 1 < height);
                let offset = y * width + context_start;
                current[context_start..context_start + count]
                    .copy_from_slice(&argb[offset..offset + count]);
                GetResidual(
                    width,
                    upper,
                    current,
                    mode,
                    start_x,
                    y,
                    exact,
                    &mut residuals[..max_x],
                );
                for s in 0..=update_up_to_index {
                    UpdateHisto(
                        &mut self.histograms[s * 14 + mode],
                        &residuals[..max_x],
                        diff_mask,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn GetBestPredictorForTile(
        &mut self,
        s: usize,
        tile_x: usize,
        tile_y: usize,
        tiles_per_row: usize,
        min_mode: usize,
        diff_mask: u32,
    ) {
        let offset = tile_y * tiles_per_row + tile_x;
        let modes = &mut self.modes[s];
        let left = if tile_x > 0 {
            (modes[offset - 1] >> 8) & 255
        } else {
            255
        };
        let above = if tile_y > 0 {
            (modes[offset - tiles_per_row] >> 8) & 255
        } else {
            255
        };
        let mut best_cost = i64::MAX;
        let mut best_mode = min_mode;
        for mode in min_mode..14 {
            let cost = PredictionCostSpatialHistogram(
                &self.accumulated[s],
                &self.histograms[s * 14 + mode],
                mode as u32,
                left,
                above,
                diff_mask,
            );
            if cost < best_cost {
                best_cost = cost;
                best_mode = mode;
            }
        }
        for plane in 0..4 {
            if diff_mask & (1 << plane) != 0 {
                for i in plane * 256..(plane + 1) * 256 {
                    self.accumulated[s][i] =
                        self.accumulated[s][i].wrapping_add(self.histograms[s * 14 + best_mode][i]);
                }
            }
        }
        modes[offset] = ARGB_BLACK | ((best_mode as u32) << 8);
        self.predictors[s][best_mode] += 1;
    }
}

/// Choose lossless predictor modes and tile size using upstream's Z traversal,
/// accumulated entropy, neighbor biases, and strict first-minimum tie rules.
/// Uniform maps may compact to a tile size larger than `max_bits` (up to 9).
pub fn GetBestPredictorsAndSubSampling(
    width: usize,
    height: usize,
    min_bits: u32,
    max_bits: u32,
    argb: &[u32],
    exact: bool,
) -> Result<(u32, Vec<u32>), TryReserveError> {
    assert!(
        width > 0
            && height > 0
            && MIN_TRANSFORM_BITS <= min_bits
            && min_bits <= max_bits
            && max_bits <= MAX_TRANSFORM_BITS
    );
    assert_eq!(
        argb.len(),
        width.checked_mul(height).expect("image geometry")
    );
    let (diff_mask, min_mode) = GetPlaneDiffMask(argb, 1, exact);
    let min_mode = min_mode as usize;
    let max_s = (max_bits - min_bits) as usize;
    let levels = max_s + 1;
    // Arrays larger than 32 elements have no Default implementation.
    let mut histograms = Vec::new();
    histograms.try_reserve_exact(levels * 14)?;
    histograms.resize(levels * 14, [0; 1024]);
    let mut accumulated = Vec::new();
    accumulated.try_reserve_exact(levels)?;
    accumulated.resize(levels, [0; 1024]);
    let mut modes = Vec::new();
    modes.try_reserve_exact(levels)?;
    for bits in min_bits..=max_bits {
        let size = width
            .div_ceil(1 << bits)
            .checked_mul(height.div_ceil(1 << bits))
            .expect("map geometry");
        modes.push(zeroed(size)?);
    }
    let mut search = PredictorSearch {
        histograms,
        accumulated,
        predictors: zeroed(levels)?,
        modes,
    };
    let mut scratch = zeroed(
        width
            .checked_add(1)
            .and_then(|n| n.checked_mul(2))
            .expect("scratch size"),
    )?;
    let update_up_to_index = (max_bits.min(4).max(min_bits) - min_bits) as usize;
    let tiles_per_row = width.div_ceil(1 << min_bits);
    let tiles_per_col = height.div_ceil(1 << min_bits);
    let max_tile_size = 1 << max_s;
    let (mut local_x, mut local_y, mut max_x, mut max_y) = (0, 0, 0, 0);
    let (mut tile_x, mut tile_y) = (0, 0);
    while tile_y < tiles_per_col {
        search.ComputeResidualsForTile(
            width,
            height,
            tile_x,
            tile_y,
            min_bits,
            update_up_to_index,
            &mut scratch,
            argb,
            exact,
            min_mode,
            diff_mask,
        );
        let mut s = 0;
        loop {
            search.GetBestPredictorForTile(
                s,
                tile_x >> s,
                tile_y >> s,
                width.div_ceil(1 << (min_bits + s as u32)),
                min_mode,
                diff_mask,
            );
            if s == max_s {
                break;
            }
            s += 1;
            if s > update_up_to_index {
                for mode in min_mode..14 {
                    for plane in 0..4 {
                        if diff_mask & (1 << plane) != 0 {
                            for i in plane * 256..(plane + 1) * 256 {
                                search.histograms[s * 14 + mode][i] = search.histograms
                                    [s * 14 + mode][i]
                                    .wrapping_add(search.histograms[(s - 1) * 14 + mode][i]);
                            }
                        }
                    }
                }
            }
            if !((tile_x == tiles_per_row - 1 || (local_x + 1) % (1 << s) == 0)
                && (tile_y == tiles_per_col - 1 || (local_y + 1) % (1 << s) == 0))
            {
                s -= 1;
                break;
            }
        }
        // Inactive planes/modes remain zero, so clearing the whole completed
        // range is equivalent to C's selective reset.
        search.histograms[..(s + 1) * 14].fill([0; 1024]);
        if s == max_s {
            if tile_x == tiles_per_row - 1 {
                max_x = 0;
                max_y += 1;
            } else {
                max_x += 1;
            }
            local_x = 0;
            local_y = 0;
        } else {
            let mut coord_x = local_x >> s;
            let mut coord_y = local_y >> s;
            if tile_x == tiles_per_row - 1 && coord_x % 2 == 0 {
                coord_y += 1;
            } else if coord_x % 2 == 0 {
                coord_x += 1;
            } else {
                coord_y += 1;
                coord_x -= 1;
            }
            local_x = coord_x << s;
            local_y = coord_y << s;
        }
        tile_x = max_x * max_tile_size + local_x;
        tile_y = max_y * max_tile_size + local_y;
    }
    let mut best_cost = i64::MAX;
    let mut best_s = 0;
    for s in 0..levels {
        let mut cost = VP8LShannonEntropy(&search.predictors[s]) as i64;
        for plane in 0..4 {
            if diff_mask & (1 << plane) != 0 {
                cost += VP8LShannonEntropy(&search.accumulated[s][plane * 256..(plane + 1) * 256])
                    as i64;
            }
        }
        if cost < best_cost {
            best_cost = cost;
            best_s = s;
        }
    }
    let mut modes = std::mem::take(&mut search.modes[best_s]);
    let best_bits = VP8LOptimizeSampling(
        &mut modes,
        width,
        height,
        min_bits + best_s as u32,
        MAX_TRANSFORM_BITS,
    );
    modes.truncate(width.div_ceil(1 << best_bits) * height.div_ceil(1 << best_bits));
    Ok((best_bits, modes))
}

/// Select predictors and convert pixels in place. Only the lossless path
/// (near-lossless quality 100) is supported. Returns (tile bits, predictor map).
/// Low effort fixes mode 11 at `max_bits` without sampling optimization.
#[allow(clippy::too_many_arguments)]
pub fn VP8LResidualImage(
    width: usize,
    height: usize,
    min_bits: u32,
    max_bits: u32,
    low_effort: bool,
    argb: &mut [u32],
    exact: bool,
) -> Result<(u32, Vec<u32>), TryReserveError> {
    assert!(
        width > 0
            && height > 0
            && MIN_TRANSFORM_BITS <= min_bits
            && min_bits <= max_bits
            && max_bits <= MAX_TRANSFORM_BITS
    );
    assert_eq!(
        argb.len(),
        width.checked_mul(height).expect("image geometry")
    );
    let (bits, modes) = if low_effort {
        let mut modes: Vec<u32> =
            zeroed(width.div_ceil(1 << max_bits) * height.div_ceil(1 << max_bits))?;
        modes.fill(ARGB_BLACK | (11 << 8));
        (max_bits, modes)
    } else {
        GetBestPredictorsAndSubSampling(width, height, min_bits, max_bits, argb, exact)?
    };
    CopyImageWithPrediction(width, height, bits, &modes, argb, low_effort, exact)?;
    Ok((bits, modes))
}
