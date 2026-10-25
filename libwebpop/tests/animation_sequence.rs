#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use image::{AnimationDecoder, ImageDecoder};
use libwebpop::*;
use std::io::Cursor;
use support::animation_mux as corpus;

fn cases() -> Vec<support::Case> {
    let mut cases = Vec::new();
    for (width, height) in [(1, 1), (1, 7), (7, 1), (5, 7)] {
        for mode in 0..5 {
            let frames: Vec<_> = (0..4)
                .map(|i| {
                    // Repeated input, hidden RGB, transparency, variable codecs
                    // and unrelated row padding; last-row padding is absent.
                    let mut f = corpus::frame(
                        width,
                        height,
                        if i == 3 { (mode + 1) % 5 } else { mode },
                        i != 2,
                        support::SEED + i / 2,
                    );
                    f.control.duration_ms = [0, 1, 0x00ff_ffff, 37][i as usize];
                    f
                })
                .collect();
            cases.push(corpus::request(
                width,
                height,
                AnimationContainerOptions {
                    loop_count: 65535,
                    background: BackgroundColor {
                        red: 12,
                        green: 34,
                        blue: 56,
                        alpha: 78,
                    },
                    icc_profile: Some(vec![1, 2, 3]),
                    exif: Some(vec![]),
                    xmp: Some(vec![4, 5]),
                },
                &frames,
            ));
        }
    }
    for mode in 0..5 {
        cases.push(corpus::request(
            5,
            7,
            Default::default(),
            &[corpus::frame(5, 7, mode, true, support::SEED)],
        ));
    }
    let frames: Vec<_> = (0..40)
        .map(|i| {
            let mut f = corpus::frame(1, 1, 0, true, support::SEED);
            f.control.duration_ms = i % 3;
            f
        })
        .collect();
    cases.push(corpus::request(1, 1, Default::default(), &frames));
    cases
}

fn encode(case: &support::Case, origin: u64) -> Vec<u8> {
    let (width, height, container, frames) = corpus::parse(case);
    let mut encoder = AnimationEncoder::new(
        width,
        height,
        AnimationOptions {
            container,
            ..Default::default()
        },
    )
    .unwrap();
    let mut timestamp = origin;
    for f in &frames {
        let source = f.rgba.clone();
        encoder
            .add_rgba(&f.rgba, f.stride, timestamp, &f.options())
            .unwrap();
        assert_eq!(f.rgba, source);
        timestamp = timestamp
            .checked_add(u64::from(f.control.duration_ms))
            .unwrap();
    }
    let mut output = vec![0xab; 3];
    encoder.finish(timestamp, &mut output).unwrap();
    output
}

fn check_decoded(case: &support::Case, bytes: &[u8]) {
    let (width, height, _, expected) = corpus::parse(case);
    let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).unwrap();
    assert_eq!(decoder.dimensions(), (width, height));
    if expected.len() == 1 {
        assert!(!decoder.has_animation());
        let actual = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        let still =
            image::load_from_memory_with_format(&expected[0].still, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
        assert_eq!(actual, still);
        return;
    }
    assert!(decoder.has_animation());
    // image/image-webp returns every composited canvas and exact stored delays,
    // including zero. Full replacements make background conventions irrelevant.
    let actual = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(actual.len(), expected.len());
    let mut actual_time = 0u64;
    let mut expected_time = 0u64;
    for (actual, f) in actual.iter().zip(&expected) {
        assert_eq!(actual.delay().numer_denom_ms(), (f.control.duration_ms, 1));
        actual_time += u64::from(actual.delay().numer_denom_ms().0);
        expected_time += u64::from(f.control.duration_ms);
        assert_eq!(actual_time, expected_time);
        assert_eq!(actual.buffer().dimensions(), (width, height));
        assert_eq!((actual.left(), actual.top()), (0, 0));
        let still = image::load_from_memory_with_format(&f.still, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = (y * width as usize + x) * 4;
                let pixel = &actual.buffer().as_raw()[offset..offset + 4];
                let reference = &still.as_raw()[offset..offset + 4];
                assert_eq!(pixel, reference);
                if f.mode == 0 {
                    let input = &f.rgba[y * f.stride + x * 4..][..4];
                    assert_eq!(pixel[3], input[3]);
                    if input[3] != 0 {
                        assert_eq!(pixel, input);
                    }
                }
            }
        }
    }
}

#[test]
fn animation_sequence_preserves_canvases_timing_and_mux_bytes() {
    for case in cases() {
        let bytes = encode(&case, 1 << 40);
        let prepared: Vec<_> = corpus::evaluate(&case)
            .into_iter()
            .map(|v| v as u8)
            .collect();
        assert_eq!(bytes, prepared);
        check_decoded(&case, &bytes);
    }
}

#[test]
fn animation_sequence_timestamps_validate_before_mutation() {
    let mut encoder = AnimationEncoder::new(1, 1, Default::default()).unwrap();
    let mut output = vec![1, 2, 3];
    assert!(matches!(
        encoder.finish(0, &mut output),
        Err(AnimationError::NoFrames)
    ));
    assert_eq!(output, [1, 2, 3]);
    let options = AnimationFrameOptions::default();
    let pixel = [17, 37, 91, 255];
    let origin = u64::MAX - 0x00ff_ffff;
    encoder.add_rgba(&pixel, 4, origin, &options).unwrap();
    for finishing in [false, true] {
        for timestamp in [0, origin - 1] {
            let result = if finishing {
                encoder.finish(timestamp, &mut output)
            } else {
                encoder.add_rgba(&pixel, 4, timestamp, &options)
            };
            assert!(matches!(
                result,
                Err(AnimationError::InvalidTimestamp { frame_index: 0, .. })
            ));
            assert_eq!(output, [1, 2, 3]);
        }
    }
    encoder.add_rgba(&pixel, 4, u64::MAX, &options).unwrap();
    encoder.finish(u64::MAX, &mut output).unwrap();
    let mut a = corpus::frame(1, 1, 0, false, support::SEED);
    a.rgba = pixel.to_vec();
    WebPEncodeLosslessRGBA(&pixel, 1, 1, 4, &mut a.still).unwrap();
    a.control.duration_ms = 0x00ff_ffff;
    let mut b = corpus::frame(1, 1, 0, false, support::SEED);
    b.rgba = pixel.to_vec();
    b.still = a.still.clone();
    let case = corpus::request(1, 1, Default::default(), &[a, b]);
    let expected: Vec<_> = corpus::evaluate(&case)
        .into_iter()
        .map(|v| v as u8)
        .collect();
    assert_eq!(output, expected);
    let saved = output.clone();
    assert!(matches!(
        encoder.finish(u64::MAX, &mut output),
        Err(AnimationError::InvalidState)
    ));
    assert!(matches!(
        encoder.add_rgba(&pixel, 4, u64::MAX, &options),
        Err(AnimationError::InvalidState)
    ));
    assert_eq!(output, saved);

    let mut encoder = AnimationEncoder::new(1, 1, Default::default()).unwrap();
    encoder.add_rgba(&pixel, 4, 0, &options).unwrap();
    for timestamp in [1 << 24, u64::MAX] {
        assert!(matches!(
            encoder.add_rgba(&pixel, 4, timestamp, &options),
            Err(AnimationError::InvalidDuration { frame_index: 0, .. })
        ));
        assert!(matches!(
            encoder.finish(timestamp, &mut output),
            Err(AnimationError::InvalidDuration { frame_index: 0, .. })
        ));
        assert_eq!(output, saved);
    }
    encoder.finish(0, &mut output).unwrap();
}

#[test]
fn animation_sequence_rejects_layout_and_settings_without_poisoning() {
    for (width, height) in [(0, 1), (1, 0), (16384, 1), (1, u32::MAX)] {
        assert!(matches!(
            AnimationEncoder::new(width, height, Default::default()),
            Err(AnimationError::InvalidCanvas)
        ));
    }
    assert!(matches!(
        AnimationEncoder::new(
            1,
            1,
            AnimationOptions {
                policy: AnimationEncodingPolicy::Optimized,
                ..Default::default()
            }
        ),
        Err(AnimationError::UnsupportedPolicy)
    ));
    let mut encoder = AnimationEncoder::new(1, 2, Default::default()).unwrap();
    let rgba = [128; 9]; // stride 5, no final padding
    let original = rgba;
    let good = AnimationFrameOptions::default();
    for frame_index in 0..2 {
        for (bytes, stride) in [
            (&rgba[..], 0),
            (&rgba[..], 3),
            (&rgba[..8], 5),
            (&rgba[..], usize::MAX),
        ] {
            assert!(
                matches!(encoder.add_rgba(bytes, stride, 0, &good), Err(AnimationError::Encoding { frame_index: i, .. }) if i == frame_index)
            );
        }
        let bad = [
            AnimationFrameOptions::Lossless(LosslessOptions {
                quality: 101,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossless(LosslessOptions {
                method: 7,
                ..Default::default()
            }),
            AnimationFrameOptions::NearLossless {
                strength: -1,
                options: Default::default(),
            },
            AnimationFrameOptions::NearLossless {
                strength: 101,
                options: Default::default(),
            },
            AnimationFrameOptions::Lossy(LossyOptions {
                quality: f32::NAN,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossy(LossyOptions {
                quality: f32::INFINITY,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossy(LossyOptions {
                quality: -1.0,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossy(LossyOptions {
                alpha_quality: -1,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossy(LossyOptions {
                alpha_quality: 101,
                ..Default::default()
            }),
            AnimationFrameOptions::Lossy(LossyOptions {
                method: u32::MAX,
                ..Default::default()
            }),
        ];
        for options in bad {
            assert!(
                matches!(encoder.add_rgba(&rgba, 5, 0, &options), Err(AnimationError::Encoding { frame_index: i, .. }) if i == frame_index)
            );
        }
        encoder.add_rgba(&rgba, 5, 0, &good).unwrap();
    }
    let mut output = Vec::new();
    encoder.finish(0, &mut output).unwrap();
    assert_eq!(rgba, original);
}

#[test]
fn animation_sequence_lossless_effort_and_exact_hidden_rgb() {
    for method in 0..=6 {
        let options = LosslessOptions {
            quality: 13,
            method,
            exact: true,
        };
        let mut encoder = AnimationEncoder::new(1, 1, Default::default()).unwrap();
        let mut mux = AnimationContainer::new(1, 1, Default::default()).unwrap();
        for (i, pixel) in [[1, 2, 3, 0], [4, 5, 6, 0]].iter().enumerate() {
            encoder
                .add_rgba(
                    pixel,
                    4,
                    i as u64,
                    &AnimationFrameOptions::Lossless(options),
                )
                .unwrap();
            mux.push_frame(
                EncodedFrame::encode_lossless_rgba(pixel, 1, 1, 4, &options).unwrap(),
                ContainerFrameOptions {
                    duration_ms: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let mut bytes = Vec::new();
        encoder.finish(2, &mut bytes).unwrap();
        let mut expected = Vec::new();
        mux.assemble(&mut expected).unwrap();
        assert_eq!(bytes, expected);
        let frames = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].buffer().as_raw(), &[1, 2, 3, 0]);
        assert_eq!(frames[1].buffer().as_raw(), &[4, 5, 6, 0]);
    }
}

#[test]
fn animation_sequence_custom_codec_settings_match_still_decodes() {
    let mut settings = Vec::new();
    for strength in [0, 60, 100] {
        for exact in [false, true] {
            settings.push(AnimationFrameOptions::NearLossless {
                strength,
                options: LosslessOptions {
                    quality: 53,
                    method: 2,
                    exact,
                },
            });
        }
    }
    for quality in [0.0, 42.5, 100.0] {
        for alpha_quality in [0, 100] {
            for use_sharp_yuv in [false, true] {
                settings.push(AnimationFrameOptions::Lossy(LossyOptions {
                    quality,
                    method: 4,
                    alpha_quality,
                    use_sharp_yuv,
                }));
            }
        }
    }
    let source = corpus::frame(65, 5, 0, true, support::SEED);
    let mut encoder = AnimationEncoder::new(65, 5, Default::default()).unwrap();
    let mut expected = Vec::new();
    for (i, options) in settings.iter().enumerate() {
        encoder
            .add_rgba(&source.rgba, source.stride, i as u64, options)
            .unwrap();
        let mut still = Vec::new();
        match options {
            AnimationFrameOptions::NearLossless { strength, options } => {
                encode_near_lossless_rgba(
                    &source.rgba,
                    65,
                    5,
                    source.stride,
                    *strength,
                    options,
                    &mut still,
                )
                .unwrap();
            }
            AnimationFrameOptions::Lossy(options) => {
                encode_lossy_rgba(&source.rgba, 65, 5, source.stride, options, &mut still).unwrap();
            }
            _ => unreachable!(),
        }
        expected.push(
            image::load_from_memory_with_format(&still, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8(),
        );
    }
    let mut bytes = Vec::new();
    encoder.finish(settings.len() as u64, &mut bytes).unwrap();
    let frames = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    assert_eq!(frames.len(), settings.len());
    for (actual, expected) in frames.iter().zip(expected) {
        assert_eq!(actual.buffer(), &expected);
        assert_eq!(actual.delay().numer_denom_ms(), (1, 1));
    }
}

#[test]
fn animation_sequence_committed_mux_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_sequence.txt"));
    assert_eq!(fixtures.len(), 26);
    for (case, expected) in fixtures {
        let actual = encode(&case, 1 << 40);
        let expected: Vec<_> = expected.into_iter().map(|v| v as u8).collect();
        assert_eq!(actual, expected);
        check_decoded(&case, &expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_sequence_matches_pinned_mux_reference() {
    use std::fmt::Write;
    let cases = cases();
    let outputs = native::run("animation-sequence", &cases);
    let updating =
        std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1"));
    let mut text = format!(
        "# Full-frame sequences via pinned scalar WebPMux.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
        support::UPSTREAM_REVISION,
        support::SEED
    );
    for (case, expected) in cases.iter().zip(&outputs) {
        let actual: Vec<_> = encode(case, 1 << 40).into_iter().map(u32::from).collect();
        native::compare(case, expected, &actual);
        let expected_bytes: Vec<_> = expected.iter().map(|&v| v as u8).collect();
        check_decoded(case, &expected_bytes);
        if updating {
            text.push_str(&case.function);
            for word in &case.input {
                write!(text, " {word:08x}").unwrap();
            }
            text.push_str(" |");
            for word in expected {
                write!(text, " {word:08x}").unwrap();
            }
            text.push('\n');
        }
    }
    if updating {
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/animation_sequence.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_sequence.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, actual)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, actual);
        }
    }
    eprintln!("{} full-frame sequences match pinned WebPMux", cases.len());
}
