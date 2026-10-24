// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/filters.c, encoder filters. See COPYING and PATENTS.

pub const WEBP_FILTER_NONE: usize = 0;
pub const WEBP_FILTER_HORIZONTAL: usize = 1;
pub const WEBP_FILTER_VERTICAL: usize = 2;
pub const WEBP_FILTER_GRADIENT: usize = 3;
pub const WEBP_FILTER_LAST: usize = 4;
pub const WEBP_FILTER_BEST: usize = 5;
pub const WEBP_FILTER_FAST: usize = 6;

pub fn plane_len(width: usize, height: usize, stride: usize) -> Option<usize> {
    if width == 0 || height == 0 || stride < width {
        return None;
    }
    (height - 1).checked_mul(stride)?.checked_add(width)
}
fn check(data: &[u8], width: usize, height: usize, stride: usize, out: &[u8]) {
    let len = plane_len(width, height, stride).expect("valid alpha geometry");
    assert!(data.len() >= len && out.len() >= len);
}
#[inline]
fn PredictLine_C(src: &[u8], pred: &[u8], dst: &mut [u8], length: usize) {
    for i in 0..length {
        dst[i] = src[i].wrapping_sub(pred[i]);
    }
}
#[inline]
pub fn DoHorizontalFilter_C(
    data: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    out: &mut [u8],
) {
    check(data, width, height, stride, out);
    out[0] = data[0];
    PredictLine_C(&data[1..], data, &mut out[1..], width - 1);
    for y in 1..height {
        let at = y * stride;
        PredictLine_C(&data[at..], &data[at - stride..], &mut out[at..], 1);
        PredictLine_C(&data[at + 1..], &data[at..], &mut out[at + 1..], width - 1);
    }
}
#[inline]
pub fn DoVerticalFilter_C(data: &[u8], width: usize, height: usize, stride: usize, out: &mut [u8]) {
    check(data, width, height, stride, out);
    out[0] = data[0];
    PredictLine_C(&data[1..], data, &mut out[1..], width - 1);
    for y in 1..height {
        let at = y * stride;
        PredictLine_C(&data[at..], &data[at - stride..], &mut out[at..], width);
    }
}
#[inline]
pub fn GradientPredictor_C(a: u8, b: u8, c: u8) -> u8 {
    (i32::from(a) + i32::from(b) - i32::from(c)).clamp(0, 255) as u8
}
#[inline]
pub fn DoGradientFilter_C(data: &[u8], width: usize, height: usize, stride: usize, out: &mut [u8]) {
    check(data, width, height, stride, out);
    out[0] = data[0];
    PredictLine_C(&data[1..], data, &mut out[1..], width - 1);
    for y in 1..height {
        let at = y * stride;
        PredictLine_C(&data[at..], &data[at - stride..], &mut out[at..], 1);
        for x in 1..width {
            out[at + x] = data[at + x].wrapping_sub(GradientPredictor_C(
                data[at + x - 1],
                data[at + x - stride],
                data[at + x - stride - 1],
            ));
        }
    }
}
pub fn HorizontalFilter_C(data: &[u8], width: usize, height: usize, stride: usize, out: &mut [u8]) {
    DoHorizontalFilter_C(data, width, height, stride, out);
}
pub fn VerticalFilter_C(data: &[u8], width: usize, height: usize, stride: usize, out: &mut [u8]) {
    DoVerticalFilter_C(data, width, height, stride, out);
}
pub fn GradientFilter_C(data: &[u8], width: usize, height: usize, stride: usize, out: &mut [u8]) {
    DoGradientFilter_C(data, width, height, stride, out);
}
pub fn WebPFilter(
    data: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    filter: usize,
    out: &mut [u8],
) {
    match filter {
        WEBP_FILTER_NONE => {
            check(data, width, height, stride, out);
            for y in 0..height {
                out[y * stride..y * stride + width]
                    .copy_from_slice(&data[y * stride..y * stride + width]);
            }
        }
        WEBP_FILTER_HORIZONTAL => HorizontalFilter_C(data, width, height, stride, out),
        WEBP_FILTER_VERTICAL => VerticalFilter_C(data, width, height, stride, out),
        WEBP_FILTER_GRADIENT => GradientFilter_C(data, width, height, stride, out),
        _ => panic!("invalid alpha filter"),
    }
}
