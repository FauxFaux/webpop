//! Reference-first cases for the methods extension. Until each algorithm lands,
//! only method 4 is evaluated in Rust; other methods are C reference artifacts.
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

pub fn method4_output(case: &Case) -> Vec<u32> {
    let a = &case.input;
    assert_eq!(case.function, "WebPEncodeLossyMethod");
    assert_eq!(a[5], 4, "only method 4 has been ported so far");
    let pixels: Vec<_> = a[7..].iter().map(|&v| v as u8).collect();
    let mut output = Vec::new();
    libwebpop::encode_lossy_rgba(
        &pixels,
        a[0],
        a[1],
        a[2] as usize,
        &libwebpop::LossyOptions {
            quality: f32::from_bits(a[3]),
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
    cases
}
