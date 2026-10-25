// Copyright 2011 Google Inc. All Rights Reserved.
// Derived from upstream/src/enc/frame_enc.c, direct residuals, token loop and fitting.
// See COPYING and PATENTS.
#![allow(non_upper_case_globals)]

use super::{
    cost_enc::*, filter_enc::VP8AdjustFilterStrength, iterator_enc::*,
    picture_enc::WebPYUVAPicture, quant_enc::*, token_enc::*, tree_enc::*, vp8i_enc::*,
};
use crate::{
    dsp::cost::{SetResidualCoeffs_C, VP8EncBands},
    utils::bit_writer_utils::*,
};
use std::collections::TryReserveError;

pub const VP8Cat3: [u8; 3] = [173, 148, 140];
pub const VP8Cat4: [u8; 4] = [176, 155, 140, 135];
pub const VP8Cat5: [u8; 5] = [180, 157, 141, 134, 130];
pub const VP8Cat6: [u8; 11] = [254, 254, 243, 230, 196, 177, 153, 140, 133, 130, 129];

pub fn ResetStats(proba: &mut VP8EncProba) {
    VP8CalculateLevelCosts(proba);
    proba.nb_skip = 0;
}

pub fn ResetTokenStats(proba: &mut VP8EncProba) {
    proba.stats = [[[[0; NUM_PROBAS]; NUM_CTX]; NUM_BANDS]; NUM_TYPES];
}

/// Floor probability of a non-skipped event. Counts must fit the u64 product.
pub fn CalcSkipProba(nb: u64, total: u64) -> u8 {
    assert!(nb <= total && total <= u64::MAX / 255);
    if total == 0 {
        255
    } else {
        ((total - nb) * 255 / total) as u8
    }
}

/// Fit skip syntax for at most the VP8 format's 1024x1024 macroblocks.
pub fn FinalizeSkipProba(proba: &mut VP8EncProba, nb_mbs: usize) -> i32 {
    assert!(nb_mbs <= 1024 * 1024 && proba.nb_skip >= 0 && proba.nb_skip as usize <= nb_mbs);
    let nb_events = proba.nb_skip;
    proba.skip_proba = CalcSkipProba(nb_events as u64, nb_mbs as u64);
    proba.use_skip_proba = proba.skip_proba < 250;
    let mut size = 256;
    if proba.use_skip_proba {
        size += nb_events * VP8BitCost(1, proba.skip_proba)
            + (nb_mbs as i32 - nb_events) * VP8BitCost(0, proba.skip_proba);
        size += 8 * 256;
    }
    size
}

pub fn CalcTokenProba(nb: u32, total: u32) -> u8 {
    assert!(nb <= total && total <= 65535);
    if nb == 0 {
        255
    } else {
        (255 - nb * 255 / total) as u8
    }
}

pub fn BranchCost(nb: u32, total: u32, proba: u8) -> i32 {
    assert!(nb <= total && total <= 65535);
    nb as i32 * VP8BitCost(1, proba) + (total - nb) as i32 * VP8BitCost(0, proba)
}

/// Fit against the default probabilities, preserving the pin's strict
/// cost comparison and its dirty flag relative to those defaults.
pub fn FinalizeTokenProbas(proba: &mut VP8EncProba) -> i32 {
    let mut has_changed = false;
    let mut size = 0;
    for t in 0..NUM_TYPES {
        for b in 0..NUM_BANDS {
            for c in 0..NUM_CTX {
                for p in 0..NUM_PROBAS {
                    let stats = proba.stats[t][b][c][p];
                    let nb = stats & 65535;
                    let total = stats >> 16;
                    let update_proba = VP8CoeffsUpdateProba[t][b][c][p];
                    let old_p = VP8CoeffsProba0[t][b][c][p];
                    let new_p = CalcTokenProba(nb, total);
                    let old_cost = BranchCost(nb, total, old_p) + VP8BitCost(0, update_proba);
                    let new_cost =
                        BranchCost(nb, total, new_p) + VP8BitCost(1, update_proba) + 8 * 256;
                    let use_new_p = old_cost > new_cost;
                    size += VP8BitCost(i32::from(use_new_p), update_proba);
                    if use_new_p {
                        proba.coeffs[t][b][c][p] = new_p;
                        has_changed |= new_p != old_p;
                        size += 8 * 256;
                    } else {
                        proba.coeffs[t][b][c][p] = old_p;
                    }
                }
            }
        }
    }
    proba.dirty = has_changed;
    size
}

pub const PARTITION0_SIZE_LIMIT: u64 = ((1 << 19) - 2048) << 11;

pub fn GetProba(a: u32, b: u32) -> u8 {
    let total = u64::from(a) + u64::from(b);
    if total == 0 {
        255
    } else {
        ((255 * u64::from(a) + total / 2) / total) as u8
    }
}

pub fn ResetSegments(enc: &mut VP8Encoder) {
    for mb in &mut enc.mb_info {
        mb.segment = 0;
    }
}

pub fn SetSegmentProbas(enc: &mut VP8Encoder) {
    let mut p = [0u32; 4];
    for mb in &enc.mb_info {
        p[usize::from(mb.segment)] += 1;
    }
    if enc.segment_hdr.num_segments > 1 {
        enc.proba.segments = [
            GetProba(p[0] + p[1], p[2] + p[3]),
            GetProba(p[0], p[1]),
            GetProba(p[2], p[3]),
        ];
        let probs = enc.proba.segments;
        enc.segment_hdr.update_map = probs.iter().any(|&v| v != 255);
        if !enc.segment_hdr.update_map {
            ResetSegments(enc);
        }
        // A valid frame's segment cost fits in i32 (at most 2^20 blocks).
        enc.segment_hdr.size = (0..4)
            .map(|s| {
                p[s] as i32
                    * (VP8BitCost(i32::from(s >= 2), probs[0])
                        + VP8BitCost((s & 1) as i32, probs[1 + s / 2]))
            })
            .sum();
    } else {
        enc.segment_hdr.update_map = false;
        enc.segment_hdr.size = 0;
    }
}

pub fn SetLoopParams(enc: &mut VP8Encoder, quality: f32) {
    VP8SetSegmentParams(enc, quality.clamp(0., 100.));
    SetSegmentProbas(enc);
    ResetStats(&mut enc.proba);
}

/// Write one residual directly with the current coefficient probabilities.
/// Allocation failures propagate immediately; callers must abandon the writer.
pub fn PutCoeffs(
    bw: &mut VP8BitWriter,
    ctx: usize,
    res: &VP8Residual<'_>,
    proba: &VP8EncProba,
) -> Result<bool, TryReserveError> {
    assert!(ctx < NUM_CTX);
    let probs = &proba.coeffs[res.coeff_type];
    let mut n = res.first;
    let mut p = &probs[n][ctx];
    if VP8PutBit(bw, i32::from(res.last >= 0), p[0])? == 0 {
        return Ok(false);
    }
    while n < 16 {
        let c = res.coeffs[n];
        n += 1;
        let mut v = u32::from(c.unsigned_abs());
        if VP8PutBit(bw, i32::from(v != 0), p[1])? == 0 {
            p = &probs[VP8EncBands[n]][0];
            continue;
        }
        if VP8PutBit(bw, i32::from(v > 1), p[2])? == 0 {
            p = &probs[VP8EncBands[n]][1];
        } else {
            if VP8PutBit(bw, i32::from(v > 4), p[3])? == 0 {
                if VP8PutBit(bw, i32::from(v != 2), p[4])? != 0 {
                    VP8PutBit(bw, i32::from(v == 4), p[5])?;
                }
            } else if VP8PutBit(bw, i32::from(v > 10), p[6])? == 0 {
                if VP8PutBit(bw, i32::from(v > 6), p[7])? == 0 {
                    VP8PutBit(bw, i32::from(v == 6), 159)?;
                } else {
                    VP8PutBit(bw, i32::from(v >= 9), 165)?;
                    VP8PutBit(bw, i32::from(v & 1 == 0), 145)?;
                }
            } else {
                let tab: &[u8];
                if v < 19 {
                    VP8PutBit(bw, 0, p[8])?;
                    VP8PutBit(bw, 0, p[9])?;
                    v -= 11;
                    tab = &VP8Cat3;
                } else if v < 35 {
                    VP8PutBit(bw, 0, p[8])?;
                    VP8PutBit(bw, 1, p[9])?;
                    v -= 19;
                    tab = &VP8Cat4;
                } else if v < 67 {
                    VP8PutBit(bw, 1, p[8])?;
                    VP8PutBit(bw, 0, p[10])?;
                    v -= 35;
                    tab = &VP8Cat5;
                } else {
                    VP8PutBit(bw, 1, p[8])?;
                    VP8PutBit(bw, 1, p[10])?;
                    v -= 67;
                    tab = &VP8Cat6;
                }
                for (i, &prob) in tab.iter().enumerate() {
                    VP8PutBit(bw, i32::from(v & (1 << (tab.len() - 1 - i)) != 0), prob)?;
                }
            }
            p = &probs[VP8EncBands[n]][2];
        }
        VP8PutBitUniform(bw, i32::from(c < 0))?;
        if n == 16 || VP8PutBit(bw, i32::from(n as i32 <= res.last), p[0])? == 0 {
            return Ok(true);
        }
    }
    Ok(true)
}

pub fn CodeResiduals(
    bw: &mut VP8BitWriter,
    it: &mut VP8EncIterator,
    enc: &mut VP8Encoder,
    rd: &VP8ModeScore,
) -> Result<(), TryReserveError> {
    VP8IteratorNzToBytes(it, enc);
    let mb = enc.mb_info[it.y * enc.mb_w + it.x];
    let i16 = mb.r#type == 1;
    let pos1 = VP8BitWriterPos(bw);
    if i16 {
        let mut res = VP8InitResidual(0, 1);
        SetResidualCoeffs_C(&rd.y_dc_levels, &mut res);
        let nz = u8::from(PutCoeffs(
            bw,
            usize::from(it.top_nz[8] + it.left_nz[8]),
            &res,
            &enc.proba,
        )?);
        it.top_nz[8] = nz;
        it.left_nz[8] = nz;
    }
    for n in 0..16 {
        let (x, y) = (n % 4, n / 4);
        let mut res = VP8InitResidual(usize::from(i16), if i16 { 0 } else { 3 });
        SetResidualCoeffs_C(&rd.y_ac_levels[n], &mut res);
        let nz = u8::from(PutCoeffs(
            bw,
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &enc.proba,
        )?);
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    let pos2 = VP8BitWriterPos(bw);
    for n in 0..8 {
        let (x, y) = (4 + 2 * (n / 4) + n % 2, 4 + 2 * (n / 4) + (n % 4) / 2);
        let mut res = VP8InitResidual(0, 2);
        SetResidualCoeffs_C(&rd.uv_levels[n], &mut res);
        let nz = u8::from(PutCoeffs(
            bw,
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &enc.proba,
        )?);
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    let pos3 = VP8BitWriterPos(bw);
    it.luma_bits = pos2 - pos1;
    it.uv_bits = pos3 - pos2;
    it.bit_count[usize::from(mb.segment)][usize::from(i16)] += it.luma_bits;
    it.bit_count[usize::from(mb.segment)][2] += it.uv_bits;
    VP8IteratorBytesToNz(it, enc);
    Ok(())
}

pub fn RecordResiduals(it: &mut VP8EncIterator, enc: &mut VP8Encoder, rd: &VP8ModeScore) {
    VP8IteratorNzToBytes(it, enc);
    let i16 = enc.mb_info[it.y * enc.mb_w + it.x].r#type == 1;
    if i16 {
        let mut res = VP8InitResidual(0, 1);
        SetResidualCoeffs_C(&rd.y_dc_levels, &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[8] + it.left_nz[8]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[8] = nz;
        it.left_nz[8] = nz;
    }
    for n in 0..16 {
        let (x, y) = (n % 4, n / 4);
        let mut res = VP8InitResidual(usize::from(i16), if i16 { 0 } else { 3 });
        SetResidualCoeffs_C(&rd.y_ac_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    for n in 0..8 {
        let (x, y) = (4 + 2 * (n / 4) + n % 2, 4 + 2 * (n / 4) + (n % 4) / 2);
        let mut res = VP8InitResidual(0, 2);
        SetResidualCoeffs_C(&rd.uv_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffs(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            &mut enc.proba,
        ));
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    VP8IteratorBytesToNz(it, enc);
}

/// Reset only the contexts invalidated by an omitted macroblock residual.
pub fn ResetAfterSkip(it: &mut VP8EncIterator, enc: &mut VP8Encoder) {
    if enc.mb_info[it.y * enc.mb_w + it.x].r#type == 1 {
        enc.nz[it.x + 1] = 0;
        it.left_nz[8] = 0;
    } else {
        enc.nz[it.x + 1] &= 1 << 24;
    }
}

pub fn RecordTokens(
    it: &mut VP8EncIterator,
    enc: &mut VP8Encoder,
    rd: &VP8ModeScore,
    tokens: &mut VP8TBuffer,
) -> Result<(), TryReserveError> {
    VP8IteratorNzToBytes(it, enc);
    let i16 = enc.mb_info[it.y * enc.mb_w + it.x].r#type == 1;
    if i16 {
        let mut res = VP8InitResidual(0, 1);
        SetResidualCoeffs_C(&rd.y_dc_levels, &mut res);
        let nz = u8::from(VP8RecordCoeffTokens(
            usize::from(it.top_nz[8] + it.left_nz[8]),
            &res,
            tokens,
            &mut enc.proba,
        )?);
        it.top_nz[8] = nz;
        it.left_nz[8] = nz;
    }
    for n in 0..16 {
        let (x, y) = (n % 4, n / 4);
        let mut res = VP8InitResidual(usize::from(i16), if i16 { 0 } else { 3 });
        SetResidualCoeffs_C(&rd.y_ac_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffTokens(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            tokens,
            &mut enc.proba,
        )?);
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    for n in 0..8 {
        let (x, y) = (4 + 2 * (n / 4) + n % 2, 4 + 2 * (n / 4) + (n % 4) / 2);
        let mut res = VP8InitResidual(0, 2);
        SetResidualCoeffs_C(&rd.uv_levels[n], &mut res);
        let nz = u8::from(VP8RecordCoeffTokens(
            usize::from(it.top_nz[x] + it.left_nz[y]),
            &res,
            tokens,
            &mut enc.proba,
        )?);
        it.top_nz[x] = nz;
        it.left_nz[y] = nz;
    }
    VP8IteratorBytesToNz(it, enc);
    Ok(())
}

/// Direct-loop observations after import/setup and after residual handling.
/// Statistics and final coding deliberately retain separate iterator lifetimes.
pub enum VP8DirectLoopEvent<'a> {
    ScanStart {
        pass: usize,
        statistics: bool,
    },
    Macroblock {
        pass: usize,
        statistics: bool,
        index: usize,
        score: &'a VP8ModeScore,
        iterator: &'a VP8EncIterator,
    },
    StatEnd {
        pass: usize,
        header_cost: u64,
        retry: bool,
    },
}

fn OneStatPass(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
    nb_mbs: usize,
    pass: usize,
    observe: &mut impl FnMut(&VP8Encoder, VP8DirectLoopEvent<'_>),
) -> u64 {
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(enc, &mut it);
    SetLoopParams(enc, quality);
    observe(
        enc,
        VP8DirectLoopEvent::ScanStart {
            pass,
            statistics: true,
        },
    );
    // With one configured pass and no target search, only the header estimate
    // is consumed. Non-RD scores legitimately leave D/R/H at zero.
    let mut header_cost = 0;
    for index in 0..nb_mbs.min(enc.mb_info.len()) {
        VP8IteratorImport(&mut it, enc, pic, false);
        let mut rd = VP8ModeScore::default();
        if VP8Decimate(&mut it, enc, &mut rd, VP8RDLevel::RD_OPT_NONE) {
            enc.proba.nb_skip += 1;
        }
        // Even zero macroblocks contribute coefficient statistics here.
        RecordResiduals(&mut it, enc, &rd);
        header_cost += rd.H as u64;
        observe(
            enc,
            VP8DirectLoopEvent::Macroblock {
                pass,
                statistics: true,
                index,
                score: &rd,
                iterator: &it,
            },
        );
        VP8IteratorSaveBoundary(&mut it, enc);
        if !VP8IteratorNext(&mut it, enc) {
            break;
        }
    }
    header_cost + enc.segment_hdr.size as u64
}

fn StatLoop(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
    header_limit: u64,
    observe: &mut impl FnMut(&VP8Encoder, VP8DirectLoopEvent<'_>),
) -> usize {
    let total = enc.mb_info.len();
    let nb_mbs = if enc.config.method == 0 {
        if total > 200 { total >> 2 } else { 50 }
    } else {
        total
    };
    // The pin clears token statistics once, retaining them across header retries.
    ResetTokenStats(&mut enc.proba);
    let mut pass = 0;
    loop {
        let header_cost = OneStatPass(enc, pic, quality, nb_mbs, pass, observe);
        let retry = enc.max_i4_header_bits > 0 && header_cost > header_limit;
        observe(
            enc,
            VP8DirectLoopEvent::StatEnd {
                pass,
                header_cost,
                retry,
            },
        );
        pass += 1;
        if !retry {
            break;
        }
        enc.max_i4_header_bits >>= 1;
    }
    FinalizeSkipProba(&mut enc.proba, total);
    FinalizeTokenProbas(&mut enc.proba);
    VP8CalculateLevelCosts(&mut enc.proba);
    pass
}

// Single coefficient partition, initialized only after segment quantization.
fn InitBitWriters(enc: &VP8Encoder) -> Result<VP8BitWriter, TryReserveError> {
    const AVERAGE_BYTES_PER_MB: [usize; 8] = [50, 24, 16, 9, 7, 5, 3, 2];
    VP8BitWriterInit(enc.mb_info.len() * AVERAGE_BYTES_PER_MB[(enc.base_quant >> 4) as usize])
}

fn PostLoopFinalize(enc: &mut VP8Encoder, bw: &mut VP8BitWriter) -> Result<(), TryReserveError> {
    VP8BitWriterFinish(bw)?;
    VP8AdjustFilterStrength(enc);
    Ok(())
}

/// One statistics pass (plus header retries), then a separately reconstructed
/// final scan. Only methods 0–2 select this path with the supported settings.
pub fn VP8EncLoop(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
) -> Result<VP8BitWriter, TryReserveError> {
    VP8EncLoopWithObserver(enc, pic, quality, PARTITION0_SIZE_LIMIT, |_, _| {})
}

#[doc(hidden)]
pub fn VP8EncLoopWithObserver(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
    header_limit: u64,
    mut observe: impl FnMut(&VP8Encoder, VP8DirectLoopEvent<'_>),
) -> Result<VP8BitWriter, TryReserveError> {
    assert!(!enc.use_tokens && enc.rd_opt_level == VP8RDLevel::RD_OPT_NONE);
    assert!(quality.is_finite() && header_limit <= PARTITION0_SIZE_LIMIT);
    assert!(enc.max_i4_header_bits >= 0);
    assert_eq!(
        (enc.mb_w, enc.mb_h),
        (
            (pic.width as usize).div_ceil(16),
            (pic.height as usize).div_ceil(16),
        )
    );
    let pass = StatLoop(enc, pic, quality, header_limit, &mut observe);
    let mut bw = InitBitWriters(enc)?;
    let mut it = VP8EncIterator::default();
    VP8IteratorInit(enc, &mut it);
    observe(
        enc,
        VP8DirectLoopEvent::ScanStart {
            pass,
            statistics: false,
        },
    );
    for index in 0..enc.mb_info.len() {
        VP8IteratorImport(&mut it, enc, pic, false);
        let mut rd = VP8ModeScore::default();
        let skip = VP8Decimate(&mut it, enc, &mut rd, enc.rd_opt_level);
        if !skip || !enc.proba.use_skip_proba {
            CodeResiduals(&mut bw, &mut it, enc, &rd)?;
        } else {
            ResetAfterSkip(&mut it, enc);
        }
        observe(
            enc,
            VP8DirectLoopEvent::Macroblock {
                pass,
                statistics: false,
                index,
                score: &rd,
                iterator: &it,
            },
        );
        VP8IteratorSaveBoundary(&mut it, enc);
        if !VP8IteratorNext(&mut it, enc) {
            break;
        }
    }
    PostLoopFinalize(enc, &mut bw)?;
    Ok(bw)
}

/// Observation points for subgraph comparisons. Borrowed state is read-only;
/// normal encoding instantiates the driver with a no-op observer.
pub enum VP8TokenLoopEvent<'a> {
    Reconstruct {
        pass: usize,
        index: usize,
        reconstruction: VP8Reconstruction,
    },
    PassStart {
        pass: usize,
    },
    Refresh {
        pass: usize,
        index: usize,
    },
    Macroblock {
        pass: usize,
        index: usize,
        score: &'a VP8ModeScore,
        iterator: &'a VP8EncIterator,
        tokens: &'a VP8TBuffer,
    },
    PassEnd {
        pass: usize,
        header_cost: u64,
        distortion: u64,
        retry: bool,
    },
    Emit {
        tokens: &'a VP8TBuffer,
    },
}

/// Buffered RD methods, one configured pass, no target search, skip syntax or autofilter.
/// Header-budget retries remain part of this default path.
pub fn VP8EncTokenLoop(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
) -> Result<VP8BitWriter, TryReserveError> {
    VP8EncTokenLoopWithObserver(enc, pic, quality, PARTITION0_SIZE_LIMIT, |_, _| {})
}

/// A smaller header limit permits bounded synthetic retry checks without
/// enormous input images. Normal encoding always uses PARTITION0_SIZE_LIMIT.
#[doc(hidden)]
pub fn VP8EncTokenLoopWithObserver(
    enc: &mut VP8Encoder,
    pic: &WebPYUVAPicture,
    quality: f32,
    header_limit: u64,
    mut observe: impl FnMut(&VP8Encoder, VP8TokenLoopEvent<'_>),
) -> Result<VP8BitWriter, TryReserveError> {
    assert!(matches!(enc.config.method, 3..=6));
    assert!(enc.use_tokens);
    let rd_opt = enc.rd_opt_level;
    assert!(!enc.proba.use_skip_proba && quality.is_finite());
    assert_eq!(
        (enc.mb_w, enc.mb_h),
        (
            (pic.width as usize).div_ceil(16),
            (pic.height as usize).div_ceil(16)
        )
    );
    assert!(header_limit <= PARTITION0_SIZE_LIMIT && enc.max_i4_header_bits >= 0);
    let max_count = (enc.mb_info.len() >> 3).max(96);
    let scale = 1f32 + quality * 5. / 100.;
    let mut tokens = VP8TBufferInit((enc.mb_info.len() as f32 * 4. * scale) as usize);
    let mut it = VP8EncIterator::default();
    let mut pass = 0;
    loop {
        VP8IteratorInit(enc, &mut it);
        SetLoopParams(enc, quality);
        ResetTokenStats(&mut enc.proba);
        VP8TBufferClear(&mut tokens);
        observe(enc, VP8TokenLoopEvent::PassStart { pass });
        let mut count = max_count as i32;
        let (mut header_cost, mut distortion) = (0u64, 0u64);
        for index in 0..enc.mb_info.len() {
            VP8IteratorImport(&mut it, enc, pic, false);
            count -= 1;
            if count < 0 {
                FinalizeTokenProbas(&mut enc.proba);
                VP8CalculateLevelCosts(&mut enc.proba);
                observe(enc, VP8TokenLoopEvent::Refresh { pass, index });
                count = max_count as i32;
            }
            let mut rd = VP8ModeScore::default();
            VP8DecimateWithObserver(&mut it, enc, &mut rd, rd_opt, &mut |enc, reconstruction| {
                observe(
                    enc,
                    VP8TokenLoopEvent::Reconstruct {
                        pass,
                        index,
                        reconstruction,
                    },
                );
            });
            RecordTokens(&mut it, enc, &rd, &mut tokens)?;
            header_cost += rd.H as u64;
            distortion += rd.D as u64;
            observe(
                enc,
                VP8TokenLoopEvent::Macroblock {
                    pass,
                    index,
                    score: &rd,
                    iterator: &it,
                    tokens: &tokens,
                },
            );
            VP8IteratorSaveBoundary(&mut it, enc);
            if !VP8IteratorNext(&mut it, enc) {
                break;
            }
        }
        header_cost += enc.segment_hdr.size as u64;
        let retry = enc.max_i4_header_bits > 0 && header_cost > header_limit;
        observe(
            enc,
            VP8TokenLoopEvent::PassEnd {
                pass,
                header_cost,
                distortion,
                retry,
            },
        );
        if !retry {
            break;
        }
        enc.max_i4_header_bits >>= 1;
        pass += 1;
    }
    FinalizeTokenProbas(&mut enc.proba);
    observe(enc, VP8TokenLoopEvent::Emit { tokens: &tokens });
    let mut bw = InitBitWriters(enc)?;
    // The probability array is fixed-size; avoid a fallible heap copy.
    let mut probabilities = [0; NUM_TYPES * NUM_BANDS * NUM_CTX * NUM_PROBAS];
    for (dst, &p) in probabilities
        .iter_mut()
        .zip(enc.proba.coeffs.iter().flatten().flatten().flatten())
    {
        *dst = p;
    }
    VP8EmitTokens(&mut tokens, &mut bw, &probabilities, true)?;
    PostLoopFinalize(enc, &mut bw)?;
    Ok(bw)
}
