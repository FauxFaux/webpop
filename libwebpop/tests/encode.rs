use libwebpop::WebPEncodeLosslessRGBA;
use libwebpop::internal::enc::picture_csp_enc::RgbaInputError;
use libwebpop::internal::{LosslessEncodeError, LosslessOptions, encode_lossless_rgba};

mod support;

#[test]
fn public_rgba_fixtures_match_scalar_c() {
    let cases = support::public_images::fixture_cases();
    let lines: Vec<_> = include_str!("fixtures/public_rgba.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .collect();
    assert_eq!(lines.len(), cases.len());
    for (line, case) in lines.into_iter().zip(cases) {
        let (request, response) = line.split_once(" | ").unwrap();
        let mut words = request.split_whitespace();
        assert_eq!(words.next().unwrap(), case.function);
        let count: usize = words.next().unwrap().parse().unwrap();
        let input: Vec<u32> = words.map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, input.len());
        assert_eq!(input, case.input);
        let mut words = response.split_whitespace();
        let count: usize = words.next().unwrap().parse().unwrap();
        let expected: Vec<u8> = words.map(|w| u8::from_str_radix(w, 16).unwrap()).collect();
        assert_eq!(count, expected.len());
        assert_eq!(support::public_images::encode(&case), expected);
        support::public_images::check_decoded(&case, &expected);
    }
    let image = support::public_images::real_image();
    let mut encoded = Vec::new();
    WebPEncodeLosslessRGBA(
        &image.rgba,
        image.width,
        image.height,
        image.stride,
        &mut encoded,
    )
    .unwrap();
    assert_eq!(encoded, include_bytes!("fixtures/corpus/test_ref.webp"));
    support::public_images::check_decoded(&image.request(), &encoded);
}

#[test]
fn public_default_api_is_layout_independent() {
    for pattern in [0, 2, 6, 12, 20, 30, 35] {
        let image = support::public_images::image(19, 17, pattern);
        let mut packed = Vec::new();
        for y in 0..image.height as usize {
            packed.extend_from_slice(
                &image.rgba[y * image.stride..y * image.stride + image.width as usize * 4],
            );
        }
        let mut unaligned = vec![0xa5];
        unaligned.extend_from_slice(&image.rgba);
        let mut a = Vec::new();
        WebPEncodeLosslessRGBA(
            &unaligned[1..],
            image.width,
            image.height,
            image.stride,
            &mut a,
        )
        .unwrap();
        let mut b = Vec::new();
        encode_lossless_rgba(
            &packed,
            image.width,
            image.height,
            image.width as usize * 4,
            &LosslessOptions::default(),
            &mut b,
        )
        .unwrap();
        assert_eq!(a, b);
        // Surplus bytes and stride beyond the last row are never consumed.
        packed.extend([0x5a; 7]);
        assert_eq!(a, {
            let mut output = Vec::new();
            WebPEncodeLosslessRGBA(
                &packed,
                image.width,
                image.height,
                image.width as usize * 4,
                &mut output,
            )
            .unwrap();
            output
        });
    }
    assert!(WebPEncodeLosslessRGBA(&[1, 2, 3, 255], 1, 1, usize::MAX, &mut Vec::new()).is_ok());
}

#[test]
fn rgba_entry_point_round_trips_palettes_photos_and_transparency() {
    let mut seed = 0x6d2b79f5u32;
    let mut cases = Vec::new();
    for (width, height) in [(1u32, 1u32), (1, 39), (41, 1), (17, 19), (65, 67)] {
        let stride = width as usize * 4 + 7;
        for pattern in 0..7 {
            let mut rgba = vec![0xa5; (height as usize - 1) * stride + width as usize * 4];
            for y in 0..height as usize {
                for x in 0..width as usize {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let pixel = match pattern {
                        0 => [12, 34, 56, 255],
                        1 => [x as u8, y as u8, x.wrapping_mul(y) as u8, 255],
                        2 => seed.to_le_bytes(),
                        3 => [seed as u8, (seed >> 8) as u8, (seed >> 16) as u8, 0],
                        4 => [
                            seed as u8,
                            (seed >> 8) as u8,
                            (seed >> 16) as u8,
                            if x % 3 == 0 { 0 } else { 255 },
                        ],
                        5 => {
                            if (x / 3 + y / 4) % 2 == 0 {
                                [12, 34, 56, 0]
                            } else {
                                [78, 90, 12, 128]
                            }
                        }
                        _ => {
                            let c = (x + y * width as usize) as u8;
                            [c, c, c, 255]
                        }
                    };
                    rgba[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&pixel);
                }
            }
            for (quality, method) in [(0, 0), (25, 1), (70, 4), (100, 5), (100, 6)] {
                for exact in [false, true] {
                    let options = LosslessOptions {
                        quality,
                        method,
                        exact,
                    };
                    cases.push((width, height, stride, rgba.clone(), options));
                }
            }
        }
    }
    assert_eq!(cases.len(), 350);
    support::parallel::map(&cases, |(width, height, stride, rgba, options)| {
        let (width, height, stride) = (*width, *height, *stride);
        let exact = options.exact;
        let original = rgba.clone();
        let mut encoded = Vec::new();
        encode_lossless_rgba(rgba, width, height, stride, options, &mut encoded).unwrap();
        assert_eq!(encoded, {
            let mut output = Vec::new();
            encode_lossless_rgba(rgba, width, height, stride, options, &mut output).unwrap();
            output
        });
        let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.dimensions(), (width, height));
        for y in 0..height as usize {
            for x in 0..width as usize {
                let expected = &rgba[y * stride + x * 4..y * stride + x * 4 + 4];
                let actual = decoded.get_pixel(x as u32, y as u32).0;
                assert_eq!(actual[3], expected[3]);
                if exact || expected[3] != 0 {
                    assert_eq!(actual, expected);
                }
            }
        }
        assert_eq!(*rgba, original);
    });
}

#[test]
fn invalid_input_and_options_return_errors() {
    let defaults = LosslessOptions::default();
    for (rgba, w, h, stride, expected) in [
        (&[][..], 0, 1, 0, RgbaInputError::InvalidDimensions),
        (&[][..], 1, 0, 4, RgbaInputError::InvalidDimensions),
        (&[][..], 16384, 1, 65536, RgbaInputError::InvalidDimensions),
        (&[][..], 1, 16384, 4, RgbaInputError::InvalidDimensions),
        (
            &[][..],
            u32::MAX,
            1,
            usize::MAX,
            RgbaInputError::InvalidDimensions,
        ),
        (&[][..], 1, u32::MAX, 4, RgbaInputError::InvalidDimensions),
        (&[][..], 16383, 16383, 65532, RgbaInputError::InputTooShort),
        (&[0; 4][..], 1, 1, 3, RgbaInputError::InvalidStride),
        (&[0; 3][..], 1, 1, 4, RgbaInputError::InputTooShort),
        (&[][..], 1, 2, usize::MAX, RgbaInputError::SizeOverflow),
        (&[][..], 1, 3, usize::MAX / 2, RgbaInputError::SizeOverflow),
    ] {
        assert!(
            matches!(encode_lossless_rgba(rgba, w, h, stride, &defaults, &mut Vec::new()),
            Err(LosslessEncodeError::Input(actual)) if actual == expected)
        );
        assert!(
            matches!(WebPEncodeLosslessRGBA(rgba, w, h, stride, &mut Vec::new()),
            Err(LosslessEncodeError::Input(actual)) if actual == expected)
        );
    }
    for options in [
        LosslessOptions {
            quality: 101,
            ..defaults
        },
        LosslessOptions {
            method: 7,
            ..defaults
        },
    ] {
        let result = encode_lossless_rgba(&[0; 4], 1, 1, 4, &options, &mut Vec::new());
        if options.quality > 100 {
            assert!(matches!(result, Err(LosslessEncodeError::InvalidQuality)));
        } else {
            assert!(matches!(result, Err(LosslessEncodeError::InvalidMethod)));
        }
    }
}

#[test]
#[ignore = "manual release timing baseline; timings are not assertions"]
fn public_encode_baseline() {
    use std::{hint::black_box, time::Instant};
    for image in [
        support::public_images::image(128, 128, 0),
        support::public_images::image(128, 128, 2),
        support::public_images::image(128, 128, 24),
        support::public_images::image(129, 33, 5),
        support::public_images::real_image(),
    ] {
        let mut timings = Vec::new();
        let mut size = 0;
        // Warm up once, then record the median of five measured encodes.
        for iteration in 0..6 {
            let start = Instant::now();
            let mut encoded = Vec::new();
            WebPEncodeLosslessRGBA(
                black_box(&image.rgba),
                image.width,
                image.height,
                image.stride,
                &mut encoded,
            )
            .unwrap();
            let elapsed = start.elapsed();
            size = black_box(encoded).len();
            if iteration > 0 {
                timings.push(elapsed);
            }
        }
        timings.sort();
        eprintln!(
            "{} {}x{}: {} bytes, median {:.3} ms",
            image.name,
            image.width,
            image.height,
            size,
            timings[2].as_secs_f64() * 1000.0
        );
    }
}
