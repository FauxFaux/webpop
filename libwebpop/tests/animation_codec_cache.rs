#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;
use support::animation_codec_cache as corpus;

use support::animation_lossy_playback::{chunks, le24, playback};

#[test]
fn animation_codec_cache_committed_states_files_and_playback() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_codec_cache.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        playback(&case, &corpus::output_bytes(&case, expected));
    }
}

#[test]
fn animation_codec_cache_skips_save_configs_and_keep_originals() {
    for mixed in [false, true] {
        for primary in 0..4 {
            let similar = mixed || primary == 0 || primary == 3;
            let case = corpus::repeats(primary, mixed, 0, 3, similar);
            let (states, _) = corpus::encode(&case);
            let size = corpus::state_size(&case);
            let first = &states[..size];
            let second = &states[size..2 * size];
            assert_eq!(&second[..4], &[2, 0, 1, 0]);
            assert_eq!(&first[4..8], &second[4..8]);
            assert_eq!(&first[29..], &second[29..]);
            let f = &corpus::frames(&case)[1];
            assert_eq!(
                &second[10..17],
                support::animation_mixed::config_words(f.config)
            );
            assert_eq!(
                &second[17..24],
                support::animation_mixed::config_words(f.config.reversed())
            );
        }
    }
}

#[test]
fn animation_codec_cache_validation_retry_and_finish_states() {
    for mixed in [false, true] {
        let mut cache = AnimationCache::new(
            1,
            1,
            AnimationCacheOptions {
                allow_mixed: mixed,
                ..Default::default()
            },
        )
        .unwrap();
        let mut output = vec![1, 2, 3];
        assert!(matches!(
            cache.finish(0, &mut output),
            Err(AnimationError::NoFrames)
        ));
        let words = [0xff304050];
        let current = ArgbView::new(&words, 1, 1, 1).unwrap();
        let origin = u64::MAX - 100;
        let config = MixedEncodingConfig::default();
        cache.add_argb(current, origin, &config).unwrap();
        cache.add_argb(current, origin + 1, &config).unwrap();
        let state = corpus::observation(&cache);
        for invalid in [
            MixedEncodingConfig {
                codec: LossyOptions {
                    quality: f32::NAN,
                    ..config.codec
                },
                ..config
            },
            MixedEncodingConfig {
                codec: LossyOptions {
                    quality: 100.1,
                    ..config.codec
                },
                ..config
            },
            MixedEncodingConfig {
                codec: LossyOptions {
                    method: 7,
                    ..config.codec
                },
                ..config
            },
            MixedEncodingConfig {
                codec: LossyOptions {
                    alpha_quality: -1,
                    ..config.codec
                },
                ..config
            },
            MixedEncodingConfig {
                near_lossless: -1,
                ..config
            },
            MixedEncodingConfig {
                near_lossless: 101,
                ..config
            },
        ] {
            assert!(matches!(
                cache.add_argb(current, origin + 2, &invalid),
                Err(AnimationError::Encoding { frame_index: 2, .. })
            ));
            assert_eq!(corpus::observation(&cache), state);
        }
        assert!(matches!(
            cache.add_argb(
                ArgbView::new(&[0; 2], 2, 1, 2).unwrap(),
                origin + 2,
                &config
            ),
            Err(AnimationError::Candidate { frame_index: 2, .. })
        ));
        assert!(matches!(
            cache.add_argb(current, origin, &config),
            Err(AnimationError::InvalidTimestamp { frame_index: 1, .. })
        ));
        assert!(cache.finish(origin, &mut output).is_err());
        assert_eq!(output, [1, 2, 3]);
        assert_eq!(corpus::observation(&cache), state);
        // An oversized interval remains retryable even on an unchanged input.
        // Use a separate origin so the invalid interval itself fits in u64.
        let mut timing = AnimationCache::new(1, 1, Default::default()).unwrap();
        timing.add_argb(current, 0, &config).unwrap();
        let before = corpus::observation(&timing);
        assert!(matches!(
            timing.add_argb(current, 1 << 24, &config),
            Err(AnimationError::InvalidDuration {
                frame_index: 0,
                duration_ms: 0x1000000
            })
        ));
        assert!(matches!(
            timing.finish(1 << 24, &mut output),
            Err(AnimationError::InvalidDuration {
                frame_index: 0,
                duration_ms: 0x1000000
            })
        ));
        assert_eq!(corpus::observation(&timing), before);
        assert_eq!(output, [1, 2, 3]);
        timing.finish(1, &mut Vec::new()).unwrap();
        cache.finish(u64::MAX, &mut output).unwrap();
        assert!(cache.canvases().0.is_empty());
        assert!(cache.pending_control().is_none());
        assert!(matches!(
            cache.finish(u64::MAX, &mut output),
            Err(AnimationError::InvalidState)
        ));
        assert!(matches!(
            cache.add_argb(current, u64::MAX, &config),
            Err(AnimationError::InvalidState)
        ));
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_codec_cache_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    native::parallel_cases("animation-codec-cache", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
    });
    eprintln!("{} shared cache cases match pinned C", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_codec_cache_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-codec-cache-fixtures", &cases);
    let mut requests = Vec::new();
    let mut observations = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        if let Some((request, observation)) = playback(case, &corpus::output_bytes(case, expected))
        {
            requests.push(request);
            observations.push(observation);
        }
    }
    for ((case, expected), actual) in requests
        .iter()
        .zip(&observations)
        .zip(native::run("animation-codec-cache-decodes", &requests))
    {
        native::compare(case, expected, &actual);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C shared animation cache v1.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for word in &case.input {
                write!(text, " {word:08x}").unwrap();
            }
            text.push_str(" |");
            for word in output {
                write!(text, " {word:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/animation_codec_cache.txt"
            ),
            text,
        )
        .unwrap();
    }
}

#[test]
fn animation_codec_cache_fixed_holds_use_saved_config_before_switch() {
    for mixed in [false, true] {
        for primary in 0..4 {
            let case = corpus::repeats(primary, mixed, 1, 3, false);
            let (states, bytes) = corpus::encode(&case);
            let size = corpus::state_size(&case);
            // At the exact maximum, the original pending frame keeps its
            // duration. The add at maximum+1 emits it and retains a 1 ms hold.
            assert_eq!(&states[size..size + 4], &[2, 0, 1, 0]);
            assert_eq!(states[size + 9], 0xffffff);
            assert_eq!(&states[2 * size..2 * size + 4], &[3, 1, 1, 1]);
            assert_eq!(&states[2 * size + 4..2 * size + 10], &[0, 0, 1, 1, 1, 1]);
            let top = chunks(&bytes[12..]);
            let frames: Vec<_> = top.iter().filter(|c| c.0 == b"ANMF").collect();
            assert_eq!(frames.len(), 2);
            assert_eq!(le24(&frames[0].1[12..]), 0xffffff);
            assert_eq!(le24(&frames[1].1[12..]), 1);
            assert_eq!(frames[1].1[15], 0); // blend, no disposal
            let submitted = corpus::frames(&case);
            let saved = submitted[1].config.primary;
            let nested = chunks(&frames[1].1[16..]);
            if mixed || saved == AnimationCandidateCodec::Vp8l {
                assert_eq!(nested.len(), 1);
                assert_eq!(
                    nested[0],
                    (
                        b"VP8L".as_slice(),
                        [0x2f, 0, 0, 0, 0x10, 0x88, 0x88, 0x08].as_slice()
                    )
                );
            } else {
                assert_eq!(nested.len(), 2);
                assert_eq!(nested[0], (b"ALPH".as_slice(), [0, 0].as_slice()));
                assert_eq!(nested[1].0, b"VP8 ");
                assert_eq!(nested[1].1.len(), 24);
            }
            let (_, decoded) = playback(&case, &bytes).unwrap();
            let canvas_size = (case.input[0] * case.input[1] * 4) as usize;
            assert_eq!(&decoded[5..5 + canvas_size], &decoded[6 + canvas_size..]);
        }
    }
}

#[test]
fn animation_codec_cache_repeated_holds_preserve_piecewise_playback() {
    for mixed in [false, true] {
        for primary in 0..4 {
            let case = corpus::repeats(primary, mixed, 2, 40, false);
            let (states, bytes) = corpus::encode(&case);
            assert!(
                states[..corpus::state_size(&case) * 40]
                    .chunks_exact(corpus::state_size(&case))
                    .all(|s| s[2] == 1)
            );
            let top = chunks(&bytes[12..]);
            let frames: Vec<_> = top.iter().filter(|c| c.0 == b"ANMF").collect();
            assert_eq!(frames.len(), 40);
            assert!(frames.iter().all(|f| le24(&f.1[12..]) == 0xffffff));
            let (_, decoded) = playback(&case, &bytes).unwrap();
            let step = (case.input[0] * case.input[1] * 4 + 1) as usize;
            let expected = &decoded[5..4 + step];
            for (i, frame) in decoded[4..].chunks_exact(step).enumerate() {
                assert_eq!(frame[0], (i as u32 + 1) * 0xffffff);
                assert_eq!(&frame[1..], expected);
            }
        }
    }
}
