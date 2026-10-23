// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/enc.c, functions of the same names.
// See COPYING and PATENTS.

/// Byte stride of the encoder's source, prediction and reconstruction blocks.
pub const BPS: usize = 32;

#[inline]
fn clip_8b(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

// The AC3 macros in upstream/src/dsp/dsp.h. Widen the product before shifting
// so signed intermediate multiplication cannot overflow for i16 coefficients.
#[inline]
fn WEBP_TRANSFORM_AC3_MUL1(v: i32) -> i32 {
    ((i64::from(v) * 20091) >> 16) as i32 + v
}

#[inline]
fn WEBP_TRANSFORM_AC3_MUL2(v: i32) -> i32 {
    ((i64::from(v) * 35468) >> 16) as i32
}

#[inline]
fn ITransformOne(reference: &[u8], input: &[i16], dst: &mut [u8]) {
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let a = i32::from(input[i]) + i32::from(input[8 + i]);
        let b = i32::from(input[i]) - i32::from(input[8 + i]);
        let c = WEBP_TRANSFORM_AC3_MUL2(i32::from(input[4 + i]))
            - WEBP_TRANSFORM_AC3_MUL1(i32::from(input[12 + i]));
        let d = WEBP_TRANSFORM_AC3_MUL1(i32::from(input[4 + i]))
            + WEBP_TRANSFORM_AC3_MUL2(i32::from(input[12 + i]));
        tmp[4 * i] = a + d;
        tmp[4 * i + 1] = b + c;
        tmp[4 * i + 2] = b - c;
        tmp[4 * i + 3] = a - d;
    }
    for i in 0..4 {
        let dc = tmp[i] + 4;
        let a = dc + tmp[8 + i];
        let b = dc - tmp[8 + i];
        let c = WEBP_TRANSFORM_AC3_MUL2(tmp[4 + i]) - WEBP_TRANSFORM_AC3_MUL1(tmp[12 + i]);
        let d = WEBP_TRANSFORM_AC3_MUL1(tmp[4 + i]) + WEBP_TRANSFORM_AC3_MUL2(tmp[12 + i]);
        for (x, v) in [a + d, b + c, b - c, a - d].into_iter().enumerate() {
            let offset = i * BPS + x;
            dst[offset] = clip_8b(i32::from(reference[offset]) + (v >> 3));
        }
    }
}

/// Reconstruct one 4×4 block, or two horizontally adjacent blocks when
/// `do_two` is set. Coefficients are consecutive raster-order 16-element blocks;
/// reference and destination use `BPS`. Destination padding is preserved.
pub fn ITransform_C(reference: &[u8], input: &[i16], dst: &mut [u8], do_two: bool) {
    let width = if do_two { 8 } else { 4 };
    assert!(reference.len() >= 3 * BPS + width && dst.len() >= 3 * BPS + width);
    assert!(input.len() >= if do_two { 32 } else { 16 });
    ITransformOne(reference, input, dst);
    if do_two {
        ITransformOne(&reference[4..], &input[16..], &mut dst[4..]);
    }
}

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

/// Forward transforms of two horizontally adjacent 4×4 blocks, with consecutive
/// 16-coefficient outputs. The source and reference use `BPS` byte stride.
pub fn FTransform2_C(src: &[u8], reference: &[u8], out: &mut [i16; 32]) {
    assert!(src.len() >= 3 * BPS + 8 && reference.len() >= 3 * BPS + 8);
    let (left, right) = out.split_at_mut(16);
    FTransform_C(src, reference, left.try_into().unwrap());
    FTransform_C(&src[4..], &reference[4..], right.try_into().unwrap());
}

/// Forward WHT of the signed 12-bit DC coefficients of sixteen luma blocks.
/// Input blocks are in raster order, each occupying 16 coefficients; only their
/// DC slots are read. Output is the sixteen raster-order WHT coefficients.
pub fn FTransformWHT_C(input: &[i16], out: &mut [i16; 16]) {
    assert!(input.len() >= 16 * 15 + 1);
    let mut tmp = [0i32; 16];
    for i in 0..4 {
        let offset = 64 * i;
        let a0 = i32::from(input[offset]) + i32::from(input[offset + 32]);
        let a1 = i32::from(input[offset + 16]) + i32::from(input[offset + 48]);
        let a2 = i32::from(input[offset + 16]) - i32::from(input[offset + 48]);
        let a3 = i32::from(input[offset]) - i32::from(input[offset + 32]);
        tmp[4 * i] = a0 + a1;
        tmp[4 * i + 1] = a3 + a2;
        tmp[4 * i + 2] = a3 - a2;
        tmp[4 * i + 3] = a0 - a1;
    }
    for i in 0..4 {
        let a0 = tmp[i] + tmp[8 + i];
        let a1 = tmp[4 + i] + tmp[12 + i];
        let a2 = tmp[4 + i] - tmp[12 + i];
        let a3 = tmp[i] - tmp[8 + i];
        out[i] = ((a0 + a1) >> 1) as i16;
        out[4 + i] = ((a3 + a2) >> 1) as i16;
        out[8 + i] = ((a3 - a2) >> 1) as i16;
        out[12 + i] = ((a0 - a1) >> 1) as i16;
    }
}
