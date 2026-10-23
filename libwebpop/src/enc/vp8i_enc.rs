// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/vp8i_enc.h, prediction scratch layout.
// See COPYING and PATENTS.

use crate::dsp::enc::BPS;

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
