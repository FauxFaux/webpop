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

#[inline]
fn PaletteComponentDistance(v: u32) -> u32 {
    if v <= 128 { v } else { 256 - v }
}

#[inline]
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

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteSorting {
    kSortedDefault,
    kMinimizeDelta,
    kModifiedZeng,
    kMinLAFromZeng,
    kMinLAFromDelta,
}

fn CoOccurrenceBuild(
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    sorted: &[u32],
) -> Result<Vec<u32>, std::collections::TryReserveError> {
    assert!(width > 0 && height > 0 && stride >= width);
    let required = (height - 1)
        .checked_mul(stride)
        .and_then(|v| v.checked_add(width));
    assert!(required.is_some_and(|n| n <= argb.len()));
    let n = sorted.len();
    let mut matrix = Vec::new();
    matrix.try_reserve_exact(n * n)?;
    matrix.resize(n * n, 0u32);
    // C uses two index rows. A single row suffices: read the top index before
    // replacing it, retaining the current row's left index separately.
    let mut top = Vec::new();
    top.try_reserve_exact(width)?;
    top.resize(width, 0usize);
    let mut previous = !argb[0];
    let mut index = 0;
    for y in 0..height {
        let mut left = 0;
        for x in 0..width {
            let pixel = argb[y * stride + x];
            if pixel != previous {
                index = SearchColorNoIdx(sorted, pixel).expect("pixel absent from palette");
                previous = pixel;
            }
            for neighbor in [(x > 0).then_some(left), (y > 0).then_some(top[x])]
                .into_iter()
                .flatten()
            {
                if index != neighbor {
                    matrix[index * n + neighbor] = matrix[index * n + neighbor].wrapping_add(1);
                    matrix[neighbor * n + index] = matrix[neighbor * n + index].wrapping_add(1);
                }
            }
            top[x] = index;
            left = index;
        }
    }
    Ok(matrix)
}

fn CoOccurrenceFindMax(matrix: &[u32], n: usize) -> (usize, usize) {
    let mut c1 = 0;
    let mut best_sum = 0;
    for i in 0..n {
        let sum = matrix[i * n..(i + 1) * n]
            .iter()
            .fold(0u32, |sum, &v| sum.wrapping_add(v));
        if sum > best_sum {
            best_sum = sum;
            c1 = i;
        }
    }
    let mut c2 = 0;
    let mut best = 0;
    for i in 0..n {
        if matrix[c1 * n + i] > best {
            best = matrix[c1 * n + i];
            c2 = i;
        }
    }
    assert_ne!(
        c1, c2,
        "multi-color spatial palette needs an adjacency edge"
    );
    (c1, c2)
}

// cut[k] is the weight crossing the boundary after the first k colors.
fn MinLACutProfile(matrix: &[u32], order: &[usize], cut: &mut [i64; 257]) {
    let n = order.len();
    let mut running = 0;
    cut[0] = 0;
    for k in 0..n {
        let row = &matrix[order[k] * n..(order[k] + 1) * n];
        let before: i64 = order[..k].iter().map(|&c| i64::from(row[c])).sum();
        let after: i64 = order[k + 1..].iter().map(|&c| i64::from(row[c])).sum();
        running += after - before;
        cut[k + 1] = running;
    }
}

// Read the order after removing order[at] and appending it, without mutation.
#[inline]
fn ActualColorIdx(order: &[usize], at: usize, j: usize) -> usize {
    if j == order.len() - 1 {
        order[at]
    } else if j < at {
        order[j]
    } else {
        order[j + 1]
    }
}

fn MinLABestSlot(
    matrix: &[u32],
    at: usize,
    cut: &[i64; 257],
    order: &[usize],
    row_sum: &[i64; 256],
) -> usize {
    let n = order.len();
    let color = order[at];
    let row = &matrix[color * n..(color + 1) * n];
    let total_w = row_sum[color];
    let (mut left_w, mut left_wr, mut pre_v) = (0, 0, 0);
    let (mut best_j, mut best, mut at_cost) = (0, i64::MAX, 0);
    for j in 0..n {
        let r = ActualColorIdx(order, at, j);
        // Moving this color also changes distances between untouched colors
        // across the insertion boundary; the cut term accounts for those edges.
        let own = j as i64 * (2 * left_w - total_w) - left_w - 2 * left_wr;
        let straddle = if j <= at {
            cut[j] - pre_v
        } else {
            cut[j + 1] - (total_w - pre_v - i64::from(row[order[j]]))
        };
        let cost = own + straddle;
        if cost < best {
            best = cost;
            best_j = j;
        }
        if j == at {
            at_cost = cost;
        }
        pre_v += i64::from(row[order[j]]);
        left_w += i64::from(row[r]);
        left_wr += i64::from(row[r]) * j as i64;
    }
    // Preserve the current position on ties, even if an earlier slot ties too.
    if at_cost == best { at } else { best_j }
}

fn PaletteMinLARefine(matrix: &[u32], order: &mut [usize]) {
    let n = order.len();
    let mut row_sum = [0i64; 256];
    for c in 0..n {
        row_sum[c] = matrix[c * n..(c + 1) * n]
            .iter()
            .map(|&v| i64::from(v))
            .sum();
    }
    let mut cut = [0; 257];
    MinLACutProfile(matrix, order, &mut cut);
    for _ in 0..40 {
        let mut moved = false;
        for at in 0..n {
            let best = MinLABestSlot(matrix, at, &cut, order, &row_sum);
            if best != at {
                if best > at {
                    order[at..=best].rotate_left(1);
                } else {
                    order[best..=at].rotate_right(1);
                }
                MinLACutProfile(matrix, order, &mut cut);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

fn PaletteSortModifiedZeng(matrix: &[u32], order: &mut [usize]) {
    let n = order.len();
    let (c1, c2) = CoOccurrenceFindMax(matrix, n);
    let mut remapping = [0usize; 256];
    remapping[0] = c1;
    remapping[1] = c2;
    let (mut first, mut last) = (0, 1);
    let mut sums = [(0usize, 0u32); 256];
    let mut count = 0;
    let mut best = 0;
    for i in 0..n {
        if i == c1 || i == c2 {
            continue;
        }
        sums[count] = (i, matrix[i * n + c1].wrapping_add(matrix[i * n + c2]));
        if sums[count].1 > sums[best].1 {
            best = count;
        }
        count += 1;
    }
    while count > 0 {
        let color = sums[best].0;
        let used = n - count;
        let mut delta = 0i32;
        for j in 0..used {
            let neighbor = remapping[(first + j) % n];
            delta = delta.wrapping_add(
                (used as i32 - 1 - 2 * j as i32).wrapping_mul(matrix[color * n + neighbor] as i32),
            );
        }
        if delta > 0 {
            first = if first == 0 { n - 1 } else { first - 1 };
            remapping[first] = color;
        } else {
            last += 1;
            remapping[last] = color;
        }
        sums[best] = sums[count - 1];
        count -= 1;
        best = 0;
        for i in 0..count {
            sums[i].1 = sums[i].1.wrapping_add(matrix[color * n + sums[i].0]);
            if sums[i].1 > sums[best].1 {
                best = i;
            }
        }
    }
    debug_assert_eq!((last + 1) % n, first);
    for i in 0..n {
        order[i] = remapping[(first + i) % n];
    }
}

/// Sort a nonempty palette (at most 256 distinct, ascending colors). Spatial
/// methods require a valid positive-stride image containing those colors.
/// Owned co-occurrence and row allocations propagate allocation failure.
#[allow(clippy::too_many_arguments)]
pub fn PaletteSort(
    method: PaletteSorting,
    argb: &[u32],
    width: usize,
    height: usize,
    stride: usize,
    palette_sorted: &[u32],
    palette: &mut [u32],
) -> Result<(), std::collections::TryReserveError> {
    use PaletteSorting::*;
    let n = palette_sorted.len();
    assert!((1..=256).contains(&n));
    assert_eq!(n, palette.len());
    if n == 1 {
        palette[0] = palette_sorted[0];
        return Ok(());
    }
    match method {
        kSortedDefault => PaletteSortDefault(palette_sorted, palette),
        kMinimizeDelta => PaletteSortMinimizeDeltas(palette_sorted, palette),
        kModifiedZeng | kMinLAFromZeng | kMinLAFromDelta => {
            let matrix = CoOccurrenceBuild(argb, width, height, stride, palette_sorted)?;
            let mut storage = [0usize; 256];
            let order = &mut storage[..n];
            if method == kMinLAFromDelta {
                PaletteSortMinimizeDeltas(palette_sorted, palette);
                for i in 0..n {
                    order[i] = SearchColorNoIdx(palette_sorted, palette[i]).unwrap();
                }
            } else {
                PaletteSortModifiedZeng(&matrix, order);
            }
            if method != kModifiedZeng {
                PaletteMinLARefine(&matrix, order);
            }
            for i in 0..n {
                palette[i] = palette_sorted[order[i]];
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrangement_cost(matrix: &[u32], order: &[usize]) -> u64 {
        let n = order.len();
        let mut cost = 0;
        for i in 0..n {
            for j in i + 1..n {
                cost += u64::from(matrix[order[i] * n + order[j]]) * (j - i) as u64;
            }
        }
        cost
    }

    #[test]
    fn spatial_counts_use_four_connectivity_and_ignore_padding() {
        // a b a
        // c b c
        let matrix =
            CoOccurrenceBuild(&[10, 20, 10, 999, 30, 20, 30], 3, 2, 4, &[10, 20, 30]).unwrap();
        assert_eq!(matrix, [0, 2, 2, 2, 0, 2, 2, 2, 0]);
        // Equal row totals and edge counts select the first distinct colors.
        assert_eq!(CoOccurrenceFindMax(&matrix, 3), (0, 1));
    }

    #[test]
    fn minla_slots_match_exhaustive_full_costs() {
        let mut seed = 0x6d2b79f5u32;
        let mut moves = 0;
        let mut ties = 0;
        for n in 2..=12 {
            for trial in 0..80 {
                let mut matrix = vec![0; n * n];
                for i in 0..n {
                    for j in i + 1..n {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let weight = if trial == 0 {
                            0
                        } else if trial == 1 {
                            u32::MAX
                        } else {
                            seed % 17
                        };
                        matrix[i * n + j] = weight;
                        matrix[j * n + i] = weight;
                    }
                }
                let mut order: Vec<_> = (0..n).collect();
                order.rotate_left(trial % n);
                let mut cut = [0; 257];
                MinLACutProfile(&matrix, &order, &mut cut);
                let mut sums = [0; 256];
                for i in 0..n {
                    sums[i] = matrix[i * n..(i + 1) * n]
                        .iter()
                        .map(|&v| i64::from(v))
                        .sum();
                }
                for at in 0..n {
                    let mut costs = Vec::new();
                    for slot in 0..n {
                        let mut candidate = order.clone();
                        let color = candidate.remove(at);
                        candidate.insert(slot, color);
                        costs.push(arrangement_cost(&matrix, &candidate));
                    }
                    let minimum = *costs.iter().min().unwrap();
                    let expected = if costs[at] == minimum {
                        ties += 1;
                        at
                    } else {
                        moves += 1;
                        costs.iter().position(|&c| c == minimum).unwrap()
                    };
                    assert_eq!(
                        MinLABestSlot(&matrix, at, &cut, &order, &sums),
                        expected,
                        "n={n} trial={trial} at={at}"
                    );
                }
                let before = arrangement_cost(&matrix, &order);
                PaletteMinLARefine(&matrix, &mut order);
                assert!(arrangement_cost(&matrix, &order) <= before);
                // Small cases settle before the 40-sweep cap: no improving insertion remains.
                for at in 0..n {
                    for slot in 0..n {
                        let mut candidate = order.clone();
                        let color = candidate.remove(at);
                        candidate.insert(slot, color);
                        assert!(
                            arrangement_cost(&matrix, &candidate)
                                >= arrangement_cost(&matrix, &order)
                        );
                    }
                }
            }
        }
        assert!(moves > 0 && ties > 0);
    }
}
