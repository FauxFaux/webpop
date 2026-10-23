// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8i_enc.h, encoder foundations.
// See COPYING and PATENTS.

use crate::dsp::enc::BPS;

pub const MAX_LEVEL: i32 = 2047;
pub const QFIX: u32 = 17;

#[inline]
pub fn BIAS(b: u32) -> u32 {
    b.wrapping_shl(QFIX - 8)
}

#[inline]
pub fn QUANTDIV(n: u32, iQ: u32, B: u32) -> i32 {
    (n.wrapping_mul(iQ).wrapping_add(B) >> QFIX) as i32
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VP8Matrix {
    pub q: [u16; 16],
    pub iq: [u16; 16],
    pub bias: [u32; 16],
    pub zthresh: [u32; 16],
    pub sharpen: [u16; 16],
}

pub const PRED_SIZE_ENC: usize = (32 + 16 + 8) * BPS;

pub const I16DC16: usize = 0;
pub const I16TM16: usize = I16DC16 + 16;
pub const I16VE16: usize = 16 * BPS;
pub const I16HE16: usize = I16VE16 + 16;

pub const C8DC8: usize = 32 * BPS;
pub const C8TM8: usize = C8DC8 + 16;
pub const C8VE8: usize = C8DC8 + 8 * BPS;
pub const C8HE8: usize = C8VE8 + 16;

pub const I4DC4: usize = 48 * BPS;
pub const I4TM4: usize = I4DC4 + 4;
pub const I4VE4: usize = I4DC4 + 8;
pub const I4HE4: usize = I4DC4 + 12;
pub const I4RD4: usize = I4DC4 + 16;
pub const I4VR4: usize = I4DC4 + 20;
pub const I4LD4: usize = I4DC4 + 24;
pub const I4VL4: usize = I4DC4 + 28;
pub const I4HD4: usize = I4DC4 + 4 * BPS;
pub const I4HU4: usize = I4HD4 + 4;
pub const I4TMP: usize = I4HD4 + 8;
