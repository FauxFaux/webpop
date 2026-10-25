use libwebpop::{OpConfig, OpImage, OpQuality, encode};

fn lossy(quality: f32, alpha_quality: u8, sharp_yuv: bool) -> OpQuality {
    OpQuality::Lossy {
        quality,
        alpha_quality,
        sharp_yuv,
    }
}

#[test]
fn stable_api_matches_committed_c_files() {
    let ppm = include_bytes!("fixtures/corpus/test_ref.ppm");
    let header = b"P6\n128 128\n255\n";
    assert!(ppm.starts_with(header));
    let mut pixels: Vec<_> = ppm[header.len()..]
        .chunks_exact(3)
        .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
        .collect();
    for (quality, expected, gradient_alpha) in [
        (
            OpQuality::default(),
            &include_bytes!("fixtures/corpus/test_ref.webp")[..],
            false,
        ),
        (
            lossy(75.0, 100, false),
            &include_bytes!("fixtures/corpus/test_ref.lossy.webp")[..],
            false,
        ),
        (
            lossy(75.0, 70, false),
            &include_bytes!("fixtures/corpus/test_ref.alpha.webp")[..],
            true,
        ),
    ] {
        if gradient_alpha {
            for (i, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                pixel[3] = ((17 * (i % 128) + 13 * (i / 128)) & 255) as u8;
            }
        }
        let config = OpConfig::new(quality);
        let output = encode(
            OpImage::Rgba {
                pixels: &pixels,
                width: 128,
                height: 128,
                stride: 128 * 4,
            },
            &config,
        )
        .unwrap();
        assert_eq!(output, expected, "{config:?}");
    }
    assert_eq!(OpConfig::default(), OpConfig::new(OpQuality::default()));
}

#[test]
fn exact_lossless_roundtrips_unaligned_padded_rows_for_all_methods() {
    let (width, height, stride) = (5usize, 7usize, 23usize);
    // Prefix and row padding are sentinels; omit padding on the final row.
    let mut storage = vec![0xa5; 1 + (height - 1) * stride + width * 4];
    let mut expected = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pixel = [
                (x * 47 + y * 13) as u8,
                (x * 19 + y * 31) as u8,
                (x * 7 + y * 53) as u8,
                [0, 1, 127, 254, 255][(x + y) % 5],
            ];
            storage[1 + y * stride + x * 4..][..4].copy_from_slice(&pixel);
            expected.extend(pixel);
        }
    }
    let original = storage.clone();
    for method in 0..=6 {
        for effort in [0, 100] {
            let mut config = OpConfig::new(OpQuality::Lossless {
                near_lossless: 100,
                effort,
                exact: true,
            });
            config.method = method;
            let bytes = encode(
                OpImage::Rgba {
                    pixels: &storage[1..],
                    width: width as u32,
                    height: height as u32,
                    stride,
                },
                &config,
            )
            .unwrap();
            let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            assert_eq!(decoded.dimensions(), (width as u32, height as u32));
            assert_eq!(decoded.as_raw(), &expected, "{config:?}");
            assert_eq!(storage, original);
        }
    }
}

#[test]
fn settings_are_validated_without_truncating_values() {
    let pixel = [12, 34, 56, 128];
    let image = OpImage::Rgba {
        pixels: &pixel,
        width: 1,
        height: 1,
        stride: 4,
    };
    let mut invalid = Vec::new();
    for method in [7, u8::MAX] {
        for quality in [OpQuality::default(), lossy(75.0, 100, true)] {
            let mut config = OpConfig::new(quality);
            config.method = method;
            invalid.push(config);
        }
    }
    for value in [101, u8::MAX] {
        invalid.extend([
            OpConfig::new(OpQuality::Lossless {
                near_lossless: value,
                effort: 70,
                exact: false,
            }),
            OpConfig::new(OpQuality::Lossless {
                near_lossless: 100,
                effort: value,
                exact: false,
            }),
            OpConfig::new(lossy(75.0, value, false)),
        ]);
    }
    for quality in [-0.01, 100.01, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
        invalid.push(OpConfig::new(lossy(quality, 100, false)));
    }
    for config in invalid {
        let error = encode(image, &config).unwrap_err();
        let error: &dyn std::error::Error = &error;
        assert!(!error.to_string().is_empty(), "{config:?}");
    }
    assert_eq!(pixel, [12, 34, 56, 128]);
}

#[test]
fn geometry_is_checked_and_final_row_padding_is_optional() {
    for quality in [OpQuality::default(), lossy(75.0, 100, true)] {
        let config = OpConfig::new(quality);
        for (pixels, width, height, stride) in [
            (&[0; 4][..], 0, 1, 4),
            (&[0; 4][..], 1, 0, 4),
            (&[][..], 16384, 1, 65536),
            (&[][..], 1, u32::MAX, 4),
            (&[0; 4][..], 1, 1, 3),
            (&[0; 3][..], 1, 1, 4),
            (&[0; 10][..], 1, 2, 7),
            (&[][..], 1, 2, usize::MAX),
        ] {
            assert!(
                encode(
                    OpImage::Rgba {
                        pixels,
                        width,
                        height,
                        stride,
                    },
                    &config,
                )
                .is_err()
            );
        }
        // Single rows never read a second row, even with a maximal stride.
        for (pixels, height, stride) in [
            (&[12, 34, 56, 255][..], 1, usize::MAX),
            (
                &[12, 34, 56, 255, 0xa5, 0xa5, 0xa5, 12, 34, 56, 255][..],
                2,
                7,
            ),
        ] {
            let output = encode(
                OpImage::Rgba {
                    pixels,
                    width: 1,
                    height,
                    stride,
                },
                &config,
            )
            .unwrap();
            let decoded =
                image::load_from_memory_with_format(&output, image::ImageFormat::WebP).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (1, height));
        }
    }
}
