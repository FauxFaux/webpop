// Copyright 2011 Google Inc. All Rights Reserved.
// Portable counterparts of upstream/src/dsp/enc_{sse2,sse41,neon}.c.
// See COPYING and PATENTS.

//! Encoder DSP selected unconditionally, including on wide's scalar backend.
//! Keep vector types private to this module. `_wide` identifies the portable
//! implementation; `_C` in enc.rs remains the scalar correctness reference.

use super::enc::{BPS, VP8DspScan, VP8Histogram, VP8SetHistogramData};
use crate::enc::vp8i_enc::*;
use wide::{i16x8, i32x4, u8x16, u32x4};

#[inline]
fn transpose(v: [i32x4; 4]) -> [i32x4; 4] {
    wide::bytemuck::cast(u32x4::transpose(wide::bytemuck::cast(v)))
}

#[inline]
fn load_pixels4(src: &[u8]) -> i32x4 {
    i32x4::from([src[0].into(), src[1].into(), src[2].into(), src[3].into()])
}

#[inline]
fn load_coeffs4(src: &[i16]) -> i32x4 {
    i32x4::from([src[0].into(), src[1].into(), src[2].into(), src[3].into()])
}

#[inline]
fn store_pixels4(dst: &mut [u8], pixels: i32x4) {
    let v = pixels.max(i32x4::ZERO).min(i32x4::splat(255)).to_array();
    // Fixed array assignment keeps these four-byte stores out of libc.
    *<&mut [u8; 4]>::try_from(&mut dst[..4]).unwrap() =
        [v[0] as u8, v[1] as u8, v[2] as u8, v[3] as u8];
}

#[inline]
fn store_coeffs(out: &mut [i16; 16], rows: [i32x4; 4]) {
    for (dst, row) in out.chunks_exact_mut(4).zip(rows) {
        *<&mut [i16; 4]>::try_from(dst).unwrap() = narrow_coeffs4(row);
    }
}

#[inline]
fn narrow_coeffs4(row: i32x4) -> [i16; 4] {
    let v = row.to_array();
    [v[0] as i16, v[1] as i16, v[2] as i16, v[3] as i16]
}

// Exact signed (v * c) >> 16, including full i16-domain inverse inputs.
// Splitting v avoids overflowing a 32-bit product in the second pass.
#[inline]
fn mul_shift16(v: i32x4, c: u32) -> i32x4 {
    let lo: u32x4 = wide::bytemuck::cast(v);
    let lo = ((lo & u32x4::splat(65535)) * u32x4::splat(c)) >> 16;
    (v >> 16) * i32x4::splat(c as i32) + wide::bytemuck::cast::<_, i32x4>(lo)
}

#[inline]
fn inverse_pass([v0, v1, v2, v3]: [i32x4; 4]) -> [i32x4; 4] {
    let a = v0 + v2;
    let b = v0 - v2;
    let c = mul_shift16(v1, 35468) - (mul_shift16(v3, 20091) + v3);
    let d = mul_shift16(v1, 20091) + v1 + mul_shift16(v3, 35468);
    [a + d, b + c, b - c, a - d]
}

#[inline]
fn inverse_one(reference: &[u8], input: &[i16], dst: &mut [u8]) {
    let rows = std::array::from_fn(|y| load_coeffs4(&input[4 * y..]));
    let mut columns = transpose(inverse_pass(rows));
    columns[0] += i32x4::splat(4);
    let rows = transpose(inverse_pass(columns));
    for (y, row) in rows.into_iter().enumerate() {
        store_pixels4(
            &mut dst[y * BPS..],
            load_pixels4(&reference[y * BPS..]) + (row >> 3),
        );
    }
}

/// Inverse transform and clipped reconstruction of one or two adjacent blocks.
pub fn ITransform_wide(reference: &[u8], input: &[i16], dst: &mut [u8], do_two: bool) {
    let width = if do_two { 8 } else { 4 };
    assert!(reference.len() >= 3 * BPS + width && dst.len() >= 3 * BPS + width);
    assert!(input.len() >= if do_two { 32 } else { 16 });
    inverse_one(reference, input, dst);
    if do_two {
        inverse_one(&reference[4..], &input[16..], &mut dst[4..]);
    }
}

/// Forward transform, four independent rows/columns per vector pass.
pub fn FTransform_wide(src: &[u8], reference: &[u8], out: &mut [i16; 16]) {
    assert!(src.len() >= 3 * BPS + 4 && reference.len() >= 3 * BPS + 4);
    let [d0, d1, d2, d3] = transpose(std::array::from_fn(|y| {
        load_pixels4(&src[y * BPS..]) - load_pixels4(&reference[y * BPS..])
    }));
    let a0 = d0 + d3;
    let a1 = d1 + d2;
    let a2 = d1 - d2;
    let a3 = d0 - d3;
    let [t0, t1, t2, t3] = transpose([
        (a0 + a1) << 3,
        (a2 * 2217 + a3 * 5352 + i32x4::splat(1812)) >> 9,
        (a0 - a1) << 3,
        (a3 * 2217 - a2 * 5352 + i32x4::splat(937)) >> 9,
    ]);
    let a0 = t0 + t3;
    let a1 = t1 + t2;
    let a2 = t1 - t2;
    let a3 = t0 - t3;
    let correction = a3.simd_ne(i32x4::ZERO) & i32x4::splat(1);
    store_coeffs(
        out,
        [
            (a0 + a1 + i32x4::splat(7)) >> 4,
            ((a2 * 2217 + a3 * 5352 + i32x4::splat(12000)) >> 16) + correction,
            (a0 - a1 + i32x4::splat(7)) >> 4,
            (a3 * 2217 - a2 * 5352 + i32x4::splat(51000)) >> 16,
        ],
    );
}

pub fn FTransform2_wide(src: &[u8], reference: &[u8], out: &mut [i16; 32]) {
    assert!(src.len() >= 3 * BPS + 8 && reference.len() >= 3 * BPS + 8);
    let (left, right) = out.split_at_mut(16);
    FTransform_wide(src, reference, left.try_into().unwrap());
    FTransform_wide(&src[4..], &reference[4..], right.try_into().unwrap());
}

#[inline]
fn hadamard([v0, v1, v2, v3]: [i32x4; 4]) -> [i32x4; 4] {
    let a0 = v0 + v2;
    let a1 = v1 + v3;
    let a2 = v1 - v3;
    let a3 = v0 - v2;
    [a0 + a1, a3 + a2, a3 - a2, a0 - a1]
}

pub fn FTransformWHT_wide(input: &[i16], out: &mut [i16; 16]) {
    assert!(input.len() >= 241);
    let rows = std::array::from_fn(|y| {
        i32x4::from(std::array::from_fn(|x| i32::from(input[64 * y + 16 * x])))
    });
    let mut rows = hadamard(transpose(hadamard(transpose(rows))));
    for row in &mut rows {
        *row >>= 1;
    }
    store_coeffs(out, rows);
}

pub fn CollectHistogram_wide(
    reference: &[u8],
    pred: &[u8],
    start_block: usize,
    end_block: usize,
    histo: &mut VP8Histogram,
) {
    assert!(start_block <= end_block && end_block <= VP8DspScan.len());
    let mut distribution = [0; 32];
    for &offset in &VP8DspScan[start_block..end_block] {
        let mut out = [0; 16];
        FTransform_wide(&reference[offset..], &pred[offset..], &mut out);
        for coeffs in out.chunks_exact(4) {
            let bins = (load_coeffs4(coeffs).abs() >> 3u32).min(i32x4::splat(31));
            // Duplicate bins require scalar increments, as in upstream.
            for bin in bins.to_array() {
                distribution[bin as usize] += 1;
            }
        }
    }
    VP8SetHistogramData(&distribution, histo);
}

#[inline]
fn zigzag(levels: [i16; 16]) -> [i16; 16] {
    // Spell out the fixed shuffle: array::map currently leaves out-of-line
    // iterator helpers in non-LTO builds of this hot kernel.
    [
        levels[0], levels[1], levels[4], levels[8], levels[5], levels[2], levels[3], levels[6],
        levels[9], levels[12], levels[13], levels[10], levels[7], levels[11], levels[14],
        levels[15],
    ]
}

/// Quantize contiguous raster lanes, then shuffle once into zigzag order.
pub fn QuantizeBlock_wide(input: &mut [i16; 16], out: &mut [i16; 16], mtx: &VP8Matrix) -> bool {
    let mut levels = [0i16; 16];
    let mut nonzero = i32x4::ZERO;
    for offset in (0..16).step_by(4) {
        let value = load_coeffs4(&input[offset..]);
        let sharp = u32x4::from(std::array::from_fn(|i| u32::from(mtx.sharpen[offset + i])));
        let coeff: u32x4 = wide::bytemuck::cast(value.abs());
        let coeff = coeff + sharp;
        let iq = u32x4::from(std::array::from_fn(|i| u32::from(mtx.iq[offset + i])));
        let bias = u32x4::from(<[u32; 4]>::try_from(&mtx.bias[offset..offset + 4]).unwrap());
        let threshold =
            u32x4::from(<[u32; 4]>::try_from(&mtx.zthresh[offset..offset + 4]).unwrap());
        // u32 operators intentionally wrap, matching QUANTDIV even for matrices
        // outside the encoder's normal range. Keep the explicit threshold mask.
        let magnitude = ((coeff * iq + bias) >> QFIX).min(u32x4::splat(MAX_LEVEL as u32))
            & coeff.simd_gt(threshold);
        let magnitude: i32x4 = wide::bytemuck::cast(magnitude);
        let sign: i32x4 = value >> 31u32;
        let level: i32x4 = (magnitude ^ sign) - sign;
        nonzero |= level;
        let q = i32x4::from(std::array::from_fn(|i| i32::from(mtx.q[offset + i])));
        let dequantized = narrow_coeffs4(level * q);
        *<&mut [i16; 4]>::try_from(&mut input[offset..offset + 4]).unwrap() = dequantized;
        *<&mut [i16; 4]>::try_from(&mut levels[offset..offset + 4]).unwrap() =
            narrow_coeffs4(level);
    }
    *out = zigzag(levels);
    nonzero.simd_ne(i32x4::ZERO).any()
}

pub fn Quantize2Blocks_wide(input: &mut [i16; 32], out: &mut [i16; 32], mtx: &VP8Matrix) -> u32 {
    let (left_in, right_in) = input.split_at_mut(16);
    let (left_out, right_out) = out.split_at_mut(16);
    let nz = u32::from(QuantizeBlock_wide(
        left_in.try_into().unwrap(),
        left_out.try_into().unwrap(),
        mtx,
    ));
    nz | (u32::from(QuantizeBlock_wide(
        right_in.try_into().unwrap(),
        right_out.try_into().unwrap(),
        mtx,
    )) << 1)
}

#[inline]
fn load_pixels8(src: &[u8]) -> i16x8 {
    i16x8::from(std::array::from_fn(|i| i16::from(src[i])))
}

#[inline]
fn squared_error<const W: usize, const H: usize>(a: &[u8], b: &[u8]) -> i32 {
    assert!(a.len() >= (H - 1) * BPS + W && b.len() >= (H - 1) * BPS + W);
    let mut sum = i32x4::ZERO;
    for y in 0..H {
        for x in (0..W).step_by(4) {
            let offset = y * BPS + x;
            let d = load_pixels4(&a[offset..]) - load_pixels4(&b[offset..]);
            sum += d * d;
        }
    }
    sum.reduce_add()
}

pub fn SSE4x4_wide(a: &[u8], b: &[u8]) -> i32 {
    squared_error::<4, 4>(a, b)
}
pub fn SSE8x8_wide(a: &[u8], b: &[u8]) -> i32 {
    squared_error::<8, 8>(a, b)
}
pub fn SSE16x8_wide(a: &[u8], b: &[u8]) -> i32 {
    squared_error::<16, 8>(a, b)
}
pub fn SSE16x16_wide(a: &[u8], b: &[u8]) -> i32 {
    squared_error::<16, 16>(a, b)
}

pub fn Mean16x4_wide(reference: &[u8], dc: &mut [u32; 4]) {
    assert!(reference.len() >= 3 * BPS + 16);
    for (x, dst) in dc.iter_mut().enumerate() {
        let mut sum = i32x4::ZERO;
        for y in 0..4 {
            sum += load_pixels4(&reference[y * BPS + 4 * x..]);
        }
        *dst = sum.reduce_add() as u32;
    }
}

#[inline]
fn texture(input: &[u8], w: &[u16; 16]) -> i32 {
    let rows = std::array::from_fn(|y| load_pixels4(&input[y * BPS..]));
    let rows = hadamard(transpose(hadamard(transpose(rows))));
    let mut sum = i32x4::ZERO;
    for (y, row) in rows.into_iter().enumerate() {
        let weight = i32x4::from(std::array::from_fn(|x| i32::from(w[4 * y + x])));
        sum += row.abs() * weight;
    }
    sum.reduce_add()
}

pub fn Disto4x4_wide(a: &[u8], b: &[u8], w: &[u16; 16]) -> i32 {
    assert!(a.len() >= 3 * BPS + 4 && b.len() >= 3 * BPS + 4);
    (texture(b, w) - texture(a, w)).abs() >> 5
}

pub fn Disto16x16_wide(a: &[u8], b: &[u8], w: &[u16; 16]) -> i32 {
    assert!(a.len() >= 15 * BPS + 16 && b.len() >= 15 * BPS + 16);
    let mut sum = 0;
    for y in (0..16).step_by(4) {
        for x in (0..16).step_by(4) {
            let offset = y * BPS + x;
            sum += Disto4x4_wide(&a[offset..], &b[offset..], w);
        }
    }
    sum
}

#[inline]
fn store4(dst: &mut [u8], rows: [[u8; 4]; 4]) {
    for (y, row) in rows.into_iter().enumerate() {
        *<&mut [u8; 4]>::try_from(&mut dst[y * BPS..y * BPS + 4]).unwrap() = row;
    }
}

/// All ten 4x4 predictors. As in NEON's batched predictor, compute the shared
/// two- and three-tap averages once, then arrange the directional rows.
pub fn Intra4Preds_wide(dst: &mut [u8], t: &[u8; 13]) {
    assert!(dst.len() >= I4HU4 + 3 * BPS + 4);
    let mut avg2 = [0u8; 16];
    let mut avg3 = [0u8; 16];
    for base in [0, 8] {
        let a = i16x8::from(std::array::from_fn(|i| i16::from(t[(base + i).min(12)])));
        let b = i16x8::from(std::array::from_fn(|i| {
            i16::from(t[(base + i + 1).min(12)])
        }));
        let c = i16x8::from(std::array::from_fn(|i| {
            i16::from(t[(base + i + 2).min(12)])
        }));
        let two = (a + b + i16x8::splat(1)) >> 1;
        let three = (a + (b << 1) + c + i16x8::splat(2)) >> 2;
        let packed = u8x16::narrow_i16x8(two, three).to_array();
        *<&mut [u8; 8]>::try_from(&mut avg2[base..base + 8]).unwrap() =
            packed[..8].try_into().unwrap();
        *<&mut [u8; 8]>::try_from(&mut avg3[base..base + 8]).unwrap() =
            packed[8..].try_into().unwrap();
    }
    let l = ((3 * u16::from(t[0]) + u16::from(t[1]) + 2) >> 2) as u8;
    let dc_samples = i16x8::from([
        t[0].into(),
        t[1].into(),
        t[2].into(),
        t[3].into(),
        t[5].into(),
        t[6].into(),
        t[7].into(),
        t[8].into(),
    ]);
    let dc = ((dc_samples.reduce_add() + 4) >> 3) as u8;
    store4(&mut dst[I4DC4..], [[dc; 4]; 4]);
    let top = i16x8::from(std::array::from_fn(|i| i16::from(t[5 + i % 4])));
    for y in [0, 2] {
        let left = i16x8::from([
            t[3 - y].into(),
            t[3 - y].into(),
            t[3 - y].into(),
            t[3 - y].into(),
            t[2 - y].into(),
            t[2 - y].into(),
            t[2 - y].into(),
            t[2 - y].into(),
        ]);
        let row = top + left - i16x8::splat(t[4].into());
        let bytes = u8x16::narrow_i16x8(row, row).to_array();
        *<&mut [u8; 4]>::try_from(&mut dst[I4TM4 + y * BPS..I4TM4 + y * BPS + 4]).unwrap() =
            bytes[..4].try_into().unwrap();
        *<&mut [u8; 4]>::try_from(&mut dst[I4TM4 + (y + 1) * BPS..I4TM4 + (y + 1) * BPS + 4])
            .unwrap() = bytes[4..8].try_into().unwrap();
    }
    let v = [avg3[4], avg3[5], avg3[6], avg3[7]];
    store4(&mut dst[I4VE4..], [v; 4]);
    store4(
        &mut dst[I4HE4..],
        [[avg3[2]; 4], [avg3[1]; 4], [avg3[0]; 4], [l; 4]],
    );
    let [a, b, c, d, e, f, g] = std::array::from_fn(|i| avg3[i]);
    store4(
        &mut dst[I4RD4..],
        [[d, e, f, g], [c, d, e, f], [b, c, d, e], [a, b, c, d]],
    );
    let [a, b, c, d, e, f, g] = std::array::from_fn(|i| avg3[5 + i]);
    store4(
        &mut dst[I4LD4..],
        [[a, b, c, d], [b, c, d, e], [c, d, e, f], [d, e, f, g]],
    );
    let [a, b, c, d] = std::array::from_fn(|i| avg2[4 + i]);
    let [e, f, g, h, i, j] = std::array::from_fn(|i| avg3[1 + i]);
    store4(
        &mut dst[I4VR4..],
        [[a, b, c, d], [g, h, i, j], [f, a, b, c], [e, g, h, i]],
    );
    let [a, b, c, d] = std::array::from_fn(|i| avg2[5 + i]);
    let [e, f, g, h, i, j] = std::array::from_fn(|i| avg3[5 + i]);
    store4(
        &mut dst[I4VL4..],
        [[a, b, c, d], [e, f, g, h], [b, c, d, i], [f, g, h, j]],
    );
    let [a, b, c, d] = std::array::from_fn(|i| avg2[3 - i]);
    let [e, f, g, h, i, j] = std::array::from_fn(|i| avg3[5 - i]);
    store4(
        &mut dst[I4HD4..],
        [[a, g, f, e], [b, h, a, g], [c, i, b, h], [d, j, c, i]],
    );
    let [a, b, c] = [avg2[2], avg2[1], avg2[0]];
    let [d, e, f] = [avg3[1], avg3[0], l];
    let l = t[0];
    store4(
        &mut dst[I4HU4..],
        [[a, d, b, e], [b, e, c, f], [c, f, l, l], [l, l, l, l]],
    );
}

#[inline]
fn store_row<const N: usize>(dst: &mut [u8], row: &[u8; N]) {
    *<&mut [u8; N]>::try_from(&mut dst[..N]).unwrap() = *row;
}

#[inline]
fn predict_square<const N: usize>(
    dst: &mut [u8],
    left: Option<&[u8]>,
    top: Option<&[u8]>,
    offsets: [usize; 4],
) {
    let [dc_offset, ve_offset, he_offset, tm_offset] = offsets;
    let total = |s: &[u8]| -> u16 {
        let lo = load_pixels8(s);
        let sum = if N == 16 {
            lo + load_pixels8(&s[8..])
        } else {
            lo
        };
        sum.reduce_add() as u16
    };
    let dc = match (left, top) {
        (Some(l), Some(t)) => (total(&l[1..]) + total(t) + N as u16) / (2 * N) as u16,
        (Some(l), None) => (total(&l[1..]) + (N / 2) as u16) / N as u16,
        (None, Some(t)) => (total(t) + (N / 2) as u16) / N as u16,
        (None, None) => 128,
    } as u8;
    let vertical = top.map_or([127; N], |t| t[..N].try_into().unwrap());
    for y in 0..N {
        let horizontal = [left.map_or(129, |l| l[1 + y]); N];
        store_row(&mut dst[dc_offset + y * BPS..], &[dc; N]);
        store_row(&mut dst[ve_offset + y * BPS..], &vertical);
        store_row(&mut dst[he_offset + y * BPS..], &horizontal);
        match (left, top) {
            (Some(l), Some(t)) => {
                let delta = i16x8::splat(i16::from(l[1 + y]) - i16::from(l[0]));
                let lo = load_pixels8(t) + delta;
                let hi = if N == 16 {
                    load_pixels8(&t[8..]) + delta
                } else {
                    lo
                };
                let row = u8x16::narrow_i16x8(lo, hi).to_array();
                store_row(
                    &mut dst[tm_offset + y * BPS..],
                    <&[u8; N]>::try_from(&row[..N]).unwrap(),
                );
            }
            (Some(_), None) | (None, None) => {
                store_row(&mut dst[tm_offset + y * BPS..], &horizontal)
            }
            (None, Some(_)) => store_row(&mut dst[tm_offset + y * BPS..], &vertical),
        }
    }
}

pub fn Intra16Preds_wide(dst: &mut [u8], left: Option<&[u8]>, top: Option<&[u8]>) {
    assert!(dst.len() >= I16HE16 + 15 * BPS + 16);
    assert!(left.is_none_or(|v| v.len() >= 17) && top.is_none_or(|v| v.len() >= 16));
    predict_square::<16>(dst, left, top, [I16DC16, I16VE16, I16HE16, I16TM16]);
}

pub fn IntraChromaPreds_wide(dst: &mut [u8], left: Option<&[u8]>, top: Option<&[u8]>) {
    assert!(dst.len() >= C8HE8 + 7 * BPS + 16);
    assert!(left.is_none_or(|v| v.len() >= 25) && top.is_none_or(|v| v.len() >= 16));
    for channel in 0..2 {
        predict_square::<8>(
            &mut dst[8 * channel..],
            left.map(|v| &v[16 * channel..]),
            top.map(|v| &v[8 * channel..]),
            [C8DC8, C8VE8, C8HE8, C8TM8],
        );
    }
}
