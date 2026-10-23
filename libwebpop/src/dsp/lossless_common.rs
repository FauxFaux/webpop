// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/lossless_common.h and the prefix tables in
// upstream/src/dsp/lossless_enc.c. See COPYING and PATENTS.

use crate::utils::utils::BitsLog2Floor;

/// Ceiling of size / 2^sampling_bits. Use a wider intermediate so even a
/// full-range u32 size is handled without an overflow in the rounding step.
#[inline]
pub const fn VP8LSubSampleSize(size: u32, sampling_bits: u32) -> u32 {
    assert!(sampling_bits < 32);
    ((size as u64 + (1_u64 << sampling_bits) - 1) >> sampling_bits) as u32
}

#[inline]
pub const fn VP8LNearLosslessBits(near_lossless_quality: u32) -> u32 {
    assert!(near_lossless_quality <= 100);
    5 - near_lossless_quality / 20
}

/// Return (code, extra_bits). The NoLUT helpers require distance >= 3;
/// callers with distances 1 and 2 must use the table-backed entry points.
#[inline]
pub const fn VP8LPrefixEncodeBitsNoLUT(distance: u32) -> (u32, u32) {
    assert!(distance >= 3 && distance <= i32::MAX as u32);
    let distance = distance - 1;
    let highest_bit = BitsLog2Floor(distance);
    let second_highest_bit = (distance >> (highest_bit - 1)) & 1;
    (2 * highest_bit + second_highest_bit, highest_bit - 1)
}

/// Return (code, extra_bits, extra_bits_value).
#[inline]
pub const fn VP8LPrefixEncodeNoLUT(distance: u32) -> (u32, u32, u32) {
    let (code, extra_bits) = VP8LPrefixEncodeBitsNoLUT(distance);
    (code, extra_bits, (distance - 1) & ((1 << extra_bits) - 1))
}

const PREFIX_LOOKUP_IDX_MAX: usize = 512;

// Generate the immutable upstream tables at compile time rather than keeping
// two large hand-copied arrays. Entries 0..=2 are the upstream special cases.
const fn prefix_table() -> [(u32, u32, u32); PREFIX_LOOKUP_IDX_MAX] {
    let mut table = [(0, 0, 0); PREFIX_LOOKUP_IDX_MAX];
    table[2] = (1, 0, 0);
    let mut distance = 3;
    while distance < PREFIX_LOOKUP_IDX_MAX {
        table[distance] = VP8LPrefixEncodeNoLUT(distance as u32);
        distance += 1;
    }
    table
}

const PREFIX_TABLE: [(u32, u32, u32); PREFIX_LOOKUP_IDX_MAX] = prefix_table();

#[inline]
pub const fn VP8LPrefixEncode(distance: u32) -> (u32, u32, u32) {
    // Entry zero is used by the traceback length-cost cache.
    assert!(distance <= i32::MAX as u32);
    if distance < PREFIX_LOOKUP_IDX_MAX as u32 {
        PREFIX_TABLE[distance as usize]
    } else {
        VP8LPrefixEncodeNoLUT(distance)
    }
}

#[inline]
pub const fn VP8LPrefixEncodeBits(distance: u32) -> (u32, u32) {
    let (code, extra_bits, _) = VP8LPrefixEncode(distance);
    (code, extra_bits)
}

/// Sum each ARGB byte independently, modulo 256.
#[inline]
pub const fn VP8LAddPixels(a: u32, b: u32) -> u32 {
    let alpha_and_green = (a & 0xff00ff00).wrapping_add(b & 0xff00ff00);
    let red_and_blue = (a & 0x00ff00ff).wrapping_add(b & 0x00ff00ff);
    (alpha_and_green & 0xff00ff00) | (red_and_blue & 0x00ff00ff)
}

/// Subtract each ARGB byte independently, modulo 256.
#[inline]
pub const fn VP8LSubPixels(a: u32, b: u32) -> u32 {
    let alpha_and_green = 0x00ff00ff_u32
        .wrapping_add(a & 0xff00ff00)
        .wrapping_sub(b & 0xff00ff00);
    let red_and_blue = 0xff00ff00_u32
        .wrapping_add(a & 0x00ff00ff)
        .wrapping_sub(b & 0x00ff00ff);
    (alpha_and_green & 0xff00ff00) | (red_and_blue & 0x00ff00ff)
}

pub const LOG_2_PRECISION_BITS: u32 = 23;
pub const LOG_2_RECIPROCAL_FIXED: u64 = 12102203;
pub const LOG_2_RECIPROCAL_FIXED_DOUBLE: f64 = 12102203.161561485;
pub const LOG_LOOKUP_IDX_MAX: u32 = 256;
pub const APPROX_LOG_MAX: u32 = 4096;
pub const APPROX_LOG_WITH_CORRECTION_MAX: u32 = 65536;

#[inline]
pub fn VP8LFastLog2(v: u32) -> u32 {
    if v < LOG_LOOKUP_IDX_MAX {
        super::lossless_enc::kLog2Table[v as usize]
    } else {
        super::lossless_enc::FastLog2Slow_C(v)
    }
}
#[inline]
pub fn VP8LFastSLog2(v: u32) -> u64 {
    if v < LOG_LOOKUP_IDX_MAX {
        super::lossless_enc::kSLog2Table[v as usize]
    } else {
        super::lossless_enc::FastSLog2Slow_C(v)
    }
}
#[inline]
pub const fn RightShiftRound(v: u64, shift: u32) -> u64 {
    assert!(shift < 64);
    v.wrapping_add((1_u64 << shift) >> 1) >> shift
}
/// C's signed nearest-integer division, with ties away from zero.
/// The denominator and intermediate arithmetic must be representable.
#[inline]
pub const fn DivRound(a: i64, b: i64) -> i64 {
    if (a < 0) == (b < 0) {
        (a + b / 2) / b
    } else {
        (a - b / 2) / b
    }
}
