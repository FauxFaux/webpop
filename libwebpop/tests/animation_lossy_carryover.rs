#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use libwebpop::*;
use support::{animation_lossless as lossless, animation_lossy_carryover as corpus};

fn rect(x: i32, y: i32, width: i32, height: i32) -> FrameRectangle {
    FrameRectangle {
        x_offset: x,
        y_offset: y,
        width,
        height,
    }
}

// Inspect every grid-aligned 8x8 square in the canvas independently of the
// port's traversal bounds. Only whole blocks strictly inside the left/top edge
// are eligible; a single mismatching pixel rejects all 64 pixels.
fn check_flatten(case: &support::Case, actual: &[u32]) {
    let base = corpus::base(case);
    let a = &base.input;
    let (src, dst) = lossless::canvases(&base);
    let (w, h, ss, ds) = (a[1] as usize, a[2] as usize, a[3] as usize, a[4] as usize);
    let (left, top, rw, rh) = (a[5] as usize, a[6] as usize, a[7] as usize, a[8] as usize);
    let mut expected = dst.to_vec();
    let mut mask = a[18 + src.len() + dst.len()..].to_vec();
    let threshold = QualityToMaxDiff(corpus::options(case).codec.quality) as u32;
    let mut changed = false;
    for by in (0..h).step_by(8) {
        for bx in (0..w).step_by(8) {
            if bx <= left || by <= top || bx + 8 > left + rw || by + 8 > top + rh {
                continue;
            }
            let source: Vec<_> = (by..by + 8)
                .flat_map(|y| (bx..bx + 8).map(move |x| src[y * ss + x]))
                .collect();
            let target: Vec<_> = (by..by + 8)
                .flat_map(|y| (bx..bx + 8).map(move |x| dst[y * ds + x]))
                .collect();
            if source.iter().zip(target).all(|(&s, d)| {
                s >> 24 == 255
                    && d >> 24 == 255
                    && [0, 8, 16]
                        .into_iter()
                        .all(|shift| ((s >> shift) & 255).abs_diff((d >> shift) & 255) <= threshold)
            }) {
                let color = [0, 8, 16].into_iter().fold(0, |v, shift| {
                    v | (source.iter().map(|p| (p >> shift) & 255).sum::<u32>() / 64) << shift
                });
                for y in by..by + 8 {
                    for x in bx..bx + 8 {
                        expected[y * ds + x] = color;
                        mask[(y - top) * rw + x - left] = 1;
                    }
                }
                changed = true;
            }
        }
    }
    let mut words = vec![u32::from(changed)];
    words.extend(expected);
    words.extend(mask);
    assert_eq!(actual, words);
}

fn check_selection(case: &support::Case, words: &[u32]) {
    let Some(candidate) = corpus::candidate(case) else {
        assert_eq!(words, [0]);
        return;
    };
    let base = corpus::base(case);
    let a = &base.input;
    let (src, dst) = lossless::canvases(&base);
    let mut previous = src.to_vec();
    if let Some(dispose) = candidate.previous_dispose {
        DisposeFrameRectangle(
            dispose,
            &lossless::rectangle(&a[13..]),
            &mut ArgbViewMut::new(&mut previous, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
        )
        .unwrap();
    }
    let mut current = dst.to_vec();
    let r = candidate.rect;
    let mut mask = vec![0; (r.width * r.height) as usize];
    if candidate.control.blend == FrameBlend::Blend {
        FlattenSimilarBlocks(
            ArgbView::new(&previous, a[1] as i32, a[2] as i32, a[3] as usize).unwrap(),
            &r,
            &mut ArgbViewMut::new(&mut current, a[1] as i32, a[2] as i32, a[4] as usize).unwrap(),
            corpus::options(case).codec.quality,
            &mut mask,
        )
        .unwrap();
    }
    if mask.contains(&1) {
        assert_eq!(candidate.carryover_mask, mask);
    } else {
        assert!(!candidate.carries_over());
    }
    // Re-encode the selected, globally flattened rectangle through the isolated
    // VP8 route. This guards crop alignment and winning-mask/payload pairing.
    let coded = encode_lossy_candidate(
        ArgbView::new(&current, a[1] as i32, a[2] as i32, a[4] as usize)
            .unwrap()
            .sub_view(r)
            .unwrap(),
        r,
        &corpus::options(case).codec,
        candidate.control.blend == FrameBlend::Blend,
    )
    .unwrap();
    let expected = corpus::candidate_words(coded);
    let at = 9 + words[8] as usize;
    let coded_at = 9 + expected[8] as usize;
    assert_eq!(&words[at..], &expected[coded_at..]);
    let bytes: Vec<_> = words[at + 1..].iter().map(|&v| v as u8).collect();
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (r.width as u32, r.height as u32)
    );
}

fn check(case: &support::Case, expected: &[u32]) {
    if case.function == "AnimationLossy" {
        if case.input[2] == 0 {
            check_flatten(case, expected);
        } else {
            check_selection(case, expected);
        }
    }
}

// Independently apply the original-word state transitions, including exact
// restoration before selection and unchanged history after a similar skip.
// Return codec rectangles separately for playback, never the carryover canvas.
fn check_sequence(
    case: &support::Case,
    words: &[u32],
) -> (
    [usize; 7],
    Vec<support::animation_lossy_playback::Frame>,
    Vec<u8>,
) {
    use support::animation_lossy_playback::Frame;
    let (w, h) = (case.input[0], case.input[1]);
    let area = (w * h) as usize;
    let mut original = vec![0; area];
    let mut carry = vec![0; area];
    let mut previous_rect = FrameRectangle::default();
    let mut stats = [0; 7];
    let mut frames: Vec<Frame> = Vec::new();
    let mut encoded: Vec<(EncodedFrame, ContainerFrameOptions)> = Vec::new();
    let mut at = 0;
    for step in corpus::sequence_steps(case) {
        if !frames.is_empty() {
            for i in 0..area {
                if original[i] == step.current[i] {
                    stats[6] += usize::from(carry[i] != original[i]);
                    carry[i] = original[i];
                }
            }
        }
        if let Some(candidate) = step.candidate {
            assert_eq!(words[at], 1);
            let r = candidate.rect;
            if let Some(dispose) = candidate.previous_dispose {
                frames.last_mut().unwrap().control.dispose = dispose;
                if let Some((_, control)) = encoded.last_mut() {
                    control.dispose = dispose;
                }
                stats[2] += usize::from(dispose == FrameDispose::Background);
                stats[3] += usize::from(candidate.carries_over());
                stats[4] +=
                    usize::from(dispose == FrameDispose::Background && candidate.carries_over());
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
                        let pixel =
                            ((r.y_offset + y) as u32 * w + (r.x_offset + x) as u32) as usize;
                        let masked = candidate.carries_over()
                            && candidate.carryover_mask[(y * r.width + x) as usize] != 0;
                        if masked {
                            stats[5] += usize::from(carry[pixel] != step.current[pixel]);
                            assert_eq!(carry[pixel] >> 24, 255);
                        } else {
                            carry[pixel] = step.current[pixel];
                        }
                    }
                }
            } else {
                assert!(frames.is_empty());
                carry.clone_from(&step.current);
            }
            original.clone_from(&step.current);
            previous_rect = r;
            let size_at = at + 9 + words[at + 8] as usize;
            let size = words[size_at] as usize;
            let still = words[size_at + 1..size_at + 1 + size]
                .iter()
                .map(|&v| v as u8)
                .collect();
            let control = ContainerFrameOptions {
                duration_ms: 1,
                ..candidate.control
            };
            frames.push(Frame {
                width: r.width as u32,
                height: r.height as u32,
                has_alpha: candidate.frame.has_alpha(),
                control,
                still,
            });
            encoded.push((candidate.frame, control));
            at = size_at + 1 + size;
        } else {
            assert_eq!(words[at], 0);
            at += 1;
            stats[0] += 1;
            stats[1] += usize::from(original != step.current);
        }
        assert_eq!(step.previous, original);
        assert_eq!(step.carryover, carry);
        assert_eq!(step.previous_rect, previous_rect);
        at += 4 + 2 * area;
    }
    assert_eq!(at, words.len());
    let mut mux = AnimationContainer::new(w, h, Default::default()).unwrap();
    for (frame, control) in encoded {
        mux.push_frame(frame, control).unwrap();
    }
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    (stats, frames, bytes)
}

#[test]
fn animation_lossy_carryover_committed_fixtures() {
    let fixtures =
        fixture_support::fixtures(include_str!("fixtures/animation_lossy_carryover.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
        check(&case, expected);
        if case.function == "AnimationLossyCarryoverSequence" {
            let (_, frames, _) = check_sequence(&case, expected);
            let rendered =
                support::animation_lossy_playback::render(case.input[0], case.input[1], &frames);
            assert_eq!(rendered[2] as usize, frames.len());
        }
    }
}

#[test]
fn animation_flatten_checks_all_pixels_source_average_and_canvas_alignment() {
    // A whole 8x8 canvas is deliberately untouched; even aligned rectangle
    // origins skip the first block. Every pixel is ineligible when alpha !=255.
    for origin in 0..16 {
        for bad in 0..65 {
            let mut source = vec![0xff102030; 32 * 32];
            let mut target = source.clone();
            let start = ((origin + 8) & !7) as usize;
            if bad < 64 {
                target[(start + bad / 8) * 32 + start + bad % 8] ^= 2;
            } else {
                // Truncated source average differs from target's average.
                for y in start..start + 8 {
                    for x in start..start + 8 {
                        source[y * 32 + x] += ((x + y) % 3) as u32;
                        target[y * 32 + x] = source[y * 32 + x] + 1;
                    }
                }
            }
            let case_rect = rect(origin, origin, 32 - origin, 32 - origin);
            let mut mask = vec![0; (case_rect.width * case_rect.height) as usize + 3];
            FlattenSimilarBlocks(
                ArgbView::new(&source, 32, 32, 32).unwrap(),
                &case_rect,
                &mut ArgbViewMut::new(&mut target, 32, 32, 32).unwrap(),
                100.,
                &mut mask,
            )
            .unwrap();
            for y in start..start + 8 {
                for x in start..start + 8 {
                    assert_eq!(
                        mask[(y - origin as usize) * case_rect.width as usize + x
                            - origin as usize],
                        u8::from(bad == 64)
                    );
                }
            }
            if bad == 64 {
                let sum: u32 = (start..start + 8)
                    .flat_map(|y| source[y * 32 + start..][..8].iter())
                    .map(|p| p & 255)
                    .sum();
                assert_eq!(target[start * 32 + start], 0x00102000 | (sum / 64));
            }
            assert_eq!(&mask[mask.len() - 3..], &[0; 3]);
        }
    }
    let mut target = [0xff123456; 64];
    let before = target;
    let mut mask = [42; 64];
    assert!(
        !FlattenSimilarBlocks(
            ArgbView::new(&before, 8, 8, 8).unwrap(),
            &rect(0, 0, 8, 8),
            &mut ArgbViewMut::new(&mut target, 8, 8, 8).unwrap(),
            0.,
            &mut mask
        )
        .unwrap()
    );
    assert_eq!(target, before);
    assert_eq!(mask, [42; 64]);
}

#[test]
fn animation_lossy_mutation_validation_and_skips_preserve_state() {
    let input = [0xff102030; 256];
    let src = ArgbView::new(&input, 16, 16, 16).unwrap();
    let mut dest = input;
    let mut mask = [42; 259];
    for quality in [f32::NAN, f32::INFINITY, -1., 101.] {
        assert_eq!(
            FlattenSimilarBlocks(
                src,
                &rect(0, 0, 16, 16),
                &mut ArgbViewMut::new(&mut dest, 16, 16, 16).unwrap(),
                quality,
                &mut mask
            ),
            Err(AnimationLeafError::InvalidQuality)
        );
    }
    for r in [rect(-1, 0, 16, 16), rect(0, 0, 17, 16)] {
        assert_eq!(
            FlattenSimilarBlocks(
                src,
                &r,
                &mut ArgbViewMut::new(&mut dest, 16, 16, 16).unwrap(),
                75.,
                &mut mask
            ),
            Err(AnimationLeafError::InvalidRectangle)
        );
    }
    assert_eq!(
        FlattenSimilarBlocks(
            src,
            &rect(0, 0, 16, 16),
            &mut ArgbViewMut::new(&mut dest, 16, 16, 16).unwrap(),
            75.,
            &mut mask[..255]
        ),
        Err(AnimationLeafError::MaskTooShort)
    );
    assert_eq!(dest, input);
    assert_eq!(mask, [42; 259]);
    let options = LossyCandidateOptions {
        codec: LossyOptions {
            quality: 100.,
            ..Default::default()
        },
        ..Default::default()
    };
    let changed = [0xff102031; 256];
    assert!(
        SetFrameLossy(
            src,
            ArgbView::new(&changed, 16, 16, 16).unwrap(),
            rect(0, 0, 16, 16),
            &options
        )
        .unwrap()
        .is_none()
    );
    assert_ne!(input, changed); // q100 still has similarity tolerance 1.
    assert!(matches!(
        SetFrameLossy(
            src,
            src,
            FrameRectangle::default(),
            &LossyCandidateOptions {
                is_first_frame: true,
                ..options
            }
        ),
        Err(AnimationCandidateError::InvalidFirstFrame)
    ));
    let mut candidate =
        encode_lossy_candidate(src, rect(0, 0, 16, 16), &options.codec, true).unwrap();
    candidate.previous_dispose = Some(FrameDispose::Background);
    candidate.carryover_mask = vec![1; 255];
    let mut previous = [99; 256];
    let mut carryover = [77; 256];
    assert_eq!(
        candidate.update_canvases(
            src,
            rect(0, 0, 16, 16),
            &mut ArgbViewMut::new(&mut previous, 16, 16, 16).unwrap(),
            &mut ArgbViewMut::new(&mut carryover, 16, 16, 16).unwrap()
        ),
        Err(AnimationLeafError::MaskTooShort)
    );
    assert_eq!(previous, [99; 256]);
    assert_eq!(carryover, [77; 256]);
}

#[test]
fn animation_lossy_equal_size_trials_keep_first_disposal_and_global_mask() {
    for method in 0..7 {
        let mut case = corpus::selection_case(25, 23, 10, 8, method);
        case.input[15..19].fill(0);
        let candidate = corpus::candidate(&case).unwrap();
        assert_eq!(candidate.previous_dispose, Some(FrameDispose::None));
        assert_eq!((candidate.rect.x_offset, candidate.rect.y_offset), (2, 2));
        assert!(candidate.carries_over());
        let r = candidate.rect;
        // The first held block is at (8,8), not crop-relative (10,10).
        for y in 0..r.height {
            for x in 0..r.width {
                let (cx, cy) = (x + r.x_offset, y + r.y_offset);
                let expected = (8..24).contains(&cx) && (8..16).contains(&cy);
                assert_eq!(
                    candidate.carryover_mask[(y * r.width + x) as usize],
                    u8::from(expected)
                );
            }
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_lossy_carryover_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-lossy-carryover", &cases, |case, expected| {
        native::compare(case, expected, &case.rust());
        check(case, expected);
        if case.function == "AnimationLossyCarryoverSequence" {
            check_sequence(case, expected).0
        } else {
            [0; 7]
        }
    });
    let mut totals = [0; 7];
    for stats in stats {
        for (sum, value) in totals.iter_mut().zip(stats) {
            *sum += value;
        }
    }
    assert!(totals.iter().all(|&n| n > 0), "{totals:?}");
    eprintln!(
        "{} flatten/candidate/carryover cases match original C; skip/similar-skip/background/masked/background-masked/drift/restored={totals:?}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_lossy_carryover_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-lossy-carryover-fixtures", &cases);
    let mut playback_requests = Vec::new();
    let mut playback_expected = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        check(case, expected);
        if case.function == "AnimationLossyCarryoverSequence" {
            let (_, frames, bytes) = check_sequence(case, expected);
            if frames.len() < 2 {
                continue;
            }
            playback_expected.push(support::animation_lossy_playback::render(
                case.input[0],
                case.input[1],
                &frames,
            ));
            let mut input = vec![bytes.len() as u32];
            input.extend(bytes.into_iter().map(u32::from));
            playback_requests.push(support::Case::new("AnimationDecode", input));
        }
    }
    let actual = native::run("animation-lossy-carryover-decodes", &playback_requests);
    for ((case, expected), actual) in playback_requests
        .iter()
        .zip(&playback_expected)
        .zip(&actual)
    {
        native::compare(case, expected, actual);
    }
    eprintln!(
        "{} lossy all-frame composites match pinned WebPAnimDecoder",
        actual.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C lossy animation flatten/carryover fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
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
                "/tests/fixtures/animation_lossy_carryover.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures =
            fixture_support::fixtures(include_str!("fixtures/animation_lossy_carryover.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
