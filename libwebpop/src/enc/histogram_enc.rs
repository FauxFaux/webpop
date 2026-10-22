// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/histogram_enc.{c,h}.
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

// Histogram clustering. Costs omit length/distance extra bits, which are
// constant across combinations. Preserve the upstream order and strict ties.
const NUM_PARTITIONS: usize = 4;
const BIN_SIZE: usize = NUM_PARTITIONS * NUM_PARTITIONS * NUM_PARTITIONS;
const MAX_HISTO_GREEDY: usize = 100;

pub fn GetCombinedEntropy(a: &VP8LHistogram, b: &VP8LHistogram, index: usize) -> u64 {
    let trivial = a.trivial_symbol[index] != VP8L_NON_TRIVIAL_SYM
        && a.trivial_symbol[index] == b.trivial_symbol[index];
    if trivial || a.is_used[index] == 0 || b.is_used[index] == 0 {
        return if a.is_used[index] != 0 {
            a.costs[index]
        } else {
            b.costs[index]
        };
    }
    let mut entropy = VP8LBitEntropy::default();
    let mut stats = VP8LStreaks::default();
    VP8LGetCombinedEntropyUnrefined(
        GetPopulationInfo(a, index),
        GetPopulationInfo(b, index),
        &mut entropy,
        &mut stats,
    );
    BitsEntropyRefine(&entropy) + FinalHuffmanCost(&stats)
}

fn SaturateAdd(a: u64, b: i64) -> i64 {
    b.saturating_add(i64::try_from(a).expect("histogram cost fits int64_t"))
}

/// None means the partial cost reached the threshold; no cost is valid then.
pub fn GetCombinedHistogramEntropy(
    a: &VP8LHistogram,
    b: &VP8LHistogram,
    threshold: i64,
) -> Option<(u64, [u64; 5])> {
    assert_eq!(a.palette_code_bits, b.palette_code_bits);
    if threshold <= 0 {
        return None;
    }
    let mut total = 0;
    let mut costs = [0; 5];
    for (i, cost) in costs.iter_mut().enumerate() {
        *cost = GetCombinedEntropy(a, b, i);
        total += *cost;
        if total >= threshold as u64 {
            return None;
        }
    }
    Some((total, costs))
}

fn GetPopulationInfoMut(h: &mut VP8LHistogram, index: usize) -> &mut [u32] {
    match index {
        0 => &mut h.literal,
        1 => &mut h.red,
        2 => &mut h.blue,
        3 => &mut h.alpha,
        4 => &mut h.distance,
        _ => unreachable!(),
    }
}

pub fn HistogramAdd(a: &VP8LHistogram, b: &VP8LHistogram, out: &mut VP8LHistogram) {
    assert_eq!(a.palette_code_bits, b.palette_code_bits);
    assert_eq!(a.palette_code_bits, out.palette_code_bits);
    for i in 0..5 {
        let x = GetPopulationInfo(a, i);
        let y = GetPopulationInfo(b, i);
        for (j, v) in GetPopulationInfoMut(out, i).iter_mut().enumerate() {
            *v = match (a.is_used[i] != 0, b.is_used[i] != 0) {
                (true, true) => x[j].wrapping_add(y[j]),
                (true, false) => x[j],
                (false, true) => y[j],
                (false, false) => 0,
            };
        }
        out.trivial_symbol[i] = if a.trivial_symbol[i] == b.trivial_symbol[i] {
            a.trivial_symbol[i]
        } else {
            VP8L_NON_TRIVIAL_SYM
        };
        out.is_used[i] = a.is_used[i] | b.is_used[i];
    }
}

// Upstream permits b == out. Keep that operation explicit in safe Rust.
pub fn HistogramAddInPlace(a: &VP8LHistogram, out: &mut VP8LHistogram) {
    assert_eq!(a.palette_code_bits, out.palette_code_bits);
    for i in 0..5 {
        if a.is_used[i] != 0 {
            let used = out.is_used[i] != 0;
            for (v, &x) in GetPopulationInfoMut(out, i)
                .iter_mut()
                .zip(GetPopulationInfo(a, i))
            {
                *v = if used { v.wrapping_add(x) } else { x };
            }
        }
        if a.trivial_symbol[i] != out.trivial_symbol[i] {
            out.trivial_symbol[i] = VP8L_NON_TRIVIAL_SYM;
        }
        out.is_used[i] |= a.is_used[i];
    }
}

fn UpdateHistogramCost(cost: (u64, [u64; 5]), h: &mut VP8LHistogram) {
    (h.bit_cost, h.costs) = cost;
}

pub fn HistogramAddEval(
    a: &VP8LHistogram,
    b: &VP8LHistogram,
    out: &mut VP8LHistogram,
    threshold: i64,
) -> bool {
    let Some(cost) =
        GetCombinedHistogramEntropy(a, b, SaturateAdd(a.bit_cost + b.bit_cost, threshold))
    else {
        return false;
    };
    HistogramAdd(a, b, out);
    UpdateHistogramCost(cost, out);
    true
}

pub fn HistogramAddThresh(a: &VP8LHistogram, b: &VP8LHistogram, threshold: i64) -> Option<i64> {
    GetCombinedHistogramEntropy(a, b, SaturateAdd(a.bit_cost, threshold))
        .map(|(cost, _)| cost as i64 - a.bit_cost as i64)
}

/// Assign a reference to its starting tile, even when a copy skips tiles/rows.
pub fn HistogramBuild(
    width: usize,
    bits: u32,
    refs: &VP8LBackwardRefs,
    set: &mut VP8LHistogramSet,
) {
    assert!(width > 0 && bits > 0);
    let tile_width = width.div_ceil(1 << bits);
    VP8LHistogramSetClear(set);
    let mut position = 0;
    for token in refs.iter() {
        let index = ((position / width) >> bits) * tile_width + ((position % width) >> bits);
        HistogramAddSinglePixOrCopy(&mut set.histograms[index], token, None, 0);
        position = position
            .checked_add(PixOrCopyLength(token) as usize)
            .expect("image geometry");
    }
}

// Empty original tiles stay owned, with is_used == [0; 5] standing for C's
// NULL pointers. Active cluster slots are compacted without extra allocations.
pub fn HistogramCopyAndAnalyze(orig: &mut VP8LHistogramSet, out: &mut VP8LHistogramSet) {
    assert_eq!(orig.histograms.len(), out.histograms.len());
    out.size = 0;
    for h in &mut orig.histograms {
        ComputeHistogramCost(h);
        if h.is_used != [0; 5] {
            HistogramCopy(h, &mut out.histograms[out.size]);
            out.size += 1;
        }
    }
    assert!(out.size > 0);
}

fn GetBinIdForEntropy(min: u64, max: u64, value: u64) -> usize {
    if max > min {
        ((NUM_PARTITIONS as f64 - 1e-6) * (value - min) as f64 / (max - min) as f64) as usize
    } else {
        0
    }
}

pub fn HistogramAnalyzeEntropyBin(set: &mut VP8LHistogramSet, low_effort: bool) {
    let mut min = [u64::MAX; 3];
    let mut max = [0; 3];
    for h in &set.histograms[..set.size] {
        for i in 0..3 {
            min[i] = min[i].min(h.costs[i]);
            max[i] = max[i].max(h.costs[i]);
        }
    }
    for h in &mut set.histograms[..set.size] {
        let mut bin = GetBinIdForEntropy(min[0], max[0], h.costs[0]);
        if !low_effort {
            for i in 1..3 {
                bin = bin * NUM_PARTITIONS + GetBinIdForEntropy(min[i], max[i], h.costs[i]);
            }
        }
        h.bin_id = bin as u16;
    }
}

fn HistogramSetRemoveHistogram(set: &mut VP8LHistogramSet, index: usize) {
    set.histograms.swap(index, set.size - 1);
    set.size -= 1;
    assert!(set.size > 0);
}

fn add_cluster(set: &mut VP8LHistogramSet, src: usize, dst: usize) {
    assert_ne!(src, dst);
    if src < dst {
        let (left, right) = set.histograms.split_at_mut(dst);
        HistogramAddInPlace(&left[src], &mut right[0]);
    } else {
        let (left, right) = set.histograms.split_at_mut(src);
        HistogramAddInPlace(&right[0], &mut left[dst]);
    }
}

pub fn HistogramCombineEntropyBin(
    set: &mut VP8LHistogramSet,
    tmp: &mut VP8LHistogram,
    num_bins: usize,
    factor: i32,
    low_effort: bool,
) {
    assert!(num_bins <= BIN_SIZE);
    let mut first = [None; BIN_SIZE];
    let mut failures = [0u16; BIN_SIZE];
    let mut index = 0;
    while index < set.size {
        let bin = set.histograms[index].bin_id as usize;
        assert!(bin < num_bins);
        let Some(head) = first[bin] else {
            first[bin] = Some(index);
            index += 1;
            continue;
        };
        if low_effort {
            add_cluster(set, index, head);
            HistogramSetRemoveHistogram(set, index);
        } else {
            let a = &set.histograms[head];
            let b = &set.histograms[index];
            let threshold = -DivRound(b.bit_cost as i64 * i64::from(factor), 100);
            if HistogramAddEval(a, b, tmp, threshold) {
                let trivial =
                    |h: &VP8LHistogram| (1..4).all(|i| h.trivial_symbol[i] != VP8L_NON_TRIVIAL_SYM);
                if trivial(tmp) || (!trivial(a) && !trivial(b)) || failures[bin] >= 32 {
                    std::mem::swap(tmp, &mut set.histograms[head]);
                    HistogramSetRemoveHistogram(set, index);
                    continue;
                }
                failures[bin] += 1;
            }
            index += 1;
        }
    }
    if low_effort {
        for h in &mut set.histograms[..set.size] {
            ComputeHistogramCost(h);
        }
    }
}

fn MyRand(seed: &mut u32) -> u32 {
    *seed = ((u64::from(*seed) * 48271) % 2147483647) as u32;
    *seed
}

#[derive(Clone, Copy, Debug)]
struct HistogramPair {
    idx1: usize,
    idx2: usize,
    cost_diff: i64,
    cost: (u64, [u64; 5]),
}

// This is deliberately the upstream unsorted queue with a best element at
// the front. A binary heap would change equal-cost traversal and merge order.
struct HistoQueue {
    queue: Vec<HistogramPair>,
    max_size: usize,
}

impl HistoQueue {
    fn new(max_size: usize) -> Result<Self, TryReserveError> {
        let mut queue = Vec::new();
        queue.try_reserve_exact(max_size)?;
        Ok(Self { queue, max_size })
    }
    fn update_head(&mut self, index: usize) {
        if self.queue[index].cost_diff < self.queue[0].cost_diff {
            self.queue.swap(0, index);
        }
    }
    fn push(&mut self, set: &VP8LHistogramSet, i: usize, j: usize, threshold: i64) -> i64 {
        if self.queue.len() == self.max_size {
            return 0;
        }
        let (idx1, idx2) = (i.min(j), i.max(j));
        let mut pair = HistogramPair {
            idx1,
            idx2,
            cost_diff: 0,
            cost: (0, [0; 5]),
        };
        if !pair.update(set, threshold) {
            return 0;
        }
        self.queue.push(pair);
        self.update_head(self.queue.len() - 1);
        pair.cost_diff
    }
}

impl HistogramPair {
    fn fix(&mut self, bad: usize, good: usize) {
        if self.idx1 == bad {
            self.idx1 = good;
        }
        if self.idx2 == bad {
            self.idx2 = good;
        }
        if self.idx1 > self.idx2 {
            std::mem::swap(&mut self.idx1, &mut self.idx2);
        }
    }
    fn update(&mut self, set: &VP8LHistogramSet, threshold: i64) -> bool {
        self.update_moved(set, threshold, None)
    }
    fn update_moved(
        &mut self,
        set: &VP8LHistogramSet,
        threshold: i64,
        moved: Option<usize>,
    ) -> bool {
        // C's removed last slot still aliases the histogram moved to idx2.
        // Stochastic updates evaluate costs before fixing that old index.
        let index = |i| if i == set.size { moved.unwrap_or(i) } else { i };
        let a = &set.histograms[index(self.idx1)];
        let b = &set.histograms[index(self.idx2)];
        let sum = a.bit_cost + b.bit_cost;
        let Some(cost) = GetCombinedHistogramEntropy(a, b, SaturateAdd(sum, threshold)) else {
            return false;
        };
        self.cost = cost;
        self.cost_diff = cost.0 as i64 - sum as i64;
        true
    }
}

pub fn HistogramCombineGreedy(set: &mut VP8LHistogramSet) -> Result<(), TryReserveError> {
    assert!(set.size <= MAX_HISTO_GREEDY);
    let mut q = HistoQueue::new(set.size * set.size)?;
    for i in 0..set.size {
        for j in i + 1..set.size {
            q.push(set, i, j, 0);
        }
    }
    while !q.queue.is_empty() {
        let best = q.queue[0];
        add_cluster(set, best.idx2, best.idx1);
        UpdateHistogramCost(best.cost, &mut set.histograms[best.idx1]);
        HistogramSetRemoveHistogram(set, best.idx2);
        let mut i = 0;
        while i < q.queue.len() {
            let p = &mut q.queue[i];
            if p.idx1 == best.idx1
                || p.idx2 == best.idx1
                || p.idx1 == best.idx2
                || p.idx2 == best.idx2
            {
                q.queue.swap_remove(i);
            } else {
                p.fix(set.size, best.idx2);
                q.update_head(i);
                i += 1;
            }
        }
        for i in 0..set.size {
            if i != best.idx1 {
                q.push(set, best.idx1, i, 0);
            }
        }
    }
    Ok(())
}

/// Returns whether the subsequent greedy pass is needed.
pub fn HistogramCombineStochastic(
    set: &mut VP8LHistogramSet,
    min_size: usize,
) -> Result<bool, TryReserveError> {
    if set.size < min_size {
        return Ok(true);
    }
    let mut q = HistoQueue::new(9)?;
    let mut seed = 1;
    let outer_iters = set.size;
    let mut failures = 0;
    for _ in 0..outer_iters {
        if set.size < min_size {
            break;
        }
        failures += 1;
        if failures >= outer_iters / 2 {
            break;
        }
        let mut best_cost = q.queue.first().map_or(0, |p| p.cost_diff);
        let range = ((set.size - 1) * set.size) as u32;
        for _ in 0..set.size / 2 {
            if set.size < 2 {
                break;
            }
            let tmp = MyRand(&mut seed) % range;
            let i = tmp as usize / (set.size - 1);
            let mut j = tmp as usize % (set.size - 1);
            if j >= i {
                j += 1;
            }
            let cost = q.push(set, i, j, best_cost);
            if cost < 0 {
                best_cost = cost;
                if q.queue.len() == q.max_size {
                    break;
                }
            }
        }
        let Some(best) = q.queue.first().copied() else {
            continue;
        };
        add_cluster(set, best.idx2, best.idx1);
        UpdateHistogramCost(best.cost, &mut set.histograms[best.idx1]);
        HistogramSetRemoveHistogram(set, best.idx2);
        let mut j = 0;
        while j < q.queue.len() {
            let p = &mut q.queue[j];
            let first_best = p.idx1 == best.idx1 || p.idx1 == best.idx2;
            let second_best = p.idx2 == best.idx1 || p.idx2 == best.idx2;
            if first_best && second_best {
                q.queue.swap_remove(j);
                continue;
            }
            if first_best || second_best {
                p.fix(best.idx2, best.idx1);
                if !p.update_moved(set, 0, Some(best.idx2)) {
                    q.queue.swap_remove(j);
                    continue;
                }
            }
            p.fix(set.size, best.idx2);
            q.update_head(j);
            j += 1;
        }
        failures = 0;
    }
    Ok(set.size <= min_size)
}

pub fn HistogramRemap(orig: &VP8LHistogramSet, out: &mut VP8LHistogramSet, symbols: &mut [u32]) {
    let size = out.size;
    let raw_size = orig.histograms.len();
    assert_eq!(raw_size, out.histograms.len());
    assert!(symbols.len() >= raw_size && size > 0);
    for i in 0..raw_size {
        symbols[i] = 0;
        if size == 1 {
            continue;
        }
        let h = &orig.histograms[i];
        if h.is_used == [0; 5] {
            assert!(i > 0);
            symbols[i] = symbols[i - 1];
            continue;
        }
        let mut best_bits = i64::MAX;
        for k in 0..size {
            if let Some(cost) = HistogramAddThresh(&out.histograms[k], h, best_bits) {
                best_bits = cost;
                symbols[i] = k as u32;
            }
        }
    }
    VP8LHistogramSetClear(out);
    out.size = size;
    for (h, &symbol) in orig.histograms.iter().zip(symbols.iter()) {
        if h.is_used != [0; 5] {
            HistogramAddInPlace(h, &mut out.histograms[symbol as usize]);
        }
    }
}

pub fn GetCombineCostFactor(size: usize, quality: u32) -> i32 {
    let mut factor = 16;
    if quality < 90 {
        for threshold in [256, 512, 1024] {
            if size > threshold {
                factor /= 2;
            }
        }
        if quality <= 50 {
            factor /= 2;
        }
    }
    factor
}

/// Cluster an image's spatially coded references. Internal callers supply valid
/// positive geometry, tile bits and output capacity; allocation failures return
/// Result. Progress callbacks are omitted, as in the other internal ports.
#[allow(clippy::too_many_arguments)]
pub fn VP8LGetHistoImageSymbols(
    width: usize,
    height: usize,
    refs: &VP8LBackwardRefs,
    quality: u32,
    low_effort: bool,
    bits: u32,
    cache_bits: u32,
    out: &mut VP8LHistogramSet,
    tmp: &mut VP8LHistogram,
    symbols: &mut [u32],
) -> Result<(), TryReserveError> {
    assert!(width > 0 && height > 0 && bits > 0 && quality <= 100);
    let size = width
        .div_ceil(1 << bits)
        .checked_mul(height.div_ceil(1 << bits))
        .expect("image geometry");
    assert_eq!(out.histograms.len(), size);
    let mut orig = VP8LAllocateHistogramSet(size, cache_bits)?;
    HistogramBuild(width, bits, refs, &mut orig);
    HistogramCopyAndAnalyze(&mut orig, out);
    let bins = if low_effort { NUM_PARTITIONS } else { BIN_SIZE };
    let entropy_combine = out.size > bins * 2 && quality < 100;
    if entropy_combine {
        HistogramAnalyzeEntropyBin(out, low_effort);
        HistogramCombineEntropyBin(
            out,
            tmp,
            bins,
            GetCombineCostFactor(size, quality),
            low_effort,
        );
    }
    if !low_effort || !entropy_combine {
        let threshold = 1 + DivRound(
            i64::from(quality).pow(3) * (MAX_HISTO_GREEDY as i64 - 1),
            1_000_000,
        ) as usize;
        if HistogramCombineStochastic(out, threshold)? {
            HistogramCombineGreedy(out)?;
        }
    }
    HistogramRemap(&orig, out, symbols);
    Ok(())
}
