use super::{Case, SEED, random};
use libwebpop::internal::sharpyuv::{sharpyuv_dsp::*, sharpyuv_gamma::*, *};

pub fn matrix_words(m: &SharpYuvConversionMatrix) -> Vec<u32> {
    m.rgb_to_y
        .iter()
        .chain(&m.rgb_to_u)
        .chain(&m.rgb_to_v)
        .map(|&v| v as u32)
        .collect()
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    let a = &case.input;
    match case.function.as_str() {
        "SharpYuvPublicInfo" => {
            let mut out = vec![SharpYuvGetVersion()];
            match SharpYuvOptionsInitInternal(&MATRICES[0], a[0]) {
                Ok(o) => out.extend([1, o.transfer_type as u32]),
                Err(_) => out.push(0),
            }
            out
        }
        "SharpYuvConvert" | "SharpYuvTrace" => {
            let mut out = vec![1];
            let trace = case.function == "SharpYuvTrace";
            let (rgb, output) = convert_case(case, |s| {
                if trace {
                    out.push(s.iteration.unwrap_or(u32::MAX));
                    out.extend(super::words64(s.diff_y_sum));
                    out.extend(s.best_y.iter().chain(s.target_y).map(|&v| u32::from(v)));
                    out.extend(
                        s.best_uv
                            .iter()
                            .chain(s.target_uv)
                            .map(|&v| v as i32 as u32),
                    );
                }
            });
            drop(rgb);
            match output {
                SharpYuvOutput::U8(p) => {
                    out.extend(p.y.iter().chain(&p.u).chain(&p.v).map(|&v| u32::from(v)))
                }
                SharpYuvOutput::U16(p) => {
                    out.extend(p.y.iter().chain(&p.u).chain(&p.v).map(|&v| u32::from(v)))
                }
            }
            out
        }
        "SharpYuvImport" => {
            use libwebpop::internal::sharpyuv::sharpyuv::*;
            with_input(case, |input| {
                let w = ((a[0] + 1) & !1) as usize;
                let mut rows = vec![0; 6 * w];
                ImportOneRow(input, 0, a[4], &mut rows);
                ImportOneRow(input, usize::from(a[1] > 1), a[4], &mut rows[3 * w..]);
                let mut out: Vec<u32> = rows.iter().map(|&v| u32::from(v)).collect();
                let mut gray = vec![0; 2 * w];
                StoreGray(&rows, &mut gray, w);
                StoreGray(&rows[3 * w..], &mut gray[w..], w);
                out.extend(gray.iter().map(|&v| u32::from(v)));
                let depth = (a[4] as i32 + GetPrecisionShift(a[4])) as u32;
                let tf = a[6].try_into().unwrap();
                UpdateW(&rows, &mut gray, w, depth, tf);
                UpdateW(&rows[3 * w..], &mut gray[w..], w, depth, tf);
                out.extend(gray.iter().map(|&v| u32::from(v)));
                let mut uv = vec![0; 3 * w / 2];
                UpdateChroma(&rows, &rows[3 * w..], &mut uv, w / 2, depth, tf);
                out.extend(uv.iter().map(|&v| v as i32 as u32));
                out
            })
        }
        "SharpYuvDsp" => {
            let (op, depth, len, sfix) = (a[0], a[1], a[2] as usize, a[3] as i32);
            let m: [i32; 12] = std::array::from_fn(|i| a[4 + i] as i32);
            let p: Vec<u16> = a[16..].iter().map(|&v| v as u16).collect();
            let s: Vec<i16> = p.iter().map(|&v| v as i16).collect();
            let out_len = if op == 2 {
                2 * len
            } else if op == 7 {
                3 * len
            } else {
                len
            };
            let mut dst = vec![0xa5a5; out_len + 2];
            let mut sd = vec![0xa55au16 as i16; out_len + 2];
            let mut prefix = Vec::new();
            match op {
                0 => {
                    dst[1..len + 1].copy_from_slice(&p[2 * len..3 * len]);
                    let diff = SharpYuvUpdateY(&p, &p[len..], &mut dst[1..], len, depth);
                    prefix.extend(super::words64(diff));
                }
                1 => {
                    sd[1..len + 1].copy_from_slice(&s[2 * len..3 * len]);
                    SharpYuvUpdateRGB(&s, &s[len..], &mut sd[1..], len);
                    return sd.iter().map(|&v| v as i32 as u32).collect();
                }
                2 => SharpYuvFilterRow(
                    &s,
                    &s[len + 1..],
                    len,
                    &p[2 * (len + 1)..],
                    &mut dst[1..],
                    depth,
                ),
                3 => SharpYuvConvertRowY(
                    &p,
                    &s[len..],
                    len,
                    len.div_ceil(2),
                    m[..4].try_into().unwrap(),
                    sfix,
                    depth,
                    &mut dst[1..],
                ),
                4 => {
                    let mut v = dst.clone();
                    SharpYuvConvertRowUV(
                        &s,
                        len,
                        m[4..8].try_into().unwrap(),
                        m[8..12].try_into().unwrap(),
                        sfix,
                        depth,
                        &mut dst[1..],
                        &mut v[1..],
                    );
                    if depth == 8 {
                        dst[len + 1] = 0xa5;
                        v[len + 1] = dst[len + 1];
                    }
                    dst.extend(v);
                }
                5 => SharpYuvUpdateWSrgb(&p, &mut dst[1..], len, depth),
                7 => {
                    SharpYuvUpdateChromaSrgb(&p, &p[6 * len..], &mut sd[1..], len, depth);
                    return sd.iter().map(|&v| v as i32 as u32).collect();
                }
                _ => unreachable!(),
            }
            if op == 3 && depth == 8 {
                dst[len + 1] = 0xa5;
            }
            prefix.extend(dst.iter().map(|&v| u32::from(v)));
            prefix
        }
        "SharpYuvTables" => {
            let t = SharpYuvInitGammaTables();
            t.gamma_to_linear
                .iter()
                .chain(&t.linear_to_gamma)
                .copied()
                .collect()
        }
        "SharpYuvGamma" => {
            let tf = SharpYuvTransferFunctionType::try_from(a[2]).unwrap();
            (a[3]..a[3] + a[4])
                .map(|v| {
                    if a[0] == 0 {
                        SharpYuvGammaToLinear(v as u16, a[1], tf)
                    } else {
                        u32::from(SharpYuvLinearToGamma(v, a[1], tf))
                    }
                })
                .collect()
        }
        "SharpYuvMatrix" => {
            let m = if a[0] < 5 {
                MATRICES[a[0] as usize]
            } else {
                SharpYuvComputeConversionMatrix(&SharpYuvColorSpace {
                    kr: f32::from_bits(a[1]),
                    kb: f32::from_bits(a[2]),
                    bit_depth: a[3],
                    range: if a[4] == 0 {
                        SharpYuvRange::kSharpYuvRangeFull
                    } else {
                        SharpYuvRange::kSharpYuvRangeLimited
                    },
                })
                .unwrap()
            };
            matrix_words(&m)
        }
        "SharpYuvMath" => vec![
            SharpYuvClip16(a[4] as i32, a[3] as i32) as u32,
            SharpYuvRGBToGray(
                i64::from(a[0] as i32),
                i64::from(a[1] as i32),
                i64::from(a[2] as i32),
            ) as u32,
            SharpYuvConvertComponent(
                a[0] as i32,
                a[1] as i32,
                a[2] as i32,
                &std::array::from_fn(|i| a[6 + i] as i32),
                a[5] as i32,
            ) as u32,
        ],
        "SharpYuvFloat" => {
            let fns: [fn(f32) -> f32; 23] = [
                ToLinear709,
                FromLinear709,
                ToLinear470M,
                FromLinear470M,
                ToLinear470Bg,
                FromLinear470Bg,
                ToLinearSmpte240,
                FromLinearSmpte240,
                ToLinearLog100,
                FromLinearLog100,
                ToLinearLog100Sqrt10,
                FromLinearLog100Sqrt10,
                ToLinearIec61966,
                FromLinearIec61966,
                ToLinearBt1361,
                FromLinearBt1361,
                ToLinearPq,
                FromLinearPq,
                ToLinearSmpte428,
                FromLinearSmpte428,
                ToLinearHlg,
                FromLinearHlg,
                Roundf,
            ];
            let x = f32::from_bits(a[1]);
            let v = match a[0] {
                23 => Powf(x, f32::from_bits(a[2])),
                24 => Log10f(x),
                _ => fns[a[0] as usize](x),
            };
            vec![if v.is_nan() { 0x7fc00000 } else { v.to_bits() }]
        }
        name => panic!("unknown SharpYUV case: {name}"),
    }
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = vec![Case::new("SharpYuvTables", vec![])];
    for kind in 0..5 {
        cases.push(Case::new("SharpYuvMatrix", vec![kind]));
    }
    for depth in [8, 10, 12] {
        for range in 0..2 {
            cases.push(Case::new(
                "SharpYuvMatrix",
                vec![5, 0.2126f32.to_bits(), 0.0722f32.to_bits(), depth, range],
            ));
        }
    }
    for tf in (1..=18).filter(|v| *v != 2 && *v != 3) {
        for inverse in 0..2 {
            cases.push(Case::new("SharpYuvGamma", vec![inverse, 14, tf, 0, 17]));
        }
    }
    for (r, g, b) in [
        (0, 0, 0),
        (65536, 65536, 65536),
        (71501, 71501, 71501),
        (-16384, 3, 16383),
    ] {
        let mut a = vec![r as u32, g as u32, b as u32, 16383, (-1i32) as u32, 2];
        a.extend(MATRICES[0].rgb_to_y.map(|v| v as u32));
        cases.push(Case::new("SharpYuvMath", a));
    }
    for op in 0..25 {
        cases.push(Case::new(
            "SharpYuvFloat",
            vec![op, 0.5f32.to_bits(), 0.45f32.to_bits()],
        ));
    }
    cases.extend(
        dsp_cases()
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| if i % 19 == 0 { Some(c) } else { None }),
    );
    for rgb in [8, 10, 12, 16] {
        for yuv in [8, 10, 12] {
            cases.push(image(3, 5, rgb, yuv, 13, 0, 4, 4, 2));
        }
    }
    for rgb in [8, 16] {
        for tf in [8, 13, 17, 18] {
            let mut c = image(3, 5, rgb, 8, tf, 3, 2, 3, 2);
            c.function = "SharpYuvTrace".into();
            cases.push(c.clone());
            c.function = "SharpYuvImport".into();
            cases.push(c);
        }
    }
    cases
}

pub fn leaf_cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    for depth in [8, 10, 12, 14, 16] {
        for tf in (1..=18).filter(|v| *v != 2 && *v != 3) {
            cases.push(Case::new(
                "SharpYuvGamma",
                vec![0, depth, tf, 0, 1 << depth],
            ));
            let inverse_count = if depth == 16 && tf != 13 {
                65536
            } else {
                65537
            };
            cases.push(Case::new(
                "SharpYuvGamma",
                vec![1, depth, tf, 0, inverse_count],
            ));
            if tf == 17 && depth <= 14 {
                cases.push(Case::new(
                    "SharpYuvGamma",
                    vec![1, depth, tf, 65537, 71502 - 65537],
                ));
            }
        }
    }
    let mut state = SEED;
    for _ in 0..1000 {
        let kr = (random(&mut state) % 490000) as f32 / 1e6;
        let kb = (random(&mut state) % 490000) as f32 / 1e6;
        cases.push(Case::new(
            "SharpYuvMatrix",
            vec![
                5,
                kr.to_bits(),
                kb.to_bits(),
                [8, 10, 12][(random(&mut state) % 3) as usize],
                random(&mut state) % 2,
            ],
        ));
        let mut a: Vec<_> = (0..3)
            .map(|_| ((random(&mut state) % 120001) as i32 - 40000) as u32)
            .collect();
        a.extend([
            16383,
            random(&mut state),
            [(-2i32) as u32, 0, 2][(random(&mut state) % 3) as usize],
        ]);
        a.extend(
            MATRICES[(random(&mut state) % 5) as usize]
                .rgb_to_u
                .map(|v| v as u32),
        );
        cases.push(Case::new("SharpYuvMath", a));
    }
    let boundaries = [
        0.0f32,
        0.018053968510807,
        4.5 * 0.018053968510807,
        0.022821585529445,
        4.0 * 0.022821585529445,
        0.01,
        0.00316227766,
        0.5,
        1.0 / 12.0,
        1.0,
    ];
    for op in 0..25 {
        for x in boundaries
            .into_iter()
            .flat_map(|v| [v.next_down(), v, v.next_up(), -v])
        {
            cases.push(Case::new(
                "SharpYuvFloat",
                vec![op, x.to_bits(), 0.45f32.to_bits()],
            ));
        }
    }
    cases
}

pub fn dsp_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut state = SEED;
    for op in [0, 1, 2, 3, 4, 5, 7] {
        for depth in [8, 10, 12, 14] {
            for len in [0, 1, 2, 3, 7, 16, 31, 64, 128] {
                if len == 0 && (op == 5 || op == 7) {
                    continue;
                }
                for mode in 0..4 {
                    let mut a = vec![op, depth, len, [-2i32, 0, 2][(mode % 3) as usize] as u32];
                    a.extend(matrix_words(&MATRICES[mode as usize]));
                    let count = match op {
                        0 | 1 => 3 * len,
                        2 => 4 * len + 2,
                        3 => len + 3 * len.div_ceil(2),
                        4 | 5 => 3 * len,
                        7 => 12 * len,
                        _ => unreachable!(),
                    };
                    for i in 0..count {
                        let mask = if op == 5 || op == 7 {
                            (1 << depth) - 1
                        } else {
                            65535
                        };
                        a.push(match mode {
                            0 => 0,
                            1 => mask,
                            2 => {
                                if i % 2 == 0 {
                                    0
                                } else {
                                    mask
                                }
                            }
                            _ => random(&mut state) & mask,
                        });
                    }
                    cases.push(Case::new("SharpYuvDsp", a));
                }
            }
        }
    }
    cases
}

pub fn convert_case(
    case: &Case,
    observe: impl FnMut(libwebpop::internal::sharpyuv::sharpyuv::SharpYuvIteration<'_>),
) -> (Vec<u16>, SharpYuvOutput) {
    let a = &case.input;
    let matrix = SharpYuvConversionMatrix {
        rgb_to_y: std::array::from_fn(|i| a[8 + i] as i32),
        rgb_to_u: std::array::from_fn(|i| a[12 + i] as i32),
        rgb_to_v: std::array::from_fn(|i| a[16 + i] as i32),
    };
    let mut options = SharpYuvOptionsInit(&matrix);
    options.transfer_type = if a[7] != 0 {
        SharpYuvTransferFunctionType::kSharpYuvTransferFunctionSrgb
    } else {
        a[6].try_into().unwrap()
    };
    let rgb: Vec<u16> = a[20..].iter().map(|&v| v as u16).collect();
    let len = rgb.len() / 3;
    let result = if a[4] == 8 {
        let bytes: Vec<u8> = rgb.iter().map(|&v| v as u8).collect();
        let input = SharpYuvRgb::U8(SharpYuvRgbView {
            r: &bytes[..len],
            g: &bytes[len..2 * len],
            b: &bytes[2 * len..],
            width: a[0],
            height: a[1],
            step: a[2] as usize,
            stride: a[3] as usize,
        });
        if a[7] != 0 {
            SharpYuvConvert(input, a[4], a[5], &matrix)
        } else {
            libwebpop::internal::sharpyuv::sharpyuv::convert_with_observer(
                input, a[4], a[5], &options, observe,
            )
        }
    } else {
        let input = SharpYuvRgb::U16(SharpYuvRgbView {
            r: &rgb[..len],
            g: &rgb[len..2 * len],
            b: &rgb[2 * len..],
            width: a[0],
            height: a[1],
            step: a[2] as usize,
            stride: a[3] as usize,
        });
        if a[7] != 0 {
            SharpYuvConvert(input, a[4], a[5], &matrix)
        } else {
            libwebpop::internal::sharpyuv::sharpyuv::convert_with_observer(
                input, a[4], a[5], &options, observe,
            )
        }
    }
    .unwrap();
    assert_eq!(rgb, a[20..].iter().map(|&v| v as u16).collect::<Vec<_>>());
    (rgb, result)
}

pub fn with_input<T>(case: &Case, f: impl FnOnce(SharpYuvRgb<'_>) -> T) -> T {
    let a = &case.input;
    let rgb: Vec<u16> = a[20..].iter().map(|&v| v as u16).collect();
    let len = rgb.len() / 3;
    if a[4] == 8 {
        let bytes: Vec<u8> = rgb.iter().map(|&v| v as u8).collect();
        f(SharpYuvRgb::U8(SharpYuvRgbView {
            r: &bytes[..len],
            g: &bytes[len..2 * len],
            b: &bytes[2 * len..],
            width: a[0],
            height: a[1],
            step: a[2] as usize,
            stride: a[3] as usize,
        }))
    } else {
        f(SharpYuvRgb::U16(SharpYuvRgbView {
            r: &rgb[..len],
            g: &rgb[len..2 * len],
            b: &rgb[2 * len..],
            width: a[0],
            height: a[1],
            step: a[2] as usize,
            stride: a[3] as usize,
        }))
    }
}

pub fn image(
    width: u32,
    height: u32,
    rgb_depth: u32,
    yuv_depth: u32,
    tf: u32,
    kind: u32,
    pattern: u32,
    step: u32,
    padding: u32,
) -> Case {
    let stride = width * step + padding;
    let mut a = vec![width, height, step, stride, rgb_depth, yuv_depth, tf, 0];
    let mut matrix = MATRICES[kind as usize];
    // Public conversion scales coefficients to the output depth, while offsets
    // pass through unchanged after removing the internal precision shift.
    let shift = yuv_depth - 8;
    for row in [
        &mut matrix.rgb_to_y,
        &mut matrix.rgb_to_u,
        &mut matrix.rgb_to_v,
    ] {
        row[3] <<= shift;
    }
    a.extend(matrix_words(&matrix));
    let len = (height - 1) * stride + (width - 1) * step + 1;
    let max = (1u32 << rgb_depth) - 1;
    let mut state = SEED ^ width ^ (height << 16) ^ pattern;
    for k in 0..3 {
        let mut plane = vec![0xa5; len as usize];
        for y in 0..height {
            for x in 0..width {
                plane[(y * stride + x * step) as usize] = match pattern {
                    0 => 0,
                    1 => max,
                    2 => {
                        if (x + y + k) % 2 == 0 {
                            0
                        } else {
                            max
                        }
                    }
                    3 => (x * 129 + y * 313 + k * 103) & max,
                    4 => random(&mut state) & max,
                    5 => {
                        if rgb_depth == 8 {
                            255
                        } else {
                            65535
                        }
                    }
                    _ => {
                        if x < width / 2 {
                            max
                        } else {
                            0
                        }
                    }
                };
            }
        }
        a.extend(plane);
    }
    Case::new("SharpYuvConvert", a)
}

pub fn public_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for rgb in [8, 10, 12, 16] {
        for yuv in [8, 10, 12] {
            for tf in (1..=18).filter(|v| *v != 2 && *v != 3) {
                for (w, h) in [(1, 1), (1, 9), (9, 1), (2, 2), (3, 5), (8, 7), (17, 16)] {
                    for pattern in [0, 1, 2, 4, 5] {
                        let kind = (tf + pattern) % 5;
                        cases.push(image(
                            w,
                            h,
                            rgb,
                            yuv,
                            tf,
                            kind,
                            pattern,
                            1 + pattern % 4,
                            pattern % 3,
                        ));
                    }
                }
            }
        }
    }
    for v in [
        0,
        1,
        0x00030000,
        0x00040000,
        0x0004ffff,
        0x00050000,
        0x01040002,
        u32::MAX,
    ] {
        cases.push(Case::new("SharpYuvPublicInfo", vec![v]));
    }
    for rgb in [8, 10, 12, 16] {
        for (w, h) in [(20000, 1), (1, 20000)] {
            cases.push(image(w, h, rgb, 8, 13, 0, 4, 1, 0));
        }
    }
    for kind in 0..5 {
        let mut c = image(17, 9, 8, 8, 13, kind, 4, 4, 3);
        cases.push(c.clone());
        c.input[7] = 1;
        cases.push(c);
    }
    for depth in [8, 10, 12] {
        for (kr, kb) in [(0.2627f32, 0.0593f32), (0.1, 0.2), (0.0, 0.0), (0.7, 0.25)] {
            for range in [
                SharpYuvRange::kSharpYuvRangeFull,
                SharpYuvRange::kSharpYuvRangeLimited,
            ] {
                let m = SharpYuvComputeConversionMatrix(&SharpYuvColorSpace {
                    kr,
                    kb,
                    bit_depth: depth,
                    range,
                })
                .unwrap();
                let mut c = image(13, 15, depth, 8, 13, 0, 4, 3, 1);
                c.input[8..20].copy_from_slice(&matrix_words(&m));
                cases.push(c);
            }
        }
    }
    cases.push(real_image(8));
    cases.push(real_image(12));
    cases
}

pub fn real_image(depth: u32) -> Case {
    let ppm = include_bytes!("../fixtures/corpus/test_ref.ppm");
    let header = b"P6\n128 128\n255\n";
    assert!(ppm.starts_with(header));
    let mut a = vec![128, 128, 1, 128, depth, depth, 13, 0];
    let mut matrix = MATRICES[0];
    for row in [
        &mut matrix.rgb_to_y,
        &mut matrix.rgb_to_u,
        &mut matrix.rgb_to_v,
    ] {
        row[3] <<= depth - 8;
    }
    a.extend(matrix_words(&matrix));
    for k in 0..3 {
        a.extend(
            ppm[header.len()..]
                .chunks_exact(3)
                .map(|rgb| u32::from(rgb[k]) << (depth - 8)),
        );
    }
    Case::new("SharpYuvConvert", a)
}

pub fn trace_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for rgb in [8, 10, 12, 16] {
        for tf in (1..=18).filter(|v| *v != 2 && *v != 3) {
            for (w, h) in [(1, 1), (1, 5), (5, 1), (3, 7), (12, 16), (17, 9)] {
                for pattern in [1, 2, 4] {
                    let mut case = image(w, h, rgb, 8, tf, tf % 5, pattern, 3, 2);
                    case.function = "SharpYuvTrace".into();
                    cases.push(case.clone());
                    case.function = "SharpYuvImport".into();
                    cases.push(case);
                }
            }
        }
    }
    cases
}
