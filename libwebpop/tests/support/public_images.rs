use super::{Case, ImageCase, SEED, random};
use libwebpop::{LosslessOptions, WebPEncodeLosslessRGBA, encode_lossless_rgba};

pub fn unpack(case: &Case) -> (ImageCase, LosslessOptions) {
    let a = &case.input;
    let (offset, options) = if case.function == "WebPEncodeLosslessRGBA" {
        (3, LosslessOptions::default())
    } else {
        (
            6,
            LosslessOptions {
                quality: a[3],
                method: a[4],
                exact: a[5] != 0,
            },
        )
    };
    (
        ImageCase {
            name: "public RGBA corpus",
            width: a[0],
            height: a[1],
            stride: a[2] as usize,
            rgba: a[offset..].iter().map(|&v| v as u8).collect(),
        },
        options,
    )
}

pub fn encode(case: &Case) -> Vec<u8> {
    let (image, options) = unpack(case);
    // Both the starting slice and padded rows may be unaligned. Do not copy
    // into a conveniently aligned buffer before invoking the public API.
    let mut allocation = vec![0xa5];
    allocation.extend_from_slice(&image.rgba);
    let original = allocation.clone();
    let result = if case.function == "WebPEncodeLosslessRGBA" {
        WebPEncodeLosslessRGBA(&allocation[1..], image.width, image.height, image.stride)
    } else {
        encode_lossless_rgba(
            &allocation[1..],
            image.width,
            image.height,
            image.stride,
            &options,
        )
    }
    .unwrap();
    assert_eq!(allocation, original);
    result
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    encode(case).into_iter().map(u32::from).collect()
}

pub fn check_decoded(case: &Case, bytes: &[u8]) {
    let (image, options) = unpack(case);
    image.check_decoded(bytes);
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    assert_eq!(bytes.len() % 2, 0);
    if options.exact {
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        for y in 0..image.height as usize {
            assert_eq!(
                &decoded.as_raw()[y * image.width as usize * 4..(y + 1) * image.width as usize * 4],
                &image.rgba[y * image.stride..y * image.stride + image.width as usize * 4]
            );
        }
    }
}

pub fn image(w: usize, h: usize, pattern: u32) -> ImageCase {
    let stride = w * 4 + 7;
    let source = super::analysis::pixels(w, h, w, pattern);
    let mut rgba = vec![0xa5; (h - 1) * stride + w * 4];
    for y in 0..h {
        for x in 0..w {
            let mut p = source[y * w + x];
            // Keep the exact palette thresholds after transparent cleanup.
            if pattern >= 14 {
                p |= 0xff000000;
            }
            rgba[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&[
                (p >> 16) as u8,
                (p >> 8) as u8,
                p as u8,
                (p >> 24) as u8,
            ]);
        }
    }
    ImageCase {
        name: "synthetic public RGBA",
        width: w as u32,
        height: h as u32,
        stride,
        rgba,
    }
}

pub fn options_case(image: &ImageCase, quality: u32, method: u32, exact: bool) -> Case {
    let mut a = vec![
        image.width,
        image.height,
        image.stride as u32,
        quality,
        method,
        u32::from(exact),
    ];
    a.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new("WebPEncodeRGBAOptions", a)
}

pub fn real_image() -> ImageCase {
    // This committed P6 image is copied verbatim from the pinned libwebp sample.
    let ppm = include_bytes!("../fixtures/corpus/test_ref.ppm");
    let header = b"P6\n128 128\n255\n";
    assert!(ppm.starts_with(header));
    assert_eq!(ppm.len(), header.len() + 128 * 128 * 3);
    let mut rgba = Vec::new();
    for rgb in ppm[header.len()..].chunks_exact(3) {
        rgba.extend([rgb[0], rgb[1], rgb[2], 255]);
    }
    ImageCase {
        name: "pinned test_ref.ppm",
        width: 128,
        height: 128,
        stride: 512,
        rgba,
    }
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases: Vec<_> = (0..36).map(|p| image(19, 17, p).request()).collect();
    for (pattern, method, quality, exact) in [
        (2, 0, 70, false),
        (2, 4, 70, true),
        (20, 0, 70, true),
        (6, 6, 100, true),
        (20, 6, 100, true),
        (30, 5, 50, false),
        (34, 6, 100, false),
    ] {
        cases.push(options_case(
            &image(19, 17, pattern),
            quality,
            method,
            exact,
        ));
    }
    cases
}

pub fn cases() -> Vec<Case> {
    let mut cases = fixture_cases();
    cases.extend(super::image_cases().iter().map(ImageCase::request));
    for (w, h) in [
        (1, 1),
        (1, 43),
        (47, 1),
        (19, 17),
        (65, 67),
        (257, 2),
        (129, 33),
    ] {
        for pattern in 0..36 {
            let image = image(w, h, pattern);
            cases.push(image.request());
            for (quality, method, exact) in [
                (0, 0, false),
                (0, 1, true),
                (49, 3, false),
                (70, 4, true),
                (49, 5, false),
                (50, 5, true),
                (99, 6, false),
                (100, 6, true),
            ] {
                cases.push(options_case(&image, quality, method, exact));
            }
        }
    }
    for (w, h) in [(16383, 1), (1, 16383)] {
        for pattern in [1, 2, 20] {
            cases.push(image(w, h, pattern).request());
        }
    }
    let mut seed = SEED;
    for _ in 0..128 {
        let (w, h) = (
            1 + random(&mut seed) as usize % 129,
            1 + random(&mut seed) as usize % 67,
        );
        let stride = w * 4 + random(&mut seed) as usize % 16;
        let mut rgba = vec![0x5a; (h - 1) * stride + w * 4];
        let colors = [0, 2, 5, 16, 17, 225, 256, 257][random(&mut seed) as usize % 8];
        for y in 0..h {
            for x in 0..w {
                let r = random(&mut seed);
                let pixel = if colors == 0 {
                    r
                } else {
                    (r % colors) * 0x123457 | 0xff000000
                };
                rgba[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&[
                    (pixel >> 16) as u8,
                    (pixel >> 8) as u8,
                    pixel as u8,
                    (pixel >> 24) as u8,
                ]);
            }
        }
        let image = ImageCase {
            name: "seeded public RGBA",
            width: w as u32,
            height: h as u32,
            stride,
            rgba,
        };
        cases.push(image.request());
        cases.push(options_case(
            &image,
            random(&mut seed) % 101,
            random(&mut seed) % 7,
            random(&mut seed) & 1 != 0,
        ));
    }
    let real = real_image();
    cases.push(real.request());
    for (q, m, exact) in [(0, 0, false), (70, 4, true), (100, 6, true)] {
        cases.push(options_case(&real, q, m, exact));
    }
    cases
}
