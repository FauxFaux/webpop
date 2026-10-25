use super::{Case, SEED, random};
use libwebpop::internal::enc::predictor_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    let cost_words = |cost: i64| vec![cost as u32, ((cost as u64) >> 32) as u32];
    match a[0] {
        0 => {
            let m = ColorCodeToMultipliers(a[1]);
            vec![
                m.green_to_red as u32,
                m.green_to_blue as u32,
                m.red_to_blue as u32,
                MultipliersToColorCode(&m),
            ]
        }
        1 => cost_words(PredictionCostCrossColor(
            a[1..257].try_into().unwrap(),
            a[257..].try_into().unwrap(),
        )),
        2..=4 => {
            let (stride, width, height) = (a[1] as usize, a[2] as usize, a[3] as usize);
            let (prev_x, prev_y) = (ColorCodeToMultipliers(a[4]), ColorCodeToMultipliers(a[5]));
            let header = if a[0] == 3 { 8 } else { 7 };
            let accumulated = a[header..header + 256].try_into().unwrap();
            if a[0] == 2 {
                cost_words(GetPredictionCostCrossColorRed(
                    &a[header + 256..],
                    stride,
                    width,
                    height,
                    prev_x,
                    prev_y,
                    a[6] as i32,
                    accumulated,
                ))
            } else if a[0] == 3 {
                cost_words(GetPredictionCostCrossColorBlue(
                    &a[header + 256..],
                    stride,
                    width,
                    height,
                    prev_x,
                    prev_y,
                    a[6] as i32,
                    a[7] as i32,
                    accumulated,
                ))
            } else {
                let tile = &a[header + 512..];
                let green_to_red = GetBestGreenToRed(
                    tile,
                    stride,
                    width,
                    height,
                    prev_x,
                    prev_y,
                    a[6],
                    accumulated,
                );
                let (green_to_blue, red_to_blue) = GetBestGreenRedToBlue(
                    tile,
                    stride,
                    width,
                    height,
                    prev_x,
                    prev_y,
                    a[6],
                    a[header + 256..header + 512].try_into().unwrap(),
                );
                vec![MultipliersToColorCode(
                    &libwebpop::internal::dsp::lossless::VP8LMultipliers {
                        green_to_red,
                        green_to_blue,
                        red_to_blue,
                    },
                )]
            }
        }
        5 => {
            let mut pixels = a[5..].to_vec();
            let (bits, map) =
                VP8LColorSpaceTransform(a[1] as usize, a[2] as usize, a[3], a[4], &mut pixels)
                    .unwrap();
            let mut out = vec![bits];
            out.extend(map);
            out.extend(pixels);
            out
        }
        6 => {
            let mut pixels = a[7..].to_vec();
            CopyTileWithColorTransform(
                a[1] as usize,
                a[2] as usize,
                a[3] as usize,
                a[4] as usize,
                a[5] as usize,
                ColorCodeToMultipliers(a[6]),
                &mut pixels,
            );
            pixels
        }
        _ => unreachable!(),
    }
}

pub fn pixels(width: usize, height: usize, pattern: u32, seed: &mut u32) -> Vec<u32> {
    (0..width * height)
        .map(|i| {
            let r = random(seed);
            let x = i % width;
            let y = i / width;
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
                4 => 0xff000000 | ((r & 255) * 65793),
                5 => {
                    0xff000000
                        | ((r & 255) << 8)
                        | ((r.wrapping_mul(2) & 255) << 16)
                        | (r.wrapping_mul(3) & 255)
                }
                6 => 0xff000000 | ((x as u32 * 65793) & 0xffffff),
                7 => 0xff000000 | ((y as u32 * 65793) & 0xffffff),
                8 => {
                    if (x / 4 + y / 4) % 2 == 0 {
                        0
                    } else {
                        0xffffffff
                    }
                }
                9 => 0xff000000 | (((x / 3) as u32 * 65793) & 0xffffff),
                10 => 0xff000000 | (((x % 3) as u32 * 654321) & 0xffffff),
                _ => {
                    if x < width / 2 {
                        0xff000000 | ((r & 255) * 65793)
                    } else {
                        r
                    }
                }
            }
        })
        .collect()
}

pub const QUALITIES: [u32; 10] = [0, 24, 25, 36, 37, 50, 51, 73, 74, 100];

pub fn leaf_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for code in [0, u32::MAX, 0xff000000, 0xff7f8081, 0x00ff00ff]
        .into_iter()
        .chain((0..256).map(|_| random(&mut seed)))
    {
        cases.push(Case::new("CrossColor", vec![0, code]));
    }
    for pattern in 0..24 {
        let mut a = vec![1];
        a.extend((0..256).map(|i| {
            if pattern == 0 {
                0
            } else {
                (i * pattern) % 1000
            }
        }));
        a.extend((0..256).map(|i| match pattern {
            0 => 0,
            1 => u32::from(i == 0) * 100,
            2 => 100,
            _ => random(&mut seed) % 10000,
        }));
        cases.push(Case::new("CrossColor", a));
    }
    for (w, h, stride) in [(1, 1, 3), (1, 9, 2), (9, 1, 11), (7, 9, 10), (17, 19, 20)] {
        for pattern in 0..12 {
            let source = pixels(w, h, pattern, &mut seed);
            let mut tile = vec![0xdeadbeef; (h - 1) * stride + w];
            for y in 0..h {
                tile[y * stride..y * stride + w].copy_from_slice(&source[y * w..(y + 1) * w]);
            }
            let accumulated: Vec<_> = (0..512)
                .map(|_| {
                    if pattern == 0 {
                        0
                    } else {
                        random(&mut seed) % 10000
                    }
                })
                .collect();
            for (prev_x, prev_y) in [
                (0, 0),
                (0xff010203, 0xfffefdfc),
                (0xff808080, 0xff7f7f7f),
                (0xffe02020, 0xffe02020),
            ] {
                for candidate in [-256i32, -32, -1, 0, 1, 32, 256] {
                    for op in [2, 3] {
                        let mut a = vec![
                            op,
                            stride as u32,
                            w as u32,
                            h as u32,
                            prev_x,
                            prev_y,
                            candidate as u32,
                        ];
                        if op == 3 {
                            a.push((-candidate + 1) as u32);
                        }
                        a.extend_from_slice(&accumulated[..256]);
                        a.extend_from_slice(&tile);
                        cases.push(Case::new("CrossColor", a));
                    }
                }
                for quality in QUALITIES {
                    let mut a = vec![
                        4,
                        stride as u32,
                        w as u32,
                        h as u32,
                        prev_x,
                        prev_y,
                        quality,
                    ];
                    a.extend_from_slice(&accumulated);
                    a.extend_from_slice(&tile);
                    cases.push(Case::new("CrossColor", a));
                }
            }
        }
    }
    for (w, h) in [(1, 1), (7, 9), (17, 19)] {
        let source = pixels(w, h, 1, &mut seed);
        for (x, y) in [(0, 0), (w - 1, h - 1), (w / 2, h / 2)] {
            for size in [1, 4, 512] {
                for code in [0xff000000, 0xff7f8081, 0xffe02020] {
                    let mut a = vec![6, w as u32, h as u32, x as u32, y as u32, size, code];
                    a.extend_from_slice(&source);
                    cases.push(Case::new("CrossColor", a));
                }
            }
        }
    }
    cases
}

pub fn search_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (w, h) in [
        (1, 1),
        (1, 65),
        (65, 1),
        (7, 9),
        (17, 19),
        (33, 35),
        (65, 67),
        (513, 17),
        (17, 513),
    ] {
        for pattern in 0..12 {
            let source = pixels(w, h, pattern, &mut seed);
            for bits in [2, 3, 4, 6, 7, 9] {
                for quality in QUALITIES {
                    let mut a = vec![5, w as u32, h as u32, bits, quality];
                    a.extend_from_slice(&source);
                    cases.push(Case::new("CrossColor", a));
                }
            }
        }
    }
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let all = leaf_cases();
    let mut selected = Vec::new();
    for (op, step) in [(0, 41), (1, 7), (2, 137), (3, 137), (4, 211), (6, 11)] {
        selected.extend(
            all.iter()
                .filter(|c| c.input[0] == op)
                .step_by(step)
                .cloned(),
        );
    }
    selected.extend(search_cases().into_iter().step_by(367));
    let transforms = transform_cases();
    selected.extend(transforms.iter().step_by(211).cloned());
    selected.push(transforms.last().unwrap().clone());
    selected
}

pub fn transform_evaluate(case: &Case) -> Vec<u32> {
    use libwebpop::internal::enc::{backward_references_enc::*, vp8l_enc::*};
    use libwebpop::internal::utils::bit_writer_utils::*;
    let a = &case.input;
    let (width, height, quality, low, exact) =
        (a[0] as usize, a[1] as usize, a[3], a[4] != 0, a[5] != 0);
    let mut pixels = a[8..].to_vec();
    let mut chain = VP8LHashChainInit(pixels.len()).unwrap();
    let mut refs = [VP8LBackwardRefsInit(256), VP8LBackwardRefsInit(256)];
    let mut bw = VP8LBitWriterInit(0).unwrap();
    let mut remaining = a[7];
    while remaining > 0 {
        let bits = remaining.min(32);
        VP8LPutBits(&mut bw, 0x05ad5ad5 & (u32::MAX >> (32 - bits)), bits).unwrap();
        remaining -= bits;
    }
    if a[6] & 1 != 0 {
        ApplySubtractGreen(&mut bw, &mut pixels).unwrap();
    }
    if a[6] & 2 != 0 {
        ApplyPredictFilter(
            &mut bw,
            &mut pixels,
            &mut chain,
            &mut refs,
            width,
            height,
            6,
            6,
            quality,
            low,
            100,
            exact,
            false,
        )
        .unwrap();
    }
    let bits = ApplyCrossColorFilter(
        &mut bw,
        &mut pixels,
        &mut chain,
        &mut refs,
        width,
        height,
        a[2],
        quality,
        low,
    )
    .unwrap();
    VP8LPutBits(&mut bw, 0x5ad, 11).unwrap();
    let mut out = vec![bits];
    out.extend(pixels);
    out.extend(
        VP8LBitWriterFinish(&mut bw)
            .unwrap()
            .iter()
            .map(|&b| u32::from(b)),
    );
    out
}

pub fn transform_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut seed = SEED;
    for (w, h) in [(1, 1), (7, 9), (17, 19), (65, 67)] {
        for pattern in [0, 1, 2, 4, 5, 10] {
            let source = pixels(w, h, pattern, &mut seed);
            for (bits, quality) in [(2, 0), (2, 51), (4, 25), (9, 100)] {
                for low in 0..=1 {
                    for exact in 0..=1 {
                        for preceding in 0..4 {
                            for prefix in [0, 5, 97] {
                                let mut a = vec![
                                    w as u32, h as u32, bits, quality, low, exact, preceding,
                                    prefix,
                                ];
                                a.extend_from_slice(&source);
                                cases.push(Case::new("CrossColorTransform", a));
                            }
                        }
                    }
                }
            }
        }
    }
    for pattern in [1, 4] {
        let source = pixels(513, 17, pattern, &mut seed);
        for bits in [2, 9] {
            for preceding in [0, 3] {
                for low in 0..=1 {
                    let mut a = vec![513, 17, bits, 100, low, 1, preceding, 97];
                    a.extend_from_slice(&source);
                    cases.push(Case::new("CrossColorTransform", a));
                }
            }
        }
    }
    cases
}
