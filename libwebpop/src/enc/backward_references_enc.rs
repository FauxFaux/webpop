// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/backward_references_enc.{c,h}.
// Use of this source code is governed by the BSD license in upstream/COPYING.
// See upstream/PATENTS for the additional intellectual property rights grant.

use std::collections::TryReserveError;

pub const MAX_COLOR_CACHE_BITS: u32 = 10;
pub const MAX_REFS_BLOCK_PER_IMAGE: usize = 16;
const MIN_BLOCK_SIZE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Literal,
    CacheIdx,
    Copy,
}

/// Logical token. No C padding or pointer ownership is reproduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixOrCopy {
    mode: Mode,
    len: u16,
    argb_or_distance: u32,
}

#[inline]
pub fn PixOrCopyCreateLiteral(argb: u32) -> PixOrCopy {
    PixOrCopy {
        mode: Mode::Literal,
        len: 1,
        argb_or_distance: argb,
    }
}

#[inline]
pub fn PixOrCopyCreateCacheIdx(idx: u32) -> PixOrCopy {
    assert!(idx < (1 << MAX_COLOR_CACHE_BITS));
    PixOrCopy {
        mode: Mode::CacheIdx,
        len: 1,
        argb_or_distance: idx,
    }
}

#[inline]
pub fn PixOrCopyCreateCopy(distance: u32, len: u16) -> PixOrCopy {
    PixOrCopy {
        mode: Mode::Copy,
        len,
        argb_or_distance: distance,
    }
}

#[inline]
pub fn PixOrCopyIsLiteral(p: &PixOrCopy) -> bool {
    p.mode == Mode::Literal
}
#[inline]
pub fn PixOrCopyIsCacheIdx(p: &PixOrCopy) -> bool {
    p.mode == Mode::CacheIdx
}
#[inline]
pub fn PixOrCopyIsCopy(p: &PixOrCopy) -> bool {
    p.mode == Mode::Copy
}

/// Component order is blue, green, red, alpha, independent of host endianness.
#[inline]
pub fn PixOrCopyLiteral(p: &PixOrCopy, component: u32) -> u32 {
    assert!(PixOrCopyIsLiteral(p) && component < 4);
    (p.argb_or_distance >> (component * 8)) & 255
}

#[inline]
pub fn PixOrCopyLength(p: &PixOrCopy) -> u32 {
    u32::from(p.len)
}
#[inline]
pub fn PixOrCopyCacheIdx(p: &PixOrCopy) -> u32 {
    assert!(PixOrCopyIsCacheIdx(p));
    p.argb_or_distance
}
#[inline]
pub fn PixOrCopyDistance(p: &PixOrCopy) -> u32 {
    assert!(PixOrCopyIsCopy(p));
    p.argb_or_distance
}

/// Owned reference sequence. A contiguous vector replaces the linked blocks;
/// growth uses the upstream minimum block size, and clearing retains storage.
/// Result replaces the C allocation-error flag. Cursors borrow the sequence,
/// preventing invalidation by appending, clearing, cloning, or swapping.
#[derive(Debug)]
pub struct VP8LBackwardRefs {
    block_size: usize,
    tokens: Vec<PixOrCopy>,
}

impl VP8LBackwardRefs {
    pub fn iter(&self) -> std::slice::Iter<'_, PixOrCopy> {
        self.tokens.iter()
    }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, PixOrCopy> {
        self.tokens.iter_mut()
    }
    pub fn len(&self) -> usize {
        self.tokens.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

pub fn VP8LBackwardRefsInit(block_size: usize) -> VP8LBackwardRefs {
    VP8LBackwardRefs {
        block_size: block_size.max(MIN_BLOCK_SIZE),
        tokens: Vec::new(),
    }
}

/// Recycle storage between trials (upstream VP8LClearBackwardRefs).
pub fn VP8LClearBackwardRefs(refs: &mut VP8LBackwardRefs) {
    refs.tokens.clear();
}

/// Release storage, preserving the configured block size.
pub fn VP8LBackwardRefsClear(refs: &mut VP8LBackwardRefs) {
    refs.tokens = Vec::new();
}

pub fn BackwardRefsSwap(refs1: &mut VP8LBackwardRefs, refs2: &mut VP8LBackwardRefs) {
    std::mem::swap(refs1, refs2);
}

pub fn BackwardRefsClone(
    from: &VP8LBackwardRefs,
    to: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    VP8LClearBackwardRefs(to);
    to.tokens.try_reserve_exact(from.tokens.len())?;
    to.tokens.extend_from_slice(&from.tokens);
    Ok(())
}

pub fn VP8LBackwardRefsCursorAdd(
    refs: &mut VP8LBackwardRefs,
    v: PixOrCopy,
) -> Result<(), TryReserveError> {
    if refs.tokens.len() == refs.tokens.capacity() {
        refs.tokens.try_reserve(refs.block_size)?;
    }
    refs.tokens.push(v);
    Ok(())
}

pub struct VP8LRefsCursor<'a> {
    pub cur_pos: Option<&'a PixOrCopy>,
    remaining: std::slice::Iter<'a, PixOrCopy>,
}

pub fn VP8LRefsCursorInit(refs: &VP8LBackwardRefs) -> VP8LRefsCursor<'_> {
    let mut remaining = refs.tokens.iter();
    VP8LRefsCursor {
        cur_pos: remaining.next(),
        remaining,
    }
}
#[inline]
pub fn VP8LRefsCursorOk(c: &VP8LRefsCursor<'_>) -> bool {
    c.cur_pos.is_some()
}
#[inline]
pub fn VP8LRefsCursorNext(c: &mut VP8LRefsCursor<'_>) {
    assert!(VP8LRefsCursorOk(c));
    c.cur_pos = c.remaining.next();
}

#[allow(non_upper_case_globals)]
const plane_to_code_lut: [u8; 128] = [
    96, 73, 55, 39, 23, 13, 5, 1, 255, 255, 255, 255, 255, 255, 255, 255, 101, 78, 58, 42, 26, 16,
    8, 2, 0, 3, 9, 17, 27, 43, 59, 79, 102, 86, 62, 46, 32, 20, 10, 6, 4, 7, 11, 21, 33, 47, 63,
    87, 105, 90, 70, 52, 37, 28, 18, 14, 12, 15, 19, 29, 38, 53, 71, 91, 110, 99, 82, 66, 48, 35,
    30, 24, 22, 25, 31, 36, 49, 67, 83, 100, 115, 108, 94, 76, 64, 50, 44, 40, 34, 41, 45, 51, 65,
    77, 95, 109, 118, 113, 103, 92, 80, 68, 60, 56, 54, 57, 61, 69, 81, 93, 104, 114, 119, 116,
    111, 106, 97, 88, 84, 74, 72, 75, 85, 89, 98, 107, 112, 117,
];

pub fn VP8LDistanceToPlaneCode(xsize: u32, dist: u32) -> u32 {
    assert!(xsize > 0 && xsize <= 16383 && dist > 0 && dist <= i32::MAX as u32 - 120);
    let yoffset = dist / xsize;
    let xoffset = dist - yoffset * xsize;
    if xoffset <= 8 && yoffset < 8 {
        u32::from(plane_to_code_lut[(yoffset * 16 + 8 - xoffset) as usize]) + 1
    } else if i64::from(xoffset) > i64::from(xsize) - 8 && yoffset < 7 {
        u32::from(plane_to_code_lut[((yoffset + 1) * 16 + 8 + (xsize - xoffset)) as usize]) + 1
    } else {
        dist + 120
    }
}

/// Convert raw copy distances exactly once, after matching is complete.
pub fn BackwardReferences2DLocality(xsize: u32, refs: &mut VP8LBackwardRefs) {
    for v in refs.iter_mut() {
        if PixOrCopyIsCopy(v) {
            v.argb_or_distance = VP8LDistanceToPlaneCode(xsize, v.argb_or_distance);
        }
    }
}

use crate::utils::color_cache_utils::*;
use crate::webp::format_constants::{LZ77_WINDOW_SIZE, MAX_LENGTH, MAX_LENGTH_BITS};

pub const HASH_BITS: u32 = 18;
pub const HASH_SIZE: usize = 1 << HASH_BITS;
const MIN_LENGTH: usize = 4;

/// Packed match offsets and lengths, as in upstream VP8LHashChain. While filling,
/// the same storage holds signed predecessor indices (u32::MAX means -1).
#[derive(Debug)]
pub struct VP8LHashChain {
    pub offset_length: Vec<u32>,
}

pub fn VP8LHashChainInit(size: usize) -> Result<VP8LHashChain, TryReserveError> {
    assert!(size > 0 && size <= i32::MAX as usize);
    let mut offset_length = Vec::new();
    offset_length.try_reserve_exact(size)?;
    offset_length.resize(size, 0);
    Ok(VP8LHashChain { offset_length })
}

pub fn VP8LHashChainClear(p: &mut VP8LHashChain) {
    p.offset_length = Vec::new();
}

#[inline]
pub fn VP8LHashChainFindOffset(p: &VP8LHashChain, pos: usize) -> u32 {
    p.offset_length[pos] >> MAX_LENGTH_BITS
}

#[inline]
pub fn VP8LHashChainFindLength(p: &VP8LHashChain, pos: usize) -> usize {
    (p.offset_length[pos] & ((1 << MAX_LENGTH_BITS) - 1)) as usize
}

#[inline]
pub fn VP8LHashChainFindCopy(p: &VP8LHashChain, pos: usize) -> (u32, usize) {
    (
        VP8LHashChainFindOffset(p, pos),
        VP8LHashChainFindLength(p, pos),
    )
}

#[inline]
fn GetPixPairHash64(argb: &[u32]) -> usize {
    (argb[1]
        .wrapping_mul(0xc6a4a793)
        .wrapping_add(argb[0].wrapping_mul(0x5bd1e996))
        >> (32 - HASH_BITS)) as usize
}

fn GetMaxItersForQuality(quality: u32) -> i32 {
    (8 + quality * quality / 128) as i32
}

fn GetWindowSizeForHashChain(quality: u32, xsize: usize) -> usize {
    let window = if quality > 75 {
        LZ77_WINDOW_SIZE as usize
    } else {
        xsize
            << if quality > 50 {
                8
            } else if quality > 25 {
                6
            } else {
                4
            }
    };
    window.min(LZ77_WINDOW_SIZE as usize)
}

#[inline]
fn MaxFindCopyLength(len: usize) -> usize {
    len.min(MAX_LENGTH as usize)
}

#[inline]
fn FindMatchLength(a: &[u32], b: &[u32], best: usize, limit: usize) -> usize {
    if a[best] != b[best] {
        return 0;
    }
    crate::dsp::lossless_enc::VectorMismatch_C(&a[..limit], &b[..limit])
}

/// Upstream VP8LHashChainFill, without the unused progress callback framework.
/// Pixels are a prepared, tightly packed image; dimensions are internal invariants.
pub fn VP8LHashChainFill(
    p: &mut VP8LHashChain,
    quality: u32,
    argb: &[u32],
    xsize: usize,
    ysize: usize,
    low_effort: bool,
) -> Result<(), TryReserveError> {
    assert!(quality <= 100 && xsize > 0 && ysize > 0);
    let size = xsize.checked_mul(ysize).expect("image geometry");
    assert!(size <= i32::MAX as usize && argb.len() >= size && p.offset_length.len() >= size);
    let iter_max = GetMaxItersForQuality(quality);
    let window = GetWindowSizeForHashChain(quality, xsize);
    if size <= 2 {
        p.offset_length[0] = 0;
        p.offset_length[size - 1] = 0;
        return Ok(());
    }
    let mut first = Vec::new();
    first.try_reserve_exact(HASH_SIZE)?;
    first.resize(HASH_SIZE, u32::MAX);
    let chain = &mut p.offset_length;
    let mut comp = argb[0] == argb[1];
    let mut pos = 0;
    while pos < size - 2 {
        let comp_next = argb[pos + 1] == argb[pos + 2];
        if comp && comp_next {
            let pixel = argb[pos];
            let mut len = 1;
            while pos + len + 2 < size && argb[pos + len + 2] == pixel {
                len += 1;
            }
            if len > MAX_LENGTH as usize {
                let skip = len - MAX_LENGTH as usize;
                chain[pos..pos + skip].fill(u32::MAX);
                pos += skip;
                len = MAX_LENGTH as usize;
            }
            while len > 0 {
                let hash = GetPixPairHash64(&[pixel, len as u32]);
                len -= 1;
                chain[pos] = first[hash];
                first[hash] = pos as u32;
                pos += 1;
            }
            comp = false;
        } else {
            let hash = GetPixPairHash64(&argb[pos..]);
            chain[pos] = first[hash];
            first[hash] = pos as u32;
            pos += 1;
            comp = comp_next;
        }
    }
    chain[pos] = first[GetPixPairHash64(&argb[pos..])];
    drop(first);
    chain[0] = 0;
    chain[size - 1] = 0;
    let mut base = size - 2;
    while base > 0 {
        let max_len = MaxFindCopyLength(size - 1 - base);
        let mut iter = iter_max;
        let mut best_len = 0;
        let mut best_dist = 0;
        let min_pos = base.saturating_sub(window) as i32;
        let length_max = max_len.min(256);
        let mut candidate = chain[base] as i32;
        if !low_effort {
            if base >= xsize {
                let len = FindMatchLength(&argb[base - xsize..], &argb[base..], best_len, max_len);
                if len > best_len {
                    best_len = len;
                    best_dist = xsize;
                }
                iter -= 1;
            }
            let len = FindMatchLength(&argb[base - 1..], &argb[base..], best_len, max_len);
            if len > best_len {
                best_len = len;
                best_dist = 1;
            }
            iter -= 1;
            if best_len == MAX_LENGTH as usize {
                candidate = min_pos - 1;
            }
        }
        let mut best_argb = argb[base + best_len];
        while candidate >= min_pos {
            iter -= 1;
            if iter == 0 {
                break;
            }
            let c = candidate as usize;
            if argb[c + best_len] == best_argb {
                let len = crate::dsp::lossless_enc::VectorMismatch_C(
                    &argb[c..c + max_len],
                    &argb[base..base + max_len],
                );
                if best_len < len {
                    best_len = len;
                    best_dist = base - c;
                    best_argb = argb[base + best_len];
                    if best_len >= length_max {
                        break;
                    }
                }
            }
            candidate = chain[c] as i32;
        }
        let mut max_base = base;
        loop {
            chain[base] = ((best_dist as u32) << MAX_LENGTH_BITS) | best_len as u32;
            base -= 1;
            if best_dist == 0 || base == 0 {
                break;
            }
            if base < best_dist || argb[base - best_dist] != argb[base] {
                break;
            }
            if best_len == MAX_LENGTH as usize
                && best_dist != 1
                && base + (MAX_LENGTH as usize) < max_base
            {
                break;
            }
            if best_len < MAX_LENGTH as usize {
                best_len += 1;
                max_base = base;
            }
        }
    }
    Ok(())
}

#[inline]
fn AddSingleLiteral(
    pixel: u32,
    cache: &mut Option<VP8LColorCache>,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    let v = if let Some(cc) = cache {
        let key = VP8LColorCacheGetIndex(cc, pixel);
        if VP8LColorCacheLookup(cc, key) == pixel {
            PixOrCopyCreateCacheIdx(key)
        } else {
            VP8LColorCacheSet(cc, key, pixel);
            PixOrCopyCreateLiteral(pixel)
        }
    } else {
        PixOrCopyCreateLiteral(pixel)
    };
    VP8LBackwardRefsCursorAdd(refs, v)
}

fn NewCache(bits: u32) -> Result<Option<VP8LColorCache>, TryReserveError> {
    assert!(bits <= MAX_COLOR_CACHE_BITS);
    if bits == 0 {
        Ok(None)
    } else {
        VP8LColorCacheTryInit(bits).map(Some)
    }
}

pub fn BackwardReferencesRle(
    xsize: usize,
    ysize: usize,
    argb: &[u32],
    cache_bits: u32,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    let size = xsize.checked_mul(ysize).expect("image geometry");
    assert!(xsize > 0 && size > 0 && argb.len() >= size);
    let mut cache = NewCache(cache_bits)?;
    VP8LClearBackwardRefs(refs);
    AddSingleLiteral(argb[0], &mut cache, refs)?;
    let mut i = 1;
    while i < size {
        let max_len = MaxFindCopyLength(size - i);
        let rle = FindMatchLength(&argb[i..], &argb[i - 1..], 0, max_len);
        let row = if i < xsize {
            0
        } else {
            FindMatchLength(&argb[i..], &argb[i - xsize..], 0, max_len)
        };
        if rle >= row && rle >= MIN_LENGTH {
            VP8LBackwardRefsCursorAdd(refs, PixOrCopyCreateCopy(1, rle as u16))?;
            i += rle;
        } else if row >= MIN_LENGTH {
            VP8LBackwardRefsCursorAdd(refs, PixOrCopyCreateCopy(xsize as u32, row as u16))?;
            if let Some(cc) = &mut cache {
                for &pixel in &argb[i..i + row] {
                    VP8LColorCacheInsert(cc, pixel);
                }
            }
            i += row;
        } else {
            AddSingleLiteral(argb[i], &mut cache, refs)?;
            i += 1;
        }
    }
    Ok(())
}

pub fn BackwardReferencesLz77(
    xsize: usize,
    ysize: usize,
    argb: &[u32],
    cache_bits: u32,
    hash_chain: &VP8LHashChain,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    let size = xsize.checked_mul(ysize).expect("image geometry");
    assert!(xsize > 0 && size > 0 && argb.len() >= size && hash_chain.offset_length.len() >= size);
    let mut cache = NewCache(cache_bits)?;
    VP8LClearBackwardRefs(refs);
    let mut i = 0;
    let mut i_last_check = 0; // max(i, -1) is always i at the first iteration.
    while i < size {
        let (offset, mut len) = VP8LHashChainFindCopy(hash_chain, i);
        if len >= MIN_LENGTH {
            let j_max = (i + len).min(size - 1);
            let mut max_reach = 0;
            i_last_check = i.max(i_last_check);
            for j in i_last_check + 1..=j_max {
                let len_j = VP8LHashChainFindLength(hash_chain, j);
                let reach = j + if len_j >= MIN_LENGTH { len_j } else { 1 };
                if reach > max_reach {
                    len = j - i;
                    max_reach = reach;
                    if max_reach >= size {
                        break;
                    }
                }
            }
        } else {
            len = 1;
        }
        assert!(len > 0);
        if len == 1 {
            AddSingleLiteral(argb[i], &mut cache, refs)?;
        } else {
            VP8LBackwardRefsCursorAdd(refs, PixOrCopyCreateCopy(offset, len as u16))?;
            if let Some(cc) = &mut cache {
                for &pixel in &argb[i..i + len] {
                    VP8LColorCacheInsert(cc, pixel);
                }
            }
        }
        i += len;
    }
    Ok(())
}

/// Restrict matches to the cheapest 32 spatial distance codes.
pub fn BackwardReferencesLz77Box(
    xsize: usize,
    ysize: usize,
    argb: &[u32],
    cache_bits: u32,
    hash_chain_best: &VP8LHashChain,
    hash_chain: &mut VP8LHashChain,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    let size = xsize.checked_mul(ysize).expect("image geometry");
    assert!(xsize > 0 && xsize <= 16383 && size > 0 && argb.len() >= size);
    assert!(hash_chain_best.offset_length.len() >= size && hash_chain.offset_length.len() >= size);
    let mut counts = Vec::new();
    counts.try_reserve_exact(size)?;
    counts.resize(size, 1u16);
    for i in (0..size - 1).rev() {
        if argb[i] == argb[i + 1] {
            counts[i] = counts[i + 1] + u16::from(counts[i + 1] != MAX_LENGTH as u16);
        }
    }
    let mut offsets = [0usize; 32];
    for y in 0..=6 {
        for x in -6..=6 {
            let offset = y * xsize as i32 + x;
            if offset <= 0 {
                continue;
            }
            let code = VP8LDistanceToPlaneCode(xsize as u32, offset as u32) as usize - 1;
            if code < 32 {
                offsets[code] = offset as usize;
            }
        }
    }
    let mut offset_size = 0;
    for i in 0..32 {
        if offsets[i] != 0 {
            offsets[offset_size] = offsets[i];
            offset_size += 1;
        }
    }
    let offsets = &offsets[..offset_size];
    let mut new_offsets = [0; 32];
    let mut new_size = 0;
    for &offset in offsets {
        if !offsets.iter().any(|&other| offset == other + 1) {
            new_offsets[new_size] = offset;
            new_size += 1;
        }
    }
    let mut best_length_prev = 0;
    let mut best_offset_prev = 0;
    hash_chain.offset_length[0] = 0;
    for i in 1..size {
        let mut best_length = VP8LHashChainFindLength(hash_chain_best, i);
        let mut best_offset = VP8LHashChainFindOffset(hash_chain_best, i) as usize;
        if best_length < MAX_LENGTH as usize || !offsets.contains(&best_offset) {
            let use_prev = best_length_prev > 1 && best_length_prev < MAX_LENGTH as usize;
            let candidates = if use_prev {
                &new_offsets[..new_size]
            } else {
                offsets
            };
            best_length = if use_prev { best_length_prev - 1 } else { 0 };
            best_offset = if use_prev { best_offset_prev } else { 0 };
            for &offset in candidates {
                if i < offset || argb[i - offset] != argb[i] {
                    continue;
                }
                let mut j = i;
                let mut j_offset = i - offset;
                let mut curr_length = 0;
                loop {
                    let a = counts[j_offset] as usize;
                    let b = counts[j] as usize;
                    if a != b {
                        curr_length += a.min(b);
                        break;
                    }
                    curr_length += a;
                    j_offset += a;
                    j += a;
                    if curr_length > MAX_LENGTH as usize || j >= size || argb[j_offset] != argb[j] {
                        break;
                    }
                }
                if best_length < curr_length {
                    best_offset = offset;
                    best_length = curr_length.min(MAX_LENGTH as usize);
                    if curr_length >= MAX_LENGTH as usize {
                        break;
                    }
                }
            }
        }
        assert!(i + best_length <= size);
        if best_length <= MIN_LENGTH {
            hash_chain.offset_length[i] = 0;
            best_offset_prev = 0;
            best_length_prev = 0;
        } else {
            hash_chain.offset_length[i] =
                ((best_offset as u32) << MAX_LENGTH_BITS) | best_length as u32;
            best_offset_prev = best_offset;
            best_length_prev = best_length;
        }
    }
    BackwardReferencesLz77(xsize, ysize, argb, cache_bits, hash_chain, refs)
}

/// Evaluate cache sizes on raw references generated without a cache. Distance
/// costs are omitted upstream because they are constant across candidates.
pub fn CalculateBestCacheSize(
    argb: &[u32],
    quality: u32,
    refs: &VP8LBackwardRefs,
    best_cache_bits: &mut u32,
) -> Result<(), TryReserveError> {
    use super::histogram_enc::{VP8LAllocateHistogram, VP8LHistogramEstimateBits};
    use crate::dsp::lossless_common::VP8LPrefixEncode;
    use crate::webp::format_constants::{NUM_LENGTH_CODES, NUM_LITERAL_CODES};
    assert!(*best_cache_bits <= MAX_COLOR_CACHE_BITS && quality <= 100);
    let max = if quality <= 25 { 0 } else { *best_cache_bits };
    if max == 0 {
        *best_cache_bits = 0;
        return Ok(());
    }
    // Fixed-size stack arrays keep allocation errors on the owned buffers fallible.
    let mut histos = std::array::from_fn::<_, 11, _>(|_| None);
    let mut caches = std::array::from_fn::<_, 11, _>(|_| None);
    for bits in 0..=max {
        histos[bits as usize] = Some(VP8LAllocateHistogram(bits)?);
        caches[bits as usize] = NewCache(bits)?;
    }
    let mut pos = 0;
    for v in refs.iter() {
        assert!(!PixOrCopyIsCacheIdx(v));
        if PixOrCopyIsLiteral(v) {
            let pixel = argb[pos];
            pos += 1;
            for bits in 0..=max as usize {
                let h = histos[bits].as_mut().unwrap();
                let key = caches[bits]
                    .as_ref()
                    .map(|c| VP8LColorCacheGetIndex(c, pixel));
                if let (Some(c), Some(key)) = (&mut caches[bits], key) {
                    if VP8LColorCacheLookup(c, key) == pixel {
                        h.literal[NUM_LITERAL_CODES + NUM_LENGTH_CODES + key as usize] += 1;
                        continue;
                    }
                    VP8LColorCacheSet(c, key, pixel);
                }
                h.blue[(pixel & 255) as usize] += 1;
                h.literal[((pixel >> 8) & 255) as usize] += 1;
                h.red[((pixel >> 16) & 255) as usize] += 1;
                h.alpha[(pixel >> 24) as usize] += 1;
            }
        } else {
            let len = PixOrCopyLength(v) as usize;
            let (code, _, _) = VP8LPrefixEncode(len as u32);
            for bits in 0..=max as usize {
                histos[bits].as_mut().unwrap().literal[NUM_LITERAL_CODES + code as usize] += 1;
            }
            let mut prev = !argb[pos];
            for &pixel in &argb[pos..pos + len] {
                if pixel != prev {
                    for c in caches[1..=max as usize].iter_mut().flatten() {
                        VP8LColorCacheInsert(c, pixel);
                    }
                    prev = pixel;
                }
            }
            pos += len;
        }
    }
    let mut min = u64::MAX;
    for bits in 0..=max {
        let entropy = VP8LHistogramEstimateBits(histos[bits as usize].as_ref().unwrap());
        if bits == 0 || entropy < min {
            min = entropy;
            *best_cache_bits = bits;
        }
    }
    Ok(())
}

pub fn BackwardRefsWithLocalCache(
    argb: &[u32],
    cache_bits: u32,
    refs: &mut VP8LBackwardRefs,
) -> Result<(), TryReserveError> {
    assert!(cache_bits > 0 && cache_bits <= MAX_COLOR_CACHE_BITS);
    let mut cache = VP8LColorCacheTryInit(cache_bits)?;
    let mut pos = 0;
    for v in refs.iter_mut() {
        if PixOrCopyIsLiteral(v) {
            let pixel = v.argb_or_distance;
            if let Some(key) = VP8LColorCacheContains(&cache, pixel) {
                *v = PixOrCopyCreateCacheIdx(key);
            } else {
                VP8LColorCacheInsert(&mut cache, pixel);
            }
            pos += 1;
        } else {
            assert!(PixOrCopyIsCopy(v));
            let end = pos + PixOrCopyLength(v) as usize;
            for &pixel in &argb[pos..end] {
                VP8LColorCacheInsert(&mut cache, pixel);
            }
            pos = end;
        }
    }
    Ok(())
}

#[allow(non_upper_case_globals)]
pub const kLZ77Standard: u32 = 1;
#[allow(non_upper_case_globals)]
pub const kLZ77RLE: u32 = 2;
#[allow(non_upper_case_globals)]
pub const kLZ77Box: u32 = 4;

/// Upstream trial selection. Returned sequences have spatially coded distances;
/// refs[1] is populated only for do_no_cache in the full-effort path. Progress
/// callbacks and C error flags are replaced by a synchronous Result.
#[allow(clippy::too_many_arguments, non_upper_case_globals)]
pub fn VP8LGetBackwardReferences(
    width: usize,
    height: usize,
    argb: &[u32],
    quality: u32,
    low_effort: bool,
    lz77_types_to_try: u32,
    cache_bits_max: u32,
    do_no_cache: bool,
    hash_chain: &VP8LHashChain,
    refs: &mut [VP8LBackwardRefs],
) -> Result<u32, TryReserveError> {
    use super::backward_references_cost_enc::VP8LBackwardReferencesTraceBackwards;
    use super::histogram_enc::*;
    assert!(quality <= 100 && cache_bits_max <= MAX_COLOR_CACHE_BITS);
    if low_effort {
        BackwardReferencesLz77(width, height, argb, 0, hash_chain, &mut refs[0])?;
        BackwardReferences2DLocality(width as u32, &mut refs[0]);
        return Ok(0);
    }
    assert!(lz77_types_to_try > 0 && lz77_types_to_try & !7 == 0);
    let tmp_index = if do_no_cache { 2 } else { 1 };
    assert!(refs.len() > tmp_index);
    let (best, tmp) = refs.split_at_mut(tmp_index);
    let tmp = &mut tmp[0];
    let mut histo = VP8LAllocateHistogram(MAX_COLOR_CACHE_BITS)?;
    let mut types_best = [0; 2];
    let mut costs_best = [u64::MAX; 2];
    let mut cache_bits_best = 0;
    let mut box_chain = None;
    for kind in [kLZ77Standard, kLZ77RLE, kLZ77Box] {
        if lz77_types_to_try & kind == 0 {
            continue;
        }
        match kind {
            kLZ77Standard => BackwardReferencesLz77(width, height, argb, 0, hash_chain, tmp)?,
            kLZ77RLE => BackwardReferencesRle(width, height, argb, 0, tmp)?,
            kLZ77Box => {
                let mut chain =
                    VP8LHashChainInit(width.checked_mul(height).expect("image geometry"))?;
                BackwardReferencesLz77Box(width, height, argb, 0, hash_chain, &mut chain, tmp)?;
                box_chain = Some(chain);
            }
            _ => unreachable!(),
        }
        let mut bit_cost = 0;
        for i in (0..=1).rev() {
            if i == 1 && !do_no_cache {
                continue;
            }
            let mut bits = if i == 1 { 0 } else { cache_bits_max };
            if i == 0 {
                CalculateBestCacheSize(argb, quality, tmp, &mut bits)?;
                if bits > 0 {
                    BackwardRefsWithLocalCache(argb, bits, tmp)?;
                }
            }
            if !(i == 0 && do_no_cache && bits == 0) {
                VP8LHistogramCreate(&mut histo, tmp, Some(bits))?;
                bit_cost = VP8LHistogramEstimateBits(&histo);
            }
            if bit_cost < costs_best[i] {
                if i == 1 {
                    BackwardRefsClone(tmp, &mut best[1])?;
                } else {
                    BackwardRefsSwap(tmp, &mut best[0]);
                }
                costs_best[i] = bit_cost;
                types_best[i] = kind;
                if i == 0 {
                    cache_bits_best = bits;
                }
            }
        }
    }
    for i in (0..=1).rev() {
        if i == 1 && !do_no_cache {
            continue;
        }
        if (types_best[i] == kLZ77Standard || types_best[i] == kLZ77Box) && quality >= 25 {
            let chain = if types_best[i] == kLZ77Standard {
                hash_chain
            } else {
                box_chain.as_ref().unwrap()
            };
            let bits = if i == 1 { 0 } else { cache_bits_best };
            VP8LBackwardReferencesTraceBackwards(width, height, argb, bits, chain, &best[i], tmp)?;
            VP8LHistogramCreate(&mut histo, tmp, Some(bits))?;
            if VP8LHistogramEstimateBits(&histo) < costs_best[i] {
                BackwardRefsSwap(tmp, &mut best[i]);
            }
        }
        BackwardReferences2DLocality(width as u32, &mut best[i]);
        if i == 1 && types_best[0] == types_best[1] && cache_bits_best == 0 {
            let (first, second) = best.split_at_mut(1);
            BackwardRefsClone(&second[0], &mut first[0])?;
            break;
        }
    }
    Ok(cache_bits_best)
}
