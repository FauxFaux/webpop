// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/utils/color_cache_utils.{c,h}.
// See COPYING and PATENTS.

use crate::webp::format_constants::MAX_CACHE_BITS;

const HASH_MUL: u32 = 0x1e35a7bd;

#[inline]
pub const fn VP8LHashPix(argb: u32, shift: u32) -> u32 {
    assert!(shift < 32);
    argb.wrapping_mul(HASH_MUL) >> shift
}

/// Owned, zero-initialized cache. Drop replaces upstream's Clear operation;
/// Clone makes an independent copy without reproducing pointer ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8LColorCache {
    colors: Vec<u32>,
    hash_shift: u32,
}

/// None means invalid cache bits or allocation failure. Encoder caches use
/// 1..=MAX_CACHE_BITS; cache_bits=0 means no cache and is handled by the caller.
pub fn VP8LColorCacheInit(hash_bits: u32) -> Option<VP8LColorCache> {
    if !(1..=MAX_CACHE_BITS).contains(&hash_bits) {
        return None;
    }
    VP8LColorCacheTryInit(hash_bits).ok()
}

/// Internal encoder allocation path, retaining the allocation error.
pub fn VP8LColorCacheTryInit(
    hash_bits: u32,
) -> Result<VP8LColorCache, std::collections::TryReserveError> {
    assert!((1..=MAX_CACHE_BITS).contains(&hash_bits));
    let mut colors = Vec::new();
    colors.try_reserve_exact(1 << hash_bits)?;
    colors.resize(1 << hash_bits, 0);
    Ok(VP8LColorCache {
        colors,
        hash_shift: 32 - hash_bits,
    })
}

#[inline]
pub fn VP8LColorCacheLookup(cc: &VP8LColorCache, key: u32) -> u32 {
    cc.colors[key as usize]
}

#[inline]
pub fn VP8LColorCacheSet(cc: &mut VP8LColorCache, key: u32, argb: u32) {
    cc.colors[key as usize] = argb;
}

#[inline]
pub fn VP8LColorCacheGetIndex(cc: &VP8LColorCache, argb: u32) -> u32 {
    VP8LHashPix(argb, cc.hash_shift)
}

#[inline]
pub fn VP8LColorCacheInsert(cc: &mut VP8LColorCache, argb: u32) {
    let key = VP8LColorCacheGetIndex(cc, argb);
    VP8LColorCacheSet(cc, key, argb);
}

#[inline]
pub fn VP8LColorCacheContains(cc: &VP8LColorCache, argb: u32) -> Option<u32> {
    let key = VP8LColorCacheGetIndex(cc, argb);
    (VP8LColorCacheLookup(cc, key) == argb).then_some(key)
}

pub fn VP8LColorCacheCopy(src: &VP8LColorCache, dst: &mut VP8LColorCache) {
    assert_eq!(src.hash_shift, dst.hash_shift);
    dst.colors.copy_from_slice(&src.colors);
}
