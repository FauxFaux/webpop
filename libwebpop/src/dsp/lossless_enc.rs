// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/lossless_enc.c, functions of the same names.
// See COPYING and PATENTS.

use super::lossless::VP8LMultipliers;

pub fn VP8LSubtractGreenFromBlueAndRed_C(argb_data: &mut [u32]) {
    for argb in argb_data {
        let green = (*argb >> 8) as u8;
        let new_r = (*argb >> 16) as u8;
        let new_b = *argb as u8;
        *argb = (*argb & 0xff00ff00)
            | (u32::from(new_r.wrapping_sub(green)) << 16)
            | u32::from(new_b.wrapping_sub(green));
    }
}

fn ColorTransformDelta(color_pred: i8, color: i8) -> i32 {
    (i32::from(color_pred) * i32::from(color)) >> 5
}

fn U32ToS8(v: u32) -> i8 {
    v as u8 as i8
}

pub fn VP8LTransformColor_C(m: &VP8LMultipliers, data: &mut [u32]) {
    for argb in data {
        let green = U32ToS8(*argb >> 8);
        let red = U32ToS8(*argb >> 16);
        let new_red =
            ((i32::from(red) & 0xff) - ColorTransformDelta(m.green_to_red as i8, green)) & 0xff;
        // The blue prediction uses the original red, before its transform.
        let new_blue = ((*argb & 0xff) as i32
            - ColorTransformDelta(m.green_to_blue as i8, green)
            - ColorTransformDelta(m.red_to_blue as i8, red))
            & 0xff;
        *argb = (*argb & 0xff00ff00) | ((new_red as u32) << 16) | new_blue as u32;
    }
}
