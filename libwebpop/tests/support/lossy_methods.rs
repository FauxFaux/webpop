//! Reference cases for public methods 0–6 and their internal tool/frame mapping.
use super::{Case, ImageCase, lossy, vp8_loop};

pub const RD_LEVELS: [u32; 7] = [0, 0, 0, 1, 1, 2, 3];

pub fn request(image: &ImageCase, quality: f32, alpha: i32, method: u32, sharp: bool) -> Case {
    let mut input = vec![
        image.width,
        image.height,
        image.stride as u32,
        quality.to_bits(),
        alpha as u32,
        method,
        u32::from(sharp),
    ];
    input.extend(image.rgba.iter().copied().map(u32::from));
    Case::new("WebPEncodeLossyMethod", input)
}

pub fn public_output(case: &Case) -> Vec<u32> {
    let a = &case.input;
    assert_eq!(case.function, "WebPEncodeLossyMethod");
    let pixels: Vec<_> = a[7..].iter().map(|&v| v as u8).collect();
    let mut output = Vec::new();
    libwebpop::internal::encode_lossy_rgba(
        &pixels,
        a[0],
        a[1],
        a[2] as usize,
        &libwebpop::internal::LossyOptions {
            quality: f32::from_bits(a[3]),
            method: a[5],
            alpha_quality: a[4] as i32,
            use_sharp_yuv: a[6] != 0,
        },
        &mut output,
    )
    .unwrap();
    output.into_iter().map(u32::from).collect()
}

pub fn check_decoded(case: &Case, output: &[u32]) -> [usize; 3] {
    let mut equivalent = case.clone();
    equivalent.function = "WebPEncodeRGBAWithAlphaQuality".into();
    equivalent.input.drain(5..7);
    lossy::check_decoded(&equivalent, output)
}

pub fn public_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (4, 4), (17, 19), (33, 17)] {
        for pattern in 0..6 {
            let image = lossy::image(w, h, pattern);
            for method in 0..=6 {
                for quality in [0., 75., 98.5, 100.] {
                    for alpha in [0, 99, 100] {
                        cases.push(request(&image, quality, alpha, method, false));
                    }
                }
            }
        }
    }
    // Preserve the newer public SharpYUV option while adding method selection.
    for (w, h) in [(3, 5), (4, 4), (17, 19)] {
        for method in 0..=6 {
            cases.push(request(&lossy::image(w, h, 4), 75., 100, method, true));
        }
    }
    let real = super::public_images::real_image();
    for method in 0..=6 {
        cases.push(request(&real, 75., 100, method, false));
    }
    cases
}

pub fn tools_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (17, 19), (16383, 1)] {
        for quality in [0f32, 98., 98.5, 100.] {
            for method in 0..=6 {
                for low_memory in [0, 1] {
                    for partitions in 0..=3 {
                        cases.push(Case::new(
                            "VP8MethodTools",
                            vec![w, h, quality.to_bits(), method, low_memory, partitions],
                        ));
                    }
                }
            }
        }
    }
    cases
}

/// Logical setup state for the normal-memory, single-partition Rust scope.
pub fn tools_output(case: &Case) -> Vec<u32> {
    use libwebpop::internal::enc::vp8i_enc::VP8Encoder;
    let a = &case.input;
    assert_eq!(&a[4..6], &[0, 0]);
    let enc =
        VP8Encoder::new_with_method(a[0], a[1], f32::from_bits(a[2]) <= 98., a[3] as i32).unwrap();
    vec![
        enc.config.method as u32,
        enc.rd_opt_level as u32,
        u32::from(enc.use_tokens),
        1, // one coefficient partition
        0, // target search disabled
        enc.max_i4_header_bits as u32,
        enc.mb_header_limit as u32,
        (enc.mb_header_limit as u64 >> 32) as u32,
        u32::from(enc.top_derr.is_some()),
        0, // default complex-filter profile
    ]
}

pub fn check_tools(case: &Case, out: &[u32]) {
    let a = &case.input;
    assert_eq!(out.len(), 10);
    let tokens = a[3] >= 3 && a[4] == 0;
    assert_eq!(out[0], a[3]);
    assert_eq!(out[1], RD_LEVELS[a[3] as usize]);
    assert_eq!(out[2], u32::from(tokens));
    assert_eq!(out[3], if tokens { 1 } else { 1 << a[5] });
    assert_eq!(out[4], 0); // no size/PSNR target
    assert_eq!(out[5], 65536);
    let mb_count = u64::from(a[0].div_ceil(16)) * u64::from(a[1].div_ceil(16));
    assert_eq!(
        u64::from(out[6]) | (u64::from(out[7]) << 32),
        256 * 510 * 8 * 1024 / mb_count
    );
    assert_eq!(out[8], u32::from(f32::from_bits(a[2]) <= 98.));
    assert_eq!(out[9], 0); // default complex-filter profile
}

pub fn loop_request(image: &ImageCase, quality: f32, method: u32) -> Case {
    let base = vp8_loop::rgba_case(image, quality);
    let mut input = vec![method];
    input.extend(base.input);
    Case::new("VP8MethodLoopFrame", input)
}

pub fn loop_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in 0..=6 {
        for (w, h) in [(1, 1), (17, 19), (33, 17)] {
            for pattern in [0, 2, 4] {
                for quality in [0f32, 75., 98.5, 100.] {
                    let base = vp8_loop::frame_case(w, h, quality, 4, pattern, false);
                    let mut input = vec![method];
                    input.extend(base.input);
                    cases.push(Case::new("VP8MethodLoopFrame", input));
                }
            }
        }
        // Method-0 partial statistics and token-probability refresh boundaries.
        for count in [49, 50, 51, 96, 97, 199, 200, 201, 204, 776] {
            let base = vp8_loop::frame_case(count * 16, 1, 75., 4, 4, false);
            let mut input = vec![method];
            input.extend(base.input);
            cases.push(Case::new("VP8MethodLoopFrame", input));
        }
        for pattern in [0, 4] {
            let mut image = lossy::image(17, 19, pattern);
            for y in 0..19 {
                for x in 0..17 {
                    image.rgba[y * image.stride + 4 * x + 3] = 255;
                }
            }
            cases.push(loop_request(&image, 75., method));
        }
    }
    cases
}

pub fn check_loop<'a>(case: &Case, out: &'a [u32]) -> &'a [u32] {
    let a = &case.input;
    let method = a[0] as usize;
    let mb_count = a[1].div_ceil(16) * a[2].div_ceil(16);
    let direct = method < 3;
    assert_eq!(&out[..4], &[a[0], RD_LEVELS[method], u32::from(!direct), 1]);
    assert_eq!(out[4], if direct { 2 } else { 1 });
    for pass in 0..out[4] as usize {
        let fields = &out[5 + 6 * pass..][..6];
        assert_eq!(fields[0], RD_LEVELS[method]);
        let visited = if method == 0 && pass == 0 {
            mb_count.min(if mb_count > 200 { mb_count >> 2 } else { 50 })
        } else {
            mb_count
        };
        assert_eq!(fields[1], visited);
        assert_eq!(fields[2] + fields[3], visited);
        assert!(fields[4] <= visited);
        assert_eq!(fields[5], if method >= 5 { visited } else { 0 });
    }
    let tail = &out[5 + 6 * out[4] as usize..];
    assert_eq!(
        tail[0],
        if direct {
            0
        } else {
            mb_count / ((mb_count >> 3).max(96) + 1)
        }
    );
    assert_eq!(tail[1], u32::from(!direct));
    if direct {
        assert_eq!(tail[2], u32::from(tail[3] < 250));
        assert!(tail[4] <= mb_count);
        assert_eq!(tail[3], (mb_count - tail[4]) * 255 / mb_count);
    } else {
        assert_eq!(tail[2..5], [0, 0, 0]);
    }
    assert!(tail[6] <= 63 && tail[7] > 0);
    assert_eq!(tail.len(), 9 + tail[8] as usize);
    let file = &tail[9..];
    let bytes: Vec<_> = file.iter().map(|&v| u8::try_from(v).unwrap()).collect();
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (a[1], a[2]));
    assert!(decoded.pixels().all(|p| p[3] == 255));
    file
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in 0..=6 {
        for quality in [98f32, 98.5] {
            cases.push(Case::new(
                "VP8MethodTools",
                vec![17, 19, quality.to_bits(), method, 0, 3],
            ));
        }
        cases.push(Case::new(
            "VP8MethodTools",
            vec![17, 19, 75f32.to_bits(), method, 1, 2],
        ));
        for (w, h, pattern, quality, alpha) in [
            (1, 1, 0, 75., 100),
            (3, 5, 2, 98.5, 100),
            (5, 7, 4, 75., 100),
            (5, 7, 4, 98.5, 71),
            (17, 19, 3, 100., 0),
        ] {
            cases.push(request(
                &lossy::image(w, h, pattern),
                quality,
                alpha,
                method,
                false,
            ));
        }
        for pattern in [0, 2] {
            let base = vp8_loop::frame_case(17, 19, 75., 4, pattern, false);
            let mut input = vec![method];
            input.extend(base.input);
            cases.push(Case::new("VP8MethodLoopFrame", input));
        }
    }
    // Append to the original 70 reference fixtures without altering their
    // inputs or outputs. Normal resource policy is now also evaluated in Rust.
    for method in 0..=6 {
        cases.push(Case::new(
            "VP8MethodTools",
            vec![17, 19, 98.5f32.to_bits(), method, 0, 0],
        ));
    }
    cases
}

pub fn driver_fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for method in 0..=4 {
        for (w, h, q, pattern) in [(1, 1, 0., 0), (17, 19, 75., 4), (17, 19, 98.5, 2)] {
            let base = vp8_loop::frame_case(w, h, q, 4, pattern, true);
            let mut input = vec![method];
            input.extend(base.input);
            cases.push(Case::new("VP8MethodLoopTrace", input));
        }
        // Zero limit: direct non-RD headers still include segment-map costs;
        // token mode exercises retries after a probability refresh.
        let mut base = vp8_loop::frame_case(97 * 16, 1, 75., 4, 4, true);
        base.input[4] = 1;
        base.input[5] = 0;
        let mut input = vec![method];
        input.extend(base.input);
        cases.push(Case::new("VP8MethodLoopTrace", input));
    }
    for count in [49, 50, 51, 199, 200, 201, 204] {
        let mut input = vec![0];
        input.extend(vp8_loop::frame_case(count * 16, 1, 75., 4, 0, false).input);
        cases.push(Case::new("VP8MethodLoopFrame", input));
    }
    cases
}

/// Exact compact observations, including the independently assembled file.
pub fn loop_output(case: &Case) -> Vec<u32> {
    use libwebpop::internal::{
        enc::{analysis_enc::*, frame_enc::*, syntax_enc::VP8EncWrite, vp8i_enc::*},
        utils::bit_writer_utils::VP8BitWriterBuf,
    };
    let method = case.input[0];
    let a = &case.input[1..];
    let quality = f32::from_bits(a[2]);
    let pic = vp8_loop::picture(a);
    let mut enc = VP8Encoder::new_with_method(a[0], a[1], quality <= 98., method as i32).unwrap();
    enc.config.quality = quality;
    enc.segment_hdr.num_segments = a[3] as usize;
    enc.max_i4_header_bits = a[4] as i32;
    VP8EncAnalyze(&mut enc, &pic);
    let mut passes: Vec<[u32; 6]> = Vec::new();
    let (mut refresh, mut emit) = (0, 0);
    fn mb(p: &mut [u32; 6], enc: &VP8Encoder, index: usize, trellis: bool) {
        let info = enc.mb_info[index];
        p[1] += 1;
        p[2 + usize::from(info.r#type == 0)] += 1;
        p[4] += u32::from(info.skip);
        p[5] += u32::from(trellis);
    }
    let bw =
        if enc.use_tokens {
            VP8EncTokenLoopWithObserver(&mut enc, &pic, quality, u64::from(a[5]), |enc, event| {
                match event {
                    VP8TokenLoopEvent::PassStart { .. } => {
                        passes.push([enc.rd_opt_level as u32, 0, 0, 0, 0, 0])
                    }
                    VP8TokenLoopEvent::Refresh { .. } => refresh += 1,
                    VP8TokenLoopEvent::Macroblock {
                        index, iterator, ..
                    } => mb(passes.last_mut().unwrap(), enc, index, iterator.do_trellis),
                    VP8TokenLoopEvent::Emit { .. } => emit += 1,
                    _ => {}
                }
            })
            .unwrap()
        } else {
            VP8EncLoopWithObserver(&mut enc, &pic, quality, u64::from(a[5]), |enc, event| {
                match event {
                    VP8DirectLoopEvent::ScanStart { .. } => {
                        passes.push([enc.rd_opt_level as u32, 0, 0, 0, 0, 0])
                    }
                    VP8DirectLoopEvent::Macroblock {
                        index, iterator, ..
                    } => mb(passes.last_mut().unwrap(), enc, index, iterator.do_trellis),
                    _ => {}
                }
            })
            .unwrap()
        };
    let mut out = vec![
        method,
        enc.rd_opt_level as u32,
        u32::from(enc.use_tokens),
        1,
        passes.len() as u32,
    ];
    out.extend(passes.into_iter().flatten());
    let residuals = VP8BitWriterBuf(&bw);
    out.extend([
        refresh,
        emit,
        u32::from(enc.proba.use_skip_proba),
        u32::from(enc.proba.skip_proba),
        enc.proba.nb_skip as u32,
        super::vp8_tokens::hash(super::vp8_cost::probability_words(&enc.proba.coeffs)),
        enc.filter_hdr.level as u32,
        residuals.len() as u32,
    ]);
    let mut file = Vec::new();
    VP8EncWrite(&enc, a[0], a[1], 0, residuals, &mut file).unwrap();
    out.push(file.len() as u32);
    out.extend(file.into_iter().map(u32::from));
    out
}

pub fn driver_output(case: &Case) -> Vec<u32> {
    if case.function == "VP8MethodLoopFrame" {
        loop_output(case)
    } else {
        assert_eq!(case.function, "VP8MethodLoopTrace");
        vp8_loop::encode_method(&case.input[1..], case.input[0] as i32, true).0
    }
}

pub fn driver_cases() -> Vec<Case> {
    let mut cases = driver_fixture_cases();
    for method in 0..=3 {
        // C-observed first-pass header costs for this fixed source.
        let cost = if method < 3 { 1555 } else { 20912 };
        for limit in [cost - 1, cost, cost + 1] {
            let mut base = vp8_loop::frame_case(17, 19, 75., 4, 4, true);
            base.input[5] = limit;
            let mut input = vec![method];
            input.extend(base.input);
            cases.push(Case::new("VP8MethodLoopTrace", input));
        }
        for (w, h) in [
            (1, 17),
            (17, 1),
            (3, 5),
            (15, 15),
            (16, 16),
            (17, 19),
            (31, 33),
            (65, 49),
        ] {
            for q in [0., 1., 30., 50., 75., 98., 98.5, 99., 100.] {
                for p in [0, 2, 3, 4] {
                    let mut input = vec![method];
                    input.extend(vp8_loop::frame_case(w, h, q, 4, p, true).input);
                    cases.push(Case::new("VP8MethodLoopTrace", input));
                }
            }
        }
        for count in [49, 50, 51, 96, 97, 199, 200, 201, 204, 776] {
            for p in [0, 4] {
                let mut input = vec![method];
                input.extend(vp8_loop::frame_case(count * 16, 1, 75., 4, p, true).input);
                cases.push(Case::new("VP8MethodLoopTrace", input));
            }
        }
        for segments in 1..=4 {
            for limit in [0, 1, 8192, 65536] {
                let mut base = vp8_loop::frame_case(17, 19, 75., segments, 4, true);
                base.input[5] = limit;
                let mut input = vec![method];
                input.extend(base.input);
                cases.push(Case::new("VP8MethodLoopTrace", input));
            }
        }
    }
    cases
}

pub fn driver_rgba_cases() -> Vec<Case> {
    let mut images = vp8_loop::rgba_cases();
    for (w, h) in [(16383, 1), (1, 16383), (193, 129)] {
        let mut image = lossy::image(w, h, 4);
        for y in 0..h as usize {
            for x in 0..w as usize {
                image.rgba[y * image.stride + 4 * x + 3] = 255;
            }
        }
        for q in [0., 75., 98.5, 100.] {
            images.push(vp8_loop::rgba_case(&image, q));
        }
    }
    // Fractional qualities around the fast-analysis integer thresholds.
    let mut image = lossy::image(33, 17, 2);
    for y in 0..17 {
        for x in 0..33 {
            image.rgba[y * image.stride + 4 * x + 3] = 255;
        }
    }
    for q in [29.99, 30., 30.01, 74.99, 75., 75.01] {
        images.push(vp8_loop::rgba_case(&image, q));
    }
    let mut cases = Vec::new();
    for method in 0..=3 {
        for image in &images {
            let mut input = vec![method];
            input.extend_from_slice(&image.input);
            cases.push(Case::new("VP8MethodLoopFrame", input));
        }
    }
    cases
}
