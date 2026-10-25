#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::mux::anim_encode::*;
use libwebpop::*;
use support::animation_lossy as corpus;

fn file(output: &[u32]) -> Vec<u8> {
    assert_eq!(output[7] as usize, output.len() - 8);
    output[8..]
        .iter()
        .map(|&v| u8::try_from(v).unwrap())
        .collect()
}

fn chunk<'a>(file: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 12;
    while at < file.len() {
        let len = u32::from_le_bytes(file[at + 4..at + 8].try_into().unwrap()) as usize;
        if &file[at..at + 4] == tag {
            return Some(&file[at + 8..at + 8 + len]);
        }
        at += 8 + len + (len & 1);
    }
    None
}

// Parse the standardized VP8 keyframe header with the existing independent
// arithmetic decoder. Observe both absolute segment and global filter levels.
fn filter_levels(file: &[u8]) -> Vec<u32> {
    use support::vp8_boolean::Decoder;
    fn bits(d: &mut Decoder<'_>, n: usize) -> u32 {
        (0..n).fold(0, |v, _| 2 * v + u32::from(d.bit(128)))
    }
    fn signed(d: &mut Decoder<'_>, n: usize) -> u32 {
        if !d.bit(128) {
            return 0;
        }
        let value = bits(d, n);
        assert!(!d.bit(128)); // All segment features in this preset are positive.
        value
    }
    let vp8 = chunk(file, b"VP8 ").unwrap();
    assert_eq!(vp8[0] & 1, 0); // keyframe
    assert!([0, 2].contains(&((vp8[0] >> 1) & 7)));
    assert_eq!(&vp8[3..6], &[0x9d, 0x01, 0x2a]);
    let size0 = (u32::from(vp8[0]) | (u32::from(vp8[1]) << 8) | (u32::from(vp8[2]) << 16)) >> 5;
    let mut d = Decoder::new(&vp8[10..10 + size0 as usize]);
    assert_eq!(bits(&mut d, 2), 0); // color space / clamp
    let mut levels = Vec::new();
    if d.bit(128) {
        let update_map = d.bit(128);
        assert!(d.bit(128)); // segment data present
        assert!(d.bit(128)); // absolute features
        for _ in 0..4 {
            signed(&mut d, 7);
        }
        for _ in 0..4 {
            levels.push(signed(&mut d, 6));
        }
        if update_map {
            for _ in 0..3 {
                if d.bit(128) {
                    bits(&mut d, 8);
                }
            }
        }
    }
    assert!(!d.bit(128)); // complex filter
    levels.push(bits(&mut d, 6));
    assert_eq!(bits(&mut d, 3), 0); // default sharpness
    levels
}

fn check(case: &support::Case, output: &[u32]) {
    let a = &case.input;
    assert_eq!(&output[..7], &[a[3], a[4], a[0], a[1], 1 - a[9], 0, 0]);
    let bytes = file(output);
    let rgba = corpus::rgba(case);
    let mut input = vec![a[0], a[1], a[0] * 4, a[5], a[7]];
    input.extend(rgba.iter().copied().map(u32::from));
    support::lossy::check_decoded(
        &support::Case::new("WebPEncodeRGBAWithAlphaQuality", input),
        &output[8..],
    );
    let levels = filter_levels(&bytes);
    let profile = (chunk(&bytes, b"VP8 ").unwrap()[0] >> 1) & 7;
    if a[9] != 0 {
        assert_eq!(profile, 2); // disabled filtering
        assert!(levels.iter().all(|&level| level == 0), "{levels:?}");
    } else {
        assert_eq!(profile, 0); // default complex filter
        let mut still = Vec::new();
        encode_lossy_rgba(
            &rgba,
            a[0],
            a[1],
            a[0] as usize * 4,
            &corpus::options(case),
            &mut still,
        )
        .unwrap();
        assert_eq!(bytes, still);
    }
}

#[test]
fn animation_lossy_candidate_fixtures_preserve_defaults_and_disable_blended_filters() {
    let fixtures =
        fixture_support::fixtures(include_str!("fixtures/animation_lossy_candidates.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut changed = [0; 7];
    for (pair, cases) in fixtures.chunks_exact(2).zip(cases.chunks_exact(2)) {
        for ((stored, expected), case) in pair.iter().zip(cases) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            assert_eq!(case.rust(), *expected);
            check(case, expected);
        }
        let replacement = file(&pair[0].1);
        let blended = file(&pair[1].1);
        assert_eq!(chunk(&replacement, b"ALPH"), chunk(&blended, b"ALPH"));
        let decode = |bytes: &[u8]| {
            image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8()
        };
        let (before, after) = (decode(&replacement), decode(&blended));
        for (a, b) in before.pixels().zip(after.pixels()) {
            assert_eq!(a[3], b[3]);
        }
        if before != after {
            changed[cases[0].input[6] as usize] += 1;
            assert!(filter_levels(&replacement).iter().any(|&level| level != 0));
        }
    }
    assert!(changed.into_iter().all(|n| n > 0), "{changed:?}");
}

#[test]
fn animation_lossy_candidate_checks_config_geometry_and_preserves_borrowed_views() {
    let pixels = [0xff123456; 9];
    let view = ArgbView::new(&pixels, 3, 3, 3).unwrap();
    let rect = FrameRectangle {
        width: 3,
        height: 3,
        ..Default::default()
    };
    for use_blending in [false, true] {
        for options in [
            LossyOptions {
                method: u32::MAX,
                ..Default::default()
            },
            LossyOptions {
                quality: f32::NAN,
                ..Default::default()
            },
            LossyOptions {
                alpha_quality: -1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                encode_lossy_candidate(view, rect, &options, use_blending),
                Err(AnimationCandidateError::Encoding(_))
            ));
        }
        for invalid in [
            FrameRectangle {
                x_offset: -2,
                ..rect
            },
            FrameRectangle {
                y_offset: 1,
                ..rect
            },
            FrameRectangle {
                x_offset: 1 << 24,
                ..rect
            },
            FrameRectangle { height: 2, ..rect },
        ] {
            assert!(matches!(
                encode_lossy_candidate(view, invalid, &LossyOptions::default(), use_blending),
                Err(AnimationCandidateError::Input(
                    AnimationLeafError::InvalidRectangle
                ))
            ));
        }
        let large = vec![0; 16384];
        let oversized = ArgbView::new(&large, 16384, 1, 16384).unwrap();
        assert!(matches!(
            encode_lossy_candidate(
                oversized,
                FrameRectangle {
                    width: 16384,
                    height: 1,
                    ..Default::default()
                },
                &LossyOptions::default(),
                use_blending
            ),
            Err(AnimationCandidateError::Encoding(_))
        ));
    }
    let image = support::lossy::image(19, 21, 4);
    let case = corpus::request(&image, LossyOptions::default(), true);
    let a = &case.input;
    let before = a.clone();
    let nested = ArgbView::new(&a[10..], 19, 21, a[2] as usize)
        .unwrap()
        .sub_view(FrameRectangle {
            x_offset: 1,
            y_offset: 1,
            width: 18,
            height: 20,
        })
        .unwrap()
        .sub_view(FrameRectangle {
            x_offset: 1,
            y_offset: 1,
            width: 17,
            height: 19,
        })
        .unwrap();
    let packed: Vec<_> = (0..19)
        .flat_map(|y| nested.row(y).unwrap().iter().copied())
        .collect();
    let options = FrameRectangle {
        width: 17,
        height: 19,
        ..Default::default()
    };
    let to_still = |view| {
        let candidate =
            encode_lossy_candidate(view, options, &LossyOptions::default(), true).unwrap();
        let mut mux = AnimationContainer::new(17, 19, Default::default()).unwrap();
        mux.push_frame(candidate.frame, Default::default()).unwrap();
        let mut bytes = Vec::new();
        mux.assemble(&mut bytes).unwrap();
        bytes
    };
    assert_eq!(
        to_still(nested),
        to_still(ArgbView::new(&packed, 17, 19, 17).unwrap())
    );
    assert_eq!(*a, before);
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_lossy_candidates_match_pinned_c() {
    let cases = corpus::differential_cases();
    native::parallel_cases("animation-lossy-candidates", &cases, |case, out| {
        native::compare(case, out, &case.rust());
        check(case, out);
    });
    eprintln!(
        "{} lossy candidates match original EncodeCandidate; all files independently decoded",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_lossy_candidate_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = corpus::fixture_cases();
    let outputs = native::run("animation-lossy-candidate-fixtures", &cases);
    for (case, out) in cases.iter().zip(&outputs) {
        native::compare(case, out, &case.rust());
        check(case, out);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C lossy animation EncodeCandidate/filter fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
            support::UPSTREAM_REVISION,
            support::SEED
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
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/animation_lossy_candidates.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures =
            fixture_support::fixtures(include_str!("fixtures/animation_lossy_candidates.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
}
