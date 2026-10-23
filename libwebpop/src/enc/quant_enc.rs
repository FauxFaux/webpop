// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/quant_enc.c, default scalar quantization/search.
// See COPYING and PATENTS.
#![allow(non_upper_case_globals)]

use super::vp8i_enc::*;
use super::{cost_enc::*, iterator_enc::*};
use crate::dsp::quant::{IsFlat, IsFlatSource16};
use crate::dsp::{dec::TransformWHT_C, enc::*};

pub const kWeightY: [u16; 16] = [38, 32, 20, 9, 32, 28, 17, 7, 20, 17, 10, 4, 9, 7, 4, 2];

pub fn InitScore(rd: &mut VP8ModeScore) {
    rd.D = 0;
    rd.SD = 0;
    rd.R = 0;
    rd.H = 0;
    rd.nz = 0;
    rd.score = MAX_COST;
}
pub fn CopyScore(dst: &mut VP8ModeScore, src: &VP8ModeScore) {
    dst.D = src.D;
    dst.SD = src.SD;
    dst.R = src.R;
    dst.H = src.H;
    dst.nz = src.nz;
    dst.score = src.score;
}
pub fn AddScore(dst: &mut VP8ModeScore, src: &VP8ModeScore) {
    dst.D += src.D;
    dst.SD += src.SD;
    dst.R += src.R;
    dst.H += src.H;
    dst.nz |= src.nz;
    dst.score += src.score;
}
#[inline]
pub fn SetRDScore(lambda: i32, rd: &mut VP8ModeScore) {
    rd.score = (rd.R + rd.H) * i64::from(lambda) + 256 * (rd.D + rd.SD);
}

pub fn PickBestIntra16(it: &mut VP8EncIterator, enc: &mut VP8Encoder, rd: &mut VP8ModeScore) {
    let segment = usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment);
    let (lambda, tlambda) = (enc.dqm[segment].lambda_i16, enc.dqm[segment].tlambda);
    let mut flat = IsFlatSource16(&it.yuv_in);
    rd.mode_i16 = -1;
    for mode in 0..4 {
        let mut cur = VP8ModeScore::default();
        let mut dst = it.yuv_out2;
        cur.mode_i16 = mode as i32;
        cur.nz = ReconstructIntra16(it, enc, &mut cur, &mut dst, mode);
        it.yuv_out2 = dst;
        cur.D = i64::from(SSE16x16_C(&it.yuv_in, &dst));
        cur.SD = if tlambda != 0 {
            i64::from((tlambda * Disto16x16_C(&it.yuv_in, &dst, &kWeightY) + 128) >> 8)
        } else {
            0
        };
        cur.H = i64::from(VP8FixedCostsI16[mode]);
        cur.R = i64::from(VP8GetCostLuma16(it, enc, &cur));
        if flat {
            flat = IsFlat(&cur.y_ac_levels, 0);
            if flat {
                cur.D *= 2;
                cur.SD *= 2;
            }
        }
        SetRDScore(lambda, &mut cur);
        if mode == 0 || cur.score < rd.score {
            CopyScore(rd, &cur);
            rd.mode_i16 = cur.mode_i16;
            rd.y_dc_levels = cur.y_dc_levels;
            rd.y_ac_levels = cur.y_ac_levels;
            std::mem::swap(&mut it.yuv_out, &mut it.yuv_out2);
        }
    }
    SetRDScore(enc.dqm[segment].lambda_mode, rd);
    VP8SetIntra16Mode(it, enc, rd.mode_i16 as u8);
    if rd.nz & 0x100ffff == 0x1000000 && rd.D > i64::from(enc.dqm[segment].min_disto) {
        StoreMaxDelta(&mut enc.dqm[segment], &rd.y_dc_levels);
    }
}

pub fn GetCostModeI4(
    it: &VP8EncIterator,
    enc: &VP8Encoder,
    modes: &[u8; 16],
) -> &'static [u16; 10] {
    let (x, y) = (it.i4 & 3, it.i4 >> 2);
    let base = enc.preds_offset(it.x, it.y);
    let left = if x == 0 {
        enc.preds[base + y * enc.preds_w - 1]
    } else {
        modes[it.i4 - 1]
    };
    let top = if y == 0 {
        enc.preds[base - enc.preds_w + x]
    } else {
        modes[it.i4 - 4]
    };
    &VP8FixedCostsI4[usize::from(top)][usize::from(left)]
}

pub fn PickBestIntra4(
    it: &mut VP8EncIterator,
    enc: &mut VP8Encoder,
    rd: &mut VP8ModeScore,
) -> bool {
    if enc.max_i4_header_bits == 0 {
        return false;
    }
    let segment = usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment);
    let (lambda, tlambda, lambda_mode) = (
        enc.dqm[segment].lambda_i4,
        enc.dqm[segment].tlambda,
        enc.dqm[segment].lambda_mode,
    );
    let mut best = VP8ModeScore::default();
    InitScore(&mut best);
    best.H = 211;
    SetRDScore(lambda_mode, &mut best);
    let mut total_header = 0;
    VP8IteratorStartI4(it, enc);
    loop {
        let offset = VP8Scan[it.i4];
        let costs = GetCostModeI4(it, enc, &rd.modes_i4);
        let mut block_best = VP8ModeScore::default();
        InitScore(&mut block_best);
        let mut best_mode = None;
        // C swaps pointers between the prediction scratch and the candidate
        // block. Track those two owned destinations explicitly, including writes
        // from trials that lose or trigger an early return.
        let mut tmp_in_scratch = true;
        let mut best_in_scratch = false;
        MakeIntra4Preds(it);
        for mode in 0..10 {
            let mut cur = VP8ModeScore::default();
            let mut levels = [0i16; 16];
            let mut dst = [0u8; 3 * BPS + 4];
            cur.nz = u32::from(ReconstructIntra4(
                it,
                enc,
                &mut levels,
                &it.yuv_in[offset..],
                &mut dst,
                mode,
            )) << it.i4;
            if tmp_in_scratch {
                Copy4x4_C(&dst, &mut it.yuv_p[I4TMP..]);
            } else {
                Copy4x4_C(&dst, &mut it.yuv_out2[offset..]);
            }
            cur.D = i64::from(SSE4x4_C(&it.yuv_in[offset..], &dst));
            cur.SD = if tlambda != 0 {
                i64::from((tlambda * Disto4x4_C(&it.yuv_in[offset..], &dst, &kWeightY) + 128) >> 8)
            } else {
                0
            };
            cur.H = i64::from(costs[mode]);
            cur.R = if mode > 0 && IsFlat(&[levels], 3) {
                140
            } else {
                0
            };
            SetRDScore(lambda, &mut cur);
            if best_mode.is_some() && cur.score >= block_best.score {
                continue;
            }
            cur.R += i64::from(VP8GetCostLuma4(it, enc, &levels));
            SetRDScore(lambda, &mut cur);
            if best_mode.is_none() || cur.score < block_best.score {
                CopyScore(&mut block_best, &cur);
                best_mode = Some(mode);
                best_in_scratch = tmp_in_scratch;
                tmp_in_scratch = !tmp_in_scratch;
                best.y_ac_levels[it.i4] = levels;
            }
        }
        SetRDScore(lambda_mode, &mut block_best);
        AddScore(&mut best, &block_best);
        if best.score >= rd.score {
            return false;
        }
        total_header += block_best.H as i32;
        if total_header > enc.max_i4_header_bits {
            return false;
        }
        if best_in_scratch {
            let mut block = [0; 3 * BPS + 4];
            Copy4x4_C(&it.yuv_p[I4TMP..], &mut block);
            Copy4x4_C(&block, &mut it.yuv_out2[offset..]);
        }
        rd.modes_i4[it.i4] = best_mode.unwrap() as u8;
        it.top_nz[it.i4 & 3] = u8::from(block_best.nz != 0);
        it.left_nz[it.i4 >> 2] = u8::from(block_best.nz != 0);
        let pixels = it.yuv_out2;
        if !VP8IteratorRotateI4(it, &pixels) {
            break;
        }
    }
    CopyScore(rd, &best);
    rd.y_ac_levels = best.y_ac_levels;
    VP8SetIntra4Mode(it, enc, &rd.modes_i4);
    std::mem::swap(&mut it.yuv_out, &mut it.yuv_out2);
    true
}

pub fn PickBestUV(it: &mut VP8EncIterator, enc: &mut VP8Encoder, rd: &mut VP8ModeScore) {
    let segment = usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment);
    let lambda = enc.dqm[segment].lambda_uv;
    let mut best = VP8ModeScore::default();
    InitScore(&mut best);
    let mut tmp_in_out2 = true;
    let mut best_in_out2 = false;
    rd.mode_uv = -1;
    for mode in 0..4 {
        let mut cur = VP8ModeScore::default();
        let mut dst = [0u8; 7 * BPS + 16];
        cur.nz = ReconstructUV(it, enc, &mut cur, &mut dst, mode);
        if tmp_in_out2 {
            Copy16x8_C(&dst, &mut it.yuv_out2[U_OFF_ENC..]);
        } else {
            Copy16x8_C(&dst, &mut it.yuv_out[U_OFF_ENC..]);
        }
        cur.D = i64::from(SSE16x8_C(&it.yuv_in[U_OFF_ENC..], &dst));
        cur.SD = 0;
        cur.H = i64::from(VP8FixedCostsUV[mode]);
        cur.R = i64::from(VP8GetCostUV(it, enc, &cur));
        if mode > 0 && IsFlat(&cur.uv_levels, 2) {
            cur.R += 140 * 8;
        }
        SetRDScore(lambda, &mut cur);
        if mode == 0 || cur.score < best.score {
            CopyScore(&mut best, &cur);
            rd.mode_uv = mode as i32;
            rd.uv_levels = cur.uv_levels;
            if enc.top_derr.is_some() {
                rd.derr = cur.derr;
            }
            best_in_out2 = tmp_in_out2;
            tmp_in_out2 = !tmp_in_out2;
        }
    }
    VP8SetIntraUVMode(it, enc, rd.mode_uv as u8);
    AddScore(rd, &best);
    if best_in_out2 {
        let pixels = it.yuv_out2;
        Copy16x8_C(&pixels[U_OFF_ENC..], &mut it.yuv_out[U_OFF_ENC..]);
    }
    if enc.top_derr.is_some() {
        StoreDiffusionErrors(it, enc, rd);
    }
}

/// Required default-preset search. Other RD levels and methods remain deferred.
pub fn VP8Decimate(
    it: &mut VP8EncIterator,
    enc: &mut VP8Encoder,
    rd: &mut VP8ModeScore,
    rd_opt: VP8RDLevel,
) -> bool {
    assert_eq!(rd_opt, VP8RDLevel::RD_OPT_BASIC);
    assert_eq!(enc.config.method, 4);
    InitScore(rd);
    VP8MakeLuma16Preds(it, enc);
    VP8MakeChroma8Preds(it, enc);
    it.do_trellis = false;
    PickBestIntra16(it, enc, rd);
    PickBestIntra4(it, enc, rd);
    PickBestUV(it, enc, rd);
    let skipped = rd.nz == 0;
    VP8SetSkip(it, enc, skipped);
    skipped
}

pub const VP8Scan: [usize; 16] = [
    0,
    4,
    8,
    12,
    4 * BPS,
    4 * BPS + 4,
    4 * BPS + 8,
    4 * BPS + 12,
    8 * BPS,
    8 * BPS + 4,
    8 * BPS + 8,
    8 * BPS + 12,
    12 * BPS,
    12 * BPS + 4,
    12 * BPS + 8,
    12 * BPS + 12,
];
pub const VP8I16ModeOffsets: [usize; 4] = [I16DC16, I16TM16, I16VE16, I16HE16];
pub const VP8ScanUV: [usize; 8] = [0, 4, 4 * BPS, 4 * BPS + 4, 8, 12, 4 * BPS + 8, 4 * BPS + 12];
pub const VP8UVModeOffsets: [usize; 4] = [C8DC8, C8TM8, C8VE8, C8HE8];
pub const VP8I4ModeOffsets: [usize; 10] = [
    I4DC4, I4TM4, I4VE4, I4HE4, I4RD4, I4VR4, I4LD4, I4VL4, I4HD4, I4HU4,
];

pub fn VP8MakeLuma16Preds(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let left = (it.x != 0).then_some(&it.y_left[..]);
    let top = (it.y != 0).then(|| <[u8; 16]>::try_from(&it.y_top(enc)[..16]).unwrap());
    crate::dsp::enc::Intra16Preds_C(&mut it.yuv_p, left, top.as_ref().map(|t| &t[..]));
}

pub fn VP8MakeChroma8Preds(it: &mut VP8EncIterator, enc: &VP8Encoder) {
    let left = (it.x != 0).then_some(&it.uv_left[..]);
    let top = (it.y != 0).then(|| <[u8; 16]>::try_from(&it.uv_top(enc)[..16]).unwrap());
    crate::dsp::enc::IntraChromaPreds_C(&mut it.yuv_p, left, top.as_ref().map(|t| &t[..]));
}

pub fn MakeIntra4Preds(it: &mut VP8EncIterator) {
    let top = *it.i4_window();
    Intra4Preds_C(&mut it.yuv_p, &top);
}

/// Default scalar reconstruction without trellis. Candidate pixels and levels
/// are returned separately from the persistent borders and coefficient context.
pub fn ReconstructIntra16(
    it: &VP8EncIterator,
    enc: &VP8Encoder,
    rd: &mut VP8ModeScore,
    yuv_out: &mut [u8],
    mode: usize,
) -> u32 {
    assert!(!it.do_trellis);
    let reference = &it.yuv_p[VP8I16ModeOffsets[mode]..];
    let dqm = &enc.dqm[usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment)];
    let mut tmp = [0i16; 256];
    let mut dc = [0i16; 16];
    for n in (0..16).step_by(2) {
        FTransform2_C(
            &it.yuv_in[VP8Scan[n]..],
            &reference[VP8Scan[n]..],
            (&mut tmp[16 * n..16 * n + 32]).try_into().unwrap(),
        );
    }
    FTransformWHT_C(&tmp, &mut dc);
    let mut nz = u32::from(QuantizeBlock_C(&mut dc, &mut rd.y_dc_levels, &dqm.y2)) << 24;
    for n in (0..16).step_by(2) {
        tmp[16 * n] = 0;
        tmp[16 * (n + 1)] = 0;
        nz |= Quantize2Blocks_C(
            (&mut tmp[16 * n..16 * n + 32]).try_into().unwrap(),
            (&mut rd.y_ac_levels.as_flattened_mut()[16 * n..16 * n + 32])
                .try_into()
                .unwrap(),
            &dqm.y1,
        ) << n;
    }
    TransformWHT_C(&dc, &mut tmp);
    for n in (0..16).step_by(2) {
        ITransform_C(
            &reference[VP8Scan[n]..],
            &tmp[16 * n..],
            &mut yuv_out[VP8Scan[n]..],
            true,
        );
    }
    nz
}

pub fn ReconstructIntra4(
    it: &VP8EncIterator,
    enc: &VP8Encoder,
    levels: &mut [i16; 16],
    src: &[u8],
    yuv_out: &mut [u8],
    mode: usize,
) -> bool {
    assert!(!it.do_trellis);
    let reference = &it.yuv_p[VP8I4ModeOffsets[mode]..];
    let dqm = &enc.dqm[usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment)];
    let mut tmp = [0i16; 16];
    FTransform_C(src, reference, &mut tmp);
    let nz = QuantizeBlock_C(&mut tmp, levels, &dqm.y1);
    ITransform_C(reference, &tmp, yuv_out, false);
    nz
}

/// Scalar DC quantization and signed error divided by two. Unlike the block
/// quantizer this helper neither sharpens nor caps the quantized level.
pub fn QuantizeSingle(v: &mut i16, mtx: &VP8Matrix) -> i32 {
    let value = i32::from(*v);
    let magnitude = value.abs();
    let quantized = if magnitude > mtx.zthresh[0] as i32 {
        QUANTDIV(magnitude as u32, u32::from(mtx.iq[0]), mtx.bias[0]) * i32::from(mtx.q[0])
    } else {
        0
    };
    let signed = if value < 0 { -quantized } else { quantized };
    *v = signed as i16;
    (value - signed) >> 1
}

pub fn CorrectDCValues(
    it: &VP8EncIterator,
    enc: &VP8Encoder,
    mtx: &VP8Matrix,
    tmp: &mut [[i16; 16]; 8],
    rd: &mut VP8ModeScore,
) {
    let top = enc.top_derr.as_ref().expect("DC diffusion storage")[it.x];
    for ch in 0..2 {
        let c = &mut tmp[4 * ch..4 * ch + 4];
        let mut errors = [0i32; 4];
        for i in 0..4 {
            let above = if i < 2 {
                i32::from(top[ch][i])
            } else {
                errors[i - 2]
            };
            let left = if i % 2 == 0 {
                i32::from(it.left_derr[ch][i / 2])
            } else {
                errors[i - 1]
            };
            c[i][0] = (i32::from(c[i][0]) + ((7 * above + 8 * left) >> 3)) as i16;
            errors[i] = QuantizeSingle(&mut c[i][0], mtx);
        }
        for i in 0..3 {
            assert!((-127..=127).contains(&errors[i + 1]));
            rd.derr[ch][i] = errors[i + 1] as i8;
        }
    }
}

/// Commit only the selected candidate's diffusion errors. Reconstruction itself
/// reads the incoming errors without changing neighboring macroblocks.
pub fn StoreDiffusionErrors(it: &mut VP8EncIterator, enc: &mut VP8Encoder, rd: &VP8ModeScore) {
    let top = &mut enc.top_derr.as_mut().expect("DC diffusion storage")[it.x];
    for ch in 0..2 {
        let bottom_right = i32::from(rd.derr[ch][2]);
        let down = (3 * bottom_right) >> 2;
        it.left_derr[ch] = [rd.derr[ch][0], down as i8];
        top[ch] = [rd.derr[ch][1], (bottom_right - down) as i8];
    }
}

/// Destination starts at the U-plane base; U and V occupy an adjacent 16×8
/// rectangle with BPS stride. The returned mask uses bits 16..23.
pub fn ReconstructUV(
    it: &VP8EncIterator,
    enc: &VP8Encoder,
    rd: &mut VP8ModeScore,
    yuv_out: &mut [u8],
    mode: usize,
) -> u32 {
    let reference = &it.yuv_p[VP8UVModeOffsets[mode]..];
    let src = &it.yuv_in[U_OFF_ENC..];
    let dqm = &enc.dqm[usize::from(enc.mb_info[it.y * enc.mb_w + it.x].segment)];
    let mut tmp = [[0i16; 16]; 8];
    for n in (0..8).step_by(2) {
        FTransform2_C(
            &src[VP8ScanUV[n]..],
            &reference[VP8ScanUV[n]..],
            (&mut tmp.as_flattened_mut()[16 * n..16 * n + 32])
                .try_into()
                .unwrap(),
        );
    }
    if enc.top_derr.is_some() {
        CorrectDCValues(it, enc, &dqm.uv, &mut tmp, rd);
    }
    let mut nz = 0;
    for n in (0..8).step_by(2) {
        nz |= Quantize2Blocks_C(
            (&mut tmp.as_flattened_mut()[16 * n..16 * n + 32])
                .try_into()
                .unwrap(),
            (&mut rd.uv_levels.as_flattened_mut()[16 * n..16 * n + 32])
                .try_into()
                .unwrap(),
            &dqm.uv,
        ) << n;
        ITransform_C(
            &reference[VP8ScanUV[n]..],
            &tmp.as_flattened()[16 * n..],
            &mut yuv_out[VP8ScanUV[n]..],
            true,
        );
    }
    nz << 16
}

pub fn StoreMaxDelta(dqm: &mut VP8SegmentInfo, dc: &[i16; 16]) {
    for i in [1, 2, 4] {
        dqm.max_edge = dqm.max_edge.max(i32::from(dc[i]).abs());
    }
}

pub const kDcTable: [u8; 128] = [
    4, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 16, 17, 17, 18, 19, 20, 20, 21, 21, 22, 22, 23,
    23, 24, 25, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 37, 38, 39, 40, 41, 42, 43, 44,
    45, 46, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67,
    68, 69, 70, 71, 72, 73, 74, 75, 76, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 91,
    93, 95, 96, 98, 100, 101, 102, 104, 106, 108, 110, 112, 114, 116, 118, 122, 124, 126, 128, 130,
    132, 134, 136, 138, 140, 143, 145, 148, 151, 154, 157,
];

pub const kAcTable: [u16; 128] = [
    4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28,
    29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
    53, 54, 55, 56, 57, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 78, 80, 82, 84, 86, 88, 90, 92, 94,
    96, 98, 100, 102, 104, 106, 108, 110, 112, 114, 116, 119, 122, 125, 128, 131, 134, 137, 140,
    143, 146, 149, 152, 155, 158, 161, 164, 167, 170, 173, 177, 181, 185, 189, 193, 197, 201, 205,
    209, 213, 217, 221, 225, 229, 234, 239, 245, 249, 254, 259, 264, 269, 274, 279, 284,
];

pub const kAcTable2: [u16; 128] = [
    8, 8, 9, 10, 12, 13, 15, 17, 18, 20, 21, 23, 24, 26, 27, 29, 31, 32, 34, 35, 37, 38, 40, 41,
    43, 44, 46, 48, 49, 51, 52, 54, 55, 57, 58, 60, 62, 63, 65, 66, 68, 69, 71, 72, 74, 75, 77, 79,
    80, 82, 83, 85, 86, 88, 89, 93, 96, 99, 102, 105, 108, 111, 114, 117, 120, 124, 127, 130, 133,
    136, 139, 142, 145, 148, 151, 155, 158, 161, 164, 167, 170, 173, 176, 179, 184, 189, 193, 198,
    203, 207, 212, 217, 221, 226, 230, 235, 240, 244, 249, 254, 258, 263, 268, 274, 280, 286, 292,
    299, 305, 311, 317, 323, 330, 336, 342, 348, 354, 362, 370, 379, 385, 393, 401, 409, 416, 424,
    432, 440,
];

pub const kFreqSharpening: [u8; 16] = [
    0, 30, 60, 90, 30, 60, 90, 90, 60, 90, 90, 90, 90, 90, 90, 90,
];

const kBiasMatrices: [[u32; 2]; 3] = [[96, 110], [96, 108], [110, 115]];

#[inline]
fn clip(v: i32, min: i32, max: i32) -> usize {
    v.clamp(min, max) as usize
}

/// Expand the DC/AC steps into the scalar quantizer's complete matrix.
/// Steps must fit the range of the VP8 tables (4..=440).
pub fn ExpandMatrix(m: &mut VP8Matrix, r#type: usize) -> i32 {
    assert!(r#type < 3 && (4..=440).contains(&m.q[0]) && (4..=440).contains(&m.q[1]));
    for i in 0..2 {
        m.iq[i] = ((1 << QFIX) / u32::from(m.q[i])) as u16;
        m.bias[i] = BIAS(kBiasMatrices[r#type][i]);
        m.zthresh[i] = ((1 << QFIX) - 1 - m.bias[i]) / u32::from(m.iq[i]);
    }
    for i in 2..16 {
        m.q[i] = m.q[1];
        m.iq[i] = m.iq[1];
        m.bias[i] = m.bias[1];
        m.zthresh[i] = m.zthresh[1];
    }
    let mut sum = 0;
    for (i, &frequency) in kFreqSharpening.iter().enumerate() {
        m.sharpen[i] = if r#type == 0 {
            ((u32::from(frequency) * u32::from(m.q[i])) >> 11) as u16
        } else {
            0
        };
        sum += i32::from(m.q[i]);
    }
    (sum + 8) >> 4
}

pub fn SetupMatrices(enc: &mut VP8Encoder) {
    assert!((1..=4).contains(&enc.segment_hdr.num_segments));
    let tlambda_scale = if enc.config.method >= 4 {
        enc.config.sns_strength
    } else {
        0
    };
    for m in &mut enc.dqm[..enc.segment_hdr.num_segments] {
        let q = m.quant;
        m.y1.q[0] = u16::from(kDcTable[clip(q + enc.dq_y1_dc, 0, 127)]);
        m.y1.q[1] = kAcTable[clip(q, 0, 127)];
        m.y2.q[0] = u16::from(kDcTable[clip(q + enc.dq_y2_dc, 0, 127)]) * 2;
        m.y2.q[1] = kAcTable2[clip(q + enc.dq_y2_ac, 0, 127)];
        m.uv.q[0] = u16::from(kDcTable[clip(q + enc.dq_uv_dc, 0, 117)]);
        m.uv.q[1] = kAcTable[clip(q + enc.dq_uv_ac, 0, 127)];
        let q_i4 = ExpandMatrix(&mut m.y1, 0);
        let q_i16 = ExpandMatrix(&mut m.y2, 1);
        let q_uv = ExpandMatrix(&mut m.uv, 2);
        m.lambda_i4 = ((3 * q_i4 * q_i4) >> 7).max(1);
        m.lambda_i16 = (3 * q_i16 * q_i16).max(1);
        m.lambda_uv = ((3 * q_uv * q_uv) >> 6).max(1);
        m.lambda_mode = ((q_i4 * q_i4) >> 7).max(1);
        m.lambda_trellis_i4 = ((7 * q_i4 * q_i4) >> 3).max(1);
        m.lambda_trellis_i16 = ((q_i16 * q_i16) >> 2).max(1);
        m.lambda_trellis_uv = ((q_uv * q_uv) << 1).max(1);
        m.tlambda = ((tlambda_scale * q_i4) >> 5).max(1);
        m.min_disto = 20 * i32::from(m.y1.q[0]);
        m.max_edge = 0;
        m.i4_penalty = i64::from(1000 * q_i4 * q_i4);
    }
}

pub fn SetupFilterStrength(enc: &mut VP8Encoder) {
    let level0 = 5 * enc.config.filter_strength;
    for m in &mut enc.dqm {
        let qstep = i32::from(kAcTable[clip(m.quant, 0, 127)] >> 2);
        // C reads the old header sharpness before storing the configured value.
        let base = super::filter_enc::VP8FilterStrengthFromDelta(enc.filter_hdr.sharpness, qstep);
        let f = base * level0 / (256 + m.beta);
        m.fstrength = if f < 2 { 0 } else { f.min(63) };
    }
    enc.filter_hdr.level = enc.dqm[0].fstrength;
    enc.filter_hdr.simple = enc.config.filter_type == 0;
    enc.filter_hdr.sharpness = enc.config.filter_sharpness;
}

pub fn QualityToCompression(c: f64) -> f64 {
    let linear_c = if c < 0.75 { c * (2. / 3.) } else { 2. * c - 1. };
    linear_c.powf(1. / 3.)
}

pub fn SimplifySegments(enc: &mut VP8Encoder) {
    let nb = enc.segment_hdr.num_segments;
    assert!((1..=4).contains(&nb));
    let mut map = [0, 1, 2, 3];
    let mut count = 1;
    for s in 1..nb {
        let found = (0..count).find(|&i| {
            enc.dqm[s].quant == enc.dqm[i].quant && enc.dqm[s].fstrength == enc.dqm[i].fstrength
        });
        map[s] = found.unwrap_or(count) as u8;
        if found.is_none() {
            if count != s {
                enc.dqm[count] = enc.dqm[s].clone();
            }
            count += 1;
        }
    }
    if count < nb {
        for mb in &mut enc.mb_info {
            mb.segment = map[usize::from(mb.segment)];
        }
        enc.segment_hdr.num_segments = count;
        for i in count..nb {
            enc.dqm[i] = enc.dqm[count - 1].clone();
        }
    }
}

/// Default (non-JPEG-emulation) quality mapping, including fractional f32
/// quality promoted to f64 before division, as in the C entry point.
pub fn VP8SetSegmentParams(enc: &mut VP8Encoder, quality: f32) {
    assert!(quality.is_finite() && (0.0..=100.0).contains(&quality));
    assert!((1..=4).contains(&enc.segment_hdr.num_segments));
    assert!((0..=100).contains(&enc.config.sns_strength));
    assert!((0..=100).contains(&enc.config.filter_strength));
    assert!((0..=7).contains(&enc.config.filter_sharpness));
    let nb = enc.segment_hdr.num_segments;
    let amp = 0.9 * f64::from(enc.config.sns_strength) / 100. / 128.;
    let c_base = QualityToCompression(f64::from(quality) / 100.);
    for m in &mut enc.dqm[..nb] {
        assert!((-127..=127).contains(&m.alpha));
        let expn = 1. - amp * f64::from(m.alpha);
        let c = c_base.powf(expn);
        m.quant = (127. * (1. - c)) as i32;
        m.quant = m.quant.clamp(0, 127);
    }
    enc.base_quant = enc.dqm[0].quant;
    for m in &mut enc.dqm[nb..] {
        m.quant = enc.base_quant;
    }
    let dq_uv_ac = (enc.uv_alpha - 64) * 10 / 70;
    enc.dq_uv_ac = (dq_uv_ac * enc.config.sns_strength / 100).clamp(-4, 6);
    enc.dq_uv_dc = (-4 * enc.config.sns_strength / 100).clamp(-15, 15);
    enc.dq_y1_dc = 0;
    enc.dq_y2_dc = 0;
    enc.dq_y2_ac = 0;
    SetupFilterStrength(enc);
    if nb > 1 {
        SimplifySegments(enc);
    }
    SetupMatrices(enc);
}
