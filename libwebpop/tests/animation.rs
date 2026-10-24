#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use support::animation;

#[test]
fn animation_committed_leaves_match_c() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation.txt"));
    let cases = animation::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected, "{case:?}");
    }
}

#[test]
fn animation_similarity_matches_channel_bounds_and_blending_rules() {
    for alpha in 0..=255u32 {
        for diff in 0..=255 {
            for bound in [0, 1, 7, 31, 255] {
                let a = alpha << 24;
                let b = a | diff;
                let expected = f64::from(diff) * f64::from(alpha) <= f64::from(bound) * 255.;
                for shift in [0, 8, 16] {
                    assert_eq!(PixelsAreSimilar(a, a | (diff << shift), bound), expected);
                    assert_eq!(PixelsAreSimilar(a | (diff << shift), a, bound), expected);
                }
                assert!(!PixelsAreSimilar(a, b ^ 0x01000000, bound));
            }
            let a = [alpha << 24];
            let b = [(alpha << 24) | diff];
            let src = ArgbView::new(&a, 1, 1, 1).unwrap();
            let dst = ArgbView::new(&b, 1, 1, 1).unwrap();
            let rect = FrameRectangle {
                width: 1,
                height: 1,
                ..Default::default()
            };
            assert_eq!(
                IsLosslessBlendingPossible(src, dst, &rect),
                alpha == 255 || diff == 0
            );
            assert_eq!(
                IsLossyBlendingPossible(src, dst, &rect, 100.),
                alpha == 255 || diff * alpha <= 255
            );
        }
    }
    assert_eq!(QualityToMaxDiff(0.), 31);
    assert_eq!(QualityToMaxDiff(100.), 1);
}

#[test]
fn animation_rectangles_match_independent_changed_pixel_bounds() {
    // Brute-force bounding box over every 3x3 change mask, including invisible
    // RGB differences, all borders, and one-pixel changes at odd origins.
    let src_pixels = [0; 13]; // stride 5, no final-row padding
    let src = ArgbView::new(&src_pixels, 3, 3, 5).unwrap();
    for mask in 0..512 {
        let mut dst_pixels = [0xdeadbeef; 17]; // different stride 7
        let mut changed = Vec::new();
        for y in 0..3 {
            for x in 0..3 {
                let changed_here = mask & (1 << (y * 3 + x)) != 0;
                dst_pixels[y * 7 + x] = u32::from(changed_here);
                if changed_here {
                    changed.push((x as i32, y as i32));
                }
            }
        }
        let dst = ArgbView::new(&dst_pixels, 3, 3, 7).unwrap();
        let expected = if changed.is_empty() {
            FrameRectangle::default()
        } else {
            let left = changed.iter().map(|p| p.0).min().unwrap();
            let top = changed.iter().map(|p| p.1).min().unwrap();
            FrameRectangle {
                x_offset: left,
                y_offset: top,
                width: changed.iter().map(|p| p.0).max().unwrap() - left + 1,
                height: changed.iter().map(|p| p.1).max().unwrap() - top + 1,
            }
        };
        let mut rect = FrameRectangle {
            width: 3,
            height: 3,
            ..Default::default()
        };
        MinimizeChangeRectangle(src, dst, &mut rect, true, 75.);
        assert_eq!(rect, expected);
        SnapToEvenOffsets(&mut rect);
        assert_eq!(rect.x_offset % 2, 0);
        assert_eq!(rect.y_offset % 2, 0);
        assert_eq!(
            rect.x_offset + rect.width,
            expected.x_offset + expected.width
        );
        assert_eq!(
            rect.y_offset + rect.height,
            expected.y_offset + expected.height
        );
        // Fully transparent RGB differences are ignored by the lossy heuristic.
        MinimizeChangeRectangle(src, dst, &mut rect, false, 100.);
        assert_eq!(rect, FrameRectangle::default());
    }
}

#[test]
fn animation_checked_views_and_refinement_errors_preserve_inputs() {
    use AnimationLeafError::*;
    assert_eq!(ArgbView::new(&[], 0, 1, 1).unwrap_err(), InvalidDimensions);
    assert_eq!(ArgbView::new(&[], -1, 1, 1).unwrap_err(), InvalidDimensions);
    assert_eq!(ArgbView::new(&[0; 4], 2, 2, 1).unwrap_err(), InvalidStride);
    assert_eq!(ArgbView::new(&[0; 4], 2, 2, 3).unwrap_err(), InputTooShort);
    assert_eq!(
        ArgbView::new(&[], 1, 2, usize::MAX).unwrap_err(),
        SizeOverflow
    );
    let pixels = [0; 5];
    let src = ArgbView::new(&pixels, 2, 2, 3).unwrap();
    let mismatched = ArgbView::new(&pixels, 1, 1, 1).unwrap();
    let initial = FrameRectangle {
        x_offset: -2,
        y_offset: -3,
        width: 9,
        height: 7,
    };
    let mut rect = initial;
    assert_eq!(
        WebPAnimEncoderRefineRect(src, mismatched, true, 75., &mut rect),
        Err(InvalidDimensions)
    );
    assert_eq!(rect, initial);
    for quality in [-1., 100.01, f32::NAN, f32::INFINITY] {
        assert_eq!(
            WebPAnimEncoderRefineRect(src, src, true, quality, &mut rect),
            Err(InvalidQuality)
        );
        assert_eq!(rect, initial);
    }
    rect.width = 0;
    let invalid = rect;
    assert_eq!(
        WebPAnimEncoderRefineRect(src, src, true, 75., &mut rect),
        Err(InvalidRectangle)
    );
    assert_eq!(rect, invalid);
    assert_eq!(pixels, [0; 5]);
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_leaves_match_pinned_reference() {
    let cases = animation::differential_cases();
    let outputs = native::run("animation-leaves", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    eprintln!("{} animation leaf cases match pinned scalar C", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = animation::fixture_cases();
    let outputs = native::run("animation-fixtures", &cases);
    for (case, out) in cases.iter().zip(&outputs) {
        native::compare(case, out, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C animation leaf fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED,
        );
        for (case, out) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for v in &case.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in out {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/animation.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/animation.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.input, case.input);
            assert_eq!(stored.function, case.function);
            native::compare(case, expected, out);
        }
    }
    eprintln!("{} animation fixtures match pinned scalar C", cases.len());
}
