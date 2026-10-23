// Copyright 2010 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/dec.c, encoder-required inverse WHT only.
// See COPYING and PATENTS.

/// Inverse WHT, scattering sixteen raster-order input coefficients into the DC
/// slots of sixteen consecutive 16-coefficient luma blocks. AC slots and any
/// trailing destination storage are preserved.
pub fn TransformWHT_C(input: &[i16; 16], out: &mut [i16]) {
    assert!(out.len() >= 16 * 15 + 1);
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let a0 = i32::from(input[i]) + i32::from(input[12 + i]);
        let a1 = i32::from(input[4 + i]) + i32::from(input[8 + i]);
        let a2 = i32::from(input[4 + i]) - i32::from(input[8 + i]);
        let a3 = i32::from(input[i]) - i32::from(input[12 + i]);
        tmp[i] = a0 + a1;
        tmp[8 + i] = a0 - a1;
        tmp[4 + i] = a3 + a2;
        tmp[12 + i] = a3 - a2;
    }
    for i in 0..4 {
        let dc = tmp[4 * i] + 3;
        let a0 = dc + tmp[4 * i + 3];
        let a1 = tmp[4 * i + 1] + tmp[4 * i + 2];
        let a2 = tmp[4 * i + 1] - tmp[4 * i + 2];
        let a3 = dc - tmp[4 * i + 3];
        out[64 * i] = ((a0 + a1) >> 3) as i16;
        out[64 * i + 16] = ((a3 + a2) >> 3) as i16;
        out[64 * i + 32] = ((a0 - a1) >> 3) as i16;
        out[64 * i + 48] = ((a3 - a2) >> 3) as i16;
    }
}
