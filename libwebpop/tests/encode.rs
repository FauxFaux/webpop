use libwebpop::enc::picture_csp_enc::RgbaInputError;
use libwebpop::{LosslessEncodeError, LosslessOptions, encode_lossless_rgba};

#[test]
fn rgba_entry_point_round_trips_palettes_photos_and_transparency() {
    let mut seed = 0x6d2b79f5u32;
    let mut count = 0;
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
            let original = rgba.clone();
            for (quality, method) in [(0, 0), (25, 1), (70, 4), (100, 5), (100, 6)] {
                for exact in [false, true] {
                    let options = LosslessOptions {
                        quality,
                        method,
                        exact,
                    };
                    let encoded =
                        encode_lossless_rgba(&rgba, width, height, stride, &options).unwrap();
                    assert_eq!(
                        encoded,
                        encode_lossless_rgba(&rgba, width, height, stride, &options).unwrap()
                    );
                    let decoded =
                        image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP)
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
                    assert_eq!(rgba, original);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 350);
}

#[test]
fn invalid_input_and_options_return_errors() {
    let defaults = LosslessOptions::default();
    for (rgba, w, h, stride, expected) in [
        (&[][..], 0, 1, 0, RgbaInputError::InvalidDimensions),
        (&[][..], 1, 0, 4, RgbaInputError::InvalidDimensions),
        (&[][..], 16384, 1, 65536, RgbaInputError::InvalidDimensions),
        (&[0; 4][..], 1, 1, 3, RgbaInputError::InvalidStride),
        (&[0; 3][..], 1, 1, 4, RgbaInputError::InputTooShort),
        (&[][..], 1, 2, usize::MAX, RgbaInputError::SizeOverflow),
    ] {
        assert!(
            matches!(encode_lossless_rgba(rgba, w, h, stride, &defaults),
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
        assert!(encode_lossless_rgba(&[0; 4], 1, 1, 4, &options).is_err());
    }
}
