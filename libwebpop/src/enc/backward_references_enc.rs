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

pub fn PixOrCopyCreateLiteral(argb: u32) -> PixOrCopy {
    PixOrCopy {
        mode: Mode::Literal,
        len: 1,
        argb_or_distance: argb,
    }
}

pub fn PixOrCopyCreateCacheIdx(idx: u32) -> PixOrCopy {
    assert!(idx < (1 << MAX_COLOR_CACHE_BITS));
    PixOrCopy {
        mode: Mode::CacheIdx,
        len: 1,
        argb_or_distance: idx,
    }
}

pub fn PixOrCopyCreateCopy(distance: u32, len: u16) -> PixOrCopy {
    PixOrCopy {
        mode: Mode::Copy,
        len,
        argb_or_distance: distance,
    }
}

pub fn PixOrCopyIsLiteral(p: &PixOrCopy) -> bool {
    p.mode == Mode::Literal
}
pub fn PixOrCopyIsCacheIdx(p: &PixOrCopy) -> bool {
    p.mode == Mode::CacheIdx
}
pub fn PixOrCopyIsCopy(p: &PixOrCopy) -> bool {
    p.mode == Mode::Copy
}

/// Component order is blue, green, red, alpha, independent of host endianness.
pub fn PixOrCopyLiteral(p: &PixOrCopy, component: u32) -> u32 {
    assert!(PixOrCopyIsLiteral(p) && component < 4);
    (p.argb_or_distance >> (component * 8)) & 255
}

pub fn PixOrCopyLength(p: &PixOrCopy) -> u32 {
    u32::from(p.len)
}
pub fn PixOrCopyCacheIdx(p: &PixOrCopy) -> u32 {
    assert!(PixOrCopyIsCacheIdx(p));
    p.argb_or_distance
}
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
pub fn VP8LRefsCursorOk(c: &VP8LRefsCursor<'_>) -> bool {
    c.cur_pos.is_some()
}
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
