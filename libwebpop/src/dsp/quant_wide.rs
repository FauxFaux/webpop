// Copyright 2018 Google Inc. All Rights Reserved.
// Portable counterpart of IsFlat_NEON in upstream/src/dsp/quant.h.
// See COPYING and PATENTS.

use wide::i16x8;

/// Count nonzero AC coefficients, ignoring each block's DC slot.
#[inline]
pub fn IsFlat_wide(levels: &[[i16; 16]], thresh: i32) -> bool {
    let ac_mask = i16x8::from([0, 1, 1, 1, 1, 1, 1, 1]);
    let mut score = 0i64;
    for block in levels {
        let lo = i16x8::from_slice_unaligned(&block[..8]).simd_ne(i16x8::ZERO) & ac_mask;
        let hi = i16x8::from_slice_unaligned(&block[8..]).simd_ne(i16x8::ZERO) & i16x8::splat(1);
        score += i64::from((lo + hi).reduce_add());
        if score > i64::from(thresh) {
            return false;
        }
    }
    true
}
