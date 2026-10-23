// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/iterator_enc.c. See COPYING and PATENTS.

#![allow(non_upper_case_globals)]

use super::{picture_enc::WebPYUVAPicture, quant_enc::VP8Scan, vp8i_enc::*};
use crate::dsp::enc::BPS;

fn InitLeft(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let corner = if it.y > 0 { 129 } else { 127 };
    it.y_left[0] = corner;
    it.y_left[1..].fill(129);
    it.uv_left[0] = corner;
    it.uv_left[16] = corner;
    it.uv_left[1..9].fill(129);
    it.uv_left[17..25].fill(129);
    it.left_nz[8] = 0;
    if enc.top_derr.is_some() {
        it.left_derr = [[0; 2]; 2];
    }
}

fn InitTop(enc: &mut VP8Encoder) {
    enc.y_top.fill(127);
    enc.uv_top.fill(127);
    enc.nz[1..].fill(0);
    if let Some(derr) = &mut enc.top_derr {
        derr.fill([[0; 2]; 2]);
    }
}

pub fn VP8IteratorSetRow(it: &mut VP8EncIterator, enc: &VP8Encoder, y: usize) {
    assert!(y <= enc.mb_h);
    it.x = 0;
    it.y = y;
    it.source_boundary = None;
    InitLeft(it, enc);
}

pub fn VP8IteratorSetCountDown(it: &mut VP8EncIterator, count_down: i32) {
    it.count_down = count_down;
    it.count_down0 = count_down;
}

pub fn VP8IteratorIsDone(it: &VP8EncIterator) -> bool {
    it.count_down <= 0
}

pub fn VP8IteratorInit(enc: &mut VP8Encoder, it: &mut VP8EncIterator) {
    VP8IteratorSetRow(it, enc, 0);
    VP8IteratorSetCountDown(it, (enc.mb_w * enc.mb_h) as i32);
    InitTop(enc);
    it.bit_count = [[0; 3]; 4];
    it.do_trellis = false;
}

#[inline]
fn MinSize(a: usize, b: usize) -> usize {
    a.min(b)
}

fn ImportBlock(src: &[u8], stride: usize, dst: &mut [u8], w: usize, h: usize, size: usize) {
    for row in 0..h {
        dst[row * BPS..row * BPS + w].copy_from_slice(&src[row * stride..row * stride + w]);
        let last = dst[row * BPS + w - 1];
        dst[row * BPS + w..row * BPS + size].fill(last);
    }
    for row in h..size {
        let (previous, next) = dst.split_at_mut(row * BPS);
        next[..size].copy_from_slice(&previous[(row - 1) * BPS..(row - 1) * BPS + size]);
    }
}

fn ImportLine(src: &[u8], stride: usize, dst: &mut [u8], len: usize, total: usize) {
    for i in 0..len {
        dst[i] = src[i * stride];
    }
    let last = dst[len - 1];
    dst[len..total].fill(last);
}

/// `source_boundary` selects the analysis path's source samples. Each import
/// selects its own border storage; analysis scans must re-import before using
/// borders after Next, matching the upstream scratch-buffer lifetime.
pub fn VP8IteratorImport(
    it: &mut VP8EncIterator,
    enc: &VP8Encoder,
    pic: &WebPYUVAPicture,
    source_boundary: bool,
) {
    assert!(it.x < enc.mb_w && it.y < enc.mb_h);
    assert_eq!(
        (enc.mb_w, enc.mb_h),
        (
            (pic.width as usize).div_ceil(16),
            (pic.height as usize).div_ceil(16)
        )
    );
    let stride = pic.width as usize;
    let uv_stride = stride.div_ceil(2);
    let y_off = 16 * it.y * stride + 16 * it.x;
    let uv_off = 8 * it.y * uv_stride + 8 * it.x;
    let w = MinSize(stride - it.x * 16, 16);
    let h = MinSize(pic.height as usize - it.y * 16, 16);
    let (uw, uh) = (w.div_ceil(2), h.div_ceil(2));
    ImportBlock(
        &pic.y[y_off..],
        stride,
        &mut it.yuv_in[Y_OFF_ENC..],
        w,
        h,
        16,
    );
    ImportBlock(
        &pic.u[uv_off..],
        uv_stride,
        &mut it.yuv_in[U_OFF_ENC..],
        uw,
        uh,
        8,
    );
    ImportBlock(
        &pic.v[uv_off..],
        uv_stride,
        &mut it.yuv_in[V_OFF_ENC..],
        uw,
        uh,
        8,
    );
    it.source_boundary = None;
    if !source_boundary {
        return;
    }
    if it.x == 0 {
        InitLeft(it, enc);
    } else {
        it.y_left[0] = if it.y == 0 {
            127
        } else {
            pic.y[y_off - stride - 1]
        };
        it.uv_left[0] = if it.y == 0 {
            127
        } else {
            pic.u[uv_off - uv_stride - 1]
        };
        it.uv_left[16] = if it.y == 0 {
            127
        } else {
            pic.v[uv_off - uv_stride - 1]
        };
        ImportLine(&pic.y[y_off - 1..], stride, &mut it.y_left[1..], h, 16);
        ImportLine(
            &pic.u[uv_off - 1..],
            uv_stride,
            &mut it.uv_left[1..9],
            uh,
            8,
        );
        ImportLine(
            &pic.v[uv_off - 1..],
            uv_stride,
            &mut it.uv_left[17..],
            uh,
            8,
        );
    }
    let mut top = [127; 32];
    if it.y != 0 {
        ImportLine(&pic.y[y_off - stride..], 1, &mut top, w, 16);
        ImportLine(&pic.u[uv_off - uv_stride..], 1, &mut top[16..], uw, 8);
        ImportLine(&pic.v[uv_off - uv_stride..], 1, &mut top[24..], uw, 8);
    }
    it.source_boundary = Some(top);
}

pub fn VP8IteratorNzToBytes(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let (tnz, lnz) = (enc.nz[it.x + 1], enc.nz[it.x]);
    for (i, bit) in [12, 13, 14, 15, 18, 19, 22, 23, 24].into_iter().enumerate() {
        it.top_nz[i] = ((tnz >> bit) & 1) as u8;
    }
    for (i, bit) in [3, 7, 11, 15, 17, 19, 21, 23].into_iter().enumerate() {
        it.left_nz[i] = ((lnz >> bit) & 1) as u8;
    }
    // left_nz[8] is the separately propagated Intra16 DC context.
}

pub fn VP8IteratorBytesToNz(it: &VP8EncIterator, enc: &mut VP8Encoder) {
    let mut nz = 0;
    for (i, bit) in [12, 13, 14, 15, 18, 19, 22, 23, 24].into_iter().enumerate() {
        nz |= u32::from(it.top_nz[i]) << bit;
    }
    for (i, bit) in [(0, 3), (1, 7), (2, 11), (4, 17), (6, 21)] {
        nz |= u32::from(it.left_nz[i]) << bit;
    }
    enc.nz[it.x + 1] = nz;
}

pub fn VP8IteratorSaveBoundary(it: &mut VP8EncIterator, enc: &mut VP8Encoder) {
    if it.x + 1 < enc.mb_w {
        for i in 0..16 {
            it.y_left[i + 1] = it.yuv_out[15 + i * BPS];
        }
        for i in 0..8 {
            it.uv_left[i + 1] = it.yuv_out[U_OFF_ENC + 7 + i * BPS];
            it.uv_left[i + 17] = it.yuv_out[V_OFF_ENC + 7 + i * BPS];
        }
        // Capture corners before replacing the top with this block's output.
        it.y_left[0] = it.y_top(enc)[15];
        it.uv_left[0] = it.uv_top(enc)[7];
        it.uv_left[16] = it.uv_top(enc)[15];
    }
    if it.y + 1 < enc.mb_h {
        let y = &it.yuv_out[15 * BPS..15 * BPS + 16];
        let uv = &it.yuv_out[7 * BPS + U_OFF_ENC..7 * BPS + V_OFF_ENC + 8];
        if let Some(top) = &mut it.source_boundary {
            top[..16].copy_from_slice(y);
            top[16..].copy_from_slice(uv);
        } else {
            enc.y_top[16 * it.x..16 * it.x + 16].copy_from_slice(y);
            enc.uv_top[16 * it.x..16 * it.x + 16].copy_from_slice(uv);
        }
    }
}

pub fn VP8IteratorNext(it: &mut VP8EncIterator, enc: &VP8Encoder) -> bool {
    it.x += 1;
    if it.x == enc.mb_w {
        VP8IteratorSetRow(it, enc, it.y + 1);
    }
    it.source_boundary = None;
    it.count_down -= 1;
    it.count_down > 0
}

pub fn VP8SetIntra16Mode(it: &VP8EncIterator, enc: &mut VP8Encoder, mode: u8) {
    let start = enc.preds_offset(it.x, it.y);
    for y in 0..4 {
        enc.preds[start + y * enc.preds_w..start + y * enc.preds_w + 4].fill(mode);
    }
    enc.mb_info[it.y * enc.mb_w + it.x].r#type = 1;
}
pub fn VP8SetIntra4Mode(it: &VP8EncIterator, enc: &mut VP8Encoder, modes: &[u8; 16]) {
    let start = enc.preds_offset(it.x, it.y);
    for y in 0..4 {
        enc.preds[start + y * enc.preds_w..start + y * enc.preds_w + 4]
            .copy_from_slice(&modes[4 * y..4 * y + 4]);
    }
    enc.mb_info[it.y * enc.mb_w + it.x].r#type = 0;
}
pub fn VP8SetIntraUVMode(it: &VP8EncIterator, enc: &mut VP8Encoder, mode: u8) {
    enc.mb_info[it.y * enc.mb_w + it.x].uv_mode = mode;
}
pub fn VP8SetSkip(it: &VP8EncIterator, enc: &mut VP8Encoder, skip: bool) {
    enc.mb_info[it.y * enc.mb_w + it.x].skip = skip;
}
pub fn VP8SetSegment(it: &VP8EncIterator, enc: &mut VP8Encoder, segment: u8) {
    enc.mb_info[it.y * enc.mb_w + it.x].segment = segment;
}

const VP8TopLeftI4: [usize; 16] = [17, 21, 25, 29, 13, 17, 21, 25, 9, 13, 17, 21, 5, 9, 13, 17];

pub fn VP8IteratorStartI4(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    it.i4 = 0;
    it.i4_top = VP8TopLeftI4[0];
    for i in 0..17 {
        it.i4_boundary[i] = it.y_left[16 - i];
    }
    let top = it.y_top(enc);
    let mut samples = [0; 20];
    samples[..16].copy_from_slice(&top[..16]);
    if it.x + 1 < enc.mb_w {
        samples[16..].copy_from_slice(&top[16..20]);
    } else {
        samples[16..].fill(top[15]);
    }
    it.i4_boundary[17..].copy_from_slice(&samples);
    VP8IteratorNzToBytes(it, enc);
}

pub fn VP8IteratorRotateI4(it: &mut VP8EncIterator, yuv_out: &[u8]) -> bool {
    let blk = &yuv_out[VP8Scan[it.i4]..];
    let top = it.i4_top;
    for i in 0..4 {
        it.i4_boundary[top - 4 + i] = blk[i + 3 * BPS];
    }
    if it.i4 & 3 != 3 {
        for i in 0..3 {
            it.i4_boundary[top + i] = blk[3 + (2 - i) * BPS];
        }
    } else {
        for i in 0..4 {
            it.i4_boundary[top + i] = it.i4_boundary[top + i + 4];
        }
    }
    it.i4 += 1;
    if it.i4 == 16 {
        return false;
    }
    it.i4_top = VP8TopLeftI4[it.i4];
    true
}
