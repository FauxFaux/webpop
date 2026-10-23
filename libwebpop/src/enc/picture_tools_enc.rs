// Copyright 2012 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/picture_tools_enc.c and scalar AlphaReplace_C.
// See COPYING and PATENTS.

use super::picture_enc::WebPPicture;

pub(crate) fn WebPReplaceTransparentPixels(picture: &mut WebPPicture, color: u32) {
    let color = color & 0xffffff;
    for pixel in &mut picture.argb {
        if *pixel >> 24 == 0 {
            *pixel = color;
        }
    }
}

fn Flatten(plane: &mut [u8], value: u8, stride: usize, size: usize) {
    for row in 0..size {
        plane[row * stride..row * stride + size].fill(value);
    }
}

// A nonzero alpha contributes equally, irrespective of its magnitude.
fn SmoothenBlock(a: &[u8], y: &mut [u8], stride: usize, width: usize, height: usize) -> bool {
    let (mut sum, mut count) = (0, 0);
    for row in 0..height {
        for x in 0..width {
            let i = row * stride + x;
            if a[i] != 0 {
                sum += u32::from(y[i]);
                count += 1;
            }
        }
    }
    if count > 0 && count < (width * height) as u32 {
        let avg = (sum / count) as u8;
        for row in 0..height {
            for x in 0..width {
                let i = row * stride + x;
                if a[i] == 0 {
                    y[i] = avg;
                }
            }
        }
    }
    count == 0
}

/// The YUVA branch of upstream cleanup. Complete transparent 8x8 blocks share
/// a color over each horizontal run; partial edge blocks only smooth luma.
/// Alpha and the luma of every nontransparent pixel are preserved.
pub fn WebPCleanupTransparentArea(pic: &mut super::picture_enc::WebPYUVAPicture) {
    let Some(a) = &pic.a else {
        return;
    };
    let (width, height) = (pic.width as usize, pic.height as usize);
    let uv_stride = width.div_ceil(2);
    for row in (0..height).step_by(8) {
        let bh = (height - row).min(8);
        let mut values = None;
        for x in (0..width).step_by(8) {
            let bw = (width - x).min(8);
            let off = row * width + x;
            let transparent = SmoothenBlock(&a[off..], &mut pic.y[off..], width, bw, bh);
            if bw == 8 && bh == 8 {
                if transparent {
                    let uv_off = (row / 2) * uv_stride + x / 2;
                    let values = *values.get_or_insert([pic.y[off], pic.u[uv_off], pic.v[uv_off]]);
                    Flatten(&mut pic.y[off..], values[0], width, 8);
                    Flatten(&mut pic.u[uv_off..], values[1], uv_stride, 4);
                    Flatten(&mut pic.v[uv_off..], values[2], uv_stride, 4);
                } else {
                    values = None;
                }
            }
        }
    }
}
