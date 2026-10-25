//! Method-dependent alpha leaves and composed files (methods milestone 7).
use super::{Case, lossy, lossy_methods, vp8_alpha};
use libwebpop::enc::alpha_enc::*;

pub fn evaluate(case: &Case) -> Vec<u32> {
    let effort = case.input[0];
    let a = &case.input[1..];
    if a[0] != 10 {
        return vp8_alpha::evaluate_with_method(a, effort);
    }
    let (w, h) = (a[1] as usize, a[2] as usize);
    let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
    let map = GetFilterMap(&data, w, h, a[5] as usize, effort);
    let mut out = vec![map];
    for filter in 0..4 {
        if map & (1 << filter) != 0 {
            let trial = EncodeAlphaInternal(
                &data,
                w,
                h,
                a[4] as u8,
                filter,
                a[6] != 0,
                effort,
                &mut vec![0; w * h],
            )
            .unwrap();
            out.extend([filter as u32, trial.len() as u32]);
            out.extend(trial.into_iter().map(u32::from));
        }
    }
    let selected =
        ApplyFiltersAndEncode(&data, w, h, a[4] as u8, a[5] as usize, a[6] != 0, effort).unwrap();
    out.push(selected.len() as u32);
    out.extend(selected.into_iter().map(u32::from));
    out
}

/// Compose the internal method-aware leaves while the public API stays method 4.
pub fn file_output(case: &Case) -> Vec<u32> {
    use libwebpop::{
        enc::{
            analysis_enc::VP8EncAnalyze, frame_enc::*, picture_csp_enc::*,
            picture_tools_enc::WebPCleanupTransparentArea, syntax_enc::VP8EncWriteWithAlpha,
            vp8i_enc::VP8Encoder,
        },
        utils::bit_writer_utils::VP8BitWriterBuf,
    };
    let a = &case.input;
    let rgba: Vec<_> = a[7..].iter().map(|&v| v as u8).collect();
    let mut pic = if a[6] != 0 {
        import_rgba_to_yuva_sharp(&rgba, a[0], a[1], a[2] as usize)
    } else {
        import_rgba_to_yuva(&rgba, a[0], a[1], a[2] as usize)
    }
    .unwrap();
    WebPCleanupTransparentArea(&mut pic);
    let quality = f32::from_bits(a[3]);
    let mut enc = VP8Encoder::new_with_method(a[0], a[1], quality <= 98., a[5] as i32).unwrap();
    enc.config.quality = quality;
    VP8EncAnalyze(&mut enc, &pic);
    let alpha = VP8EncStartAlpha(&pic, a[4] as i32, a[5]).unwrap();
    let residuals = if enc.use_tokens {
        VP8EncTokenLoop(&mut enc, &pic, quality)
    } else {
        VP8EncLoop(&mut enc, &pic, quality)
    }
    .unwrap();
    let mut out = Vec::new();
    VP8EncWriteWithAlpha(
        &enc,
        a[0],
        a[1],
        0,
        VP8BitWriterBuf(&residuals),
        alpha.as_ref().map(|a| a.data.as_slice()),
        &mut out,
    )
    .unwrap();
    out.into_iter().map(u32::from).collect()
}

pub fn request(
    method: u32,
    op: u32,
    w: usize,
    h: usize,
    stride: usize,
    p: [u32; 4],
    pattern: u32,
) -> Case {
    let base = vp8_alpha::request(op, w, h, stride, p, pattern);
    let mut input = vec![method];
    input.extend(base.input);
    Case::new("AlphaMethodOps", input)
}

pub fn color_case(method: u32, colors: u32, reduce: u32, filter: u32) -> Case {
    let mut case = request(method, 10, 16, 16, 16, [1, filter, reduce, 0], 0);
    for (i, v) in case.input[10..].iter_mut().enumerate() {
        *v = (i as u32 % colors) * 255 / (colors - 1);
    }
    case
}

pub fn leaf_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for method in 0..=6 {
        // Tiny constant planes cross exact compressed/raw payload-size ties.
        for h in 1..=24 {
            out.push(request(method, 7, 1, h, 1, [1, 0, 0, 0], 0));
        }
        // Every filtered noise trial falls back to raw at this geometry,
        // forcing a first-filter size tie in the all-filter search.
        out.push(request(method, 10, 3, 5, 3, [1, 5, 0, 0], 7));
        for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (17, 19), (33, 17)] {
            for pattern in [0, 3, 4, 7, 8, 9, 10, 11] {
                for reduce in 0..=1 {
                    out.push(request(method, 6, w, h, w, [0, 0, reduce, 0], pattern));
                }
            }
        }
        for (w, h) in [(1, 1), (1, 17), (3, 5), (17, 19)] {
            for pattern in [0, 4, 7, 8] {
                for filter in 0..4 {
                    for reduce in 0..=1 {
                        out.push(request(method, 7, w, h, w, [1, filter, reduce, 0], pattern));
                    }
                }
            }
        }
        for colors in [16, 17, 192, 193, 256] {
            for reduce in 0..=1 {
                for filter in [0, 5, 6] {
                    out.push(color_case(method, colors, reduce, filter));
                }
            }
        }
        for pattern in [0, 3, 4, 5, 6, 7, 8, 11] {
            for reduce in 0..=1 {
                out.push(request(method, 10, 17, 19, 17, [1, 6, reduce, 0], pattern));
            }
        }
        for quality in 0..=100 {
            out.push(request(method, 9, 17, 19, 17, [quality, 1, 6, 0], 8));
        }
        for quality in [0, 4, 5, 69, 70, 71, 99, 100] {
            for pattern in [0, 4, 7] {
                out.push(request(method, 9, 3, 5, 6, [quality, 1, 6, 0], pattern));
            }
        }
        for (w, h) in [(16383, 1), (1, 16383), (64, 64)] {
            for quality in [99, 100] {
                out.push(request(method, 9, w, h, w, [quality, 1, 6, 0], 4));
            }
        }
    }
    out
}

pub fn file_cases() -> Vec<Case> {
    let mut out = lossy_methods::public_cases();
    for method in 0..=6 {
        let image = lossy::image(17, 19, 4);
        for quality in [0., 1., 30., 50., 75., 98., 98.5, 99., 100.] {
            for alpha in [0, 4, 5, 69, 70, 71, 99, 100] {
                out.push(lossy_methods::request(
                    &image, quality, alpha, method, false,
                ));
            }
        }
        let mut sparse = image.clone();
        let alpha = vp8_alpha::plane(17, 19, 17, 8);
        for y in 0..19 {
            for x in 0..17 {
                sparse.rgba[y * sparse.stride + 4 * x + 3] = alpha[y * 17 + x] as u8;
            }
        }
        for alpha in [0, 71, 100] {
            out.push(lossy_methods::request(&sparse, 75., alpha, method, false));
        }
        for (w, h) in [(16383, 1), (1, 16383), (193, 129)] {
            for alpha in [71, 100] {
                out.push(lossy_methods::request(
                    &lossy::image(w, h, 3),
                    75.,
                    alpha,
                    method,
                    false,
                ));
            }
        }
        let mut real = super::public_images::real_image();
        for y in 0..real.height as usize {
            for x in 0..real.width as usize {
                real.rgba[y * real.stride + 4 * x + 3] = ((x * 17 + y * 13) & 255) as u8;
            }
        }
        for alpha in [71, 100] {
            out.push(lossy_methods::request(&real, 75., alpha, method, false));
        }
    }
    out
}

pub fn fixture_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for method in 0..=6 {
        out.push(request(method, 10, 3, 5, 3, [1, 5, 0, 0], 7));
        for reduce in 0..=1 {
            out.push(request(method, 6, 17, 19, 17, [0, 0, reduce, 0], 8));
            out.push(request(method, 7, 1, 17, 1, [1, 2, reduce, 0], 4));
            out.push(color_case(
                method,
                if reduce == 0 { 17 } else { 193 },
                reduce,
                6,
            ));
            out.push(request(method, 9, 3, 5, 6, [99 + reduce, 1, 6, 0], 8));
            out.push(lossy_methods::request(
                &lossy::image(5, 7, 4),
                98.5,
                if reduce == 0 { 99 } else { 100 },
                method,
                false,
            ));
        }
    }
    out
}
