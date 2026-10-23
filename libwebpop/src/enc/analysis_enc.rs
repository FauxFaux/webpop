// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/analysis_enc.c (default method-4 scalar path).
// See COPYING and PATENTS.

use super::{iterator_enc::*, picture_enc::WebPYUVAPicture, quant_enc::*, vp8i_enc::*};
use crate::dsp::enc::{CollectHistogram_C, VP8Histogram};

const MAX_ALPHA: usize = 255;
const ALPHA_SCALE: i32 = 2 * MAX_ALPHA as i32;
const MAX_ITERS_K_MEANS: usize = 6;

#[inline]
fn clip(v: i32, min: i32, max: i32) -> i32 {
    v.clamp(min, max)
}

pub fn SetSegmentAlphas(enc: &mut VP8Encoder, centers: &[i32; NUM_MB_SEGMENTS], mid: i32) {
    let nb = enc.segment_hdr.num_segments;
    assert!((1..=NUM_MB_SEGMENTS).contains(&nb));
    let min = *centers[..nb].iter().min().unwrap();
    let mut max = *centers[..nb].iter().max().unwrap();
    if max == min {
        max = min + 1;
    }
    assert!(mid >= min && mid <= max);
    for (segment, &center) in enc.dqm[..nb].iter_mut().zip(centers) {
        segment.alpha = clip(255 * (center - mid) / (max - min), -127, 127);
        segment.beta = clip(255 * (center - min) / (max - min), 0, 255);
    }
}

pub fn FinalAlphaValue(alpha: i32) -> u8 {
    clip(MAX_ALPHA as i32 - alpha, 0, MAX_ALPHA as i32) as u8
}

pub fn GetAlpha(histo: &VP8Histogram) -> i32 {
    if histo.max_value > 1 {
        ALPHA_SCALE * histo.last_non_zero / histo.max_value
    } else {
        0
    }
}

fn InitHistogram() -> VP8Histogram {
    VP8Histogram {
        max_value: 0,
        last_non_zero: 1,
    }
}

/// Use the current histogram's map after the last Lloyd iteration, without
/// reassigning it after the final centroid movement. This preserves the pin's
/// six-iteration cap, strict adjacent-center improvements and displacement
/// threshold. Tied adjacent centers can block access to later, closer centers.
/// Every macroblock's alpha must have a positive histogram population.
pub fn AssignSegments(enc: &mut VP8Encoder, alphas: &[i32; 256]) {
    let nb = enc.segment_hdr.num_segments;
    assert!((1..=NUM_MB_SEGMENTS).contains(&nb));
    assert!(alphas.iter().all(|&n| n >= 0));
    assert!(alphas.iter().map(|&n| i64::from(n)).sum::<i64>() <= 1 << 20);
    let min = alphas
        .iter()
        .position(|&n| n != 0)
        .expect("nonempty susceptibility histogram");
    let max = alphas.iter().rposition(|&n| n != 0).unwrap();
    let range = (max - min) as i32;
    let mut centers = [0; NUM_MB_SEGMENTS];
    for (k, center) in centers[..nb].iter_mut().enumerate() {
        *center = min as i32 + ((2 * k + 1) as i32 * range) / (2 * nb) as i32;
    }
    let mut map = [0usize; 256];
    let mut weighted_average = 0;
    for _ in 0..MAX_ITERS_K_MEANS {
        let mut accum = [0; NUM_MB_SEGMENTS];
        let mut dist_accum = [0; NUM_MB_SEGMENTS];
        let mut n = 0;
        for a in min..=max {
            if alphas[a] != 0 {
                while n + 1 < nb
                    && (a as i32 - centers[n + 1]).abs() < (a as i32 - centers[n]).abs()
                {
                    n += 1;
                }
                map[a] = n;
                dist_accum[n] += a as i32 * alphas[a];
                accum[n] += alphas[a];
            }
        }
        let mut displaced = 0;
        weighted_average = 0;
        let mut total_weight = 0;
        for n in 0..nb {
            if accum[n] != 0 {
                let new_center = (dist_accum[n] + accum[n] / 2) / accum[n];
                displaced += (centers[n] - new_center).abs();
                centers[n] = new_center;
                weighted_average += new_center * accum[n];
                total_weight += accum[n];
            }
        }
        weighted_average = (weighted_average + total_weight / 2) / total_weight;
        if displaced < 5 {
            break;
        }
    }
    for mb in &mut enc.mb_info {
        assert!(alphas[usize::from(mb.alpha)] > 0);
        let n = map[usize::from(mb.alpha)];
        mb.segment = n as u8;
        mb.alpha = centers[n] as u8;
    }
    // Default preprocessing is zero: no spatial segment smoothing.
    SetSegmentAlphas(enc, &centers, weighted_average);
}

pub fn MBAnalyzeBestIntra16Mode(it: &mut VP8EncIterator, enc: &mut VP8Encoder) -> i32 {
    let (mut best_alpha, mut best_mode) = (-1, 0);
    VP8MakeLuma16Preds(it, enc);
    for (mode, &offset) in VP8I16ModeOffsets[..2].iter().enumerate() {
        let mut histo = InitHistogram();
        CollectHistogram_C(
            &it.yuv_in[Y_OFF_ENC..],
            &it.yuv_p[offset..],
            0,
            16,
            &mut histo,
        );
        let alpha = GetAlpha(&histo);
        if alpha > best_alpha {
            best_alpha = alpha;
            best_mode = mode;
        }
    }
    VP8SetIntra16Mode(it, enc, best_mode as u8);
    best_alpha
}

pub fn MBAnalyzeBestUVMode(it: &mut VP8EncIterator, enc: &mut VP8Encoder) -> i32 {
    let (mut best_alpha, mut smallest_alpha, mut best_mode) = (-1, 0, 0);
    VP8MakeChroma8Preds(it, enc);
    for (mode, &offset) in VP8UVModeOffsets[..2].iter().enumerate() {
        let mut histo = InitHistogram();
        CollectHistogram_C(
            &it.yuv_in[U_OFF_ENC..],
            &it.yuv_p[offset..],
            16,
            24,
            &mut histo,
        );
        let alpha = GetAlpha(&histo);
        if alpha > best_alpha {
            best_alpha = alpha;
        }
        if mode == 0 || alpha < smallest_alpha {
            smallest_alpha = alpha;
            best_mode = mode;
        }
    }
    VP8SetIntraUVMode(it, enc, best_mode as u8);
    best_alpha
}

/// Synchronous analysis statistics. Wide aggregate sums avoid overflowing on
/// very large pictures; per-block and centroid arithmetic retains C's integers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VP8AnalysisStats {
    pub alphas: [i32; 256],
    pub alpha_sum: i64,
    pub uv_alpha_sum: i64,
}
impl Default for VP8AnalysisStats {
    fn default() -> Self {
        Self {
            alphas: [0; 256],
            alpha_sum: 0,
            uv_alpha_sum: 0,
        }
    }
}

pub fn MBAnalyze(it: &mut VP8EncIterator, enc: &mut VP8Encoder, stats: &mut VP8AnalysisStats) {
    VP8SetIntra16Mode(it, enc, 0);
    VP8SetSkip(it, enc, false);
    VP8SetSegment(it, enc, 0);
    let alpha = MBAnalyzeBestIntra16Mode(it, enc);
    let uv_alpha = MBAnalyzeBestUVMode(it, enc);
    let final_alpha = FinalAlphaValue((3 * alpha + uv_alpha + 2) >> 2);
    stats.alphas[usize::from(final_alpha)] += 1;
    enc.mb_info[it.y * enc.mb_w + it.x].alpha = final_alpha;
    stats.alpha_sum += i64::from(final_alpha);
    stats.uv_alpha_sum += i64::from(uv_alpha);
}

pub fn DoSegmentsJob(enc: &mut VP8Encoder, pic: &WebPYUVAPicture) -> VP8AnalysisStats {
    let mut stats = VP8AnalysisStats::default();
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(enc, &mut it);
    loop {
        VP8IteratorImport(&mut it, enc, pic, true);
        MBAnalyze(&mut it, enc, &mut stats);
        if !VP8IteratorNext(&mut it, enc) {
            break;
        }
    }
    stats
}

fn DefaultMBInfo(mb: &mut VP8MBInfo) {
    *mb = VP8MBInfo {
        r#type: 1,
        ..Default::default()
    };
}
fn ResetAllMBInfo(enc: &mut VP8Encoder) {
    for mb in &mut enc.mb_info {
        DefaultMBInfo(mb);
    }
    enc.dqm[0].alpha = 0;
    enc.dqm[0].beta = 0;
    enc.alpha = 0;
    enc.uv_alpha = 0;
}

/// Default method 4, preprocessing 0, no JPEG-size emulation or worker threads.
/// Returns diagnostics in addition to mutating modes and segment parameters.
pub fn VP8EncAnalyze(enc: &mut VP8Encoder, pic: &WebPYUVAPicture) -> VP8AnalysisStats {
    assert!((1..=NUM_MB_SEGMENTS).contains(&enc.segment_hdr.num_segments));
    if enc.segment_hdr.num_segments > 1 {
        let stats = DoSegmentsJob(enc, pic);
        let total = (enc.mb_w * enc.mb_h) as i64;
        enc.alpha = (stats.alpha_sum / total) as i32;
        enc.uv_alpha = (stats.uv_alpha_sum / total) as i32;
        AssignSegments(enc, &stats.alphas);
        stats
    } else {
        ResetAllMBInfo(enc);
        VP8AnalysisStats::default()
    }
}
