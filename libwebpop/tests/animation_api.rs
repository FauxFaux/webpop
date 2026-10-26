use std::io::Cursor;

use image::AnimationDecoder;
use libwebpop::{
    OpAnimEncoder, OpAnimKeyframes, OpAnimOptions, OpAnimPolicy, OpConfig, OpImage, OpQuality,
    encode,
};

fn image(pixels: &[u8], width: u32, height: u32, stride: usize) -> OpImage<'_> {
    OpImage::Rgba {
        pixels,
        width,
        height,
        stride,
    }
}

fn chunks(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    assert_eq!(&bytes[8..12], b"WEBP");
    let mut rest = &bytes[12..];
    let mut result = Vec::new();
    while !rest.is_empty() {
        let size = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
        result.push((&rest[..4], &rest[8..8 + size]));
        if size & 1 != 0 {
            assert_eq!(rest[8 + size], 0);
        }
        rest = &rest[8 + size + (size & 1)..];
    }
    result
}

#[test]
fn defaults_empty_finish_and_single_frame_cleanup() {
    let defaults = OpAnimOptions::default();
    assert_eq!(defaults.policy, OpAnimPolicy::Optimized);
    assert_eq!(defaults.background, [255; 4]);
    assert_eq!(defaults.loop_count, 0);
    assert_eq!(defaults.keyframes, OpAnimKeyframes::Disabled);
    assert!(!defaults.allow_mixed && !defaults.minimize_size);
    let error = OpAnimEncoder::new(1, 1, defaults.clone())
        .unwrap()
        .finish()
        .unwrap_err();
    let error: &dyn std::error::Error = &error;
    assert!(!error.to_string().is_empty());
    for (width, height) in [(0, 1), (1, 0), (16384, 1), (1, u32::MAX)] {
        assert!(OpAnimEncoder::new(width, height, defaults.clone()).is_err());
    }
    let pixels = [12, 34, 56, 255];
    let mut encoder = OpAnimEncoder::new(1, 1, defaults).unwrap();
    encoder
        .add(image(&pixels, 1, 1, 4), 37, &OpConfig::default())
        .unwrap();
    assert_eq!(
        encoder.finish().unwrap(),
        encode(image(&pixels, 1, 1, 4), &OpConfig::default()).unwrap()
    );
}

#[test]
fn full_frame_playback_metadata_and_exact_durations() {
    let mut options = OpAnimOptions::default();
    options.policy = OpAnimPolicy::FullFrame;
    options.loop_count = 13;
    options.background = [12, 34, 56, 78];
    options.icc_profile = Some(vec![1, 2, 3]);
    options.exif = Some(vec![]);
    options.xmp = Some(vec![4, 5]);
    let config = OpConfig::new(OpQuality::Lossless {
        near_lossless: 100,
        effort: 70,
        exact: true,
    });
    let mut encoder = OpAnimEncoder::new(3, 2, options).unwrap();
    let mut expected = Vec::new();
    let delays = [0, 1, 0x00ff_ffff, 37];
    for (i, delay) in delays.into_iter().enumerate() {
        let mut pixels = vec![0xa5; 27]; // 15-byte stride; no final row padding.
        let mut canvas = Vec::new();
        for y in 0..2 {
            for x in 0..3 {
                let pixel = [(20 * i + x) as u8, (30 + y) as u8, 70, [0, 127, 255][x]];
                pixels[y * 15 + x * 4..][..4].copy_from_slice(&pixel);
                canvas.extend(pixel);
            }
        }
        let original = pixels.clone();
        encoder
            .add(image(&pixels, 3, 2, 15), delay, &config)
            .unwrap();
        assert_eq!(pixels, original);
        expected.push(canvas);
    }
    let bytes = encoder.finish().unwrap();
    let chunks = chunks(&bytes);
    let tags: Vec<_> = chunks.iter().map(|(tag, _)| *tag).collect();
    assert_eq!(
        tags,
        [
            b"VP8X", b"ICCP", b"ANIM", b"ANMF", b"ANMF", b"ANMF", b"ANMF", b"EXIF", b"XMP "
        ]
    );
    assert_eq!(chunks[2].1, [56, 34, 12, 78, 13, 0]);
    assert_eq!(chunks[7].1, []);
    let frames = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    assert_eq!(frames.len(), expected.len());
    for ((frame, expected), delay) in frames.iter().zip(expected).zip(delays) {
        assert_eq!(frame.buffer().as_raw(), &expected);
        assert_eq!(frame.delay().numer_denom_ms(), (delay, 1));
    }
}

#[test]
fn invalid_adds_allow_retry_without_advancing_time_or_frame_index() {
    for policy in [OpAnimPolicy::Optimized, OpAnimPolicy::FullFrame] {
        let mut options = OpAnimOptions::default();
        options.policy = policy;
        let mut actual = OpAnimEncoder::new(2, 1, options.clone()).unwrap();
        let mut expected = OpAnimEncoder::new(2, 1, options).unwrap();
        let pixels = [12, 34, 56, 255, 7, 8, 9, 255];
        let changed = [70, 80, 90, 255, 2, 3, 4, 255];
        let config = OpConfig::default();
        for encoder in [&mut actual, &mut expected] {
            encoder.add(image(&pixels, 2, 1, 8), 10, &config).unwrap();
        }
        let mut bad_method = config;
        bad_method.method = 7;
        for bad in [
            bad_method,
            OpConfig::new(OpQuality::Lossless {
                near_lossless: 101,
                effort: 70,
                exact: false,
            }),
            OpConfig::new(OpQuality::Lossless {
                near_lossless: 100,
                effort: 101,
                exact: false,
            }),
            OpConfig::new(OpQuality::Lossy {
                quality: f32::NAN,
                alpha_quality: 100,
                sharp_yuv: false,
            }),
            OpConfig::new(OpQuality::Lossy {
                quality: 75.0,
                alpha_quality: 101,
                sharp_yuv: false,
            }),
        ] {
            let error = actual.add(image(&pixels, 2, 1, 8), 500, &bad).unwrap_err();
            assert!(error.to_string().contains("frame 1"));
        }
        for (pixels, width, height, stride, duration) in [
            (&pixels[..], 1, 2, 4, 500), // Same buffer size, different geometry.
            (&pixels[..7], 2, 1, 8, 500),
            (&pixels[..], 2, 1, 7, 500),
            (&pixels[..], 2, 1, 8, 1 << 24),
        ] {
            assert!(
                actual
                    .add(image(pixels, width, height, stride), duration, &config)
                    .is_err()
            );
        }
        for encoder in [&mut actual, &mut expected] {
            encoder.add(image(&changed, 2, 1, 8), 40, &config).unwrap();
        }
        assert_eq!(actual.finish().unwrap(), expected.finish().unwrap());
    }
}

#[test]
fn optimized_repeat_overflow_keeps_total_duration() {
    let mut encoder = OpAnimEncoder::new(1, 1, OpAnimOptions::default()).unwrap();
    for _ in 0..3 {
        encoder
            .add(
                image(&[12, 34, 56, 255], 1, 1, 4),
                0x00ff_ffff,
                &OpConfig::default(),
            )
            .unwrap();
    }
    let bytes = encoder.finish().unwrap();
    let frames: Vec<_> = chunks(&bytes)
        .into_iter()
        .filter(|(tag, _)| *tag == b"ANMF")
        .collect();
    assert!(frames.len() >= 3);
    let sum: u64 = frames
        .iter()
        .map(|(_, data)| u64::from(u32::from_le_bytes([data[12], data[13], data[14], 0])))
        .sum();
    assert_eq!(sum, 3 * 0x00ff_ffff);
}

#[test]
fn checked_global_controls() {
    for keyframes in [
        OpAnimKeyframes::Disabled,
        OpAnimKeyframes::AllFrames,
        OpAnimKeyframes::Bounded {
            min_interval: 2,
            max_interval: 5,
        },
    ] {
        let mut options = OpAnimOptions::default();
        options.keyframes = keyframes;
        assert!(OpAnimEncoder::new(1, 1, options).is_ok());
    }
    for (min_interval, max_interval) in [(0, 0), (0, 1), (u32::MAX, 5), (0, u32::MAX)] {
        let mut options = OpAnimOptions::default();
        options.keyframes = OpAnimKeyframes::Bounded {
            min_interval,
            max_interval,
        };
        assert!(OpAnimEncoder::new(1, 1, options).is_err());
    }
    for control in 0..3 {
        let mut options = OpAnimOptions::default();
        options.policy = OpAnimPolicy::FullFrame;
        match control {
            0 => options.minimize_size = true,
            1 => options.allow_mixed = true,
            _ => options.keyframes = OpAnimKeyframes::AllFrames,
        }
        assert!(OpAnimEncoder::new(1, 1, options).is_err());
    }
}

#[test]
fn supported_facade_matches_existing_sequence_and_mixed_mappings() {
    use libwebpop::internal::{
        AnimationEncoder, AnimationEncodingPolicy, AnimationFrameOptions, AnimationKeyframePolicy,
        AnimationOptimizerOptions, AnimationOptions, LosslessOptions, LossyOptions,
    };
    let mut configs = [
        OpConfig::default(),
        OpConfig::new(OpQuality::Lossless {
            near_lossless: 35,
            effort: 90,
            exact: true,
        }),
        OpConfig::new(OpQuality::Lossy {
            quality: 78.25,
            alpha_quality: 70,
            sharp_yuv: true,
        }),
    ];
    configs[0].method = 0;
    configs[1].method = 6;
    configs[2].method = 3;
    let settings = [
        AnimationFrameOptions::NearLossless {
            strength: 100,
            options: LosslessOptions {
                quality: 70,
                method: 0,
                exact: false,
            },
        },
        AnimationFrameOptions::NearLossless {
            strength: 35,
            options: LosslessOptions {
                quality: 90,
                method: 6,
                exact: true,
            },
        },
        AnimationFrameOptions::Lossy(LossyOptions {
            quality: 78.25,
            method: 3,
            alpha_quality: 70,
            use_sharp_yuv: true,
        }),
    ];
    for (policy, allow_mixed) in [
        (OpAnimPolicy::FullFrame, false),
        (OpAnimPolicy::Optimized, false),
        (OpAnimPolicy::Optimized, true),
    ] {
        for keyframes in [
            OpAnimKeyframes::Disabled,
            OpAnimKeyframes::AllFrames,
            OpAnimKeyframes::Bounded {
                min_interval: 2,
                max_interval: 5,
            },
        ] {
            if policy == OpAnimPolicy::FullFrame && keyframes != OpAnimKeyframes::Disabled {
                continue;
            }
            let mut options = OpAnimOptions::default();
            options.policy = policy;
            options.allow_mixed = allow_mixed;
            options.keyframes = keyframes;
            let mut actual = OpAnimEncoder::new(5, 7, options).unwrap();
            let mut expected = AnimationEncoder::new(
                5,
                7,
                AnimationOptions {
                    policy: if policy == OpAnimPolicy::FullFrame {
                        AnimationEncodingPolicy::FullFrame
                    } else {
                        AnimationEncodingPolicy::Optimized
                    },
                    optimizer: AnimationOptimizerOptions {
                        allow_mixed,
                        minimize_size: false,
                        keyframes: match keyframes {
                            OpAnimKeyframes::Disabled => AnimationKeyframePolicy::Disabled,
                            OpAnimKeyframes::AllFrames => AnimationKeyframePolicy::AllFrames,
                            OpAnimKeyframes::Bounded {
                                min_interval,
                                max_interval,
                            } => AnimationKeyframePolicy::Bounded {
                                min_interval,
                                max_interval,
                            },
                            _ => unreachable!(),
                        },
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            let mut timestamp = 500;
            for (i, duration) in [100, 0, 40, 10, 50, 1, 70, 20, 0].into_iter().enumerate() {
                let pixels: Vec<u8> = (0..35)
                    .flat_map(|p| {
                        [
                            (p * 13 + (i / 2) * 31) as u8,
                            (p * 7) as u8,
                            (p * 19) as u8,
                            if p % 3 == 0 { 128 } else { 255 },
                        ]
                    })
                    .collect();
                actual
                    .add(image(&pixels, 5, 7, 20), duration, &configs[i % 3])
                    .unwrap();
                expected
                    .add_rgba(&pixels, 20, timestamp, &settings[i % 3])
                    .unwrap();
                timestamp += u64::from(duration);
            }
            let mut bytes = Vec::new();
            expected.finish(timestamp, &mut bytes).unwrap();
            assert_eq!(
                actual.finish().unwrap(),
                bytes,
                "{policy:?} {allow_mixed} {keyframes:?}"
            );
        }
    }
}
