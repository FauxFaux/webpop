#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;
use support::animation_cache as corpus;
use support::animation_lossless::composite_lossless;

fn canonical_rgba(words: impl IntoIterator<Item = u32>) -> Vec<u32> {
    words
        .into_iter()
        .flat_map(|p| {
            if p >> 24 == 0 {
                [0; 4]
            } else {
                [(p >> 16) & 255, (p >> 8) & 255, p & 255, p >> 24]
            }
        })
        .collect()
}

fn has_animation(bytes: &[u8]) -> bool {
    let mut at = 12;
    while at < bytes.len() {
        let tag = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if tag == b"ANMF" {
            return true;
        }
        at += 8 + size + (size & 1);
    }
    assert_eq!(at, bytes.len());
    false
}

fn check_playback(case: &support::Case, bytes: &[u8]) -> Option<(support::Case, Vec<u32>)> {
    let (w, h) = (case.input[0], case.input[1]);
    let mut timeline = Vec::new();
    let mut total = 0u32;
    for f in corpus::frames(case) {
        let view = ArgbView::new(f.words, w as i32, h as i32, f.stride).unwrap();
        let rgba =
            canonical_rgba((0..h as usize).flat_map(|y| view.row(y).unwrap().iter().copied()));
        timeline.push((total, total + f.duration, rgba));
        total += f.duration;
    }
    if !has_animation(bytes) {
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(image.dimensions(), (w, h));
        let actual: Vec<_> = image
            .as_raw()
            .chunks_exact(4)
            .flat_map(|p| {
                if p[3] == 0 {
                    [0; 4]
                } else {
                    p.try_into().map(|p: [u8; 4]| p.map(u32::from)).unwrap()
                }
            })
            .collect();
        for (_, _, expected) in &timeline {
            assert_eq!(&actual, expected);
        }
        return None;
    }
    let observations = composite_lossless(bytes);
    assert_eq!(&observations[..2], &[w, h]);
    assert_eq!(observations[3], if case.input[5] != 0 { 65535 } else { 0 });
    let canvas_words = (w * h * 4) as usize;
    let mut start = 0;
    for frame in observations[4..].chunks_exact(canvas_words + 1) {
        let end = frame[0];
        assert!(end >= start);
        if end > start {
            let mut covered = 0;
            for (a, b, expected) in &timeline {
                let overlap = end.min(*b).saturating_sub(start.max(*a));
                if overlap != 0 {
                    assert_eq!(&frame[1..], expected);
                    covered += overlap;
                }
            }
            assert_eq!(covered, end - start);
        } else {
            assert!(
                timeline
                    .iter()
                    .any(|(a, b, expected)| *a <= end && end <= *b && &frame[1..] == expected)
            );
        }
        start = end;
    }
    assert_eq!(start, total);
    let mut request = vec![bytes.len() as u32];
    request.extend(bytes.iter().copied().map(u32::from));
    Some((support::Case::new("AnimationDecode", request), observations))
}

#[test]
fn animation_cache_committed_files_states_and_playback() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_cache.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        let bytes = corpus::output_bytes(&case, expected);
        let _ = check_playback(&case, &bytes);
    }
}

#[test]
fn animation_cache_duration_split_threshold_and_final_hold() {
    for timing in [2, 3] {
        let case = corpus::case(1, 1, 1, 6, timing, true, 2);
        let (states, bytes) = corpus::encode(&case);
        // Two identical original inputs leave just the first keyframe pending.
        assert_eq!(&states[10..14], &[2, 0, 1, 0]);
        assert_eq!(states[19], if timing == 2 { 0xffffff } else { 0xfffffe });
        let final_state = &states[20..30];
        assert_eq!(final_state[0], 2);
        assert_eq!(final_state[2], 0);
        assert_eq!(final_state[8], 0);
        if timing == 2 {
            assert_eq!(final_state[1], 2);
            assert_eq!(final_state[3], 1);
            let decoded = composite_lossless(&bytes);
            assert_eq!(decoded[2], 2);
            assert_eq!(decoded[4], 0xffffff);
            assert_eq!(decoded[9], 0x1000000);
        } else {
            assert_eq!(final_state[1], 1);
            assert!(!has_animation(&bytes));
        }
        let _ = check_playback(&case, &bytes);
    }
}

#[test]
fn animation_cache_repeated_overflow_flushes_before_changed_frame() {
    // Each maximum interval after a repeat may fill the two-slot cache with a
    // hold immediately before a changed canvas needs another candidate slot.
    for pattern in [1, 2, 4, 7] {
        let case = corpus::case(5, 7, pattern, 4, 4, true, 40);
        let (states, bytes) = corpus::encode(&case);
        for state in states[..400].chunks_exact(10) {
            assert_eq!(state[2], 1);
            assert!(state[1] + state[2] <= 2 * state[0]);
        }
        let _ = check_playback(&case, &bytes);
    }
}

#[test]
fn animation_cache_validation_recovery_wide_timing_and_terminal_states() {
    assert!(matches!(
        LosslessAnimationCache::new(0, 1, Default::default()),
        Err(AnimationError::InvalidCanvas)
    ));
    let mut cache = LosslessAnimationCache::new(1, 1, Default::default()).unwrap();
    let mut output = vec![1, 2, 3];
    assert!(matches!(
        cache.finish(0, &mut output),
        Err(AnimationError::NoFrames)
    ));
    let words = [0xff123456];
    let view = ArgbView::new(&words, 1, 1, 1).unwrap();
    let codec = LosslessOptions::default();
    let origin = u64::MAX - 0x1000001;
    cache.add_argb(view, origin, &codec).unwrap();
    // Merge a repeated input. Errors must refer to submitted indexes rather
    // than accepted frame indexes, and must not alter its pending duration.
    cache.add_argb(view, origin + 1, &codec).unwrap();
    let saved = cache.status();
    for finish in [false, true] {
        for timestamp in [origin, u64::MAX] {
            let error = if finish {
                cache.finish(timestamp, &mut output)
            } else {
                cache.add_argb(view, timestamp, &codec)
            }
            .unwrap_err();
            match error {
                AnimationError::InvalidTimestamp { frame_index, .. }
                | AnimationError::InvalidDuration { frame_index, .. } => assert_eq!(frame_index, 1),
                _ => panic!("unexpected timing error: {error}"),
            }
            assert_eq!(cache.status(), saved);
            assert_eq!(output, [1, 2, 3]);
        }
    }
    for invalid in [
        LosslessOptions { method: 7, ..codec },
        LosslessOptions {
            quality: 101,
            ..codec
        },
    ] {
        assert!(matches!(
            cache.add_argb(view, origin + 2, &invalid),
            Err(AnimationError::Encoding { frame_index: 2, .. })
        ));
        assert_eq!(cache.status(), saved);
    }
    let wrong = [0; 2];
    assert!(matches!(
        cache.add_argb(ArgbView::new(&wrong, 2, 1, 2).unwrap(), origin + 2, &codec),
        Err(AnimationError::Candidate { frame_index: 2, .. })
    ));
    assert_eq!(cache.status(), saved);
    cache.finish(u64::MAX - 2, &mut output).unwrap();
    assert_eq!(cache.status().emitted_frames, 1);
    assert!(matches!(
        cache.finish(u64::MAX, &mut output),
        Err(AnimationError::InvalidState)
    ));
    assert!(matches!(
        cache.add_argb(view, u64::MAX, &codec),
        Err(AnimationError::InvalidState)
    ));
    assert_eq!(words, [0xff123456]);
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_cache_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-cache", &cases, |case, expected| {
        let (mut full, mut before_decode, mut holds, mut merged) = (0, 0, 0, 0);
        native::compare(case, expected, &case.rust());
        if case.input[3] == 1 {
            full += 1;
        } else {
            before_decode += 1;
        }
        let mut previous = 0;
        for (i, state) in expected[..case.input[4] as usize * 10]
            .chunks_exact(10)
            .enumerate()
        {
            let accepted = state[1] + state[2];
            holds += usize::from(accepted > previous + 1);
            merged += usize::from(i > 0 && accepted == previous);
            assert_eq!(state[2], 1);
            previous = accepted;
        }
        let _ = check_playback(case, &corpus::output_bytes(case, expected));
        [full, before_decode, holds, merged]
    });
    let full: usize = stats.iter().map(|counts| counts[0]).sum();
    let before_decode: usize = stats.iter().map(|counts| counts[1]).sum();
    let holds: usize = stats.iter().map(|counts| counts[2]).sum();
    let merged: usize = stats.iter().map(|counts| counts[3]).sum();

    assert!(full > 0 && before_decode > 0 && holds > 0 && merged > 0);
    eprintln!(
        "{} cache cases match scalar C; full={full}, before_decode={before_decode}, split-before-change={holds}, merged={merged}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_cache_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-cache-fixtures", &cases);
    let mut requests = Vec::new();
    let mut observations = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        if let Some((request, observation)) =
            check_playback(case, &corpus::output_bytes(case, expected))
        {
            requests.push(request);
            observations.push(observation);
        }
    }
    let decoded = native::run("animation-cache-decodes", &requests);
    for ((case, expected), actual) in requests.iter().zip(&observations).zip(&decoded) {
        native::compare(case, expected, actual);
    }
    eprintln!(
        "{} timed animation composites match pinned WebPAnimDecoder",
        requests.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C lossless animation cache fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
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
                "/tests/fixtures/animation_cache.txt"
            ),
            text,
        )
        .unwrap();
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_cache_merged_single_rectangle_decode_gate_is_explicit() {
    let mut case = corpus::case(8, 7, 0, 4, 0, false, 8);
    case.input[3] = 2; // Record pre-optimization and full public C results.
    let output = native::run("animation-cache-single-frame-gate", &[case.clone()]).remove(0);
    let before = corpus::output_bytes(&case, &output);
    let at = corpus::file_offset(&case) + 1 + before.len();
    let after: Vec<_> = output[at + 1..].iter().map(|&v| v as u8).collect();
    assert_eq!(output[at] as usize, after.len());
    assert!(has_animation(&before));
    assert!(!has_animation(&after));
    assert!(after.len() < before.len());
    let (states, actual) = corpus::encode(&case);
    assert_eq!(states, output[..corpus::file_offset(&case)]);
    assert_eq!(actual, before);
    assert_eq!(states[80], 8);
    assert_eq!(states[81], 1);
    let _ = check_playback(&case, &after);
}
