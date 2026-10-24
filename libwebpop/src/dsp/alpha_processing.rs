// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/dsp/alpha_processing.c. See COPYING and PATENTS.

/// Alpha is stored in green, with A/R/B all zero for the headerless VP8L image.
pub fn DispatchAlphaToGreen_C(
    alpha: &[u8],
    alpha_stride: usize,
    width: usize,
    height: usize,
    dst: &mut [u32],
    dst_stride: usize,
) {
    assert!(width > 0 && height > 0 && alpha_stride >= width && dst_stride >= width);
    assert!(
        super::filters::plane_len(width, height, alpha_stride).is_some_and(|n| alpha.len() >= n)
    );
    assert!(super::filters::plane_len(width, height, dst_stride).is_some_and(|n| dst.len() >= n));
    for y in 0..height {
        for x in 0..width {
            dst[y * dst_stride + x] = u32::from(alpha[y * alpha_stride + x]) << 8;
        }
    }
}
