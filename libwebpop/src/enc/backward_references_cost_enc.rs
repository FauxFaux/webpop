// Copyright 2017 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/backward_references_cost_enc.c.
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

use super::{backward_references_enc::*, histogram_enc::*};
use crate::dsp::lossless_common::*;
use crate::utils::color_cache_utils::*;
use crate::webp::format_constants::*;
use std::collections::TryReserveError;

#[derive(Debug)]
pub struct CostModel {
    pub alpha: [u32; 256],
    pub red: [u32; 256],
    pub blue: [u32; 256],
    pub distance: [u32; NUM_DISTANCE_CODES],
    pub literal: Vec<u32>,
}

pub fn ConvertPopulationCountTableToBitEstimates(counts: &[u32], output: &mut [u32]) {
    assert_eq!(counts.len(), output.len());
    let sum = counts.iter().fold(0u32, |sum, &v| sum.wrapping_add(v));
    if counts.iter().filter(|&&v| v > 0).count() <= 1 {
        output.fill(0);
    } else {
        let logsum = VP8LFastLog2(sum);
        for (&count, cost) in counts.iter().zip(output) {
            *cost = logsum.wrapping_sub(VP8LFastLog2(count));
        }
    }
}

pub fn CostModelBuild(
    xsize: u32,
    cache_bits: u32,
    refs: &VP8LBackwardRefs,
) -> Result<CostModel, TryReserveError> {
    let mut histo = VP8LAllocateHistogram(cache_bits)?;
    VP8LHistogramStoreRefs(refs, Some(VP8LDistanceToPlaneCode), xsize, &mut histo);
    let mut literal = Vec::new();
    literal.try_reserve_exact(histo.literal.len())?;
    literal.resize(histo.literal.len(), 0);
    let mut m = CostModel {
        alpha: [0; 256],
        red: [0; 256],
        blue: [0; 256],
        distance: [0; NUM_DISTANCE_CODES],
        literal,
    };
    ConvertPopulationCountTableToBitEstimates(&histo.literal, &mut m.literal);
    ConvertPopulationCountTableToBitEstimates(&histo.red, &mut m.red);
    ConvertPopulationCountTableToBitEstimates(&histo.blue, &mut m.blue);
    ConvertPopulationCountTableToBitEstimates(&histo.alpha, &mut m.alpha);
    ConvertPopulationCountTableToBitEstimates(&histo.distance, &mut m.distance);
    Ok(m)
}

pub fn GetLiteralCost(m: &CostModel, v: u32) -> i64 {
    i64::from(m.alpha[(v >> 24) as usize])
        + i64::from(m.red[((v >> 16) & 255) as usize])
        + i64::from(m.literal[((v >> 8) & 255) as usize])
        + i64::from(m.blue[(v & 255) as usize])
}
pub fn GetCacheCost(m: &CostModel, idx: u32) -> i64 {
    i64::from(m.literal[NUM_LITERAL_CODES + NUM_LENGTH_CODES + idx as usize])
}
pub fn GetLengthCost(m: &CostModel, length: u32) -> i64 {
    let (code, bits) = VP8LPrefixEncodeBits(length);
    i64::from(m.literal[NUM_LITERAL_CODES + code as usize])
        + (i64::from(bits) << LOG_2_PRECISION_BITS)
}
pub fn GetDistanceCost(m: &CostModel, distance: u32) -> i64 {
    let (code, bits) = VP8LPrefixEncodeBits(distance);
    i64::from(m.distance[code as usize]) + (i64::from(bits) << LOG_2_PRECISION_BITS)
}

const COST_CACHE_INTERVAL_SIZE_MAX: usize = 500;
#[derive(Clone, Copy, Default, Debug)]
struct CostInterval {
    cost: i64,
    start: usize,
    end: usize,
    index: usize,
    previous: Option<usize>,
    next: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
struct CostCacheInterval {
    cost: i64,
    start: usize,
    end: usize,
}

/// Stable indices replace C's intrusive pointers. The fixed slot pool retains
/// the upstream 500-interval bound and immediate-write fallback, including its
/// strict cost comparison and insertion order. Allocation/free-list layout has
/// no algorithmic meaning.
struct CostManager {
    head: Option<usize>,
    count: usize,
    intervals: Vec<CostInterval>,
    free: Vec<usize>,
    cache_intervals: Vec<CostCacheInterval>,
    cost_cache: Vec<i64>,
    costs: Vec<i64>,
    dist_array: Vec<u16>,
}

impl CostManager {
    fn CostManagerInit(size: usize, model: &CostModel) -> Result<Self, TryReserveError> {
        let mut cost_cache = Vec::new();
        cost_cache.try_reserve_exact(size.min(MAX_LENGTH as usize))?;
        for i in 0..size.min(MAX_LENGTH as usize) {
            cost_cache.push(GetLengthCost(model, i as u32));
        }
        let mut cache_intervals: Vec<CostCacheInterval> = Vec::new();
        cache_intervals.try_reserve_exact(cost_cache.len())?;
        for (i, &cost) in cost_cache.iter().enumerate() {
            if let Some(cur) = cache_intervals.last_mut().filter(|v| v.cost == cost) {
                cur.end = i + 1;
            } else {
                cache_intervals.push(CostCacheInterval {
                    cost,
                    start: i,
                    end: i + 1,
                });
            }
        }
        let mut costs = Vec::new();
        costs.try_reserve_exact(size)?;
        costs.resize(size, i64::MAX);
        let mut dist_array = Vec::new();
        dist_array.try_reserve_exact(size)?;
        dist_array.resize(size, 0);
        let mut intervals = Vec::new();
        intervals.try_reserve_exact(COST_CACHE_INTERVAL_SIZE_MAX)?;
        intervals.resize(COST_CACHE_INTERVAL_SIZE_MAX, CostInterval::default());
        let mut free = Vec::new();
        free.try_reserve_exact(COST_CACHE_INTERVAL_SIZE_MAX)?;
        free.extend(0..COST_CACHE_INTERVAL_SIZE_MAX);
        Ok(Self {
            head: None,
            count: 0,
            intervals,
            free,
            cache_intervals,
            cost_cache,
            costs,
            dist_array,
        })
    }

    fn UpdateCost(&mut self, i: usize, position: usize, cost: i64) {
        let k = i - position;
        assert!(k < MAX_LENGTH as usize);
        if self.costs[i] > cost {
            self.costs[i] = cost;
            self.dist_array[i] = (k + 1) as u16;
        }
    }
    fn UpdateCostPerInterval(&mut self, start: usize, end: usize, position: usize, cost: i64) {
        for i in start..end {
            self.UpdateCost(i, position, cost);
        }
    }
    fn ConnectIntervals(&mut self, prev: Option<usize>, next: Option<usize>) {
        if let Some(p) = prev {
            self.intervals[p].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.intervals[n].previous = prev;
        }
    }
    fn PopInterval(&mut self, i: usize) {
        let v = self.intervals[i];
        self.ConnectIntervals(v.previous, v.next);
        self.free.push(i);
        self.count -= 1;
    }
    fn UpdateCostAtIndex(&mut self, i: usize, clean: bool) {
        let mut cur = self.head;
        while let Some(idx) = cur {
            let v = self.intervals[idx];
            if v.start > i {
                break;
            }
            if v.end <= i {
                if clean {
                    self.PopInterval(idx);
                }
            } else {
                self.UpdateCost(i, v.index, v.cost);
            }
            cur = v.next;
        }
    }
    fn PositionOrphanInterval(&mut self, cur: usize, mut prev: Option<usize>) {
        if prev.is_none() {
            prev = self.head;
        }
        while let Some(p) = prev {
            if self.intervals[cur].start >= self.intervals[p].start {
                break;
            }
            prev = self.intervals[p].previous;
        }
        while let Some(p) = prev {
            let Some(next) = self.intervals[p].next else {
                break;
            };
            if self.intervals[next].start >= self.intervals[cur].start {
                break;
            }
            prev = Some(next);
        }
        let next = prev.map_or(self.head, |p| self.intervals[p].next);
        self.ConnectIntervals(Some(cur), next);
        self.ConnectIntervals(prev, Some(cur));
    }
    fn InsertInterval(
        &mut self,
        hint: Option<usize>,
        cost: i64,
        position: usize,
        start: usize,
        end: usize,
    ) {
        if start >= end {
            return;
        }
        if self.count >= COST_CACHE_INTERVAL_SIZE_MAX {
            self.UpdateCostPerInterval(start, end, position, cost);
            return;
        }
        let idx = self.free.pop().unwrap();
        self.intervals[idx] = CostInterval {
            cost,
            start,
            end,
            index: position,
            previous: None,
            next: None,
        };
        self.PositionOrphanInterval(idx, hint);
        self.count += 1;
    }
    fn PushInterval(&mut self, distance_cost: i64, position: usize, len: usize) {
        if len < 10 {
            for j in position..position + len {
                self.UpdateCost(j, position, distance_cost + self.cost_cache[j - position]);
            }
            return;
        }
        let mut interval = self.head;
        for i in 0..self.cache_intervals.len() {
            let cached = self.cache_intervals[i];
            if cached.start >= len {
                break;
            }
            let mut start = position + cached.start;
            let end = position + cached.end.min(len);
            let cost = distance_cost + cached.cost;
            while let Some(idx) = interval {
                let v = self.intervals[idx];
                if v.start >= end {
                    break;
                }
                let next = v.next;
                if start >= v.end {
                    interval = next;
                    continue;
                }
                if cost >= v.cost {
                    self.InsertInterval(Some(idx), cost, position, start, v.start);
                    start = v.end;
                    if start >= end {
                        break;
                    }
                    interval = next;
                    continue;
                }
                if start <= v.start {
                    if v.end <= end {
                        self.PopInterval(idx);
                    } else {
                        self.intervals[idx].start = end;
                        break;
                    }
                } else if end < v.end {
                    self.intervals[idx].end = start;
                    self.InsertInterval(Some(idx), v.cost, v.index, end, v.end);
                    interval = self.intervals[idx].next;
                    break;
                } else {
                    self.intervals[idx].end = start;
                }
                interval = next;
            }
            self.InsertInterval(interval, cost, position, start, end);
        }
    }
}

fn AddSingleLiteralWithCostModel(
    argb: &[u32],
    cache: &mut Option<VP8LColorCache>,
    m: &CostModel,
    idx: usize,
    prev: i64,
    manager: &mut CostManager,
) {
    let color = argb[idx];
    let key = cache
        .as_ref()
        .and_then(|c| VP8LColorCacheContains(c, color));
    let cost = prev
        + if let Some(key) = key {
            DivRound(GetCacheCost(m, key) * 68, 100)
        } else {
            if let Some(c) = cache {
                VP8LColorCacheInsert(c, color);
            }
            DivRound(GetLiteralCost(m, color) * 82, 100)
        };
    if manager.costs[idx] > cost {
        manager.costs[idx] = cost;
        manager.dist_array[idx] = 1;
    }
}

pub fn BackwardReferencesHashChainDistanceOnly(
    xsize: usize,
    ysize: usize,
    argb: &[u32],
    cache_bits: u32,
    hash_chain: &VP8LHashChain,
    refs: &VP8LBackwardRefs,
) -> Result<Vec<u16>, TryReserveError> {
    let size = xsize.checked_mul(ysize).expect("image geometry");
    assert!(size > 0 && argb.len() >= size && cache_bits <= MAX_COLOR_CACHE_BITS);
    let m = CostModelBuild(xsize as u32, cache_bits, refs)?;
    let mut manager = CostManager::CostManagerInit(size, &m)?;
    let mut cache = if cache_bits > 0 {
        Some(VP8LColorCacheTryInit(cache_bits)?)
    } else {
        None
    };
    let mut offset_prev = None;
    let mut len_prev = 0;
    let mut offset_cost = -1;
    let mut first_offset_is_constant = false;
    let mut reach = 0;
    AddSingleLiteralWithCostModel(argb, &mut cache, &m, 0, 0, &mut manager);
    for i in 1..size {
        let prev = manager.costs[i - 1];
        let (offset, len) = VP8LHashChainFindCopy(hash_chain, i);
        AddSingleLiteralWithCostModel(argb, &mut cache, &m, i, prev, &mut manager);
        if len >= 2 {
            if Some(offset) != offset_prev {
                let code = VP8LDistanceToPlaneCode(xsize as u32, offset);
                offset_cost = GetDistanceCost(&m, code);
                first_offset_is_constant = true;
                manager.PushInterval(prev + offset_cost, i, len);
            } else {
                assert!(offset_cost >= 0);
                if first_offset_is_constant {
                    reach = i - 1 + len_prev - 1;
                    first_offset_is_constant = false;
                }
                if i + len - 1 > reach {
                    assert!(len == MAX_LENGTH as usize || len == size - i);
                    let mut j = i;
                    let mut len_j = 0;
                    while j <= reach {
                        let (offset_j, next_len) = VP8LHashChainFindCopy(hash_chain, j + 1);
                        len_j = next_len;
                        if offset_j != offset {
                            len_j = VP8LHashChainFindLength(hash_chain, j);
                            break;
                        }
                        j += 1;
                    }
                    manager.UpdateCostAtIndex(j - 1, false);
                    manager.UpdateCostAtIndex(j, false);
                    manager.PushInterval(manager.costs[j - 1] + offset_cost, j, len_j);
                    reach = j + len_j - 1;
                }
            }
        }
        manager.UpdateCostAtIndex(i, true);
        offset_prev = Some(offset);
        len_prev = len;
    }
    Ok(manager.dist_array)
}

/// Pack the selected path at the end of dist_array, returning its first index.
pub fn TraceBackwards(dist_array: &mut [u16]) -> usize {
    let mut path = dist_array.len();
    let mut cur = dist_array.len();
    while cur > 0 {
        let k = dist_array[cur - 1] as usize;
        assert!(k > 0 && k <= cur);
        path -= 1;
        dist_array[path] = k as u16;
        cur -= k;
    }
    path
}

pub fn BackwardReferencesHashChainFollowChosenPath(
    argb: &[u32],
    cache_bits: u32,
    chosen_path: &[u16],
    hash_chain: &VP8LHashChain,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    assert!(cache_bits <= MAX_COLOR_CACHE_BITS);
    let mut cache = if cache_bits > 0 {
        Some(VP8LColorCacheTryInit(cache_bits)?)
    } else {
        None
    };
    VP8LClearBackwardRefs(refs);
    let mut i = 0;
    for &len in chosen_path {
        let v = if len != 1 {
            assert!(len > 1);
            let offset = VP8LHashChainFindOffset(hash_chain, i);
            if let Some(c) = &mut cache {
                for &pixel in &argb[i..i + len as usize] {
                    VP8LColorCacheInsert(c, pixel);
                }
            }
            PixOrCopyCreateCopy(offset, len)
        } else {
            let key = cache
                .as_ref()
                .and_then(|c| VP8LColorCacheContains(c, argb[i]));
            if let Some(key) = key {
                PixOrCopyCreateCacheIdx(key)
            } else {
                if let Some(c) = &mut cache {
                    VP8LColorCacheInsert(c, argb[i]);
                }
                PixOrCopyCreateLiteral(argb[i])
            }
        };
        VP8LBackwardRefsCursorAdd(refs, v)?;
        i += len as usize;
    }
    Ok(())
}

pub fn VP8LBackwardReferencesTraceBackwards(
    xsize: usize,
    ysize: usize,
    argb: &[u32],
    cache_bits: u32,
    hash_chain: &VP8LHashChain,
    refs_src: &VP8LBackwardRefs,
    refs_dst: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    let mut dist = BackwardReferencesHashChainDistanceOnly(
        xsize, ysize, argb, cache_bits, hash_chain, refs_src,
    )?;
    let start = TraceBackwards(&mut dist);
    BackwardReferencesHashChainFollowChosenPath(
        argb,
        cache_bits,
        &dist[start..],
        hash_chain,
        refs_dst,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> CostModel {
        let mut literal = vec![0; NUM_LITERAL_CODES + NUM_LENGTH_CODES];
        for (i, v) in literal.iter_mut().enumerate() {
            *v = ((i * 71 + 13) % 23) as u32 * 123456;
        }
        CostModel {
            alpha: [0; 256],
            red: [0; 256],
            blue: [0; 256],
            distance: [0; NUM_DISTANCE_CODES],
            literal,
        }
    }

    // Independent eager minimum-cost expansion checks split, trim, remove,
    // outdated-interval cleanup, short-interval writes and the 500-slot fallback.
    fn check_intervals(disjoint: bool) {
        let size = 18000;
        let mut manager = CostManager::CostManagerInit(size, &model()).unwrap();
        let mut expected = vec![i64::MAX; size];
        let mut candidates = Vec::new();
        let mut seed = 0x6d2b79f5u32;
        let mut saw_full = false;
        for trial in 0..1500 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let position = if disjoint {
                trial * 11
            } else {
                seed as usize % 4000
            };
            let len = if disjoint {
                10
            } else {
                1 + (seed >> 8) as usize % 4095
            };
            let distance_cost = if disjoint {
                trial as i64 * 100000
            } else {
                i64::from(seed % 100000000)
            };
            // For saturation, use disjoint intervals with varying
            // cost; already pending intervals remain live until final inspection.
            manager.PushInterval(distance_cost, position, len);
            saw_full |= manager.count == COST_CACHE_INTERVAL_SIZE_MAX;
            candidates.push((position, len, distance_cost));
            for i in position..position + len {
                expected[i] = expected[i].min(distance_cost + manager.cost_cache[i - position]);
            }
            if !disjoint && trial % 17 == 0 {
                manager.UpdateCostAtIndex(0, true);
            }
            let mut prev_end = 0;
            let mut cur = manager.head;
            let mut count = 0;
            while let Some(idx) = cur {
                let v = manager.intervals[idx];
                assert!(v.start < v.end && v.start >= prev_end);
                prev_end = v.end;
                cur = v.next;
                count += 1;
            }
            assert_eq!(count, manager.count);
        }
        if disjoint {
            assert!(saw_full, "must exercise bounded-interval fallback");
        }
        for i in 0..size {
            manager.UpdateCostAtIndex(i, true);
            assert_eq!(manager.costs[i], expected[i], "pixel {i}");
            if expected[i] != i64::MAX {
                let length = usize::from(manager.dist_array[i]);
                assert!(length > 0);
                let position = i + 1 - length;
                assert!(candidates.iter().any(|&(p, len, cost)| p == position
                    && length <= len
                    && cost + manager.cost_cache[length - 1] == expected[i]));
            }
        }
        assert_eq!(manager.count, 0);
    }

    #[test]
    fn cost_intervals_match_eager_expansion() {
        check_intervals(false);
    }
    #[test]
    fn cost_intervals_at_capacity_match_eager_expansion() {
        check_intervals(true);
    }

    #[test]
    fn path_packs_at_end_without_losing_unread_lengths() {
        let mut dist = [1, 9, 2, 9, 9, 3, 9, 2];
        let start = TraceBackwards(&mut dist);
        assert_eq!(&dist[start..], &[1, 2, 3, 2]);
    }
}
