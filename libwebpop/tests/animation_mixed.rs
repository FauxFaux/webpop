#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::WebPEncodingError;
use libwebpop::internal::mux::anim_encode::*;
use libwebpop::internal::*;
use support::{animation_lossless as ll, animation_mixed as corpus};

struct Observation<'a> {
    trials: Vec<&'a [u32]>,
    candidate: &'a [u32],
    end: usize,
}

fn observation<'a>(words: &'a [u32], start: usize, config: MixedEncodingConfig) -> Observation<'a> {
    assert_eq!(&words[start..start + 7], corpus::config_words(config));
    assert_eq!(
        &words[start + 7..start + 14],
        corpus::config_words(config.reversed())
    );
    assert_eq!(&words[start + 1..start + 7], &words[start + 8..start + 14]);
    assert_eq!(words[start] + words[start + 7], 1);
    let count = words[start + 14] as usize;
    assert!(count <= 4);
    let mut at = start + 15;
    let mut trials = Vec::new();
    for _ in 0..count {
        let size = words[at + 11] as usize;
        let trial = &words[at..at + 12 + size];
        assert_eq!(
            &trial[3..9],
            &[
                config.codec.quality.to_bits(),
                config.codec.method,
                config.near_lossless as u32,
                u32::from(config.exact),
                config.codec.alpha_quality as u32,
                u32::from(config.codec.use_sharp_yuv)
            ]
        );
        assert_eq!(trial[10], 0); // autofilter
        assert!([0, 60].contains(&trial[9]));
        let bytes: Vec<_> = trial[12..]
            .iter()
            .map(|&v| u8::try_from(v).unwrap())
            .collect();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(
            bytes.len(),
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8
        );
        assert_eq!(trial[0] != 0, &bytes[12..16] == b"VP8L");
        let decoded =
            image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (trial[1], trial[2]));
        if trial[0] != 0 {
            assert_eq!(trial[9], 60);
        }
        trials.push(trial);
        at += 12 + size;
    }
    let end = if words[at] == 0 {
        at + 1
    } else {
        assert_eq!(words[at], 1);
        let size_at = at + 9 + words[at + 8] as usize;
        size_at + 1 + words[size_at] as usize
    };
    let candidate = &words[at..end];
    assert_eq!(candidate[0] != 0, !trials.is_empty());
    if let Some(best) = trials.iter().min_by_key(|t| t[11]) {
        assert_eq!(corpus::file_words(candidate), &best[12..]);
    }
    Observation {
        trials,
        candidate,
        end,
    }
}

fn check_trials(observed: &Observation<'_>, trials: &[corpus::Trial]) -> [usize; 5] {
    assert_eq!(observed.trials.len(), trials.len());
    let mut stats = [0; 5];
    for (actual, trial) in observed.trials.iter().zip(trials) {
        assert_eq!(*actual, trial.record);
        stats[usize::from(trial.codec == AnimationCandidateCodec::Vp8)] += 1;
    }
    // Strict size minimum keeps the first disposal/codec/mask, even when the
    // same size was reached by a different stream later in the evaluation.
    if let Some(best) = trials.iter().min_by_key(|t| t.record[11]) {
        assert_eq!(observed.candidate, best.words);
        for trial in trials {
            stats[2] += usize::from(
                trial.record[11] == best.record[11] && trial.record[12..] != best.record[12..],
            );
        }
        stats[3] += usize::from(best.codec == AnimationCandidateCodec::Vp8l);
        stats[4] += usize::from(best.codec == AnimationCandidateCodec::Vp8);
    }
    stats
}

fn check_candidate(case: &support::Case, words: &[u32]) -> [usize; 5] {
    let base = corpus::base(case);
    let a = &base.input;
    let (s, d) = ll::canvases(&base);
    let settings = corpus::options(case);
    let trials = corpus::trials(
        ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        ArgbView::new(d, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
        ll::rectangle(&a[13..]),
        &settings,
    );
    let observed = observation(words, 0, settings.codec);
    assert_eq!(observed.end, words.len());
    check_trials(&observed, &trials)
}

// Verify raw history independently of either codec's reconstruction; assemble
// accepted rectangles separately for actual decoded playback.
fn check_sequence(
    case: &support::Case,
    words: &[u32],
) -> (
    [usize; 5],
    Vec<support::animation_lossy_playback::Frame>,
    Vec<u8>,
) {
    use support::animation_lossy_playback::Frame;
    let (w, h) = (case.input[0], case.input[1]);
    let area = (w * h) as usize;
    let mut previous = vec![0; area];
    let mut carry = previous.clone();
    let mut previous_rect = FrameRectangle::default();
    let mut at = 0;
    let mut frames: Vec<Frame> = Vec::new();
    let mut encoded: Vec<(EncodedFrame, ContainerFrameOptions)> = Vec::new();
    let mut totals = [0; 5];
    for step in corpus::sequence_steps(case) {
        let observed = observation(words, at, step.config);
        for (sum, value) in totals.iter_mut().zip(check_trials(&observed, &step.trials)) {
            *sum += value;
        }
        if !frames.is_empty() {
            for i in 0..area {
                if previous[i] == step.current[i] {
                    carry[i] = previous[i];
                }
            }
        }
        if let Some(c) = step.candidate {
            let r = c.rect;
            if let Some(dispose) = c.previous_dispose {
                frames.last_mut().unwrap().control.dispose = dispose;
                encoded.last_mut().unwrap().1.dispose = dispose;
                if dispose == FrameDispose::Background {
                    for y in previous_rect.y_offset..previous_rect.y_offset + previous_rect.height {
                        for x in
                            previous_rect.x_offset..previous_rect.x_offset + previous_rect.width
                        {
                            carry[(y as u32 * w + x as u32) as usize] = 0;
                        }
                    }
                }
                for y in 0..r.height {
                    for x in 0..r.width {
                        let i = ((r.y_offset + y) as u32 * w + (r.x_offset + x) as u32) as usize;
                        if !c.carries_over() || c.carryover_mask[(y * r.width + x) as usize] == 0 {
                            carry[i] = step.current[i];
                        }
                    }
                }
            } else {
                assert!(frames.is_empty());
                carry.clone_from(&step.current);
            }
            previous.clone_from(&step.current);
            previous_rect = r;
            let control = ContainerFrameOptions {
                duration_ms: 1,
                ..c.control
            };
            frames.push(Frame {
                width: r.width as u32,
                height: r.height as u32,
                has_alpha: c.frame.has_alpha(),
                control,
                still: corpus::file_words(observed.candidate)
                    .iter()
                    .map(|&v| v as u8)
                    .collect(),
            });
            encoded.push((c.frame, control));
        } else {
            assert_eq!(observed.candidate, [0]);
        }
        assert_eq!(step.previous, previous);
        assert_eq!(step.carryover, carry);
        assert_eq!(step.previous_rect, previous_rect);
        at = observed.end;
        assert_eq!(
            &words[at..at + 4],
            &[
                previous_rect.x_offset as u32,
                previous_rect.y_offset as u32,
                previous_rect.width as u32,
                previous_rect.height as u32
            ]
        );
        at += 4;
        assert_eq!(&words[at..at + area], previous);
        at += area;
        assert_eq!(&words[at..at + area], carry);
        at += area;
    }
    assert_eq!(at, words.len());
    let mut mux = AnimationContainer::new(w, h, Default::default()).unwrap();
    for (frame, control) in encoded {
        mux.push_frame(frame, control).unwrap();
    }
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    (totals, frames, bytes)
}

fn check(case: &support::Case, words: &[u32]) -> [usize; 5] {
    if case.function == "AnimationMixed" {
        check_candidate(case, words)
    } else {
        check_sequence(case, words).0
    }
}

#[test]
fn animation_mixed_palette_thresholds_count_original_argb_and_minimize_bypasses_palette() {
    for count in 1..=260 {
        let mut words = vec![0xdeadbeef; 18 * 19 + 16];
        for y in 0..19 {
            for x in 0..16 {
                words[y * 18 + x] = ((y * 16 + x) % count) as u32;
            }
        }
        // Every color is fully transparent with distinct hidden RGB. Those words
        // still count before transparency and codec cleanup are applied.
        let view = ArgbView::new(&words, 16, 19, 18).unwrap();
        let e = mixed_candidate_eligibility(view, false);
        assert_eq!(e.palette_size, Some(count.min(257)));
        assert_eq!((e.lossless, e.lossy), (count < 194, count >= 31));
        assert_eq!(
            mixed_candidate_eligibility(view, true),
            MixedCandidateEligibility {
                palette_size: None,
                lossless: true,
                lossy: true
            }
        );
    }
    let case = corpus::palette_case(194, false, false, 4, 75., false);
    let b = corpus::base(&case);
    let a = &b.input;
    let (s, d) = ll::canvases(&b);
    let source = ArgbView::new(s, a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
    let mut changed = d.to_vec();
    let mut dst = ArgbViewMut::new(&mut changed, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
    let region = FrameRectangle {
        width: a[1] as i32,
        height: a[2] as i32,
        ..Default::default()
    };
    IncreaseTransparency(
        source,
        &region,
        &mut dst,
        &mut vec![0; (a[1] * a[2]) as usize],
    )
    .unwrap();
    assert!(
        mixed_candidate_eligibility(dst.as_view(), false)
            .palette_size
            .unwrap()
            < 31
    );
    let words = case.rust();
    let observed = observation(&words, 0, corpus::options(&case).codec);
    assert_eq!(observed.trials.len(), 1);
    assert_eq!(observed.trials[0][0], 0); // VP8 only, original 194 colors.
}

#[test]
fn animation_mixed_primary_area_and_fractional_quality_control_selection() {
    let ll = corpus::candidate(&corpus::primary_area_case(true)).unwrap();
    let vp8 = corpus::candidate(&corpus::primary_area_case(false)).unwrap();
    assert_eq!(ll.codec, AnimationCandidateCodec::Vp8l);
    assert_eq!(vp8.codec, AnimationCandidateCodec::Vp8l);
    assert_eq!(ll.previous_dispose, Some(FrameDispose::Background));
    assert_eq!(vp8.previous_dispose, Some(FrameDispose::None));
    assert_eq!((ll.rect.width, ll.rect.height), (2, 2));
    assert_eq!((vp8.rect.width, vp8.rect.height), (19, 17));
    for primary in [false, true] {
        let mut case = corpus::fractional_case(primary);
        assert_eq!(QualityToMaxDiff(61.5), 7);
        assert_eq!(QualityToMaxDiff(61.), 8);
        assert!(corpus::candidate(&case).is_some());
        case.input[14] = 61f32.to_bits();
        assert!(corpus::candidate(&case).is_none());
        let mut case = corpus::palette_case(1, primary, false, 4, 100., false);
        // A one-level RGB difference skips before palette eligibility, even
        // though this single-color palette could only encode a VP8L candidate.
        let a = &mut case.input[4..];
        let size = ((a[2] - 1) * a[3] + a[1]) as usize;
        for y in 0..a[2] {
            for x in 0..a[1] {
                a[18 + (y * a[3] + x) as usize] = 0xff304050;
                a[18 + size + (y * a[4] + x) as usize] = 0xff304051;
            }
        }
        assert!(corpus::candidate(&case).is_none());
    }
}

#[test]
fn animation_mixed_reversal_and_both_codec_validation_precede_heuristics() {
    let pixels = [0xff102030; 4];
    let view = ArgbView::new(&pixels, 2, 2, 2).unwrap();
    let default = MixedCandidateOptions::default();
    assert_eq!(default.codec.near_lossless, 100);
    for primary in [AnimationCandidateCodec::Vp8l, AnimationCandidateCodec::Vp8] {
        let mut config = MixedEncodingConfig {
            primary,
            codec: LossyOptions {
                quality: 98.5,
                method: 6,
                alpha_quality: 71,
                use_sharp_yuv: true,
            },
            near_lossless: 60,
            exact: true,
        };
        assert_ne!(config.primary, config.reversed().primary);
        assert_eq!(config.reversed().reversed(), config);
        for (field, value) in [(0, 7), (1, 101), (2, 101)] {
            let original = config;
            match field {
                0 => config.codec.method = value,
                1 => config.codec.alpha_quality = value as i32,
                _ => config.near_lossless = value as i32,
            }
            assert!(matches!(
                SetFrameMixed(
                    view,
                    view,
                    FrameRectangle::default(),
                    &MixedCandidateOptions {
                        codec: config,
                        ..default
                    }
                ),
                Err(AnimationCandidateError::Encoding(_))
            ));
            config = original;
        }
        for quality in [f32::NAN, f32::INFINITY, -1., 101.] {
            config.codec.quality = quality;
            assert!(matches!(
                SetFrameMixed(
                    view,
                    view,
                    FrameRectangle::default(),
                    &MixedCandidateOptions {
                        codec: config,
                        ..default
                    }
                ),
                Err(AnimationCandidateError::Encoding(
                    WebPEncodingError::InvalidQuality
                ))
            ));
        }
    }
    assert!(matches!(
        SetFrameMixed(
            view,
            view,
            FrameRectangle::default(),
            &MixedCandidateOptions {
                is_first_frame: true,
                ..default
            }
        ),
        Err(AnimationCandidateError::InvalidFirstFrame)
    ));
    assert_eq!(pixels, [0xff102030; 4]);
}

fn still(frame: EncodedFrame) -> Vec<u8> {
    let mut mux =
        AnimationContainer::new(frame.width(), frame.height(), Default::default()).unwrap();
    mux.push_frame(frame, Default::default()).unwrap();
    let mut result = Vec::new();
    mux.assemble(&mut result).unwrap();
    result
}

#[test]
fn animation_mixed_exact_reversal_reaches_vp8_cleanup_and_preserves_still_defaults() {
    for method in 0..7 {
        for sharp in [false, true] {
            let case = corpus::exact_case(false, sharp);
            let b = corpus::base(&case);
            let a = &b.input;
            let (_, pixels) = ll::canvases(&b);
            let before = pixels.to_vec();
            let view = ArgbView::new(pixels, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
            let rect = FrameRectangle {
                width: view.width(),
                height: view.height(),
                ..Default::default()
            };
            let mut config = corpus::options(&case).codec;
            config.codec.method = method;
            let cleaned = still(
                encode_mixed_candidate(view, rect, &config, AnimationCandidateCodec::Vp8, false)
                    .unwrap()
                    .frame,
            );
            config.exact = true;
            let exact = still(
                encode_mixed_candidate(view, rect, &config, AnimationCandidateCodec::Vp8, false)
                    .unwrap()
                    .frame,
            );
            assert_ne!(cleaned, exact, "method={method}, sharp={sharp}");
            let rgba: Vec<_> = (0..view.height() as usize)
                .flat_map(|y| {
                    view.row(y)
                        .unwrap()
                        .iter()
                        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8])
                })
                .collect();
            let mut default = Vec::new();
            encode_lossy_rgba(
                &rgba,
                a[1],
                a[2],
                a[1] as usize * 4,
                &config.codec,
                &mut default,
            )
            .unwrap();
            assert_eq!(default, cleaned);
            let decode = |bytes: &[u8]| {
                image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
                    .unwrap()
                    .into_rgba8()
            };
            for (cleaned, exact) in decode(&cleaned).pixels().zip(decode(&exact).pixels()) {
                assert_eq!(cleaned[3], exact[3]);
            }
            assert_eq!(pixels, before);
        }
    }
}

#[test]
fn animation_mixed_equal_size_codecs_keep_vp8l_before_vp8_primary() {
    let case = corpus::tie_case();
    let words = case.rust();
    let observed = observation(&words, 0, corpus::options(&case).codec);
    assert_eq!(observed.trials.len(), 2);
    assert_eq!((observed.trials[0][0], observed.trials[1][0]), (1, 0));
    assert_eq!((observed.trials[0][11], observed.trials[1][11]), (118, 118));
    assert_ne!(&observed.trials[0][12..], &observed.trials[1][12..]);
    let winner = corpus::candidate(&case).unwrap();
    assert_eq!(winner.codec, AnimationCandidateCodec::Vp8l);
    assert_eq!(
        corpus::options(&case).codec.primary,
        AnimationCandidateCodec::Vp8
    );
    assert_eq!(
        corpus::file_words(observed.candidate),
        &observed.trials[0][12..]
    );
}

#[test]
fn animation_mixed_committed_trials_winners_and_carryover() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_mixed.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        check(&case, expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_mixed_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-mixed", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
        check(case, expected)
    });
    let mut totals = [0; 5];
    for stats in stats {
        for (sum, value) in totals.iter_mut().zip(stats) {
            *sum += value;
        }
    }
    assert!(totals.iter().all(|&n| n > 0), "{totals:?}");
    eprintln!(
        "{} mixed candidate/sequence cases match original C; VP8L/VP8 trials, distinct ties, VP8L/VP8 winners={totals:?}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_mixed_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-mixed-fixtures", &cases);
    let mut requests = Vec::new();
    let mut renders = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        check(case, expected);
        if case.function == "AnimationMixedCarryoverSequence" {
            let (_, frames, bytes) = check_sequence(case, expected);
            if frames.len() < 2 {
                continue;
            }
            renders.push(support::animation_lossy_playback::render(
                case.input[0],
                case.input[1],
                &frames,
            ));
            let mut input = vec![bytes.len() as u32];
            input.extend(bytes.into_iter().map(u32::from));
            requests.push(support::Case::new("AnimationDecode", input));
        }
    }
    let actual = native::run("animation-mixed-decodes", &requests);
    for ((case, expected), actual) in requests.iter().zip(&renders).zip(&actual) {
        native::compare(case, expected, actual);
    }
    eprintln!(
        "{} mixed all-frame composites match pinned WebPAnimDecoder",
        actual.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C mixed animation trials/config/carryover fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for v in &case.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in output {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/animation_mixed.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_mixed.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
