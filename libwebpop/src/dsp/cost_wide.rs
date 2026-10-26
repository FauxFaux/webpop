// Copyright 2015 Google Inc. All Rights Reserved.
// Portable counterparts of upstream/src/dsp/cost_{sse2,neon}.c.
// See COPYING and PATENTS.

use super::cost::{VP8EncBands, VP8LevelFixedCosts};
use crate::enc::{cost_enc::*, vp8i_enc::*};
use wide::{i16x8, u16x8};

pub fn SetResidualCoeffs_wide<'a>(coeffs: &'a [i16; 16], res: &mut VP8Residual<'a>) {
    assert!(res.first == 0 || coeffs[0] == 0);
    let lo = i16x8::from_slice_unaligned(&coeffs[..8])
        .simd_ne(i16x8::ZERO)
        .to_bitmask();
    let hi = i16x8::from_slice_unaligned(&coeffs[8..])
        .simd_ne(i16x8::ZERO)
        .to_bitmask();
    let mask = lo | (hi << 8);
    res.last = 31 - mask.leading_zeros() as i32;
    res.coeffs = coeffs;
}

pub fn GetResidualCost_wide(ctx0: usize, res: &VP8Residual<'_>, proba: &VP8EncProba) -> i32 {
    assert!(ctx0 < NUM_CTX);
    let mut n = res.first;
    let prob = &proba.coeffs[res.coeff_type];
    let p0 = prob[n][ctx0][0];
    if res.last < 0 {
        return VP8BitCost(0, p0);
    }
    // Only the coefficient classification is vectorized. The dependent table
    // walk remains scalar, as in upstream SSE2 and NEON.
    let mut abs = [0u16; 16];
    let mut levels = [0u16; 16];
    let mut contexts = [0u16; 16];
    for base in [0, 8] {
        let v: u16x8 = wide::bytemuck::cast(i16x8::from_slice_unaligned(&res.coeffs[base..]).abs());
        *<&mut [u16; 8]>::try_from(&mut abs[base..base + 8]).unwrap() = v.to_array();
        *<&mut [u16; 8]>::try_from(&mut levels[base..base + 8]).unwrap() =
            v.min(u16x8::splat(MAX_VARIABLE_LEVEL as u16)).to_array();
        *<&mut [u16; 8]>::try_from(&mut contexts[base..base + 8]).unwrap() =
            v.min(u16x8::splat(2)).to_array();
    }
    let mut table = proba.remapped_costs(res.coeff_type, n, ctx0);
    let mut cost = if ctx0 == 0 { VP8BitCost(1, p0) } else { 0 };
    while n < res.last as usize {
        cost +=
            i32::from(VP8LevelFixedCosts[abs[n] as usize]) + i32::from(table[levels[n] as usize]);
        n += 1;
        table = proba.remapped_costs(res.coeff_type, n, contexts[n - 1] as usize);
    }
    assert!(abs[n] != 0);
    cost += i32::from(VP8LevelFixedCosts[abs[n] as usize]) + i32::from(table[levels[n] as usize]);
    if n < 15 {
        cost += VP8BitCost(0, prob[VP8EncBands[n + 1]][contexts[n] as usize][0]);
    }
    cost
}
