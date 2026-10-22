// Copyright 2023 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/palette.c. See COPYING and PATENTS.

use crate::dsp::lossless_common::VP8LSubPixels;
use crate::utils::color_cache_utils::VP8LHashPix;

/// Find a color in a sorted palette. Unlike C's internal infinite search,
/// missing colors have an explicit representation.
pub fn SearchColorNoIdx(sorted: &[u32], color: u32) -> Option<usize> {
    sorted.binary_search(&color).ok()
}

/// Build the sorted-color to original-palette-index mapping.
/// Palette entries must be distinct; all slices must have the same length.
pub fn PrepareMapToPalette(palette: &[u32], sorted: &mut [u32], idx_map: &mut [u32]) {
    assert_eq!(palette.len(), idx_map.len());
    sorted.copy_from_slice(palette);
    sorted.sort_unstable();
    for (i, &color) in palette.iter().enumerate() {
        idx_map[SearchColorNoIdx(sorted, color).unwrap()] = i as u32;
    }
}

/// Extract at most 256 colors from positive-stride ARGB storage. Returns 257
/// when the image is ineligible for a palette, leaving output unspecified.
/// The input must contain every active pixel; final-row padding is unnecessary.
pub fn GetColorPalette(
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    palette: &mut [u32; 256],
) -> usize {
    assert!(width > 0 && height > 0 && stride >= width);
    let required = (height - 1)
        .checked_mul(stride)
        .and_then(|v| v.checked_add(width));
    assert!(required.is_some_and(|n| n <= argb.len()));
    let mut in_use = [false; 1024];
    let mut colors = [0; 1024];
    let mut count = 0;
    let mut last_pix = !argb[0];
    for y in 0..height {
        for &pixel in &argb[y * stride..y * stride + width] {
            if pixel == last_pix {
                continue;
            }
            last_pix = pixel;
            let mut key = VP8LHashPix(pixel, 22) as usize;
            while in_use[key] && colors[key] != pixel {
                key = (key + 1) & 1023;
            }
            if !in_use[key] {
                colors[key] = pixel;
                in_use[key] = true;
                count += 1;
                if count > 256 {
                    return 257;
                }
            }
        }
    }
    let mut index = 0;
    for i in 0..1024 {
        if in_use[i] {
            palette[index] = colors[i];
            index += 1;
        }
    }
    palette[..count].sort_unstable();
    count
}

fn PaletteComponentDistance(v: u32) -> u32 {
    if v <= 128 { v } else { 256 - v }
}

fn PaletteColorDistance(col1: u32, col2: u32) -> u32 {
    let diff = VP8LSubPixels(col1, col2);
    let rgb: u32 = [0, 8, 16]
        .into_iter()
        .map(|shift| PaletteComponentDistance((diff >> shift) & 255))
        .sum();
    9 * rgb + PaletteComponentDistance(diff >> 24)
}

fn PaletteHasNonMonotonousDeltas(palette: &[u32]) -> bool {
    let mut predict = 0;
    let mut sign_found = 0u8;
    for &color in palette {
        let diff = VP8LSubPixels(color, predict);
        for (shift, positive, negative) in [(16, 1, 2), (8, 8, 16), (0, 64, 128)] {
            let delta = (diff >> shift) & 255;
            if delta != 0 {
                sign_found |= if delta < 128 { positive } else { negative };
            }
        }
        predict = color;
    }
    sign_found & sign_found.wrapping_shl(1) != 0
}

/// The default PaletteSort branch. Input is sorted in ascending ARGB order.
pub fn PaletteSortDefault(palette_sorted: &[u32], palette: &mut [u32]) {
    palette.copy_from_slice(palette_sorted);
    if palette.len() > 17 && palette[0] == 0 {
        palette.rotate_left(1);
    }
}

/// Greedy delta ordering, retaining the first candidate on equal distances.
pub fn PaletteSortMinimizeDeltas(palette_sorted: &[u32], palette: &mut [u32]) {
    palette.copy_from_slice(palette_sorted);
    if !PaletteHasNonMonotonousDeltas(palette_sorted) {
        return;
    }
    let mut count = palette.len();
    if count > 17 && palette[0] == 0 {
        count -= 1;
        palette.swap(0, count);
    }
    let mut predict = 0;
    for i in 0..count {
        let mut best = i;
        let mut score = u32::MAX;
        for (k, &color) in palette.iter().enumerate().take(count).skip(i) {
            let candidate = PaletteColorDistance(color, predict);
            if candidate < score {
                score = candidate;
                best = k;
            }
        }
        palette.swap(i, best);
        predict = palette[i];
    }
}
