#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::internal::mux::anim_encode::*;
use support::animation_subframes as corpus;

fn rect(x: i32, y: i32, w: i32, h: i32) -> FrameRectangle {
    FrameRectangle {
        x_offset: x,
        y_offset: y,
        width: w,
        height: h,
    }
}

fn check_view(view: Option<ArgbView<'_>>, rect: FrameRectangle, canvas: ArgbView<'_>) {
    if IsEmptyRect(&rect) {
        assert!(view.is_none());
        return;
    }
    let view = view.unwrap();
    assert_eq!(
        (view.width(), view.height(), view.stride()),
        (rect.width, rect.height, canvas.stride())
    );
    for y in 0..rect.height as usize {
        let expected = &canvas.row(y + rect.y_offset as usize).unwrap()[rect.x_offset as usize..]
            [..rect.width as usize];
        assert_eq!(view.row(y).unwrap(), expected);
        assert_eq!(view.row(y).unwrap().as_ptr(), expected.as_ptr());
    }
    assert!(view.row(rect.height as usize).is_none());
    assert!(view.row(usize::MAX).is_none());
}

#[test]
fn animation_subframe_views_are_checked_borrowed_and_nested() {
    let pixels: Vec<_> = (0..29).collect(); // 5x4, stride 8, final padding omitted
    let canvas = ArgbView::new(&pixels, 5, 4, 8).unwrap();
    let first = canvas.sub_view(rect(1, 1, 4, 3)).unwrap();
    check_view(Some(first), rect(1, 1, 4, 3), canvas);
    let nested = first.sub_view(rect(1, 1, 3, 2)).unwrap();
    check_view(Some(nested), rect(2, 2, 3, 2), canvas);
    let last = nested.sub_view(rect(2, 1, 1, 1)).unwrap();
    check_view(Some(last), rect(4, 3, 1, 1), canvas);
    for invalid in [
        rect(-1, 0, 1, 1),
        rect(0, -1, 1, 1),
        rect(0, 0, 0, 1),
        rect(0, 0, 1, 0),
        rect(0, 0, -1, 1),
        rect(5, 0, 1, 1),
        rect(0, 4, 1, 1),
        rect(4, 3, 2, 1),
        rect(4, 3, 1, 2),
        rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX),
        rect(i32::MIN, i32::MIN, i32::MAX, i32::MAX),
    ] {
        assert_eq!(
            canvas.sub_view(invalid).unwrap_err(),
            AnimationLeafError::InvalidRectangle
        );
    }
    let one_row = ArgbView::new(&pixels[..5], 5, 1, usize::MAX).unwrap();
    assert_eq!(
        one_row.sub_view(rect(4, 0, 1, 1)).unwrap().row(0).unwrap(),
        &[4]
    );
    assert_eq!(pixels, (0..29).collect::<Vec<_>>());
}

fn independent_bounds(
    points: &[(i32, i32)],
    key: bool,
    first: bool,
    empty: bool,
) -> FrameRectangle {
    if key && !first {
        return rect(0, 0, 3, 3);
    }
    if points.is_empty() {
        return if empty {
            rect(0, 0, 0, 0)
        } else {
            rect(0, 0, 1, 1)
        };
    }
    let left = points.iter().map(|p| p.0).min().unwrap() & !1;
    let top = points.iter().map(|p| p.1).min().unwrap() & !1;
    let right = points.iter().map(|p| p.0).max().unwrap() + 1;
    let bottom = points.iter().map(|p| p.1).max().unwrap() + 1;
    rect(left, top, right - left, bottom - top)
}

#[test]
fn animation_subframe_routing_matches_independent_bounds() {
    // All 3x3 change masks, hidden RGB, small/large RGB and alpha differences;
    // independent lossy channel bounds and every key/first/empty combination.
    for kind in 0..3 {
        let mut source = [0xdeadbeef; 13];
        for y in 0..3 {
            for x in 0..3 {
                source[y * 5 + x] = ([0u32, 1, 127, 254, 255][(y * 3 + x) % 5] << 24) | 0x304050;
            }
        }
        let src = ArgbView::new(&source, 3, 3, 5).unwrap();
        for mask in 0..512 {
            let mut target = [0xaabbccdd; 17];
            let mut lossless_points = Vec::new();
            for y in 0..3 {
                for x in 0..3 {
                    let a = source[y * 5 + x];
                    let changed = mask & (1 << (y * 3 + x)) != 0;
                    target[y * 7 + x] = a ^ if changed {
                        [1, 128, 0x01000000][kind]
                    } else {
                        0
                    };
                    if changed {
                        lossless_points.push((x as i32, y as i32));
                    }
                }
            }
            let dst = ArgbView::new(&target, 3, 3, 7).unwrap();
            for (quality, bound) in [(0., 31), (100., 1)] {
                let mut lossy_points = Vec::new();
                for y in 0..3 {
                    for x in 0..3 {
                        let a = source[y * 5 + x];
                        let b = target[y * 7 + x];
                        let alpha = b >> 24;
                        if a >> 24 != alpha
                            || [0, 8, 16].into_iter().any(|shift| {
                                ((a >> shift) & 255).abs_diff((b >> shift) & 255) * alpha
                                    > bound * 255
                            })
                        {
                            lossy_points.push((x as i32, y as i32));
                        }
                    }
                }
                for flags in 0..8 {
                    let (key, first, empty) = (flags & 1 != 0, flags & 2 != 0, flags & 4 != 0);
                    let mut params = SubFrameParams::new(flags & 1 != 0, empty);
                    GetSubRects(src, dst, key, first, quality, &mut params).unwrap();
                    assert_eq!(params.should_try, flags & 1 != 0);
                    assert_eq!(params.empty_rect_allowed, empty);
                    assert_eq!(
                        params.rect_ll,
                        independent_bounds(&lossless_points, key, first, empty)
                    );
                    assert_eq!(
                        params.rect_lossy,
                        independent_bounds(&lossy_points, key, first, empty)
                    );
                    check_view(params.sub_frame_ll, params.rect_ll, dst);
                    check_view(params.sub_frame_lossy, params.rect_lossy, dst);
                }
            }
        }
    }
}

#[test]
fn animation_subframe_validation_and_empty_results_preserve_state() {
    let source = [0; 9];
    let mut target = [0; 9];
    target[4] = 0xff000000;
    let src = ArgbView::new(&source, 3, 3, 3).unwrap();
    let dst = ArgbView::new(&target, 3, 3, 3).unwrap();
    let mismatched = ArgbView::new(&source, 1, 1, 1).unwrap();
    let mut params = SubFrameParams::new(true, true);
    GetSubRects(src, dst, false, false, 75., &mut params).unwrap();
    assert!(params.sub_frame_ll.is_some());
    let before = format!("{params:?}");
    for quality in [-1., 100.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            GetSubRects(src, dst, false, false, quality, &mut params),
            Err(AnimationLeafError::InvalidQuality)
        );
        assert_eq!(format!("{params:?}"), before);
        let mut guess = rect(1, 1, 2, 2);
        assert_eq!(
            GetSubRect(src, dst, true, false, true, true, quality, &mut guess).unwrap_err(),
            AnimationLeafError::InvalidQuality
        );
        assert_eq!(guess, rect(1, 1, 2, 2));
    }
    assert_eq!(
        GetSubRects(mismatched, dst, false, false, 75., &mut params),
        Err(AnimationLeafError::InvalidDimensions)
    );
    assert_eq!(format!("{params:?}"), before);
    for mut guess in [
        rect(-1, 0, 1, 1),
        rect(3, 3, 1, 1),
        rect(0, 0, -1, 1),
        rect(i32::MAX, 0, i32::MAX, 1),
    ] {
        let before = guess;
        assert_eq!(
            GetSubRect(src, dst, false, false, true, true, 75., &mut guess).unwrap_err(),
            AnimationLeafError::InvalidRectangle
        );
        assert_eq!(guess, before);
    }
    let mut noncanonical = rect(1, 1, 0, 0);
    assert_eq!(
        GetSubRect(src, dst, true, false, false, true, 75., &mut noncanonical).unwrap_err(),
        AnimationLeafError::InvalidRectangle
    );
    assert_eq!(noncanonical, rect(1, 1, 0, 0));
    // Reusing params must clear the earlier borrowed views on a skipped frame.
    GetSubRects(dst, dst, false, false, 75., &mut params).unwrap();
    assert_eq!(params.rect_ll, FrameRectangle::default());
    assert_eq!(params.rect_lossy, FrameRectangle::default());
    assert!(params.sub_frame_ll.is_none());
    assert!(params.sub_frame_lossy.is_none());
    // First frame must still route to 1x1 when the transparent canvases match.
    params.empty_rect_allowed = false;
    GetSubRects(src, src, true, true, 75., &mut params).unwrap();
    assert_eq!(params.rect_ll, rect(0, 0, 1, 1));
    assert_eq!(params.rect_lossy, rect(0, 0, 1, 1));
    check_view(params.sub_frame_ll, params.rect_ll, src);
    check_view(params.sub_frame_lossy, params.rect_lossy, src);
}

#[test]
fn animation_subframes_committed_fixtures_match_c() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_subframes.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_subframes_match_pinned_reference() {
    let cases = corpus::differential_cases();
    let outputs = native::run("animation-subframes", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    eprintln!(
        "{} animation subframe cases match pinned scalar C",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_subframe_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-subframe-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C animation subframe fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
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
                "/tests/fixtures/animation_subframes.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_subframes.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, actual)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.input, case.input);
            assert_eq!(stored.function, case.function);
            native::compare(case, expected, actual);
        }
    }
    eprintln!(
        "{} animation subframe fixtures match pinned scalar C",
        cases.len()
    );
}
