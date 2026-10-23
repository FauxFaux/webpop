use super::{Case, SEED, random};
use libwebpop::enc::predictor_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let cost_words = |cost: i64| vec![cost as u32, ((cost as u64) >> 32) as u32];
    match a[0] {
        0 => {
            let (mask, min) = GetPlaneDiffMask(&a[3..], a[1], a[2] != 0);
            vec![mask, min]
        }
        1 => cost_words(PredictionCostSpatialHistogram(
            a[1029..].try_into().unwrap(),
            a[5..1029].try_into().unwrap(),
            a[1],
            a[2],
            a[3],
            a[4],
        )),
        2 => {
            let (w, h, bits) = (a[1] as usize, a[2] as usize, a[3]);
            let size = w.div_ceil(1 << bits) * h.div_ceil(1 << bits);
            let mut pixels = a[6 + size..].to_vec();
            CopyImageWithPrediction(
                w,
                h,
                bits,
                &a[6..6 + size],
                &mut pixels,
                a[4] != 0,
                1,
                a[5] != 0,
                false,
            )
            .unwrap();
            pixels
        }
        3 => {
            let w = a[1] as usize;
            let mut current = a[7..8 + w].to_vec();
            let mut upper = a[8 + w..].to_vec();
            let mut out = vec![0; a[6] as usize];
            GetResidual(
                w,
                a[4] as usize + 2,
                &mut upper,
                &mut current,
                &[],
                a[2] as usize,
                a[3] as usize,
                a[4] as usize,
                1,
                a[5] != 0,
                false,
                &mut out,
            );
            out.extend(current);
            out.extend(upper);
            out
        }
        4 => {
            let mut out: [u32; 1024] = a[2..1026].try_into().unwrap();
            UpdateHisto(&mut out, &a[1026..], a[1]);
            out.to_vec()
        }
        5 => cost_words(PredictionCostBias(
            a[3..].try_into().unwrap(),
            u64::from(a[1]),
            u64::from(a[2]),
        )),
        6 | 7 => {
            let mut pixels = a[7..].to_vec();
            let (bits, map) = if a[0] == 6 {
                VP8LResidualImage(
                    a[1] as usize,
                    a[2] as usize,
                    a[3],
                    a[4],
                    a[5] != 0,
                    &mut pixels,
                    100,
                    a[6] != 0,
                    false,
                )
                .unwrap()
            } else {
                GetBestPredictorsAndSubSampling(
                    a[1] as usize,
                    a[2] as usize,
                    a[3],
                    a[4],
                    &pixels,
                    1,
                    a[6] != 0,
                    false,
                )
                .unwrap()
            };
            let mut out = vec![bits];
            out.extend(map);
            if a[0] == 6 {
                out.extend(pixels);
            }
            out
        }
        _ => unreachable!(),
    }
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for pixels in [
        vec![0],
        vec![0xff000000],
        vec![0xff123456; 3],
        vec![0x00123456; 3],
        vec![0, 0xff000000],
        vec![0x00123456, 0xff123456],
        vec![0x80120000, 0xff120000],
        vec![0x80003400, 0xff003400],
        vec![0x80000056, 0xff000056],
        vec![0x00123456, 0x00789abc],
        vec![0xff123456, 0xff123457],
    ] {
        for quant in [1, 2, 32] {
            for exact in 0..=1 {
                let mut a = vec![0, quant, exact];
                a.extend_from_slice(&pixels);
                cases.push(Case::new("PredictorFoundations", a));
            }
        }
    }
    for pattern in 0..12 {
        let mut histo = vec![0; 1024];
        let mut accumulated = histo.clone();
        for i in 0..1024 {
            histo[i] = match pattern {
                0 => 0,
                1 => u32::from(i % 256 == 0),
                2 => 100,
                _ => random(&mut seed) % 10000,
            };
            accumulated[i] = random(&mut seed) % 20000;
        }
        for mask in 0..16 {
            for (mode, left, above) in [(0, 255, 255), (1, 1, 255), (11, 255, 11), (13, 13, 13)] {
                let mut a = vec![1, mode, left, above, mask];
                a.extend_from_slice(&histo);
                a.extend_from_slice(&accumulated);
                cases.push(Case::new("PredictorFoundations", a));
            }
            let mut a = vec![4, mask];
            a.extend_from_slice(&histo);
            a.extend((0..31).map(|_| random(&mut seed)));
            cases.push(Case::new("PredictorFoundations", a));
        }
        for (weight, exp) in [(1, 94), (3, 50)] {
            let mut a = vec![5, weight, exp];
            a.extend_from_slice(&histo[..256]);
            cases.push(Case::new("PredictorFoundations", a));
        }
    }
    for (w, h) in [
        (1usize, 1usize),
        (1, 19),
        (19, 1),
        (7, 9),
        (17, 19),
        (33, 35),
    ] {
        for pattern in 0..6 {
            let pixels: Vec<_> = (0..w * h)
                .map(|i| {
                    let r = random(&mut seed);
                    match pattern {
                        0 => 0xff123456,
                        1 => r,
                        2 => r & 0xffffff,
                        3 => {
                            if i % 3 == 0 {
                                r & 0xffffff
                            } else {
                                r | 0xff000000
                            }
                        }
                        4 => ((i % w) as u32) * 65793 | 0xff000000,
                        _ => {
                            if i % 2 == 0 {
                                0
                            } else {
                                0xffffffff
                            }
                        }
                    }
                })
                .collect();
            for bits in [2, 3, 6] {
                for mode in 0..15 {
                    for exact in 0..=1 {
                        let map_size = w.div_ceil(1 << bits) * h.div_ceil(1 << bits);
                        let mut a = vec![2, w as u32, h as u32, bits, 0, exact];
                        a.extend((0..map_size).map(|i| {
                            0xff000000 | ((if mode == 14 { i as u32 % 14 } else { mode }) << 8)
                        }));
                        a.extend_from_slice(&pixels);
                        cases.push(Case::new("PredictorFoundations", a.clone()));
                        if mode == 11 {
                            a[4] = 1;
                            cases.push(Case::new("PredictorFoundations", a));
                        }
                    }
                }
            }
        }
    }
    for mode in 0..14 {
        for start in [0, 1, 6] {
            for y in [0, 1, 7] {
                for exact in 0..=1 {
                    let mut a = vec![3, 7, mode, start, y, exact, 7 - start];
                    a.extend((0..8).map(|i| {
                        let r = random(&mut seed);
                        if i % 3 == 0 { r & 0xffffff } else { r }
                    }));
                    a.extend((0..8).map(|_| random(&mut seed)));
                    cases.push(Case::new("PredictorFoundations", a));
                }
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    // Keep both cheap branch helpers and representative complete residual passes.
    let all = cases();
    let mut selected = Vec::new();
    for op in 0..6 {
        selected.extend(
            all.iter()
                .filter(|c| c.input[0] == op)
                .step_by(if op == 2 { 307 } else { 59 })
                .cloned(),
        );
    }
    selected.extend(search_cases().into_iter().step_by(277));
    selected.extend(clamp_cases().into_iter().step_by(97));
    selected.extend(
        transform_cases()
            .into_iter()
            .filter(|c| c.input.len() < 20000)
            .step_by(173),
    );
    selected
}

pub fn transform_evaluate(case: &Case) -> Vec<u32> {
    use libwebpop::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::utils::bit_writer_utils::*;
    let a = &case.input;
    if case.function == "ClampBits" {
        return vec![ClampBits(
            a[0] as usize,
            a[1] as usize,
            a[2] as i32,
            a[3],
            a[4],
            a[5] as usize,
        )];
    }
    let mut pixels = a[8..].to_vec();
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut remaining = a[7];
    while remaining > 0 {
        let bits = remaining.min(32);
        VP8LPutBits(&mut bw, 0x05ad5ad5 & (u32::MAX >> (32 - bits)), bits).unwrap();
        remaining -= bits;
    }
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
    let bits = ApplyPredictFilter(
        &mut bw,
        &mut pixels,
        &mut chain,
        &mut refs,
        a[0] as usize,
        a[1] as usize,
        a[2] as i32,
        a[3],
        a[4],
        a[5] != 0,
        100,
        a[6] != 0,
        false,
    )
    .unwrap();
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    let mut out = vec![bits];
    out.extend(pixels);
    out.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&v| u32::from(v)),
    );
    out
}

pub fn clamp_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 16383),
        (16383, 1),
        (3, 5),
        (7, 9),
        (16, 16),
        (17, 33),
        (513, 513),
        (1025, 257),
        (16383, 16383),
    ] {
        for bits in [-8i32, -1, 0, 1, 2, 3, 4, 6, 8, 9, 10, 15] {
            for (min, max) in [(2, 9), (2, 5), (4, 9), (6, 6)] {
                for limit in [0, 1, 4, 16384] {
                    cases.push(Case::new(
                        "ClampBits",
                        vec![w, h, bits as u32, min, max, limit],
                    ));
                }
            }
        }
    }
    cases
}

pub fn transform_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (w, h) in [
        (1usize, 1usize),
        (1, 65),
        (65, 1),
        (7, 9),
        (17, 19),
        (65, 67),
        (513, 17),
    ] {
        for pattern in 0..8 {
            let pixels: Vec<_> = (0..w * h)
                .map(|i| {
                    let r = random(&mut seed);
                    match pattern {
                        0 => 0xff123456,
                        1 => r,
                        2 => r & 0xffffff,
                        3 => {
                            if i % 3 == 0 {
                                r & 0xffffff
                            } else {
                                r | 0xff000000
                            }
                        }
                        4 => 0xff000000 | ((i % w) as u32 * 65793),
                        5 => 0xff000000 | ((i / w) as u32 * 65793),
                        6 => {
                            if (i % w / 4 + i / w / 4) % 2 == 0 {
                                0
                            } else {
                                0xffffffff
                            }
                        }
                        _ => {
                            if i % w < w / 2 {
                                0xff000000 | ((i / w) as u32 * 65793)
                            } else {
                                r
                            }
                        }
                    }
                })
                .collect();
            for (bits, method, quality) in [
                (2i32, 0, 0),
                (6, 4, 25),
                (6, 5, 75),
                (9, 6, 100),
                (-3, 6, 50),
                (12, 0, 100),
            ] {
                for low in 0..=1 {
                    for exact in 0..=1 {
                        for prefix in [0, 5, 97] {
                            let mut a = vec![
                                w as u32,
                                h as u32,
                                bits as u32,
                                method,
                                quality,
                                low,
                                exact,
                                prefix,
                            ];
                            a.extend_from_slice(&pixels);
                            cases.push(Case::new("PredictorTransform", a));
                        }
                    }
                }
            }
        }
    }
    // Force the predictor-map size limit, with right/bottom partial tiles.
    for (w, h) in [(513usize, 513usize), (1025, 257)] {
        for method in [4, 6] {
            for low in 0..=1 {
                let mut a = vec![w as u32, h as u32, 2, method, 75, low, 1, 5];
                a.extend(
                    (0..w * h).map(|i| 0xff000000 | (((i % w + i / w) as u32 * 65793) & 0xffffff)),
                );
                cases.push(Case::new("PredictorTransform", a));
            }
        }
    }
    cases
}

pub fn search_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (w, h) in [
        (1usize, 1usize),
        (1, 65),
        (65, 1),
        (7, 9),
        (17, 19),
        (33, 35),
        (65, 67),
        (97, 37),
        (513, 17),
        (17, 513),
    ] {
        for pattern in 0..24 {
            let pixels: Vec<_> = (0..w * h)
                .map(|i| {
                    let r = random(&mut seed);
                    let x = i % w;
                    let y = i / w;
                    match pattern {
                        0..16 => {
                            let mut p = 0xff123456;
                            for plane in 0..4 {
                                if pattern & (1 << plane) != 0 {
                                    let shift = (3 - plane) * 8;
                                    p = (p & !(255 << shift)) | (r & (255 << shift));
                                }
                            }
                            p
                        }
                        16 => 0xff000000,
                        17 => r & 0xffffff,
                        18 => {
                            if i % 3 == 0 {
                                r & 0xffffff
                            } else {
                                r | 0xff000000
                            }
                        }
                        19 => 0xff000000 | (x as u32 * 65793),
                        20 => 0xff000000 | (y as u32 * 65793),
                        21 => {
                            if (x / 4 + y / 4) % 2 == 0 {
                                0
                            } else {
                                0xffffffff
                            }
                        }
                        22 => 0xff000000 | (((x * y + x + y) as u32 * 12345) & 0xffffff),
                        _ => {
                            if x < w / 2 {
                                0xff000000 | (y as u32 * 65793)
                            } else {
                                r
                            }
                        }
                    }
                })
                .collect();
            for (min, max) in [
                (2, 2),
                (2, 4),
                (2, 6),
                (3, 5),
                (4, 6),
                (5, 6),
                (6, 6),
                (2, 9),
                (7, 8),
                (9, 9),
            ] {
                for exact in 0..=1 {
                    let mut a = vec![6, w as u32, h as u32, min, max, 0, exact];
                    a.extend_from_slice(&pixels);
                    cases.push(Case::new("PredictorFoundations", a.clone()));
                    if (min, max) == (2, 6) {
                        a[5] = 1;
                        cases.push(Case::new("PredictorFoundations", a));
                    }
                }
            }
            if pattern == 23 {
                let mut a = vec![7, w as u32, h as u32, 2, 6, 0, 1];
                a.extend_from_slice(&pixels);
                cases.push(Case::new("PredictorFoundations", a));
            }
        }
    }
    cases
}
