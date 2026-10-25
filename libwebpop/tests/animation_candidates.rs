#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use libwebpop::*;
use support::animation_lossless::{self as corpus, composite_lossless};

fn rect(x: i32, y: i32, w: i32, h: i32) -> FrameRectangle {
    FrameRectangle {
        x_offset: x,
        y_offset: y,
        width: w,
        height: h,
    }
}

fn rgba(view: ArgbView<'_>) -> Vec<u8> {
    let mut result = Vec::new();
    for y in 0..view.height() as usize {
        for &p in view.row(y).unwrap() {
            result.extend([(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
    }
    result
}

#[test]
fn animation_transparency_and_disposal_match_independent_pixel_rules() {
    let words = [
        0, 0x00123456, 0x01010203, 0x7f010203, 0xfe010203, 0xff010203,
    ];
    for &a in &words {
        for &b in &words {
            let source = [a, 99, 88, a];
            let mut target = [b, 77, 66, b, 55];
            let original = target;
            let src = ArgbView::new(&source, 1, 2, 3).unwrap();
            let mut mask = [7, 255, 42];
            let mut dst = ArgbViewMut::new(&mut target, 1, 2, 3).unwrap();
            let changed =
                IncreaseTransparency(src, &rect(0, 0, 1, 2), &mut dst, &mut mask).unwrap();
            assert_eq!(changed, a == b && b != 0);
            for i in [0, 3] {
                assert_eq!(
                    dst.as_view().row(i / 3).unwrap()[0],
                    if changed { 0 } else { b }
                );
            }
            assert_eq!(mask, if changed { [1, 1, 42] } else { [7, 255, 42] });
            assert!(!IncreaseTransparency(src, &rect(0, 0, 1, 2), &mut dst, &mut mask).unwrap());
            assert_eq!(
                [target[1], target[2], target[4]],
                [original[1], original[2], original[4]]
            );
        }
    }
    let mut pixels: Vec<u32> = (1..30).collect();
    let original = pixels.clone();
    {
        let mut canvas = ArgbViewMut::new(&mut pixels, 5, 4, 8).unwrap();
        DisposeFrameRectangle(FrameDispose::None, &rect(1, 1, 3, 2), &mut canvas).unwrap();
        let mut view = canvas.sub_view(rect(1, 1, 4, 3)).unwrap();
        DisposeFrameRectangle(FrameDispose::Background, &rect(1, 1, 3, 2), &mut view).unwrap();
    }
    for i in 0..pixels.len() {
        let (x, y) = (i % 8, i / 8);
        assert_eq!(
            pixels[i],
            if (2..5).contains(&x) && (2..4).contains(&y) {
                0
            } else {
                original[i]
            }
        );
    }
}

#[test]
fn animation_mutation_validation_preserves_pixels_and_masks() {
    let src_pixels = [1; 9];
    let src = ArgbView::new(&src_pixels, 3, 3, 3).unwrap();
    let mismatch = ArgbView::new(&src_pixels, 1, 1, 1).unwrap();
    let mut pixels = [1; 9];
    let mut mask = [9; 9];
    let mut dst = ArgbViewMut::new(&mut pixels, 3, 3, 3).unwrap();
    for guess in [
        rect(-1, 0, 1, 1),
        rect(2, 2, 2, 2),
        rect(0, 0, -1, 1),
        rect(i32::MAX, 0, i32::MAX, 1),
    ] {
        assert_eq!(
            IncreaseTransparency(src, &guess, &mut dst, &mut mask),
            Err(AnimationLeafError::InvalidRectangle)
        );
        assert_eq!(
            DisposeFrameRectangle(FrameDispose::Background, &guess, &mut dst),
            Err(AnimationLeafError::InvalidRectangle)
        );
        assert_eq!(dst.as_view().row(0).unwrap(), &[1; 3]);
        assert_eq!(mask, [9; 9]);
    }
    assert_eq!(
        IncreaseTransparency(mismatch, &rect(0, 0, 1, 1), &mut dst, &mut mask),
        Err(AnimationLeafError::InvalidDimensions)
    );
    assert_eq!(
        IncreaseTransparency(src, &rect(0, 0, 3, 3), &mut dst, &mut mask[..8]),
        Err(AnimationLeafError::MaskTooShort)
    );
    assert!(dst.row_mut(usize::MAX).is_none());
    assert_eq!(pixels, [1; 9]);
    assert_eq!(mask, [9; 9]);
    assert!(ArgbViewMut::new(&mut pixels, 3, 3, 4).is_err());
}

fn check_selection(case: &support::Case, words: &[u32]) -> Option<(support::Case, Vec<u32>)> {
    let a = &case.input;
    if a[0] != 3 {
        return None;
    }
    let (source, target) = corpus::canvases(case);
    let src = ArgbView::new(source, a[1] as i32, a[2] as i32, a[3] as usize).unwrap();
    let dst = ArgbView::new(target, a[1] as i32, a[2] as i32, a[4] as usize).unwrap();
    if words[0] == 0 {
        assert!(!corpus::options(case).is_key_frame);
        assert_eq!(rgba(src), rgba(dst));
        return None;
    }
    let rect = corpus::rectangle(&words[1..5]);
    let blend = words[5] == 0;
    let bg = words[6] == 2;
    let carries = words[7] != 0;
    let mask_len = words[8] as usize;
    let mask = &words[9..9 + mask_len];
    let bytes: Vec<_> = words[10 + mask_len..].iter().map(|&v| v as u8).collect();
    assert_eq!(bytes.len(), words[9 + mask_len] as usize);
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(
        decoded.dimensions(),
        (rect.width as u32, rect.height as u32)
    );
    let prev_rect = corpus::rectangle(&a[13..17]);
    let mut expected_mask = Vec::new();
    for y in 0..rect.height as usize {
        for x in 0..rect.width as usize {
            let (cx, cy) = (rect.x_offset as usize + x, rect.y_offset as usize + y);
            let mut old = src.row(cy).unwrap()[cx];
            if bg
                && (prev_rect.x_offset..prev_rect.x_offset + prev_rect.width).contains(&(cx as i32))
                && (prev_rect.y_offset..prev_rect.y_offset + prev_rect.height)
                    .contains(&(cy as i32))
            {
                old = 0;
            }
            let current = dst.row(cy).unwrap()[cx];
            let transparent = blend && current == old && current != 0;
            expected_mask.push(u32::from(transparent));
            let actual = decoded.get_pixel(x as u32, y as u32).0;
            assert_eq!(
                actual[3],
                if transparent {
                    0
                } else {
                    (current >> 24) as u8
                }
            );
            if actual[3] != 0 {
                assert_eq!(
                    actual,
                    [
                        (current >> 16) as u8,
                        (current >> 8) as u8,
                        current as u8,
                        (current >> 24) as u8
                    ]
                );
            }
        }
    }
    assert_eq!(carries, expected_mask.contains(&1));
    if carries {
        assert_eq!(mask, expected_mask);
    } else {
        assert!(mask.is_empty());
    }

    // Assemble an independently decoded/composited sequence, then also check
    // it with pinned WebPAnimDecoder in native runs. image-webp 0.2.4's animated
    // compositor rounds some opaque pixels down by one, so use its still
    // decoder with exact endpoint-alpha compositing for these candidates.
    let candidate = SetFrameLossless(src, dst, prev_rect, &corpus::options(case))
        .unwrap()
        .unwrap();
    let source_rgba = rgba(src);
    let mut mux = AnimationContainer::new(
        a[1],
        a[2],
        AnimationContainerOptions {
            background: BackgroundColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            ..Default::default()
        },
    )
    .unwrap();
    let frame = EncodedFrame::encode_lossless_rgba(
        &source_rgba,
        a[1],
        a[2],
        a[1] as usize * 4,
        &LosslessOptions {
            exact: true,
            ..Default::default()
        },
    )
    .unwrap();
    // Dispose only the actual preceding rectangle. A full-canvas source frame
    // would clear too much, so use a separately prepared prefix then a cropped
    // previous frame, representing the same canvas before applying disposal.
    mux.push_frame(
        frame,
        ContainerFrameOptions {
            duration_ms: 1,
            ..Default::default()
        },
    )
    .unwrap();
    if bg
        && prev_rect.width > 0
        && prev_rect.height > 0
        && prev_rect.x_offset % 2 == 0
        && prev_rect.y_offset % 2 == 0
    {
        let view = src.sub_view(prev_rect).unwrap();
        let pixels = rgba(view);
        let f = EncodedFrame::encode_lossless_rgba(
            &pixels,
            view.width() as u32,
            view.height() as u32,
            view.width() as usize * 4,
            &LosslessOptions {
                exact: true,
                ..Default::default()
            },
        )
        .unwrap();
        mux.push_frame(
            f,
            ContainerFrameOptions {
                x_offset: prev_rect.x_offset as u32,
                y_offset: prev_rect.y_offset as u32,
                duration_ms: 1,
                dispose: FrameDispose::Background,
                ..Default::default()
            },
        )
        .unwrap();
    } else {
        assert!(!bg);
    }
    mux.push_frame(
        candidate.frame,
        ContainerFrameOptions {
            duration_ms: 1,
            ..candidate.control
        },
    )
    .unwrap();
    let mut animated = Vec::new();
    mux.assemble(&mut animated).unwrap();
    let observation = composite_lossless(&animated);
    let frame_bytes = a[1] as usize * a[2] as usize * 4;
    let final_canvas = &observation[observation.len() - frame_bytes..];
    for y in 0..a[2] {
        for x in 0..a[1] {
            let expected = dst.row(y as usize).unwrap()[x as usize];
            let offset = (y * a[1] + x) as usize * 4;
            let actual = <[u32; 4]>::try_from(&final_canvas[offset..offset + 4])
                .unwrap()
                .map(|v| v as u8);
            assert_eq!(actual[3], (expected >> 24) as u8);
            if actual[3] != 0 {
                assert_eq!(
                    actual,
                    [
                        (expected >> 16) as u8,
                        (expected >> 8) as u8,
                        expected as u8,
                        (expected >> 24) as u8
                    ]
                );
            }
        }
    }
    let mut request = vec![animated.len() as u32];
    request.extend(animated.into_iter().map(u32::from));
    Some((support::Case::new("AnimationDecode", request), observation))
}

#[test]
fn animation_candidate_ties_keep_the_first_payload_and_controls() {
    let pixels = [0xff123456];
    let view = ArgbView::new(&pixels, 1, 1, 1).unwrap();
    let mut first = EncodeCandidate(view, rect(0, 0, 1, 1), &Default::default(), false).unwrap();
    first.previous_dispose = Some(FrameDispose::None);
    let mut second = EncodeCandidate(view, rect(0, 0, 1, 1), &Default::default(), true).unwrap();
    second.previous_dispose = Some(FrameDispose::Background);
    assert_eq!(first.encoded_size(), second.encoded_size());
    let mut best = None;
    PickBestCandidate(&mut best, first);
    PickBestCandidate(&mut best, second);
    let best = best.unwrap();
    assert_eq!(best.control.blend, FrameBlend::Replace);
    assert_eq!(best.previous_dispose, Some(FrameDispose::None));
    assert!(!best.carries_over());
    let mut tie = corpus::base(5, 7, 5, 8);
    tie.input[13..17].copy_from_slice(&[0; 4]);
    assert_eq!(tie.rust()[6], 1);
}

#[test]
fn animation_candidate_validation_and_sources_are_unchanged() {
    let source = [0; 9];
    let target = [0xff123456; 9];
    let src = ArgbView::new(&source, 3, 3, 3).unwrap();
    let dst = ArgbView::new(&target, 3, 3, 3).unwrap();
    let mismatch = ArgbView::new(&source, 1, 1, 1).unwrap();
    let options = LosslessCandidateOptions::default();
    assert!(matches!(
        SetFrameLossless(mismatch, dst, rect(0, 0, 1, 1), &options),
        Err(AnimationCandidateError::Input(
            AnimationLeafError::InvalidDimensions
        ))
    ));
    assert!(matches!(
        SetFrameLossless(
            src,
            dst,
            rect(0, 0, 3, 3),
            &LosslessCandidateOptions {
                is_first_frame: true,
                ..options
            }
        ),
        Err(AnimationCandidateError::InvalidFirstFrame)
    ));
    for codec in [
        LosslessOptions {
            quality: 101,
            ..Default::default()
        },
        LosslessOptions {
            method: 7,
            ..Default::default()
        },
    ] {
        assert!(
            SetFrameLossless(
                src,
                dst,
                rect(0, 0, 3, 3),
                &LosslessCandidateOptions { codec, ..options }
            )
            .is_err()
        );
        assert!(EncodeCandidate(dst, rect(0, 0, 3, 3), &codec, false).is_err());
    }
    for guess in [
        rect(-1, 0, 3, 3),
        rect(1, 0, 3, 3),
        rect(0, 1, 3, 3),
        rect(0, 0, 2, 3),
    ] {
        assert!(EncodeCandidate(dst, guess, &options.codec, false).is_err());
    }
    assert!(SetFrameLossless(src, dst, rect(1, 1, 3, 3), &options).is_err());
    SetFrameLossless(src, dst, rect(0, 0, 3, 3), &options).unwrap();
    assert_eq!(source, [0; 9]);
    assert_eq!(target, [0xff123456; 9]);
}

#[test]
fn animation_candidates_committed_bytes_and_compositing() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_candidates.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.input, case.input);
        assert_eq!(stored.function, case.function);
        assert_eq!(&case.rust(), expected);
        let _ = check_selection(&case, expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_candidates_match_pinned_reference() {
    let cases = corpus::differential_cases();
    let stats = native::parallel_cases("animation-candidates", &cases, |case, expected| {
        let (
            mut skipped,
            mut blended,
            mut replaced,
            mut background,
            mut carries,
            mut background_carry,
        ) = (0, 0, 0, 0, 0, 0);
        native::compare(case, expected, &case.rust());
        if case.input[0] == 3 {
            if expected[0] == 0 {
                skipped += 1;
            } else {
                if expected[5] == 0 {
                    blended += 1;
                } else {
                    replaced += 1;
                }
                background += usize::from(expected[6] == 2);
                carries += usize::from(expected[7] == 1);
                background_carry += usize::from(expected[6] == 2 && expected[7] == 1);
            }
        }
        [
            skipped,
            blended,
            replaced,
            background,
            carries,
            background_carry,
        ]
    });
    let skipped: usize = stats.iter().map(|counts| counts[0]).sum();
    let blended: usize = stats.iter().map(|counts| counts[1]).sum();
    let replaced: usize = stats.iter().map(|counts| counts[2]).sum();
    let background: usize = stats.iter().map(|counts| counts[3]).sum();
    let carries: usize = stats.iter().map(|counts| counts[4]).sum();
    let background_carry: usize = stats.iter().map(|counts| counts[5]).sum();

    assert!(
        skipped > 0
            && blended > 0
            && replaced > 0
            && background > 0
            && carries > 0
            && background_carry > 0
    );
    eprintln!(
        "{} lossless candidate/mutation cases match scalar C; skip={skipped}, blend={blended}, replace={replaced}, background={background}, carries={carries}, background_carry={background_carry}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_candidate_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-candidate-fixtures", &cases);
    let mut decode_requests = Vec::new();
    let mut decode_expected = Vec::new();
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
        if let Some((request, observation)) = check_selection(case, expected) {
            decode_requests.push(request);
            decode_expected.push(observation);
        }
    }
    let decoded = native::run("animation-candidate-decodes", &decode_requests);
    for ((case, expected), actual) in decode_requests.iter().zip(&decode_expected).zip(&decoded) {
        native::compare(case, expected, actual);
    }
    eprintln!(
        "{} all-frame composites match pinned WebPAnimDecoder",
        decode_requests.len()
    );
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C lossless candidate fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, out) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for word in &case.input {
                write!(text, " {word:08x}").unwrap();
            }
            text.push_str(" |");
            for word in out {
                write!(text, " {word:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/animation_candidates.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_candidates.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, actual)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.input, case.input);
            assert_eq!(stored.function, case.function);
            native::compare(case, expected, actual);
        }
    }
    eprintln!("{} lossless candidate fixtures match scalar C", cases.len());
}
