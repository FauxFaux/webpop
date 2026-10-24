//! Direct scalar port of `sharpyuv/sharpyuv_dsp.{c,h}`.
// Copyright 2022 Google Inc. All Rights Reserved.
// Use of this source code is governed by the BSD-style license in COPYING.
// An additional intellectual property rights grant is in PATENTS.

pub const SHARPYUV_YUV_FIX: u32 = 16;

use super::{
    SharpYuvTransferFunctionType::kSharpYuvTransferFunctionSrgb as SRGB,
    sharpyuv_gamma::{SharpYuvGammaToLinear, SharpYuvLinearToGamma},
};

#[inline]
pub fn SharpYuvClip16(v: i32, max: i32) -> u16 {
    v.clamp(0, max) as u16
}

#[inline]
pub fn SharpYuvRGBToGray(r: i64, g: i64, b: i64) -> i32 {
    ((13933 * r + 46871 * g + 4732 * b + (1 << 15)) >> 16) as i32
}

#[inline]
pub fn SharpYuvConvertComponent(r: i32, g: i32, b: i32, coeffs: &[i32; 4], sfix: i32) -> i32 {
    let shift = 16 + sfix;
    let luma = i64::from(coeffs[0]) * i64::from(r)
        + i64::from(coeffs[1]) * i64::from(g)
        + i64::from(coeffs[2]) * i64::from(b)
        + i64::from(coeffs[3])
        + (1i64 << (shift - 1));
    (luma >> shift) as i32
}

/// Internal kernels require slices covering the specified logical row length.
pub fn SharpYuvUpdateY_C(
    reference: &[u16],
    src: &[u16],
    dst: &mut [u16],
    len: usize,
    bit_depth: u32,
) -> u64 {
    let mut diff = 0;
    let max_y = (1 << bit_depth) - 1;
    for i in 0..len {
        let diff_y = i32::from(reference[i]) - i32::from(src[i]);
        dst[i] = SharpYuvClip16(i32::from(dst[i]) + diff_y, max_y);
        diff += u64::from(diff_y.unsigned_abs());
    }
    diff
}

pub fn SharpYuvUpdateRGB_C(reference: &[i16], src: &[i16], dst: &mut [i16], len: usize) {
    for i in 0..len {
        let diff_uv = i32::from(reference[i]) - i32::from(src[i]);
        // C promotes to int, adds, then narrows back to int16_t.
        dst[i] = (i32::from(dst[i]) + diff_uv) as i16;
    }
}

pub fn SharpYuvFilterRow_C(
    a: &[i16],
    b: &[i16],
    len: usize,
    best_y: &[u16],
    out: &mut [u16],
    bit_depth: u32,
) {
    let max_y = (1 << bit_depth) - 1;
    for i in 0..len {
        let [a0, a1, b0, b1] = [a[i], a[i + 1], b[i], b[i + 1]].map(i32::from);
        let v0 = (a0 * 9 + a1 * 3 + b0 * 3 + b1 + 8) >> 4;
        let v1 = (a1 * 9 + a0 * 3 + b1 * 3 + b0 + 8) >> 4;
        out[2 * i] = SharpYuvClip16(i32::from(best_y[2 * i]) + v0, max_y);
        out[2 * i + 1] = SharpYuvClip16(i32::from(best_y[2 * i + 1]) + v1, max_y);
    }
}

/// Output is expressed as logical samples for both 8-bit and high-depth rows.
pub fn SharpYuvConvertRowY_C(
    best_y: &[u16],
    best_uv: &[i16],
    width: usize,
    uv_w: usize,
    coeffs: &[i32; 4],
    sfix: i32,
    bit_depth: u32,
    out: &mut [u16],
) {
    let max_y = (1 << bit_depth) - 1;
    for i in 0..width {
        let off = i >> 1;
        let w = i32::from(best_y[i]);
        let r = i32::from(best_uv[off]) + w;
        let g = i32::from(best_uv[off + uv_w]) + w;
        let b = i32::from(best_uv[off + 2 * uv_w]) + w;
        out[i] = SharpYuvClip16(SharpYuvConvertComponent(r, g, b, coeffs, sfix), max_y);
    }
}

pub fn SharpYuvConvertRowUV_C(
    best_uv: &[i16],
    uv_w: usize,
    coeffs_u: &[i32; 4],
    coeffs_v: &[i32; 4],
    sfix: i32,
    bit_depth: u32,
    u_out: &mut [u16],
    v_out: &mut [u16],
) {
    let max_y = (1 << bit_depth) - 1;
    for i in 0..uv_w {
        let r = i32::from(best_uv[i]);
        let g = i32::from(best_uv[i + uv_w]);
        let b = i32::from(best_uv[i + 2 * uv_w]);
        u_out[i] = SharpYuvClip16(SharpYuvConvertComponent(r, g, b, coeffs_u, sfix), max_y);
        v_out[i] = SharpYuvClip16(SharpYuvConvertComponent(r, g, b, coeffs_v, sfix), max_y);
    }
}

pub fn SharpYuvUpdateWSrgb_C(src: &[u16], dst: &mut [u16], w: usize, bit_depth: u32) {
    for i in 0..w {
        let r = SharpYuvGammaToLinear(src[i], bit_depth, SRGB);
        let g = SharpYuvGammaToLinear(src[w + i], bit_depth, SRGB);
        let b = SharpYuvGammaToLinear(src[2 * w + i], bit_depth, SRGB);
        let y = SharpYuvRGBToGray(i64::from(r), i64::from(g), i64::from(b));
        dst[i] = SharpYuvLinearToGamma(y as u32, bit_depth, SRGB);
    }
}

pub fn ScaleDownSrgb_C(a: u16, b: u16, c: u16, d: u16, bit_depth: u32) -> u32 {
    let a = SharpYuvGammaToLinear(a, bit_depth, SRGB);
    let b = SharpYuvGammaToLinear(b, bit_depth, SRGB);
    let c = SharpYuvGammaToLinear(c, bit_depth, SRGB);
    let d = SharpYuvGammaToLinear(d, bit_depth, SRGB);
    u32::from(SharpYuvLinearToGamma(
        (a + b + c + d + 2) >> 2,
        bit_depth,
        SRGB,
    ))
}

pub fn SharpYuvUpdateChromaSrgb_C(
    src1: &[u16],
    src2: &[u16],
    dst: &mut [i16],
    uv_w: usize,
    bit_depth: u32,
) {
    for i in 0..uv_w {
        let rgb: [i32; 3] = std::array::from_fn(|k| {
            let off = 2 * (k * uv_w + i);
            ScaleDownSrgb_C(
                src1[off],
                src1[off + 1],
                src2[off],
                src2[off + 1],
                bit_depth,
            ) as i32
        });
        let [r, g, b] = rgb;
        let w = SharpYuvRGBToGray(i64::from(r), i64::from(g), i64::from(b));
        for k in 0..3 {
            dst[k * uv_w + i] = (rgb[k] - w) as i16;
        }
    }
}

// Scalar selection replaces SharpYuvInitDsp's mutable dispatch table.
pub use SharpYuvConvertRowUV_C as SharpYuvConvertRowUV;
pub use SharpYuvConvertRowY_C as SharpYuvConvertRowY;
pub use SharpYuvFilterRow_C as SharpYuvFilterRow;
pub use SharpYuvUpdateChromaSrgb_C as SharpYuvUpdateChromaSrgb;
pub use SharpYuvUpdateRGB_C as SharpYuvUpdateRGB;
pub use SharpYuvUpdateWSrgb_C as SharpYuvUpdateWSrgb;
pub use SharpYuvUpdateY_C as SharpYuvUpdateY;
