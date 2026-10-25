#![allow(dead_code)]

use crate::support::{Case, ImageCase};

pub fn request(image: &ImageCase, strength: u32) -> Case {
    let mut words = vec![image.width, image.height, image.stride as u32, strength];
    words.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new("WebPEncodeRGBAWithNearLossless", words)
}

pub fn unpack(case: &Case) -> ImageCase {
    let a = &case.input;
    ImageCase {
        name: "near-lossless public reference",
        width: a[0],
        height: a[1],
        stride: a[2] as usize,
        rgba: a[4..].iter().map(|&v| u8::try_from(v).unwrap()).collect(),
    }
}

pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h, pattern) in [
        (5, 3, 24),
        (19, 17, 2),
        (65, 5, 9),
        (19, 17, 12),
        (65, 5, 2),
    ] {
        let image = crate::support::public_images::image(w, h, pattern);
        for strength in [0, 100] {
            cases.push(request(&image, strength));
        }
    }
    for (w, h, pattern) in [(19, 17, 2), (65, 5, 9), (19, 17, 12), (65, 5, 2)] {
        let image = crate::support::public_images::image(w, h, pattern);
        cases.extend([20, 40, 60, 80].map(|strength| request(&image, strength)));
    }
    cases
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [
        (1, 1),
        (1, 65),
        (65, 1),
        (2, 65),
        (65, 2),
        (3, 64),
        (64, 3),
        (63, 63),
        (64, 65),
        (65, 64),
        (65, 65),
        (63, 65),
        (65, 63),
        (65, 3),
        (3, 65),
        (19, 17),
    ] {
        for pattern in [2, 5, 9, 10, 12, 24] {
            let image = crate::support::public_images::image(w, h, pattern);
            for strength in [0, 19, 20, 39, 40, 59, 60, 79, 80, 99, 100] {
                cases.push(request(&image, strength));
            }
        }
    }
    // All strengths on palette and non-palette inputs, plus the committed sample.
    for pattern in [2, 12, 24] {
        let image = crate::support::public_images::image(19, 17, pattern);
        cases.extend((0..=100).map(|strength| request(&image, strength)));
    }
    let mut seed = crate::support::SEED;
    for _ in 0..128 {
        let w = (crate::support::random(&mut seed) % 67 + 1) as usize;
        let h = (crate::support::random(&mut seed) % 67 + 1) as usize;
        let pattern = crate::support::random(&mut seed) % 36;
        let image = crate::support::public_images::image(w, h, pattern);
        cases.extend([0, 20, 40, 60, 80, 100].map(|strength| request(&image, strength)));
    }
    let sample = crate::support::public_images::real_image();
    cases.extend([0, 20, 40, 60, 80, 100].map(|strength| request(&sample, strength)));
    cases
}

/// Return changed visible RGB components and alpha components, respectively.
pub fn check_reference(case: &Case, words: &[u32]) -> (usize, usize) {
    let image = unpack(case);
    let bytes: Vec<_> = words.iter().map(|&v| u8::try_from(v).unwrap()).collect();
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (image.width, image.height));
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    assert_eq!(bytes.len() % 2, 0);
    let mut changes = (0, 0);
    for y in 0..image.height as usize {
        for x in 0..image.width as usize {
            let source = &image.rgba[y * image.stride + x * 4..][..4];
            let actual = decoded.get_pixel(x as u32, y as u32).0;
            if source[3] == 0 || source[3] == 255 {
                assert_eq!(source[3], actual[3]);
            }
            changes.1 += usize::from(source[3] != actual[3]);
            if source[3] != 0 {
                changes.0 += source[..3]
                    .iter()
                    .zip(&actual[..3])
                    .filter(|(a, b)| a != b)
                    .count();
            }
        }
    }
    let mut allocation = vec![0xa5];
    allocation.extend(&image.rgba);
    let original = allocation.clone();
    let actual = libwebpop::encode(
        libwebpop::OpImage::Rgba {
            pixels: &allocation[1..],
            width: image.width,
            height: image.height,
            stride: image.stride,
        },
        &libwebpop::OpConfig::new(libwebpop::OpQuality::Lossless {
            near_lossless: case.input[3].try_into().unwrap(),
            effort: 70,
            exact: false,
        }),
    )
    .unwrap();
    assert_eq!(allocation, original);
    assert!(
        actual == bytes,
        "near-lossless strength {}, {}x{}, first differing byte {:?}; lengths {}/{}",
        case.input[3],
        image.width,
        image.height,
        actual.iter().zip(&bytes).position(|(a, b)| a != b),
        actual.len(),
        bytes.len()
    );
    let actual_decoded = image::load_from_memory_with_format(&actual, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(actual_decoded, decoded);
    if case.input[3] == 100 {
        image.check_decoded(&bytes);
        // Retain an unaligned start and exact minimum input length in Rust.
        let mut allocation = vec![0xa5];
        allocation.extend(&image.rgba);
        let original = allocation.clone();
        assert_eq!(
            {
                let mut output = Vec::new();
                libwebpop::WebPEncodeLosslessRGBA(
                    &allocation[1..],
                    image.width,
                    image.height,
                    image.stride,
                    &mut output,
                )
                .unwrap();
                output
            },
            bytes
        );
        assert_eq!(allocation, original);
    }
    changes
}
