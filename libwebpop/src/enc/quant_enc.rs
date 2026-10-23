// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/quant_enc.c, scan and prediction offset tables.
// See COPYING and PATENTS.
#![allow(non_upper_case_globals)]

use super::vp8i_enc::*;
use crate::dsp::enc::BPS;

pub const VP8Scan: [usize; 16] = [
    0,
    4,
    8,
    12,
    4 * BPS,
    4 * BPS + 4,
    4 * BPS + 8,
    4 * BPS + 12,
    8 * BPS,
    8 * BPS + 4,
    8 * BPS + 8,
    8 * BPS + 12,
    12 * BPS,
    12 * BPS + 4,
    12 * BPS + 8,
    12 * BPS + 12,
];
pub const VP8I16ModeOffsets: [usize; 4] = [I16DC16, I16TM16, I16VE16, I16HE16];
pub const VP8ScanUV: [usize; 8] = [0, 4, 4 * BPS, 4 * BPS + 4, 8, 12, 4 * BPS + 8, 4 * BPS + 12];
pub const VP8UVModeOffsets: [usize; 4] = [C8DC8, C8TM8, C8VE8, C8HE8];
pub const VP8I4ModeOffsets: [usize; 10] = [
    I4DC4, I4TM4, I4VE4, I4HE4, I4RD4, I4VR4, I4LD4, I4VL4, I4HD4, I4HU4,
];

pub fn VP8MakeLuma16Preds(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let left = (it.x != 0).then_some(&it.y_left[..]);
    let top = (it.y != 0).then(|| <[u8; 16]>::try_from(&it.y_top(enc)[..16]).unwrap());
    crate::dsp::enc::Intra16Preds_C(&mut it.yuv_p, left, top.as_ref().map(|t| &t[..]));
}

pub fn VP8MakeChroma8Preds(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let left = (it.x != 0).then_some(&it.uv_left[..]);
    let top = (it.y != 0).then(|| <[u8; 16]>::try_from(&it.uv_top(enc)[..16]).unwrap());
    crate::dsp::enc::IntraChromaPreds_C(&mut it.yuv_p, left, top.as_ref().map(|t| &t[..]));
}
