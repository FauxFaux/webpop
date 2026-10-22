// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/histogram_enc.{c,h} (histogram foundations).
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

use super::backward_references_enc::*;
use crate::dsp::{
    lossless::{VP8LBitEntropy, VP8LStreaks},
    lossless_common::*,
    lossless_enc::*,
};
use crate::webp::format_constants::*;
use std::collections::TryReserveError;

pub const VP8L_NON_TRIVIAL_SYM: u16 = 0xffff;

#[derive(Debug, PartialEq, Eq)]
pub struct VP8LHistogram {
    pub literal: Vec<u32>,
    pub red: [u32; NUM_LITERAL_CODES],
    pub blue: [u32; NUM_LITERAL_CODES],
    pub alpha: [u32; NUM_LITERAL_CODES],
    pub distance: [u32; NUM_DISTANCE_CODES],
    pub palette_code_bits: u32,
    pub trivial_symbol: [u16; 5],
    pub bit_cost: u64,
    pub costs: [u64; 5],
    pub is_used: [u8; 5],
    pub bin_id: u16,
}

pub fn VP8LHistogramNumCodes(palette_code_bits: u32) -> usize {
    assert!(palette_code_bits <= MAX_CACHE_BITS);
    NUM_LITERAL_CODES
        + NUM_LENGTH_CODES
        + if palette_code_bits > 0 {
            1 << palette_code_bits
        } else {
            0
        }
}

pub fn HistogramStatsClear(h: &mut VP8LHistogram) {
    h.trivial_symbol = [VP8L_NON_TRIVIAL_SYM; 5];
    h.is_used = [1; 5];
    h.bit_cost = 0;
    h.costs = [0; 5];
}

fn HistogramArraysClear(h: &mut VP8LHistogram) {
    h.literal.fill(0);
    h.red.fill(0);
    h.blue.fill(0);
    h.alpha.fill(0);
    h.distance.fill(0);
    h.bin_id = 0;
}

pub fn HistogramClear(h: &mut VP8LHistogram) {
    HistogramArraysClear(h);
    HistogramStatsClear(h);
}

/// Resizing changes the active literal alphabet; init_arrays=false preserves
/// existing counts and bin_id, and only resets cached cost statistics.
pub fn VP8LHistogramInit(
    h: &mut VP8LHistogram,
    palette_code_bits: u32,
    init_arrays: bool,
) -> Result<(), TryReserveError> {
    let size = VP8LHistogramNumCodes(palette_code_bits);
    if size > h.literal.len() {
        h.literal.try_reserve_exact(size - h.literal.len())?;
    }
    h.literal.resize(size, 0);
    h.palette_code_bits = palette_code_bits;
    if init_arrays {
        HistogramClear(h);
    } else {
        HistogramStatsClear(h);
    }
    Ok(())
}

/// Unlike C's uninitialized allocation, all counts start at zero in Rust.
pub fn VP8LAllocateHistogram(cache_bits: u32) -> Result<VP8LHistogram, TryReserveError> {
    let mut literal = Vec::new();
    literal.try_reserve_exact(VP8LHistogramNumCodes(cache_bits))?;
    literal.resize(VP8LHistogramNumCodes(cache_bits), 0);
    Ok(VP8LHistogram {
        literal,
        red: [0; NUM_LITERAL_CODES],
        blue: [0; NUM_LITERAL_CODES],
        alpha: [0; NUM_LITERAL_CODES],
        distance: [0; NUM_DISTANCE_CODES],
        palette_code_bits: cache_bits,
        trivial_symbol: [VP8L_NON_TRIVIAL_SYM; 5],
        bit_cost: 0,
        costs: [0; 5],
        is_used: [1; 5],
        bin_id: 0,
    })
}

pub fn HistogramCopy(src: &VP8LHistogram, dst: &mut VP8LHistogram) {
    assert_eq!(src.palette_code_bits, dst.palette_code_bits);
    dst.literal.copy_from_slice(&src.literal);
    dst.red = src.red;
    dst.blue = src.blue;
    dst.alpha = src.alpha;
    dst.distance = src.distance;
    dst.trivial_symbol = src.trivial_symbol;
    dst.bit_cost = src.bit_cost;
    dst.costs = src.costs;
    dst.is_used = src.is_used;
    dst.bin_id = src.bin_id;
}

#[derive(Debug)]
pub struct VP8LHistogramSet {
    pub size: usize,
    // Length is upstream max_size; slots beyond size remain owned for reuse.
    pub histograms: Vec<VP8LHistogram>,
}

pub fn VP8LAllocateHistogramSet(
    size: usize,
    cache_bits: u32,
) -> Result<VP8LHistogramSet, TryReserveError> {
    let mut histograms = Vec::new();
    histograms.try_reserve_exact(size)?;
    for _ in 0..size {
        histograms.push(VP8LAllocateHistogram(cache_bits)?);
    }
    Ok(VP8LHistogramSet { size, histograms })
}

pub fn VP8LHistogramSetClear(set: &mut VP8LHistogramSet) {
    set.size = set.histograms.len();
    for h in &mut set.histograms {
        HistogramArraysClear(h);
        // Upstream clears the entire set with memset, without StatsClear.
        h.trivial_symbol = [0; 5];
        h.is_used = [0; 5];
        h.bit_cost = 0;
        h.costs = [0; 5];
    }
}

pub type DistanceModifier = fn(u32, u32) -> u32;

pub fn HistogramAddSinglePixOrCopy(
    histo: &mut VP8LHistogram,
    v: &PixOrCopy,
    distance_modifier: Option<DistanceModifier>,
    distance_modifier_arg0: u32,
) {
    if PixOrCopyIsLiteral(v) {
        for (population, component) in [
            (&mut histo.alpha[..], 3),
            (&mut histo.red[..], 2),
            (&mut histo.literal[..], 1),
            (&mut histo.blue[..], 0),
        ] {
            let bin = &mut population[PixOrCopyLiteral(v, component) as usize];
            *bin = bin.wrapping_add(1);
        }
    } else if PixOrCopyIsCacheIdx(v) {
        assert!(histo.palette_code_bits > 0);
        let ix = NUM_LITERAL_CODES + NUM_LENGTH_CODES + PixOrCopyCacheIdx(v) as usize;
        histo.literal[ix] = histo.literal[ix].wrapping_add(1);
    } else {
        let (code, _) = VP8LPrefixEncodeBits(PixOrCopyLength(v));
        assert!((code as usize) < NUM_LENGTH_CODES);
        let bin = &mut histo.literal[NUM_LITERAL_CODES + code as usize];
        *bin = bin.wrapping_add(1);
        let distance = PixOrCopyDistance(v);
        let distance = distance_modifier.map_or(distance, |f| f(distance_modifier_arg0, distance));
        let (code, _) = VP8LPrefixEncodeBits(distance);
        let bin = &mut histo.distance[code as usize];
        *bin = bin.wrapping_add(1);
    }
}

pub fn VP8LHistogramStoreRefs(
    refs: &VP8LBackwardRefs,
    distance_modifier: Option<DistanceModifier>,
    distance_modifier_arg0: u32,
    histo: &mut VP8LHistogram,
) {
    for v in refs.iter() {
        HistogramAddSinglePixOrCopy(histo, v, distance_modifier, distance_modifier_arg0);
    }
}

/// None preserves the existing palette bits (upstream's negative sentinel).
pub fn VP8LHistogramCreate(
    h: &mut VP8LHistogram,
    refs: &VP8LBackwardRefs,
    palette_code_bits: Option<u32>,
) -> Result<(), TryReserveError> {
    VP8LHistogramInit(h, palette_code_bits.unwrap_or(h.palette_code_bits), true)?;
    VP8LHistogramStoreRefs(refs, None, 0, h);
    Ok(())
}

pub fn BitsEntropyRefine(entropy: &VP8LBitEntropy) -> u64 {
    if entropy.nonzeros <= 1 {
        return 0;
    }
    if entropy.nonzeros == 2 {
        return DivRound(
            (99 * (u64::from(entropy.sum) << LOG_2_PRECISION_BITS) + entropy.entropy) as i64,
            100,
        ) as u64;
    }
    let mix: u64 = match entropy.nonzeros {
        3 => 950,
        4 => 700,
        _ => 627,
    };
    // The subtraction/multiplication take place in uint32_t in C.
    let min_limit = u64::from(entropy.sum.wrapping_mul(2).wrapping_sub(entropy.max_val))
        << LOG_2_PRECISION_BITS;
    let min_limit = DivRound(
        mix.wrapping_mul(min_limit)
            .wrapping_add((1000 - mix).wrapping_mul(entropy.entropy)) as i64,
        1000,
    ) as u64;
    entropy.entropy.max(min_limit)
}

pub fn VP8LBitsEntropy(array: &[u32]) -> u64 {
    let mut entropy = VP8LBitEntropy::default();
    VP8LBitsEntropyUnrefined(array, &mut entropy);
    BitsEntropyRefine(&entropy)
}

pub fn InitialHuffmanCost() -> u64 {
    ((CODE_LENGTH_CODES as u64 * 3) << LOG_2_PRECISION_BITS)
        - DivRound(91_i64 << LOG_2_PRECISION_BITS, 10) as u64
}

pub fn FinalHuffmanCost(stats: &VP8LStreaks) -> u64 {
    let mut retval_extra = (stats.counts[0] as u32)
        .wrapping_mul(1600)
        .wrapping_add((stats.streaks[0][1] as u32).wrapping_mul(240));
    retval_extra = retval_extra
        .wrapping_add((stats.counts[1] as u32).wrapping_mul(2640))
        .wrapping_add((stats.streaks[1][1] as u32).wrapping_mul(720))
        .wrapping_add((stats.streaks[0][0] as u32).wrapping_mul(1840))
        .wrapping_add((stats.streaks[1][0] as u32).wrapping_mul(3360));
    InitialHuffmanCost() + (u64::from(retval_extra) << (LOG_2_PRECISION_BITS - 10))
}

/// Returns (cost, trivial_symbol, is_used).
pub fn PopulationCost(population: &[u32]) -> (u64, u16, u8) {
    let mut entropy = VP8LBitEntropy::default();
    let mut stats = VP8LStreaks::default();
    VP8LGetEntropyUnrefined(population, &mut entropy, &mut stats);
    let trivial = if entropy.nonzeros == 1 {
        entropy.nonzero_code as u16
    } else {
        VP8L_NON_TRIVIAL_SYM
    };
    (
        BitsEntropyRefine(&entropy) + FinalHuffmanCost(&stats),
        trivial,
        u8::from(stats.streaks[1][0] != 0 || stats.streaks[1][1] != 0),
    )
}

pub fn GetPopulationInfo(histo: &VP8LHistogram, index: usize) -> &[u32] {
    match index {
        0 => &histo.literal,
        1 => &histo.red,
        2 => &histo.blue,
        3 => &histo.alpha,
        4 => &histo.distance,
        _ => panic!("invalid histogram index"),
    }
}

pub fn VP8LHistogramEstimateBits(h: &VP8LHistogram) -> u64 {
    let cost: u64 = (0..5)
        .map(|i| PopulationCost(GetPopulationInfo(h, i)).0)
        .sum();
    let extra = VP8LExtraCost(&h.literal[NUM_LITERAL_CODES..NUM_LITERAL_CODES + NUM_LENGTH_CODES])
        .wrapping_add(VP8LExtraCost(&h.distance));
    cost + (u64::from(extra) << LOG_2_PRECISION_BITS)
}

/// Cache population costs; length/distance extra bits are deliberately excluded.
pub fn ComputeHistogramCost(h: &mut VP8LHistogram) {
    for i in 0..5 {
        let (cost, trivial, used) = PopulationCost(GetPopulationInfo(h, i));
        h.costs[i] = cost;
        h.trivial_symbol[i] = trivial;
        h.is_used[i] = used;
    }
    h.bit_cost = h.costs.iter().sum();
}
