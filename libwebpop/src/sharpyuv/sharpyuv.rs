//! Direct port of `sharpyuv/sharpyuv.c` and its public options.
// Copyright 2022 Google Inc. All Rights Reserved.
// Use of this source code is governed by the BSD-style license in COPYING.
// An additional intellectual property rights grant is in PATENTS.

use super::{sharpyuv_dsp::*, sharpyuv_gamma::*, *};
use SharpYuvTransferFunctionType::kSharpYuvTransferFunctionSrgb as SRGB;

pub const SHARPYUV_VERSION: u32 = (4 << 16) | 2;
const NUM_ITERATIONS: u32 = 4;
const MAX_BIT_DEPTH: i32 = 14;

pub fn SharpYuvGetVersion() -> u32 {
    SHARPYUV_VERSION
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharpYuvOptions {
    pub yuv_matrix: SharpYuvConversionMatrix,
    pub transfer_type: SharpYuvTransferFunctionType,
}

pub fn SharpYuvOptionsInitInternal(
    matrix: &SharpYuvConversionMatrix,
    version: u32,
) -> Result<SharpYuvOptions, SharpYuvError> {
    let major = version >> 24;
    let minor = (version >> 16) & 0xff;
    if major != 0 || minor != 4 {
        return Err(SharpYuvError::InvalidVersion);
    }
    Ok(SharpYuvOptions {
        yuv_matrix: *matrix,
        transfer_type: SRGB,
    })
}

#[inline]
pub fn SharpYuvOptionsInit(matrix: &SharpYuvConversionMatrix) -> SharpYuvOptions {
    SharpYuvOptions {
        yuv_matrix: *matrix,
        transfer_type: SRGB,
    }
}

pub fn GetPrecisionShift(bit_depth: u32) -> i32 {
    if bit_depth as i32 + 2 <= MAX_BIT_DEPTH {
        2
    } else {
        MAX_BIT_DEPTH - bit_depth as i32
    }
}

pub fn clip_bit_depth(y: i32, bit_depth: u32) -> u16 {
    y.clamp(0, (1 << bit_depth) - 1) as u16
}

pub fn ScaleDown(
    a: u16,
    b: u16,
    c: u16,
    d: u16,
    depth: u32,
    tf: SharpYuvTransferFunctionType,
) -> u32 {
    let a = SharpYuvGammaToLinear(a, depth, tf);
    let b = SharpYuvGammaToLinear(b, depth, tf);
    let c = SharpYuvGammaToLinear(c, depth, tf);
    let d = SharpYuvGammaToLinear(d, depth, tf);
    u32::from(SharpYuvLinearToGamma((a + b + c + d + 2) >> 2, depth, tf))
}

#[inline]
pub fn UpdateW(
    src: &[u16],
    dst: &mut [u16],
    w: usize,
    depth: u32,
    tf: SharpYuvTransferFunctionType,
) {
    if tf == SRGB {
        SharpYuvUpdateWSrgb(src, dst, w, depth);
        return;
    }
    for i in 0..w {
        let r = SharpYuvGammaToLinear(src[i], depth, tf);
        let g = SharpYuvGammaToLinear(src[w + i], depth, tf);
        let b = SharpYuvGammaToLinear(src[2 * w + i], depth, tf);
        let y = SharpYuvRGBToGray(i64::from(r), i64::from(g), i64::from(b));
        dst[i] = SharpYuvLinearToGamma(y as u32, depth, tf);
    }
}

pub fn UpdateChroma(
    src1: &[u16],
    src2: &[u16],
    dst: &mut [i16],
    uv_w: usize,
    depth: u32,
    tf: SharpYuvTransferFunctionType,
) {
    if tf == SRGB {
        SharpYuvUpdateChromaSrgb(src1, src2, dst, uv_w, depth);
        return;
    }
    for i in 0..uv_w {
        let rgb: [i32; 3] = std::array::from_fn(|k| {
            let off = 2 * (k * uv_w + i);
            ScaleDown(
                src1[off],
                src1[off + 1],
                src2[off],
                src2[off + 1],
                depth,
                tf,
            ) as i32
        });
        let [r, g, b] = rgb;
        let w = SharpYuvRGBToGray(i64::from(r), i64::from(g), i64::from(b));
        for k in 0..3 {
            dst[k * uv_w + i] = (rgb[k] - w) as i16;
        }
    }
}

pub fn StoreGray(rgb: &[u16], y: &mut [u16], w: usize) {
    for i in 0..w {
        y[i] = SharpYuvRGBToGray(
            i64::from(rgb[i]),
            i64::from(rgb[w + i]),
            i64::from(rgb[2 * w + i]),
        ) as u16;
    }
}

#[inline]
pub fn Filter2(a: i32, b: i32, w0: i32, depth: u32) -> u16 {
    clip_bit_depth(((a * 3 + b + 2) >> 2) + w0, depth)
}

#[inline]
fn Shift(v: i32, shift: i32) -> i32 {
    if shift >= 0 { v << shift } else { v >> -shift }
}

impl SharpYuvRgb<'_> {
    fn dimensions(&self) -> (usize, usize) {
        match self {
            Self::U8(v) => (v.width as usize, v.height as usize),
            Self::U16(v) => (v.width as usize, v.height as usize),
        }
    }
}

pub fn ImportOneRow(input: SharpYuvRgb<'_>, row: usize, depth: u32, dst: &mut [u16]) {
    let (width, _) = input.dimensions();
    let w = (width + 1) & !1;
    let shift = GetPrecisionShift(depth);
    let max = (1 << depth) - 1;
    match input {
        SharpYuvRgb::U8(v) => {
            for (k, src) in [v.r, v.g, v.b].into_iter().enumerate() {
                for i in 0..width {
                    dst[k * w + i] =
                        Shift(i32::from(src[row * v.stride + i * v.step]), shift) as u16;
                }
            }
        }
        SharpYuvRgb::U16(v) => {
            for (k, src) in [v.r, v.g, v.b].into_iter().enumerate() {
                for i in 0..width {
                    dst[k * w + i] =
                        Shift(i32::from(src[row * v.stride + i * v.step]).min(max), shift) as u16;
                }
            }
        }
    }
    if width & 1 != 0 {
        for k in 0..3 {
            dst[k * w + width] = dst[k * w + width - 1];
        }
    }
}

pub fn InterpolateTwoRows(
    best_y: &[u16],
    prev_uv: &[i16],
    cur_uv: &[i16],
    next_uv: &[i16],
    w: usize,
    out1: &mut [u16],
    out2: &mut [u16],
    depth: u32,
) {
    let uv_w = w >> 1;
    let len = (w - 1) >> 1;
    for k in 0..3 {
        let prev = &prev_uv[k * uv_w..];
        let cur = &cur_uv[k * uv_w..];
        let next = &next_uv[k * uv_w..];
        let out1 = &mut out1[k * w..];
        let out2 = &mut out2[k * w..];
        out1[0] = Filter2(
            i32::from(cur[0]),
            i32::from(prev[0]),
            i32::from(best_y[0]),
            depth,
        );
        out2[0] = Filter2(
            i32::from(cur[0]),
            i32::from(next[0]),
            i32::from(best_y[w]),
            depth,
        );
        SharpYuvFilterRow(cur, prev, len, &best_y[1..], &mut out1[1..], depth);
        SharpYuvFilterRow(cur, next, len, &best_y[w + 1..], &mut out2[1..], depth);
        if w & 1 == 0 {
            out1[w - 1] = Filter2(
                i32::from(cur[uv_w - 1]),
                i32::from(prev[uv_w - 1]),
                i32::from(best_y[w - 1]),
                depth,
            );
            out2[w - 1] = Filter2(
                i32::from(cur[uv_w - 1]),
                i32::from(next[uv_w - 1]),
                i32::from(best_y[2 * w - 1]),
                depth,
            );
        }
    }
}

pub fn ConvertWRGBToYUV(
    best_y: &[u16],
    best_uv: &[i16],
    width: usize,
    height: usize,
    rgb_depth: u32,
    yuv_depth: u32,
    matrix: &SharpYuvConversionMatrix,
) -> Result<SharpYuvPlanes<u16>, SharpYuvError> {
    let w = (width + 1) & !1;
    let uv_w = w / 2;
    let uv_h = height.div_ceil(2);
    let sfix = GetPrecisionShift(rgb_depth);
    let mut y = allocate(
        width
            .checked_mul(height)
            .ok_or(SharpYuvError::SizeOverflow)?,
    )?;
    let mut u = allocate(uv_w.checked_mul(uv_h).ok_or(SharpYuvError::SizeOverflow)?)?;
    let mut v = allocate(u.len())?;
    for j in 0..height {
        SharpYuvConvertRowY(
            &best_y[j * w..],
            &best_uv[(j / 2) * 3 * uv_w..],
            width,
            uv_w,
            &matrix.rgb_to_y,
            sfix,
            yuv_depth,
            &mut y[j * width..],
        );
    }
    for j in 0..uv_h {
        SharpYuvConvertRowUV(
            &best_uv[j * 3 * uv_w..],
            uv_w,
            &matrix.rgb_to_u,
            &matrix.rgb_to_v,
            sfix,
            yuv_depth,
            &mut u[j * uv_w..],
            &mut v[j * uv_w..],
        );
    }
    Ok(SharpYuvPlanes {
        width: width as u32,
        height: height as u32,
        y,
        u,
        v,
    })
}

/// Borrowed diagnostics for differential tests, emitted initially and after
/// each complete iteration. Normal conversion uses a no-op observer.
#[doc(hidden)]
pub struct SharpYuvIteration<'a> {
    pub iteration: Option<u32>,
    pub diff_y_sum: u64,
    pub best_y: &'a [u16],
    pub target_y: &'a [u16],
    pub best_uv: &'a [i16],
    pub target_uv: &'a [i16],
}

fn DoSharpArgbToYuv(
    input: SharpYuvRgb<'_>,
    rgb_depth: u32,
    yuv_depth: u32,
    matrix: &SharpYuvConversionMatrix,
    tf: SharpYuvTransferFunctionType,
    mut observe: impl FnMut(SharpYuvIteration<'_>),
) -> Result<SharpYuvPlanes<u16>, SharpYuvError> {
    let (width, height) = input.dimensions();
    let w = (width + 1) & !1;
    let h = (height + 1) & !1;
    let pixels = w.checked_mul(h).ok_or(SharpYuvError::SizeOverflow)?;
    let uv_w = w / 2;
    let uv_size = uv_w
        .checked_mul(3)
        .and_then(|n| n.checked_mul(h / 2))
        .ok_or(SharpYuvError::SizeOverflow)?;
    let scratch_size = w.checked_mul(6).ok_or(SharpYuvError::SizeOverflow)?;
    let y_depth = (rgb_depth as i32 + GetPrecisionShift(rgb_depth)) as u32;
    let mut tmp_buffer = allocate::<u16>(scratch_size)?;
    let mut best_y = allocate::<u16>(pixels)?;
    let mut target_y = allocate::<u16>(pixels)?;
    let mut best_rgb_y = allocate::<u16>(2 * w)?;
    let mut best_uv = allocate::<i16>(uv_size)?;
    let mut target_uv = allocate::<i16>(uv_size)?;
    let mut best_rgb_uv = allocate::<i16>(3 * uv_w)?;
    let (src1, src2) = tmp_buffer.split_at_mut(3 * w);
    for j in (0..height).step_by(2) {
        ImportOneRow(input, j, rgb_depth, src1);
        if j == height - 1 {
            src2.copy_from_slice(src1);
        } else {
            ImportOneRow(input, j + 1, rgb_depth, src2);
        }
        StoreGray(src1, &mut best_y[j * w..], w);
        StoreGray(src2, &mut best_y[(j + 1) * w..], w);
        UpdateW(src1, &mut target_y[j * w..], w, y_depth, tf);
        UpdateW(src2, &mut target_y[(j + 1) * w..], w, y_depth, tf);
        let uv = j / 2 * 3 * uv_w;
        UpdateChroma(src1, src2, &mut target_uv[uv..], uv_w, y_depth, tf);
        best_uv[uv..uv + 3 * uv_w].copy_from_slice(&target_uv[uv..uv + 3 * uv_w]);
    }
    observe(SharpYuvIteration {
        iteration: None,
        diff_y_sum: 0,
        best_y: &best_y,
        target_y: &target_y,
        best_uv: &best_uv,
        target_uv: &target_uv,
    });
    let mut prev_diff_y_sum = u64::MAX;
    let diff_y_threshold = 3 * pixels as u64;
    for iter in 0..NUM_ITERATIONS {
        let mut diff_y_sum = 0;
        for j in (0..h).step_by(2) {
            let uv = j / 2 * 3 * uv_w;
            let prev = if j == 0 { uv } else { uv - 3 * uv_w };
            let next = if j < h - 2 { uv + 3 * uv_w } else { uv };
            // These borrow the current plane: previous rows already contain
            // this iteration's updates, and subsequent rows still contain old UV.
            InterpolateTwoRows(
                &best_y[j * w..],
                &best_uv[prev..],
                &best_uv[uv..],
                &best_uv[next..],
                w,
                src1,
                src2,
                y_depth,
            );
            UpdateW(src1, &mut best_rgb_y, w, y_depth, tf);
            UpdateW(src2, &mut best_rgb_y[w..], w, y_depth, tf);
            UpdateChroma(src1, src2, &mut best_rgb_uv, uv_w, y_depth, tf);
            diff_y_sum += SharpYuvUpdateY(
                &target_y[j * w..],
                &best_rgb_y,
                &mut best_y[j * w..],
                2 * w,
                y_depth,
            );
            SharpYuvUpdateRGB(&target_uv[uv..], &best_rgb_uv, &mut best_uv[uv..], 3 * uv_w);
        }
        observe(SharpYuvIteration {
            iteration: Some(iter),
            diff_y_sum,
            best_y: &best_y,
            target_y: &target_y,
            best_uv: &best_uv,
            target_uv: &target_uv,
        });
        if diff_y_sum < diff_y_threshold {
            break;
        }
        if iter > 0 && diff_y_sum > prev_diff_y_sum {
            break;
        }
        prev_diff_y_sum = diff_y_sum;
    }
    ConvertWRGBToYUV(
        &best_y, &best_uv, width, height, rgb_depth, yuv_depth, matrix,
    )
}

fn scaled_matrix(
    matrix: &SharpYuvConversionMatrix,
    rgb_depth: u32,
    yuv_depth: u32,
) -> Result<SharpYuvConversionMatrix, SharpYuvError> {
    let mut scaled = *matrix;
    let rgb_max = (1i64 << rgb_depth) - 1;
    let rgb_round = 1i64 << (rgb_depth - 1);
    let yuv_max = (1i64 << yuv_depth) - 1;
    let shift = GetPrecisionShift(rgb_depth);
    for row in [
        &mut scaled.rgb_to_y,
        &mut scaled.rgb_to_u,
        &mut scaled.rgb_to_v,
    ] {
        if rgb_depth != yuv_depth {
            for c in &mut row[..3] {
                // Reject the input if C's signed int multiplication/addition
                // would overflow, rather than emulate undefined behavior.
                let product = i32::try_from(i64::from(*c) * yuv_max)
                    .map_err(|_| SharpYuvError::InvalidMatrix)?;
                let numerator = i64::from(product) + rgb_round;
                let numerator =
                    i32::try_from(numerator).map_err(|_| SharpYuvError::InvalidMatrix)?;
                *c = (i64::from(numerator) / rgb_max) as i32;
            }
        }
        let offset = if shift >= 0 {
            i64::from(row[3]) << shift
        } else {
            i64::from(row[3]) >> -shift
        };
        row[3] = i32::try_from(offset).map_err(|_| SharpYuvError::InvalidMatrix)?;
        // Bound the final component cast to int for every possible W+UV.
        let max_rgb = (1i64 << (rgb_depth as i32 + shift)) - 1 + 32768;
        let bound = row[..3]
            .iter()
            .map(|&v| i64::from(v).abs() * max_rgb)
            .sum::<i64>()
            + i64::from(row[3]).abs()
            + (1i64 << (15 + shift));
        if (bound >> (16 + shift)) > i64::from(i32::MAX) {
            return Err(SharpYuvError::InvalidMatrix);
        }
    }
    Ok(scaled)
}

/// Default sRGB conversion. Step/stride units are samples; alpha is ignored.
pub fn SharpYuvConvert(
    input: SharpYuvRgb<'_>,
    rgb_bit_depth: u32,
    yuv_bit_depth: u32,
    matrix: &SharpYuvConversionMatrix,
) -> Result<SharpYuvOutput, SharpYuvError> {
    SharpYuvConvertWithOptions(
        input,
        rgb_bit_depth,
        yuv_bit_depth,
        &SharpYuvOptionsInit(matrix),
    )
}

pub fn SharpYuvConvertWithOptions(
    input: SharpYuvRgb<'_>,
    rgb_bit_depth: u32,
    yuv_bit_depth: u32,
    options: &SharpYuvOptions,
) -> Result<SharpYuvOutput, SharpYuvError> {
    convert_with_observer(input, rgb_bit_depth, yuv_bit_depth, options, |_| {})
}

#[doc(hidden)]
pub fn convert_with_observer(
    input: SharpYuvRgb<'_>,
    rgb_bit_depth: u32,
    yuv_bit_depth: u32,
    options: &SharpYuvOptions,
    observe: impl FnMut(SharpYuvIteration<'_>),
) -> Result<SharpYuvOutput, SharpYuvError> {
    if !matches!(rgb_bit_depth, 8 | 10 | 12 | 16) {
        return Err(SharpYuvError::InvalidRgbBitDepth);
    }
    if !matches!(yuv_bit_depth, 8 | 10 | 12) {
        return Err(SharpYuvError::InvalidYuvBitDepth);
    }
    match input {
        SharpYuvRgb::U8(v) => {
            if rgb_bit_depth != 8 {
                return Err(SharpYuvError::InvalidStorage);
            }
            v.validate()?;
        }
        SharpYuvRgb::U16(v) => {
            if rgb_bit_depth == 8 {
                return Err(SharpYuvError::InvalidStorage);
            }
            v.validate()?;
        }
    }
    let matrix = scaled_matrix(&options.yuv_matrix, rgb_bit_depth, yuv_bit_depth)?;
    // SharpYuvInit is replaced by immutable OnceLock tables and static DSP.
    SharpYuvInitGammaTables();
    let planes = DoSharpArgbToYuv(
        input,
        rgb_bit_depth,
        yuv_bit_depth,
        &matrix,
        options.transfer_type,
        observe,
    )?;
    if yuv_bit_depth == 8 {
        fn pack(src: &[u16]) -> Result<Vec<u8>, SharpYuvError> {
            let mut out = allocate::<u8>(src.len())?;
            for (out, &v) in out.iter_mut().zip(src) {
                *out = v as u8;
            }
            Ok(out)
        }
        Ok(SharpYuvOutput::U8(SharpYuvPlanes {
            width: planes.width,
            height: planes.height,
            y: pack(&planes.y)?,
            u: pack(&planes.u)?,
            v: pack(&planes.v)?,
        }))
    } else {
        Ok(SharpYuvOutput::U16(planes))
    }
}
