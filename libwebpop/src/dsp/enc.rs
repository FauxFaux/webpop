// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/enc.c, functions of the same names.
// See COPYING and PATENTS.

use crate::enc::vp8i_enc::*;

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

#[inline]
fn Fill(dst: &mut [u8], value: u8, size: usize) {
    for y in 0..size {
        dst[y * BPS..y * BPS + size].fill(value);
    }
}

#[inline]
fn VerticalPred(dst: &mut [u8], top: Option<&[u8]>, size: usize) {
    if let Some(top) = top {
        for y in 0..size {
            dst[y * BPS..y * BPS + size].copy_from_slice(&top[..size]);
        }
    } else {
        Fill(dst, 127, size);
    }
}

#[inline]
fn HorizontalPred(dst: &mut [u8], left: Option<&[u8]>, size: usize) {
    if let Some(left) = left {
        for y in 0..size {
            dst[y * BPS..y * BPS + size].fill(left[1 + y]);
        }
    } else {
        Fill(dst, 129, size);
    }
}

#[inline]
fn TrueMotion(dst: &mut [u8], left: Option<&[u8]>, top: Option<&[u8]>, size: usize) {
    match (left, top) {
        (Some(left), Some(top)) => {
            for y in 0..size {
                for x in 0..size {
                    dst[y * BPS + x] =
                        clip_8b(i32::from(left[1 + y]) + i32::from(top[x]) - i32::from(left[0]));
                }
            }
        }
        (Some(_), None) => HorizontalPred(dst, left, size),
        (None, Some(_)) => VerticalPred(dst, top, size),
        (None, None) => Fill(dst, 129, size),
    }
}

#[inline]
fn DCMode(
    dst: &mut [u8],
    left: Option<&[u8]>,
    top: Option<&[u8]>,
    size: usize,
    round: u32,
    shift: u32,
) {
    let mut dc: u32;
    if let Some(top) = top {
        dc = top[..size].iter().map(|&v| u32::from(v)).sum();
        if let Some(left) = left {
            dc += left[1..=size].iter().map(|&v| u32::from(v)).sum::<u32>();
        } else {
            dc *= 2;
        }
        dc = (dc + round) >> shift;
    } else if let Some(left) = left {
        dc = left[1..=size].iter().map(|&v| u32::from(v)).sum::<u32>() * 2;
        dc = (dc + round) >> shift;
    } else {
        dc = 128;
    }
    Fill(dst, dc as u8, size);
}

/// Write four luma 16×16 predictions into the upstream prediction scratch area.
/// `left`, when present, is `[top_left, left[0], …, left[15]]`; `top` has sixteen
/// samples. Missing borders use the upstream DC/vertical/horizontal defaults.
pub fn Intra16Preds_C(dst: &mut [u8], left: Option<&[u8]>, top: Option<&[u8]>) {
    assert!(dst.len() >= I16HE16 + 15 * BPS + 16);
    assert!(left.is_none_or(|v| v.len() >= 17) && top.is_none_or(|v| v.len() >= 16));
    DCMode(&mut dst[I16DC16..], left, top, 16, 16, 5);
    VerticalPred(&mut dst[I16VE16..], top, 16);
    HorizontalPred(&mut dst[I16HE16..], left, 16);
    TrueMotion(&mut dst[I16TM16..], left, top, 16);
}

/// Write four chroma predictions, each containing adjacent U/V 8×8 blocks.
/// `top` contains U then V (eight bytes each). In `left`, U's corner/sample
/// window is `[0..9]` and V's is `[16..25]`; intervening bytes are ignored.
pub fn IntraChromaPreds_C(dst: &mut [u8], left: Option<&[u8]>, top: Option<&[u8]>) {
    assert!(dst.len() >= C8HE8 + 7 * BPS + 16);
    assert!(left.is_none_or(|v| v.len() >= 25) && top.is_none_or(|v| v.len() >= 16));
    for channel in 0..2 {
        let left = left.map(|v| &v[16 * channel..]);
        let top = top.map(|v| &v[8 * channel..]);
        let offset = 8 * channel;
        DCMode(&mut dst[C8DC8 + offset..], left, top, 8, 8, 4);
        VerticalPred(&mut dst[C8VE8 + offset..], top, 8);
        HorizontalPred(&mut dst[C8HE8 + offset..], left, 8);
        TrueMotion(&mut dst[C8TM8 + offset..], left, top, 8);
    }
}

#[inline]
fn AVG2(a: u8, b: u8) -> u8 {
    ((u16::from(a) + u16::from(b) + 1) >> 1) as u8
}

#[inline]
fn AVG3(a: u8, b: u8, c: u8) -> u8 {
    ((u16::from(a) + 2 * u16::from(b) + u16::from(c) + 2) >> 2) as u8
}

fn store_4x4(dst: &mut [u8], rows: [[u8; 4]; 4]) {
    for (y, row) in rows.iter().enumerate() {
        dst[y * BPS..y * BPS + 4].copy_from_slice(row);
    }
}

fn VE4(dst: &mut [u8], top: &[u8; 13]) {
    let vals = std::array::from_fn(|x| AVG3(top[4 + x], top[5 + x], top[6 + x]));
    store_4x4(dst, [vals; 4]);
}

fn HE4(dst: &mut [u8], top: &[u8; 13]) {
    let rows =
        std::array::from_fn(|y| [AVG3(top[4 - y], top[3 - y], top[2usize.saturating_sub(y)]); 4]);
    store_4x4(dst, rows);
}

fn DC4(dst: &mut [u8], top: &[u8; 13]) {
    let dc = 4 + top[..4]
        .iter()
        .chain(&top[5..9])
        .map(|&v| u32::from(v))
        .sum::<u32>();
    Fill(dst, (dc >> 3) as u8, 4);
}

fn RD4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG3(t[2], t[1], t[0]);
    let b = AVG3(t[3], t[2], t[1]);
    let c = AVG3(t[4], t[3], t[2]);
    let d = AVG3(t[5], t[4], t[3]);
    let e = AVG3(t[6], t[5], t[4]);
    let f = AVG3(t[7], t[6], t[5]);
    let g = AVG3(t[8], t[7], t[6]);
    store_4x4(
        dst,
        [[d, e, f, g], [c, d, e, f], [b, c, d, e], [a, b, c, d]],
    );
}

fn LD4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG3(t[5], t[6], t[7]);
    let b = AVG3(t[6], t[7], t[8]);
    let c = AVG3(t[7], t[8], t[9]);
    let d = AVG3(t[8], t[9], t[10]);
    let e = AVG3(t[9], t[10], t[11]);
    let f = AVG3(t[10], t[11], t[12]);
    let g = AVG3(t[11], t[12], t[12]);
    store_4x4(
        dst,
        [[a, b, c, d], [b, c, d, e], [c, d, e, f], [d, e, f, g]],
    );
}

fn VR4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG2(t[4], t[5]);
    let b = AVG2(t[5], t[6]);
    let c = AVG2(t[6], t[7]);
    let d = AVG2(t[7], t[8]);
    let e = AVG3(t[1], t[2], t[3]);
    let f = AVG3(t[2], t[3], t[4]);
    let g = AVG3(t[3], t[4], t[5]);
    let h = AVG3(t[4], t[5], t[6]);
    let i = AVG3(t[5], t[6], t[7]);
    let j = AVG3(t[6], t[7], t[8]);
    store_4x4(
        dst,
        [[a, b, c, d], [g, h, i, j], [f, a, b, c], [e, g, h, i]],
    );
}

fn VL4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG2(t[5], t[6]);
    let b = AVG2(t[6], t[7]);
    let c = AVG2(t[7], t[8]);
    let d = AVG2(t[8], t[9]);
    let e = AVG3(t[5], t[6], t[7]);
    let f = AVG3(t[6], t[7], t[8]);
    let g = AVG3(t[7], t[8], t[9]);
    let h = AVG3(t[8], t[9], t[10]);
    let i = AVG3(t[9], t[10], t[11]);
    let j = AVG3(t[10], t[11], t[12]);
    store_4x4(
        dst,
        [[a, b, c, d], [e, f, g, h], [b, c, d, i], [f, g, h, j]],
    );
}

fn HU4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG2(t[3], t[2]);
    let b = AVG2(t[2], t[1]);
    let c = AVG2(t[1], t[0]);
    let d = AVG3(t[3], t[2], t[1]);
    let e = AVG3(t[2], t[1], t[0]);
    let f = AVG3(t[1], t[0], t[0]);
    let l = t[0];
    store_4x4(
        dst,
        [[a, d, b, e], [b, e, c, f], [c, f, l, l], [l, l, l, l]],
    );
}

fn HD4(dst: &mut [u8], t: &[u8; 13]) {
    let a = AVG2(t[3], t[4]);
    let b = AVG2(t[2], t[3]);
    let c = AVG2(t[1], t[2]);
    let d = AVG2(t[0], t[1]);
    let e = AVG3(t[5], t[6], t[7]);
    let f = AVG3(t[4], t[5], t[6]);
    let g = AVG3(t[3], t[4], t[5]);
    let h = AVG3(t[2], t[3], t[4]);
    let i = AVG3(t[1], t[2], t[3]);
    let j = AVG3(t[0], t[1], t[2]);
    store_4x4(
        dst,
        [[a, g, f, e], [b, h, a, g], [c, i, b, h], [d, j, c, i]],
    );
}

fn TM4(dst: &mut [u8], top: &[u8; 13]) {
    for y in 0..4 {
        for x in 0..4 {
            dst[y * BPS + x] =
                clip_8b(i32::from(top[3 - y]) + i32::from(top[5 + x]) - i32::from(top[4]));
        }
    }
}

/// Write all ten 4×4 modes into the upstream prediction scratch area. `top` is
/// `[L, K, J, I, X, A, B, C, D, E, F, G, H]`: four reversed left samples,
/// top-left, four top samples and four top-right samples. The caller supplies
/// any missing-border substitution or top-right replication.
pub fn Intra4Preds_C(dst: &mut [u8], top: &[u8; 13]) {
    assert!(dst.len() >= I4HU4 + 3 * BPS + 4);
    DC4(&mut dst[I4DC4..], top);
    TM4(&mut dst[I4TM4..], top);
    VE4(&mut dst[I4VE4..], top);
    HE4(&mut dst[I4HE4..], top);
    RD4(&mut dst[I4RD4..], top);
    VR4(&mut dst[I4VR4..], top);
    LD4(&mut dst[I4LD4..], top);
    VL4(&mut dst[I4VL4..], top);
    HD4(&mut dst[I4HD4..], top);
    HU4(&mut dst[I4HU4..], top);
}
