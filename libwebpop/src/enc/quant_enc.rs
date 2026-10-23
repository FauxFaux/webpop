// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/quant_enc.c, quantizer setup and predictions.
// See COPYING and PATENTS.
#![allow(non_upper_case_globals)]

use super::vp8i_enc::*;
use crate::dsp::enc::BPS;

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
