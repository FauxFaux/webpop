#![allow(dead_code)]

use crate::support::{Case, SEED, random};
use libwebpop::dsp::lossless_common::VP8LNearLosslessBits;
use libwebpop::dsp::lossless_enc::VP8LSubtractGreenFromBlueAndRed;
use libwebpop::enc::predictor_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let (w, h) = (a[1] as usize, a[2] as usize);
    match a[0] {
        0 => {
            let mut current = a[10..11 + w].to_vec();
            let mut upper = a[11 + w..12 + 2 * w].to_vec();
            let diffs: Vec<_> = a[12 + 2 * w..].iter().map(|&v| v as u8).collect();
            let mut out = vec![0; a[6] as usize];
            GetResidual(
                w,
                h,
                &mut upper,
                &mut current,
                &diffs,
                a[3] as usize,
                a[4] as usize,
                a[5] as usize,
                a[7],
                a[8] != 0,
                a[9] != 0,
                &mut out,
            );
            out.extend(current);
            out.extend(upper);
            out.extend(diffs.into_iter().map(u32::from));
            out
        }
        1 => {
            let bits = a[3];
            let size = w.div_ceil(1 << bits) * h.div_ceil(1 << bits);
            let mut pixels = a[8 + size..].to_vec();
            CopyImageWithPrediction(
                w,
                h,
                bits,
                &a[8..8 + size],
                &mut pixels,
                a[4] != 0,
                a[5],
                a[6] != 0,
                a[7] != 0,
            )
            .unwrap();
            pixels
        }
        2 | 3 => {
            let mut pixels = a[9..].to_vec();
            let (bits, map) = if a[0] == 2 {
                VP8LResidualImage(
                    w,
                    h,
                    a[3],
                    a[4],
                    a[5] != 0,
                    &mut pixels,
                    a[6],
                    a[7] != 0,
                    a[8] != 0,
                )
                .unwrap()
            } else {
                GetBestPredictorsAndSubSampling(
                    w,
                    h,
                    a[3],
                    a[4],
                    &pixels,
                    1 << VP8LNearLosslessBits(a[6]),
                    a[7] != 0,
                    a[8] != 0,
                )
                .unwrap()
            };
            let mut out = vec![bits];
            out.extend(map);
            if a[0] == 2 {
                out.extend(pixels);
            }
            out
        }
        4 => {
            let mut histograms = vec![[0; 1024]; (a[6] as usize + 1) * 14];
            let mut scratch = vec![0; 2 * (w + 1)];
            let mut diffs = vec![0xa5; w];
            ComputeResidualsForTile(
                &mut histograms,
                w,
                h,
                a[4] as usize,
                a[5] as usize,
                a[3],
                a[6] as usize,
                &mut scratch,
                &a[12..],
                &mut diffs,
                a[7],
                a[8] != 0,
                a[9] != 0,
                a[10] as usize,
                a[11],
            );
            let mut out: Vec<_> = histograms.into_iter().flatten().collect();
            out.extend(scratch);
            out.extend(diffs.into_iter().map(u32::from));
            out
        }
        _ => unreachable!(),
    }
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for q in [1, 2, 4, 8, 16, 32] {
        for mode in 0..14 {
            for y in [0, 1, 3, 4] {
                for start in [0, 1, 6] {
                    for exact in [0, 1] {
                        for sg in [0, 1] {
                            let mut a = vec![0, 7, 5, mode, start, y, 7 - start, q, exact, sg];
                            a.extend((0..16).map(|i| {
                                let p = random(&mut seed);
                                if i % 3 == 0 { p & 0xffffff } else { p }
                            }));
                            a.extend([0, 2, 3, q, q + 1, 255, 0]);
                            cases.push(Case::new("NearLosslessFeedback", a));
                        }
                    }
                }
            }
        }
    }
    for (w, h) in [
        (1usize, 9usize),
        (9, 1),
        (2, 9),
        (9, 2),
        (3, 3),
        (7, 9),
        (17, 19),
        (33, 35),
        (65, 67),
        (129, 9),
    ] {
        for pattern in [0, 1, 2, 3, 5, 9, 12, 15] {
            let source = crate::support::analysis::pixels(w, h, w, pattern);
            for sg in [0, 1] {
                let mut pixels = source.clone();
                if sg != 0 {
                    VP8LSubtractGreenFromBlueAndRed(&mut pixels);
                }
                for strength in [0, 20, 40, 60, 80, 100] {
                    let q = 1 << VP8LNearLosslessBits(strength);
                    for (low, exact) in [(0, 0), (0, 1), (1, 0)] {
                        for (min, max) in [(2, 2), (2, 6), (4, 6)] {
                            let mut a =
                                vec![2, w as u32, h as u32, min, max, low, strength, exact, sg];
                            a.extend_from_slice(&pixels);
                            cases.push(Case::new("NearLosslessFeedback", a));
                        }
                    }
                    let size = w.div_ceil(4) * h.div_ceil(4);
                    for mode in [0, 1, 7, 11, 13, 14] {
                        let mut a = vec![1, w as u32, h as u32, 2, 0, q, 0, sg];
                        a.extend((0..size).map(|i| {
                            0xff000000 | ((if mode == 14 { i as u32 % 14 } else { mode }) << 8)
                        }));
                        a.extend_from_slice(&pixels);
                        cases.push(Case::new("NearLosslessFeedback", a));
                    }
                    let mut a = vec![3, w as u32, h as u32, 2, 6, 0, strength, 0, sg];
                    a.extend_from_slice(&pixels);
                    cases.push(Case::new("NearLosslessFeedback", a));
                }
            }
        }
    }
    // Inspect per-mode histograms, both physical scratch rows and differences
    // on full/partial tiles, at the left/right/bottom edges and in the interior.
    for (tx, ty) in [(0, 0), (1, 0), (0, 1), (1, 1), (4, 4)] {
        for q in [1, 2, 32] {
            for sg in [0, 1] {
                let mut pixels = crate::support::analysis::pixels(17, 19, 17, 2);
                if sg != 0 {
                    VP8LSubtractGreenFromBlueAndRed(&mut pixels);
                }
                let mut a = vec![4, 17, 19, 2, tx, ty, 1, q, 0, sg, 0, 15];
                a.extend(pixels);
                cases.push(Case::new("NearLosslessFeedback", a));
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let all = cases();
    let mut selected = Vec::new();
    for op in 0..5 {
        selected.extend(
            all.iter()
                .filter(|c| c.input[0] == op)
                .step_by(if op == 4 { 11 } else { 787 })
                .cloned(),
        );
    }
    selected
}
