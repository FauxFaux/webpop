#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use libwebpop::*;
use support::animation_carryover as corpus;
use support::animation_lossless::composite_lossless;

fn rect(x: i32, y: i32, width: i32, height: i32) -> FrameRectangle {
    FrameRectangle {
        x_offset: x,
        y_offset: y,
        width,
        height,
    }
}

#[test]
fn animation_carryover_copies_exact_words_and_zero_mask_only() {
    let words = [
        0, 0x00123456, 0x01010203, 0x7f010203, 0xfeabcdef, 0xffabcdef,
    ];
    for &a in &words {
        for &b in &words {
            for mask in [0, 1, 7, 255] {
                let source = [a, 99, a];
                let other = [b, 88, 77, b];
                let original = [0x55667788, 66, 55, 44, 0x11223344, 33];
                let mut target = original;
                let src = ArgbView::new(&source, 1, 2, 2).unwrap();
                let cmp = ArgbView::new(&other, 1, 2, 3).unwrap();
                {
                    let mut dst = ArgbViewMut::new(&mut target, 1, 2, 4).unwrap();
                    CopyIdenticalPixels(src, cmp, &mut dst).unwrap();
                }
                for i in 0..target.len() {
                    assert_eq!(
                        target[i],
                        if [0, 4].contains(&i) && a == b {
                            a
                        } else {
                            original[i]
                        }
                    );
                }
                target = original;
                {
                    let mut dst = ArgbViewMut::new(&mut target, 1, 2, 4).unwrap();
                    CopyMaskedPixels(src, &[mask, mask, 42], &mut dst).unwrap();
                }
                for i in 0..target.len() {
                    assert_eq!(
                        target[i],
                        if [0, 4].contains(&i) && mask == 0 {
                            a
                        } else {
                            original[i]
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn animation_carryover_candidate_updates_dispose_then_mask_and_save_original() {
    // Deliberately different original/carryover canvases make masked holds and
    // the rectangle exterior observable, unlike adjacent-original comparison.
    let current: Vec<_> = (0..42).map(|i| 0x7f123400 + i).collect();
    let original_previous = vec![0x01020304; 42];
    let original_carry = vec![0xfeabcdef; 42];
    let current_view = ArgbView::new(&current, 5, 4, 11).unwrap();
    let region = rect(2, 2, 3, 2);
    let previous_rect = rect(0, 0, 4, 3);
    let mut candidate = EncodeCandidate(
        current_view.sub_view(region).unwrap(),
        region,
        &Default::default(),
        true,
    )
    .unwrap();
    for key in [false, true] {
        for dispose in [FrameDispose::None, FrameDispose::Background] {
            for masked in [false, true] {
                candidate.previous_dispose = if key { None } else { Some(dispose) };
                candidate.carryover_mask = if masked {
                    vec![0, 1, 7, 255, 0, 1]
                } else {
                    vec![]
                };
                let mut previous = original_previous.clone();
                let mut carryover = original_carry.clone();
                candidate
                    .update_canvases(
                        current_view,
                        previous_rect,
                        &mut ArgbViewMut::new(&mut previous, 5, 4, 10).unwrap(),
                        &mut ArgbViewMut::new(&mut carryover, 5, 4, 9).unwrap(),
                    )
                    .unwrap();
                for i in 0..previous.len() {
                    let (x, y) = (i % 10, i / 10);
                    assert_eq!(
                        previous[i],
                        if x < 5 && y < 4 {
                            current[y * 11 + x]
                        } else {
                            original_previous[i]
                        }
                    );
                }
                for i in 0..carryover.len() {
                    let (x, y) = (i % 9, i / 9);
                    let mut expected = original_carry[i];
                    if x < 5 && y < 4 {
                        if key {
                            expected = current[y * 11 + x];
                        } else {
                            if dispose == FrameDispose::Background && x < 4 && y < 3 {
                                expected = 0;
                            }
                            if (2..5).contains(&x)
                                && (2..4).contains(&y)
                                && (!masked || candidate.carryover_mask[(y - 2) * 3 + x - 2] == 0)
                            {
                                expected = current[y * 11 + x];
                            }
                        }
                    }
                    assert_eq!(
                        carryover[i], expected,
                        "key={key}, dispose={dispose:?}, masked={masked}, x={x}, y={y}"
                    );
                }
            }
        }
    }
}

#[test]
fn animation_carryover_validation_preserves_both_destinations() {
    let source = [0xff123456; 12];
    let src = ArgbView::new(&source, 3, 3, 3).unwrap();
    let mismatch = ArgbView::new(&source, 1, 1, 1).unwrap();
    let mut previous = [42; 12];
    let mut carryover = [77; 12];
    let mut prev = ArgbViewMut::new(&mut previous, 3, 3, 4).unwrap();
    let mut carry = ArgbViewMut::new(&mut carryover, 3, 3, 4).unwrap();
    assert_eq!(
        CopyIdenticalPixels(src, mismatch, &mut carry),
        Err(AnimationLeafError::InvalidDimensions)
    );
    assert_eq!(
        CopyMaskedPixels(mismatch, &[0; 9], &mut carry),
        Err(AnimationLeafError::InvalidDimensions)
    );
    assert_eq!(
        CopyMaskedPixels(src, &[0; 8], &mut carry),
        Err(AnimationLeafError::MaskTooShort)
    );
    let mut candidate = EncodeCandidate(src, rect(0, 0, 3, 3), &Default::default(), true).unwrap();
    candidate.previous_dispose = Some(FrameDispose::Background);
    candidate.carryover_mask = vec![1; 8];
    assert_eq!(
        candidate.update_canvases(src, rect(0, 0, 3, 3), &mut prev, &mut carry),
        Err(AnimationLeafError::MaskTooShort)
    );
    candidate.carryover_mask.clear();
    assert_eq!(
        candidate.update_canvases(src, rect(1, 1, 3, 3), &mut prev, &mut carry),
        Err(AnimationLeafError::InvalidRectangle)
    );
    assert_eq!(
        candidate.update_canvases(mismatch, rect(0, 0, 1, 1), &mut prev, &mut carry),
        Err(AnimationLeafError::InvalidDimensions)
    );
    for r in [rect(1, 0, 3, 3), rect(0, 0, 0, 0), rect(-1, 0, 3, 3)] {
        candidate.rect = r;
        assert_eq!(
            candidate.update_canvases(src, rect(0, 0, 3, 3), &mut prev, &mut carry),
            Err(AnimationLeafError::InvalidRectangle)
        );
    }
    assert_eq!(previous, [42; 12]);
    assert_eq!(carryover, [77; 12]);
}

// Prepare an actual animation from selected candidates, assigning unit delays
// to accepted frames. This validates canvas transitions; timestamp merging and
// single-frame optimization remain separate milestones.
fn check_sequence(case: &support::Case) -> Option<(support::Case, Vec<u32>)> {
    if case.function != "AnimationCarryoverSequence" {
        return None;
    }
    let (w, h) = (case.input[0], case.input[1]);
    let steps = corpus::sequence_steps(case);
    let mut frames: Vec<(EncodedFrame, ContainerFrameOptions)> = Vec::new();
    let mut expected = Vec::new();
    let mut previous_original = vec![0; (w * h) as usize];
    let mut previous_rect = FrameRectangle::default();
    for step in steps {
        if let Some(candidate) = step.candidate {
            assert_eq!(step.previous, step.current);
            assert_eq!(step.previous_rect, candidate.rect);
            previous_original = step.current.clone();
            previous_rect = candidate.rect;
            if let Some(dispose) = candidate.previous_dispose {
                frames.last_mut().unwrap().1.dispose = dispose;
            } else {
                assert!(frames.is_empty());
            }
            frames.push((
                candidate.frame,
                ContainerFrameOptions {
                    duration_ms: 1,
                    ..candidate.control
                },
            ));
            expected.push(step.current);
        } else {
            assert_eq!(step.previous, previous_original);
            assert_eq!(step.previous_rect, previous_rect);
            assert_eq!(step.current, previous_original);
        }
    }
    // A sole full-canvas frame becomes a still and has no animation timing.
    if frames.len() < 2 {
        return None;
    }
    let mut mux = AnimationContainer::new(w, h, Default::default()).unwrap();
    for (frame, control) in frames {
        mux.push_frame(frame, control).unwrap();
    }
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    let actual = composite_lossless(&bytes);
    let mut wanted = vec![w, h, expected.len() as u32, 0];
    for (index, canvas) in expected.iter().enumerate() {
        wanted.push(index as u32 + 1);
        for &p in canvas {
            wanted.extend(if p >> 24 == 0 {
                [0; 4]
            } else {
                [(p >> 16) & 255, (p >> 8) & 255, p & 255, p >> 24]
            });
        }
    }
    assert_eq!(actual, wanted);
    let mut request = vec![bytes.len() as u32];
    request.extend(bytes.into_iter().map(u32::from));
    Some((support::Case::new("AnimationDecode", request), actual))
}

#[test]
fn animation_carryover_committed_fixtures_and_sequence_composites() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_carryover.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        let _ = check_sequence(&case);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_carryover_match_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-carryover", &cases, |case, expected| {
        let (mut skipped, mut background, mut carries, mut background_carry) = (0, 0, 0, 0);
        native::compare(case, expected, &case.rust());
        if case.function == "AnimationCarryoverSequence" {
            let mut at = 0;
            for _ in 0..case.input[3] {
                if expected[at] == 0 {
                    skipped += 1;
                    at += 1;
                } else {
                    background += usize::from(expected[at + 6] == 2);
                    carries += usize::from(expected[at + 7] != 0);
                    background_carry += usize::from(expected[at + 6] == 2 && expected[at + 7] != 0);
                    let mask_len = expected[at + 8] as usize;
                    at += 10 + mask_len + expected[at + 9 + mask_len] as usize;
                }
                at += 4 + 2 * (case.input[0] * case.input[1]) as usize;
            }
            assert_eq!(at, expected.len());
        }
        [skipped, background, carries, background_carry]
    });
    let skipped: usize = stats.iter().map(|counts| counts[0]).sum();
    let background: usize = stats.iter().map(|counts| counts[1]).sum();
    let carries: usize = stats.iter().map(|counts| counts[2]).sum();
    let background_carry: usize = stats.iter().map(|counts| counts[3]).sum();

    assert!(skipped > 0 && background > 0 && carries > 0 && background_carry > 0);
    eprintln!(
        "{} carryover helper/sequence cases match scalar C; skip={skipped}, background={background}, carries={carries}, background_carry={background_carry}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_carryover_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-carryover-fixtures", &cases);
    let mut requests = Vec::new();
    let mut observations = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        if let Some((request, observation)) = check_sequence(case) {
            requests.push(request);
            observations.push(observation);
        }
    }
    let decoded = native::run("animation-carryover-decodes", &requests);
    for ((case, expected), actual) in requests.iter().zip(&observations).zip(&decoded) {
        native::compare(case, expected, actual);
    }
    eprintln!(
        "{} multi-frame carryover composites match pinned WebPAnimDecoder",
        requests.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C animation carryover fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
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
                "/tests/fixtures/animation_carryover.txt"
            ),
            text,
        )
        .unwrap();
    }
}
