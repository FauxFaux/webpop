use super::{Case, SEED, random};
use libwebpop::{
    dsp::yuv::*,
    enc::{picture_csp_enc::*, picture_enc::WebPYUVAPicture, picture_tools_enc::*},
};

pub fn picture(case: &Case) -> WebPYUVAPicture {
    let a = &case.input;
    let mut storage = vec![0xa5];
    storage.extend(a[4..].iter().map(|&v| v as u8));
    let before = storage.clone();
    let mut pic = import_rgba_to_yuva(&storage[1..], a[0], a[1], a[2] as usize).unwrap();
    assert_eq!(storage, before);
    if a[3] != 0 {
        WebPCleanupTransparentArea(&mut pic);
    }
    pic
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "YuvTables" => {
            let t = WebPInitGammaTables();
            t.gamma_to_linear
                .iter()
                .copied()
                .map(u32::from)
                .chain(t.linear_to_gamma.iter().map(|&v| v as u32))
                .chain(kInvAlpha)
                .collect()
        }
        "YuvGamma" => a
            .chunks_exact(2)
            .flat_map(|p| {
                [
                    Interpolate(p[0] << p[1]) as u32,
                    u32::from(LinearToGamma(p[0], p[1])),
                ]
            })
            .collect(),
        "YuvLuma" => vec![u32::from(VP8RGBToY(
            a[0] as i32,
            a[1] as i32,
            a[2] as i32,
            a[3] as i32,
        ))],
        "YuvUV" => vec![
            u32::from(VP8RGBToU(
                a[0] as i32,
                a[1] as i32,
                a[2] as i32,
                a[3] as i32,
            )),
            u32::from(VP8RGBToV(
                a[0] as i32,
                a[1] as i32,
                a[2] as i32,
                a[3] as i32,
            )),
        ],
        "YuvAccumulate" => {
            let (w, stride) = (a[0] as usize, a[1] as usize);
            let src: Vec<_> = a[3..].iter().map(|&v| v as u8).collect();
            let mut tmp = vec![0xa55a; 4 * w.div_ceil(2)];
            if a[2] != 0 {
                WebPAccumulateRGBA(&src, &src[1..], &src[2..], &src[3..], stride, &mut tmp, w);
            } else {
                WebPAccumulateRGB(&src, &src[1..], &src[2..], 4, stride, &mut tmp, w);
            }
            let mut u = vec![0; w.div_ceil(2)];
            let mut v = u.clone();
            WebPConvertRGBA32ToUV_C(&tmp, &mut u, &mut v, w.div_ceil(2));
            tmp.into_iter()
                .map(u32::from)
                .chain(u.into_iter().chain(v).map(u32::from))
                .collect()
        }
        "YuvaImport" => {
            let pic = picture(case);
            let mut out = vec![
                u32::from(has_yuva_transparency(&pic)),
                u32::from(pic.a.is_some()),
            ];
            out.extend(
                pic.y
                    .into_iter()
                    .chain(pic.u)
                    .chain(pic.v)
                    .chain(pic.a.into_iter().flatten())
                    .map(u32::from),
            );
            out
        }
        _ => unreachable!(),
    }
}

pub fn scalar_cases() -> Vec<Case> {
    let mut cases = vec![Case::new("YuvTables", [])];
    for shift in 0..=2 {
        let pairs: Vec<_> = (0..=(16380 >> shift)).flat_map(|v| [v, shift]).collect();
        for chunk in pairs.chunks(128) {
            cases.push(Case::new("YuvGamma", chunk));
        }
    }
    for v in 0..=255 {
        for rgb in [[v, v, v], [v, 0, 255], [255, v, 0], [0, 255, v]] {
            cases.push(Case::new(
                "YuvLuma",
                [rgb[0], rgb[1], rgb[2], YUV_HALF as u32],
            ));
            cases.push(Case::new(
                "YuvUV",
                [4 * rgb[0], 4 * rgb[1], 4 * rgb[2], (YUV_HALF << 2) as u32],
            ));
        }
    }
    let mut seed = SEED;
    for _ in 0..512 {
        cases.push(Case::new(
            "YuvLuma",
            [
                random(&mut seed) & 255,
                random(&mut seed) & 255,
                random(&mut seed) & 255,
                random(&mut seed) % 131073,
            ],
        ));
        cases.push(Case::new(
            "YuvUV",
            [
                random(&mut seed) % 1021,
                random(&mut seed) % 1021,
                random(&mut seed) % 1021,
                random(&mut seed) % 131073,
            ],
        ));
    }
    cases
}

pub fn image(width: u32, height: u32, pad: u32, pattern: u32, cleanup: bool) -> Case {
    let stride = width * 4 + pad;
    let mut src = vec![0xa5; ((height - 1) * stride + 4 * width) as usize];
    let mut seed = SEED ^ width ^ (height << 16) ^ pattern;
    for y in 0..height {
        for x in 0..width {
            let rgb = random(&mut seed);
            let alpha = match pattern {
                0 => 255,
                1 => 0,
                2 => (rgb >> 24) as u8,
                3 => {
                    if (x + y) % 3 == 0 {
                        0
                    } else {
                        255
                    }
                }
                4 => {
                    if x % 8 == 0 && y % 8 == 0 {
                        1
                    } else {
                        0
                    }
                }
                5 => {
                    if y % 4 < 2 {
                        255
                    } else {
                        (x * 17 + y * 29) as u8
                    }
                }
                6 => {
                    if (x / 8) % 4 == 2 {
                        255
                    } else {
                        0
                    }
                }
                7 => {
                    if y / 8 % 2 == 0 {
                        0
                    } else {
                        255
                    }
                }
                8 => 1,
                9 => 254,
                10 => {
                    if x == width - 1 || y == height - 1 {
                        0
                    } else {
                        255
                    }
                }
                _ => {
                    if (x ^ y) & 1 == 0 {
                        0
                    } else {
                        255
                    }
                }
            };
            let at = (y * stride + 4 * x) as usize;
            src[at..at + 4].copy_from_slice(&[
                rgb as u8,
                (rgb >> 8) as u8,
                (rgb >> 16) as u8,
                alpha,
            ]);
        }
    }
    let mut input = vec![width, height, stride, u32::from(cleanup)];
    input.extend(src.into_iter().map(u32::from));
    Case::new("YuvaImport", input)
}

pub fn accumulate_cases() -> Vec<Case> {
    let mut cases = vec![reciprocal_boundary_case()];
    for total in 0..=1020u32 {
        for reverse in [false, true] {
            let mut input = vec![2, 8, 1];
            let mut left = total;
            let alphas: Vec<_> = (0..4)
                .map(|_| {
                    let v = left.min(255);
                    left -= v;
                    v
                })
                .collect();
            for i in 0..4 {
                let a = alphas[if reverse { 3 - i } else { i }];
                input.extend([255 * (i as u32 & 1), 255 - 85 * i as u32, 85 * i as u32, a]);
            }
            cases.push(Case::new("YuvAccumulate", input));
        }
    }
    for w in [1, 2, 3, 7, 8, 15, 16, 17, 31, 32, 33, 16383] {
        for last in [false, true] {
            for weighted in [false, true] {
                for pattern in 0..12 {
                    let im = image(w, if last { 1 } else { 2 }, 7, pattern, false);
                    let mut input =
                        vec![w, if last { 0 } else { im.input[2] }, u32::from(weighted)];
                    input.extend_from_slice(&im.input[4..]);
                    cases.push(Case::new("YuvAccumulate", input));
                }
            }
        }
    }
    cases
}

pub fn reciprocal_boundary_case() -> Case {
    Case::new(
        "YuvAccumulate",
        [2, 8, 1, 5, 5, 5, 3, 5, 5, 5, 0, 5, 5, 5, 0, 5, 5, 5, 0],
    )
}

pub fn image_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 17),
        (17, 1),
        (3, 5),
        (4, 4),
        (7, 9),
        (8, 8),
        (9, 7),
        (15, 17),
        (16, 16),
        (17, 15),
        (31, 33),
        (32, 32),
        (33, 31),
        (48, 24),
        (49, 25),
        (16383, 1),
        (1, 16383),
    ] {
        for pattern in 0..12 {
            for cleanup in [false, true] {
                cases.push(image(
                    w,
                    h,
                    if pattern % 2 == 0 { 0 } else { 7 },
                    pattern,
                    cleanup,
                ));
            }
        }
    }
    let sample = super::public_images::real_image();
    for cleanup in [false, true] {
        let mut input = vec![
            sample.width,
            sample.height,
            sample.stride as u32,
            u32::from(cleanup),
        ];
        input.extend(sample.rgba.iter().copied().map(u32::from));
        cases.push(Case::new("YuvaImport", input));
    }
    cases
}

pub fn cases() -> Vec<Case> {
    scalar_cases()
        .into_iter()
        .chain(accumulate_cases())
        .chain(image_cases())
        .collect()
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = vec![Case::new("YuvTables", []), reciprocal_boundary_case()];
    for shift in 0..=2 {
        cases.push(Case::new(
            "YuvGamma",
            [
                0, 1, 63, 64, 127, 128, 129, 255, 256, 511, 512, 513, 4095, 8191, 16380,
            ]
            .into_iter()
            .map(|v| v >> shift)
            .flat_map(|v| [v, shift])
            .collect::<Vec<_>>(),
        ));
    }
    cases.extend(
        scalar_cases()
            .into_iter()
            .filter(|c| c.function != "YuvTables" && c.function != "YuvGamma")
            .step_by(151),
    );
    cases.extend(
        accumulate_cases()
            .into_iter()
            .filter(|c| c.input[0] <= 33)
            .step_by(71),
    );
    for (w, h) in [(1, 1), (1, 9), (9, 1), (3, 5), (17, 9), (25, 8)] {
        for pattern in [0, 1, 4, 6] {
            cases.push(image(w, h, 7, pattern, true));
        }
    }
    cases
}
