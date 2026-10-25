use super::{Case, SEED, random};
use libwebpop::{
    dsp::{alpha_processing::*, filters::*},
    enc::{
        alpha_enc::*, analysis_enc::VP8EncAnalyze, frame_enc::VP8EncTokenLoop,
        picture_csp_enc::import_rgba_to_yuva, picture_tools_enc::WebPCleanupTransparentArea,
        syntax_enc::VP8EncWriteWithAlpha, vp8i_enc::VP8Encoder,
    },
    utils::{
        bit_writer_utils::*, filters_utils::WebPEstimateBestFilter,
        quant_levels_utils::QuantizeLevels,
    },
};

pub fn file(a: &[u32]) -> Vec<u8> {
    let rgba: Vec<_> = a[5..].iter().map(|&v| v as u8).collect();
    let q = f32::from_bits(a[3]);
    let mut pic = import_rgba_to_yuva(&rgba, a[0], a[1], a[2] as usize).unwrap();
    WebPCleanupTransparentArea(&mut pic);
    let mut enc = VP8Encoder::new(a[0], a[1], q <= 98.).unwrap();
    VP8EncAnalyze(&mut enc, &pic);
    let alpha = VP8EncStartAlpha(&pic, a[4] as i32, 4).unwrap();
    let tokens = VP8EncTokenLoop(&mut enc, &pic, q).unwrap();
    let mut output = Vec::new();
    VP8EncWriteWithAlpha(
        &enc,
        a[0],
        a[1],
        0,
        VP8BitWriterBuf(&tokens),
        alpha.as_ref().map(|a| a.data.as_slice()),
        &mut output,
    )
    .unwrap();
    output
}
pub fn evaluate(case: &Case) -> Vec<u32> {
    if case.function == "VP8AlphaFile" {
        return file(&case.input).into_iter().map(u32::from).collect();
    }
    evaluate_with_method(&case.input, 4)
}
pub fn evaluate_with_method(a: &[u32], effort: u32) -> Vec<u32> {
    let (w, h, stride) = (a[1] as usize, a[2] as usize, a[3] as usize);
    let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
    match a[0] {
        0 => {
            let mut dst = vec![0x5aa5a55a; (h - 1) * a[8] as usize + w + 5];
            DispatchAlphaToGreen_C(&data, stride, w, h, &mut dst, a[8] as usize);
            dst
        }
        1 => {
            let mut dst = vec![0xa5; data.len() + 5];
            WebPFilter(&data, w, h, stride, a[4] as usize, &mut dst);
            dst.into_iter().map(u32::from).collect()
        }
        2 => vec![WebPEstimateBestFilter(&data, w, h) as u32],
        3 => {
            let mut data = data;
            let error = if a[6] != 0 {
                None
            } else {
                QuantizeLevels(&mut data, w, h, a[4] as usize)
            };
            let sse = if a[5] != 0 {
                0x12345678
            } else {
                error.unwrap_or(0x12345678)
            };
            let mut out = vec![u32::from(error.is_some()), sse as u32, (sse >> 32) as u32];
            out.extend(data.into_iter().map(u32::from));
            out
        }
        4 => vec![GetNumColors(&data, w, h, stride) as u32],
        5 => vec![GetFilterMap(&data, w, h, a[4] as usize, a[5])],
        6 => {
            let mut bw = VP8LBitWriterInit(0).unwrap();
            EncodeLossless(&data, w, h, effort, a[6] == 0, &mut bw).unwrap();
            VP8LBitWriterFinish(&mut bw)
                .unwrap()
                .iter()
                .map(|&v| u32::from(v))
                .collect()
        }
        7 => {
            let mut tmp = vec![0; w * h];
            EncodeAlphaInternal(
                &data,
                w,
                h,
                a[4] as u8,
                a[5] as usize,
                a[6] != 0,
                effort,
                &mut tmp,
            )
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect()
        }
        8 => ApplyFiltersAndEncode(&data, w, h, a[4] as u8, a[5] as usize, a[6] != 0, effort)
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect(),
        9 => match EncodeAlpha(
            &data,
            w,
            h,
            stride,
            a[4] as i32,
            a[5] as u8,
            a[6] as usize,
            effort,
        ) {
            Ok(result) => {
                let mut out = vec![1, result.sse as u32, (result.sse >> 32) as u32];
                out.extend(result.data.into_iter().map(u32::from));
                out
            }
            Err(_) => vec![0, 0, 0],
        },
        _ => unreachable!(),
    }
}
pub fn plane(w: usize, h: usize, stride: usize, pattern: u32) -> Vec<u32> {
    let mut data = vec![0xa5; (h - 1) * stride + w];
    let mut seed = SEED ^ (pattern * 17);
    for y in 0..h {
        for x in 0..w {
            let r = random(&mut seed);
            data[y * stride + x] = match pattern {
                0 => 0,
                1 => 255,
                2 => 128,
                3 => ((x + y) & 1) * 255,
                4 => (x * 7 + y * 11) & 255,
                5 => (x * 17) & 255,
                6 => (y * 19) & 255,
                7 => r as usize & 255,
                8 => {
                    if (x + y) % 5 == 0 {
                        r as usize & 255
                    } else {
                        17
                    }
                }
                9 => (x + 13 * y) % 17,
                10 => (x + 13 * y) % 193,
                11 => (x + y) % 2 * 79 + 41,
                _ => (x * 31 + y * 29).min(255),
            } as u32;
        }
    }
    data
}
pub fn request(op: u32, w: usize, h: usize, stride: usize, p: [u32; 4], pattern: u32) -> Case {
    let mut a = vec![
        op,
        w as u32,
        h as u32,
        stride as u32,
        p[0],
        p[1],
        p[2],
        p[3],
        (w + 3) as u32,
    ];
    a.extend(plane(w, h, stride, pattern));
    Case::new("AlphaOps", a)
}
pub fn leaf_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 17),
        (17, 1),
        (2, 2),
        (3, 5),
        (7, 8),
        (17, 19),
        (33, 17),
    ] {
        for pad in [0, 3] {
            for pattern in 0..13 {
                out.push(request(0, w, h, w + pad, [0; 4], pattern));
                for filter in 0..4 {
                    out.push(request(1, w, h, w + pad, [filter, 0, 0, 0], pattern));
                }
                out.push(request(4, w, h, w + pad, [0; 4], pattern));
            }
        }
    }
    for levels in 0..=257 {
        for pattern in [4, 7, 8, 11] {
            out.push(request(3, 37, 17, 37, [levels, 0, 0, 0], pattern));
        }
    }
    for null_data in 0..=1 {
        for no_sse in 0..=1 {
            out.push(request(3, 3, 5, 3, [2, no_sse, null_data, 0], 7));
        }
    }
    for (w, h) in [(0, 0), (0, 1), (1, 0)] {
        out.push(Case::new("AlphaOps", vec![3, w, h, w, 2, 0, 0, 0, w]));
    }
    for colors in [1, 2, 16, 17, 192, 193, 256] {
        let data: Vec<_> = (0..256).map(|i| i % colors).collect();
        for meta in [0, 5, 6] {
            for effort in [0, 3, 4, 6] {
                let mut c = request(5, 16, 16, 16, [meta, effort, 0, 0], 0);
                c.input[9..].copy_from_slice(&data);
                out.push(c);
            }
        }
    }
    for (w, h) in [(1, 1), (3, 3), (4, 4), (5, 7), (17, 19), (33, 17)] {
        for pattern in 0..13 {
            out.push(request(2, w, h, w, [0; 4], pattern));
            out.push(request(5, w, h, w, [6, 4, 0, 0], pattern));
        }
    }
    out.push(centroid_rounding_case());
    out
}
pub fn centroid_rounding_case() -> Case {
    let mut a = vec![3, 6, 1, 6, 3, 0, 0, 0, 6];
    a.extend([0, 4, 5, 6, 7, 10]);
    Case::new("AlphaOps", a)
}
pub fn stream_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (17, 19), (33, 17)] {
        for pattern in 0..13 {
            out.push(request(6, w, h, w, [0; 4], pattern));
            for filter in 0..4 {
                for reduce in 0..=1 {
                    for method in 0..=1 {
                        out.push(request(7, w, h, w, [method, filter, reduce, 0], pattern));
                    }
                }
            }
            for meta in [0, 5, 6] {
                for reduce in 0..=1 {
                    out.push(request(8, w, h, w, [1, meta, reduce, 0], pattern));
                }
            }
            for q in [0, 4, 5, 10, 30, 69, 70, 71, 99, 100] {
                out.push(request(9, w, h, w, [q, 1, 6, 0], pattern));
            }
        }
    }
    for (w, h) in [(1, 16383), (16383, 1), (64, 64)] {
        for pattern in [4, 7, 8] {
            out.push(request(6, w, h, w, [0; 4], pattern));
            for q in [0, 70, 100] {
                out.push(request(9, w, h, w, [q, 1, 6, 0], pattern));
            }
        }
    }
    for q in 0..=100 {
        for method in 0..=1 {
            out.push(request(9, 17, 19, 17, [q, method, 5, 0], 8));
        }
    }
    for q in [u32::MAX, 101] {
        out.push(request(9, 3, 5, 3, [q, 1, 6, 0], 7));
    }
    for method in [2, 6] {
        out.push(request(9, 3, 5, 3, [100, method, 6, 0], 7));
    }
    for reduce in 0..=1 {
        out.push(request(
            8,
            17,
            19,
            17,
            [0, WEBP_FILTER_BEST as u32, reduce, 0],
            4,
        ));
    }
    for pattern in [0, 7, 8] {
        for q in [0, 70, 100] {
            out.push(request(
                9,
                3,
                5,
                6,
                [q, 1, WEBP_FILTER_FAST as u32, 0],
                pattern,
            ));
        }
    }
    out
}
pub fn file_cases() -> Vec<Case> {
    let mut out: Vec<_> = super::lossy::public_cases()
        .into_iter()
        .filter(|c| c.function == "WebPEncodeRGBAWithAlphaQuality")
        .map(|mut c| {
            c.function = "VP8AlphaFile".into();
            c
        })
        .collect();
    let mut real = super::public_images::real_image();
    for y in 0..real.height as usize {
        for x in 0..real.width as usize {
            real.rgba[y * real.stride + 4 * x + 3] = ((x * 17 + y * 13) & 255) as u8;
        }
    }
    for q in [30f32, 75., 98.5, 100.] {
        for aq in [0, 70, 100] {
            let mut c = super::lossy::request(&real, q, Some(aq));
            c.function = "VP8AlphaFile".into();
            out.push(c);
        }
    }
    for (w, h) in [(1, 16383), (16383, 1)] {
        for aq in [0, 70, 100] {
            let image = super::lossy::image(w, h, 3);
            let mut c = super::lossy::request(&image, 75., Some(aq));
            c.function = "VP8AlphaFile".into();
            out.push(c);
        }
    }
    out
}
pub fn cases() -> Vec<Case> {
    leaf_cases()
        .into_iter()
        .chain(stream_cases())
        .chain(file_cases())
        .collect()
}
pub fn fixture_cases() -> Vec<Case> {
    let mut out: Vec<_> = leaf_cases()
        .into_iter()
        .step_by(79)
        .chain(
            stream_cases()
                .into_iter()
                .filter(|c| c.input[1] <= 33 && c.input[2] <= 19)
                .step_by(137),
        )
        .chain(
            file_cases()
                .into_iter()
                .filter(|c| c.input[0] <= 17 && c.input[1] <= 19)
                .step_by(211),
        )
        .collect();
    out.push(centroid_rounding_case());
    out.push(request(7, 1, 17, 1, [1, 0, 1, 0], 6));
    out.push(request(
        8,
        17,
        19,
        17,
        [0, WEBP_FILTER_BEST as u32, 0, 0],
        4,
    ));
    out.push(request(9, 3, 5, 6, [70, 1, WEBP_FILTER_FAST as u32, 0], 8));
    out
}

pub fn inverse(filtered: &[u8], w: usize, h: usize, stride: usize, filter: usize) -> Vec<u8> {
    let mut out = vec![0; w * h];
    for y in 0..h {
        for x in 0..w {
            let pred = if filter == 0 || (x == 0 && y == 0) {
                0
            } else if y == 0 || (filter == 1 && x > 0) {
                out[y * w + x - 1]
            } else if x == 0 || filter == 2 {
                out[(y - 1) * w + x]
            } else {
                let sum = u16::from(out[y * w + x - 1]) + u16::from(out[(y - 1) * w + x]);
                sum.saturating_sub(u16::from(out[(y - 1) * w + x - 1]))
                    .min(255) as u8
            };
            out[y * w + x] = filtered[y * stride + x].wrapping_add(pred);
        }
    }
    out
}
pub fn green_decode(bytes: &[u8], w: usize, h: usize) -> Vec<u8> {
    use libwebpop::enc::vp8l_enc::WriteRiffHeader;
    let mut payload = Vec::new();
    payload.extend_from_slice(&((w as u32 - 1) | ((h as u32 - 1) << 14) | (1 << 28)).to_le_bytes());
    payload.extend_from_slice(bytes);
    let mut file = WriteRiffHeader(payload.len()).unwrap().to_vec();
    file.extend_from_slice(&payload);
    if (payload.len() + 1) & 1 != 0 {
        file.push(0);
    }
    let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
        .unwrap()
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (w as u32, h as u32));
    decoded
        .pixels()
        .map(|p| {
            assert_eq!([p.0[0], p.0[2], p.0[3]], [0, 0, 0]);
            p.0[1]
        })
        .collect()
}
