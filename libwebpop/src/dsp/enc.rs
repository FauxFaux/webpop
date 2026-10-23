// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/enc.c, functions of the same names.
// See COPYING and PATENTS.

/// Byte stride of the encoder's source, prediction and reconstruction blocks.
pub const BPS: usize = 32;

/// Forward VP8 transform of a 4×4 pixel residual, in raster coefficient order.
/// Both inputs use the encoder's fixed `BPS` stride; row padding is ignored.
pub fn FTransform_C(src: &[u8], reference: &[u8], out: &mut [i16; 16]) {
    assert!(src.len() >= 3 * BPS + 4 && reference.len() >= 3 * BPS + 4);
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let offset = i * BPS;
        let d0 = i32::from(src[offset]) - i32::from(reference[offset]);
        let d1 = i32::from(src[offset + 1]) - i32::from(reference[offset + 1]);
        let d2 = i32::from(src[offset + 2]) - i32::from(reference[offset + 2]);
        let d3 = i32::from(src[offset + 3]) - i32::from(reference[offset + 3]);
        let a0 = d0 + d3;
        let a1 = d1 + d2;
        let a2 = d1 - d2;
        let a3 = d0 - d3;
        tmp[4 * i] = (a0 + a1) * 8;
        tmp[4 * i + 1] = (a2 * 2217 + a3 * 5352 + 1812) >> 9;
        tmp[4 * i + 2] = (a0 - a1) * 8;
        tmp[4 * i + 3] = (a3 * 2217 - a2 * 5352 + 937) >> 9;
    }
    for i in 0..4 {
        let a0 = tmp[i] + tmp[12 + i];
        let a1 = tmp[4 + i] + tmp[8 + i];
        let a2 = tmp[4 + i] - tmp[8 + i];
        let a3 = tmp[i] - tmp[12 + i];
        out[i] = ((a0 + a1 + 7) >> 4) as i16;
        out[4 + i] = (((a2 * 2217 + a3 * 5352 + 12000) >> 16) + i32::from(a3 != 0)) as i16;
        out[8 + i] = ((a0 - a1 + 7) >> 4) as i16;
        out[12 + i] = ((a3 * 2217 - a2 * 5352 + 51000) >> 16) as i16;
    }
}
