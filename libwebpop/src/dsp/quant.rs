// Copyright 2018 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/quant.h. See COPYING and PATENTS.

use super::enc::BPS;

#[inline]
pub fn IsFlat(levels: &[[i16; 16]], thresh: i32) -> bool {
    let mut score = 0;
    for block in levels {
        for &level in &block[1..] {
            score += i32::from(level != 0);
            if score > thresh {
                return false;
            }
        }
    }
    true
}

#[inline]
pub fn IsFlatSource16(src: &[u8]) -> bool {
    assert!(src.len() >= 15 * BPS + 16);
    (0..16).all(|y| src[y * BPS..y * BPS + 16].iter().all(|&v| v == src[0]))
}
