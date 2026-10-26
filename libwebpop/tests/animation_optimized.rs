#[path = "support/animation_public.rs"]
mod corpus;
#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::internal::*;
use support::animation_lossy_playback::{chunks, playback};

#[test]
fn animation_optimized_committed_public_rgba_files_and_playback() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_public.txt"));
    let cases = corpus::fixtures();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(&cases) {
        assert_eq!(stored.input, case.input);
        let bytes: Vec<_> = expected.iter().map(|&v| v as u8).collect();
        assert_eq!(corpus::encode(case), bytes);
        playback(&corpus::base(case), &bytes);
    }
}

#[test]
fn animation_optimized_reuses_committed_mixed_keyframe_and_overflow_evidence() {
    let mut count = 0;
    for text in [
        include_str!("fixtures/animation_keyframe_selection.txt"),
        include_str!("fixtures/animation_keyframe_overflow.txt"),
    ] {
        for (case, expected) in fixture_support::fixtures(text) {
            if case.input[2] & 2 == 0 {
                continue;
            }
            let bytes = corpus::file_bytes(&case, &expected);
            assert_eq!(corpus::encode(&case), bytes);
            playback(&corpus::base(&case), &bytes);
            count += 1;
        }
    }
    assert!(count > 10);
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_optimized_public_rgba_matches_pinned_reference_and_playback() {
    let cases = corpus::cases();
    let outputs = native::run("animation-public", &cases);
    let mut public_cases = Vec::new();
    let mut public_files = Vec::new();
    let mut decode_cases = Vec::new();
    let mut rendered = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        let bytes = corpus::file_bytes(case, expected);
        assert_eq!(
            corpus::encode(case),
            bytes,
            "public RGBA request {:?}",
            &case.input[..10]
        );
        let frame_count = chunks(&bytes[12..])
            .iter()
            .filter(|(tag, _)| *tag == b"ANMF")
            .count();
        if frame_count != 1 {
            let mut public = case.clone();
            public.input[3] = 1;
            public_cases.push(public);
            public_files.push(bytes.clone());
        }
        if let Some((request, pixels)) = playback(&corpus::base(case), &bytes) {
            decode_cases.push(request);
            rendered.push(pixels);
        }
    }
    for ((case, actual), expected) in public_cases
        .iter()
        .zip(&public_files)
        .zip(native::run("animation-public-assemble", &public_cases))
    {
        assert_eq!(*actual, corpus::file_bytes(case, &expected));
    }
    for ((case, actual), expected) in decode_cases
        .iter()
        .zip(&rendered)
        .zip(native::run("animation-public-playback", &decode_cases))
    {
        native::compare(case, &expected, actual);
    }
    eprintln!(
        "{} public RGBA files match pinned pre-reconstruction C; {} public C assemblies and {} independent/native playbacks match",
        cases.len(),
        public_cases.len(),
        decode_cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_optimized_public_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixtures();
    let outputs = native::run("animation-public-fixtures", &cases);
    let mut text = format!(
        "# Public RGBA mappings: complete files extracted from pinned scalar C AnimationCache.\n# upstream={} seed={:08x}; mode 0 before OptimizeSingleFrame.\n",
        support::UPSTREAM_REVISION,
        support::SEED
    );
    for (case, output) in cases.iter().zip(&outputs) {
        let bytes = corpus::file_bytes(case, output);
        assert_eq!(corpus::encode(case), bytes);
        text.push_str(&case.function);
        for word in &case.input {
            write!(text, " {word:08x}").unwrap();
        }
        text.push_str(" |");
        for byte in bytes {
            write!(text, " {byte:08x}").unwrap();
        }
        text.push('\n');
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::write(
            format!(
                "{}/tests/fixtures/animation_public.txt",
                env!("CARGO_MANIFEST_DIR")
            ),
            text,
        )
        .unwrap();
    }
}

fn mixed(options: LossyOptions, near_lossless: i32) -> AnimationFrameOptions {
    AnimationFrameOptions::Mixed {
        primary: AnimationFrameCodec::Vp8l,
        options,
        near_lossless,
        exact: true,
    }
}

#[test]
fn animation_optimized_defaults_checked_global_options_and_unsupported_combinations() {
    let defaults = AnimationOptions::default();
    assert_eq!(defaults.policy, AnimationEncodingPolicy::FullFrame);
    assert_eq!(
        defaults.optimizer,
        AnimationOptimizerOptions {
            minimize_size: false,
            allow_mixed: false,
            keyframes: AnimationKeyframePolicy::Disabled
        }
    );
    assert_eq!(
        defaults.container.background,
        BackgroundColor {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 255
        }
    );
    assert_eq!(defaults.container.loop_count, 0);
    for policy in [
        AnimationEncodingPolicy::FullFrame,
        AnimationEncodingPolicy::Optimized,
    ] {
        for (width, height) in [(0, 1), (1, 0), (16384, 1), (1, u32::MAX)] {
            assert!(matches!(
                AnimationEncoder::new(
                    width,
                    height,
                    AnimationOptions {
                        policy,
                        ..Default::default()
                    }
                ),
                Err(AnimationError::InvalidCanvas)
            ));
        }
        let mut encoder = AnimationEncoder::new(
            1,
            1,
            AnimationOptions {
                policy,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(matches!(
            encoder.add_rgba(&[], usize::MAX, 0, &mixed(Default::default(), 100)),
            Err(AnimationError::UnsupportedFrameOptions { frame_index: 0 })
        ));
        encoder
            .add_rgba(&[1, 2, 3, 255], 4, 0, &Default::default())
            .unwrap();
        assert!(matches!(
            encoder.add_rgba(&[], usize::MAX, 0, &mixed(Default::default(), 100)),
            Err(AnimationError::UnsupportedFrameOptions { frame_index: 1 })
        ));
        let mut bytes = Vec::new();
        encoder.finish(0, &mut bytes).unwrap();
        let mut still = Vec::new();
        libwebpop::WebPEncodeLosslessRGBA(&[1, 2, 3, 255], 1, 1, 4, &mut still).unwrap();
        assert_eq!(bytes, still);
        assert!(matches!(
            encoder.add_rgba(&[], usize::MAX, 0, &mixed(Default::default(), 100)),
            Err(AnimationError::InvalidState)
        ));
    }
    for optimizer in [
        AnimationOptimizerOptions {
            minimize_size: true,
            ..Default::default()
        },
        AnimationOptimizerOptions {
            allow_mixed: true,
            ..Default::default()
        },
        AnimationOptimizerOptions {
            keyframes: AnimationKeyframePolicy::AllFrames,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            AnimationEncoder::new(
                1,
                1,
                AnimationOptions {
                    optimizer,
                    ..Default::default()
                }
            ),
            Err(AnimationError::UnsupportedOptimizerOptions)
        ));
    }
    for (min_interval, max_interval) in [(0, 1), (u32::MAX, 2), (0, u32::MAX)] {
        for minimize_size in [false, true] {
            assert!(
                AnimationEncoder::new(
                    1,
                    1,
                    AnimationOptions {
                        policy: AnimationEncodingPolicy::Optimized,
                        optimizer: AnimationOptimizerOptions {
                            minimize_size,
                            keyframes: AnimationKeyframePolicy::Bounded {
                                min_interval,
                                max_interval
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }
}

#[test]
fn animation_optimized_validation_retries_include_skips_and_preserve_final_output() {
    for (policy, allow_mixed) in [
        (AnimationEncodingPolicy::FullFrame, false),
        (AnimationEncodingPolicy::Optimized, false),
        (AnimationEncodingPolicy::Optimized, true),
    ] {
        let make = || {
            AnimationEncoder::new(
                1,
                2,
                AnimationOptions {
                    policy,
                    optimizer: AnimationOptimizerOptions {
                        allow_mixed,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let (mut encoder, mut control) = (make(), make());
        let rgba = [12, 34, 56, 255, 0xa5, 12, 34, 56, 255];
        let original = rgba;
        let options = AnimationFrameOptions::default();
        let origin = 1u64 << 40;
        let mut output = vec![11, 22, 33];
        assert!(matches!(
            encoder.finish(origin, &mut output),
            Err(AnimationError::NoFrames)
        ));
        assert_eq!(output, [11, 22, 33]);
        for timestamp in [origin, origin, origin + 0xffffff] {
            encoder.add_rgba(&rgba, 5, timestamp, &options).unwrap();
            control.add_rgba(&rgba, 5, timestamp, &options).unwrap();
        }
        for (bytes, stride) in [
            (&rgba[..], 0),
            (&rgba[..], 3),
            (&rgba[..8], 5),
            (&rgba[..], usize::MAX),
        ] {
            assert!(matches!(
                encoder.add_rgba(bytes, stride, origin + 0x1000000, &options),
                Err(AnimationError::Encoding { frame_index: 3, .. })
            ));
        }
        let mut bad = vec![
            AnimationFrameOptions::Lossless(LosslessOptions {
                quality: u32::MAX,
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
                method: 7,
                ..Default::default()
            }),
        ];
        for quality in [f32::NAN, f32::INFINITY, -1., 100.001] {
            bad.push(AnimationFrameOptions::Lossy(LossyOptions {
                quality,
                ..Default::default()
            }));
            if allow_mixed {
                bad.push(mixed(
                    LossyOptions {
                        quality,
                        ..Default::default()
                    },
                    100,
                ));
            }
        }
        if allow_mixed {
            bad.extend([
                mixed(Default::default(), -1),
                mixed(Default::default(), 101),
                mixed(
                    LossyOptions {
                        alpha_quality: 101,
                        ..Default::default()
                    },
                    100,
                ),
                mixed(
                    LossyOptions {
                        method: 7,
                        ..Default::default()
                    },
                    100,
                ),
            ]);
        }
        for options in bad {
            // Same pixels would skip; palette eligibility must not hide invalid
            // settings, including dormant VP8 settings with VP8L primary.
            assert!(matches!(
                encoder.add_rgba(&rgba, 5, origin + 0x1000000, &options),
                Err(AnimationError::Encoding { frame_index: 3, .. })
            ));
        }
        for finishing in [false, true] {
            let result = if finishing {
                encoder.finish(origin, &mut output)
            } else {
                encoder.add_rgba(&rgba, 5, origin, &options)
            };
            assert!(matches!(
                result,
                Err(AnimationError::InvalidTimestamp { frame_index: 2, .. })
            ));
            let result = if finishing {
                encoder.finish(origin + 0x1ffffff, &mut output)
            } else {
                encoder.add_rgba(&rgba, 5, origin + 0x1ffffff, &options)
            };
            assert!(matches!(
                result,
                Err(AnimationError::InvalidDuration { frame_index: 2, .. })
            ));
        }
        assert_eq!(output, [11, 22, 33]);
        let changed = [56, 34, 12, 255, 0xa5, 56, 34, 12, 255];
        let lossy = AnimationFrameOptions::Lossy(LossyOptions {
            quality: 75.25,
            ..Default::default()
        });
        encoder
            .add_rgba(&changed, 5, origin + 0x1000000, &lossy)
            .unwrap();
        control
            .add_rgba(&changed, 5, origin + 0x1000000, &lossy)
            .unwrap();
        encoder.finish(origin + 0x1000001, &mut output).unwrap();
        let mut expected = Vec::new();
        control.finish(origin + 0x1000001, &mut expected).unwrap();
        assert_eq!(output, expected);
        assert_eq!(rgba, original);
        let finished = output.clone();
        assert!(matches!(
            encoder.finish(u64::MAX, &mut output),
            Err(AnimationError::InvalidState)
        ));
        assert!(matches!(
            encoder.add_rgba(&rgba, 5, u64::MAX, &options),
            Err(AnimationError::InvalidState)
        ));
        assert_eq!(output, finished);
    }
    let mut encoder = AnimationEncoder::new(
        1,
        1,
        AnimationOptions {
            policy: AnimationEncodingPolicy::Optimized,
            ..Default::default()
        },
    )
    .unwrap();
    encoder
        .add_rgba(
            &[12, 34, 56, 255],
            4,
            u64::MAX - 0xffffff,
            &Default::default(),
        )
        .unwrap();
    encoder
        .add_rgba(&[12, 34, 56, 255], 4, u64::MAX, &Default::default())
        .unwrap();
    encoder.finish(u64::MAX, &mut Vec::new()).unwrap();
}

#[test]
fn animation_optimized_cropped_and_merged_single_frames_keep_anmf() {
    for case in corpus::reconstruction_cases() {
        let bytes = corpus::encode(&case);
        let top = chunks(&bytes[12..]);
        let frames: Vec<_> = top.iter().filter(|(tag, _)| *tag == b"ANMF").collect();
        assert_eq!(frames.len(), 1);
        let duration = support::animation_lossy_playback::le24(&frames[0].1[12..]);
        assert_eq!(duration, case.input[4]);
        let (_, pixels) = playback(&corpus::base(&case), &bytes).unwrap();
        assert!(pixels[5..].iter().all(|&v| v == 0));
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_optimized_single_frame_reconstruction_exception_matches_recorded_boundary() {
    let before_cases = corpus::reconstruction_cases();
    let mut after_cases = before_cases.clone();
    for case in &mut after_cases {
        case.input[3] = 1;
    }
    let before = native::run("animation-public-single-before", &before_cases);
    let after = native::run("animation-public-single-after", &after_cases);
    for (((case, expected), after_case), reconstructed) in before_cases
        .iter()
        .zip(&before)
        .zip(&after_cases)
        .zip(&after)
    {
        let actual = corpus::encode(case);
        assert_eq!(actual, corpus::file_bytes(case, expected));
        let smaller = corpus::file_bytes(after_case, reconstructed);
        assert!(smaller.len() < actual.len());
        assert!(chunks(&actual[12..]).iter().any(|(tag, _)| *tag == b"ANMF"));
        assert!(
            !chunks(&smaller[12..])
                .iter()
                .any(|(tag, _)| *tag == b"ANMF")
        );
        let decoded = image::load_from_memory_with_format(&smaller, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.dimensions(), (8, 7));
        assert!(decoded.pixels().all(|p| p.0[3] == 0));
    }
}
