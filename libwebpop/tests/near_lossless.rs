#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod near_lossless;
mod support;

use libwebpop::enc::near_lossless_enc::*;

#[test]
fn committed_preprocessing_fixtures() {
    let fixtures = near_lossless::fixtures(include_str!("fixtures/near_lossless.txt"));
    assert_eq!(fixtures.len(), near_lossless::fixture_cases().len());
    for (case, expected) in fixtures {
        assert_eq!(near_lossless::evaluate(&case), expected, "{case:?}");
    }
}

#[test]
fn discretization_rounds_ties_to_even_and_clamps() {
    for bits in 1..=5 {
        let step = 1 << bits;
        for value in 0..=255 {
            // Independent nearest-lattice calculation, followed by the C
            // quantizer's saturation rule (255 is not an extra lattice point).
            let nearest = (0..=256_u32)
                .step_by(step)
                .min_by_key(|&q| (q.abs_diff(value), (q / step as u32) % 2))
                .unwrap()
                .min(255);
            assert_eq!(FindClosestDiscretized(value, bits), nearest);
        }
    }
    assert!(!IsNear(0x10203040, 0x10203048, 8));
    assert!(!IsNear(0x10203048, 0x10203040, 8));
    assert!(IsNear(0x10203040, 0x10203047, 8));
}

#[test]
fn saved_rows_match_an_immutable_neighborhood_pass() {
    for (w, h) in [(1_usize, 2_usize), (2, 5), (3, 3), (9, 11), (65, 7)] {
        for pattern in 0..6 {
            let src = near_lossless::image(w as u32, h as u32, w as u32, pattern);
            for bits in 1..=5 {
                let mut expected = src.clone();
                for y in 1..h - 1 {
                    for x in 1..w.saturating_sub(1) {
                        let i = y * w + x;
                        let smooth = [i - 1, i + 1, i - w, i + w].into_iter().all(|j| {
                            src[i]
                                .to_le_bytes()
                                .into_iter()
                                .zip(src[j].to_le_bytes())
                                .all(|(a, b)| u32::from(a.abs_diff(b)) < (1 << bits))
                        });
                        if !smooth {
                            expected[i] = ClosestDiscretizedArgb(src[i], bits);
                        }
                    }
                }
                let mut actual = src.clone();
                // Dirty reusable scratch must never influence logical output.
                NearLossless(w, h, bits, &mut vec![0xdeadc0de; 3 * w], &mut actual);
                assert_eq!(actual, expected, "{w}x{h}, pattern {pattern}, bits {bits}");
            }
        }
    }
}

#[test]
fn preprocessing_shortcuts_borders_alpha_and_quality_buckets() {
    for (w, h) in [
        (1, 65),
        (2, 65),
        (63, 63),
        (64, 2),
        (64, 3),
        (3, 64),
        (65, 65),
    ] {
        for pattern in 0..6 {
            let stride = w + 3;
            let source = near_lossless::image(w, h, stride, pattern);
            let before = source.clone();
            let packed = near_lossless::packed(&source, w as usize, h as usize, stride as usize);
            for bucket in 0..5 {
                let mut previous = None;
                for quality in bucket * 20..bucket * 20 + 20 {
                    let output =
                        VP8ApplyNearLossless(&source, w, h, stride as usize, quality).unwrap();
                    if let Some(previous) = &previous {
                        assert_eq!(&output, previous, "quality {quality}");
                    }
                    assert_eq!(output.len(), (w * h) as usize);
                    for y in 0..h {
                        for x in 0..w {
                            let i = (y * w + x) as usize;
                            if x == 0
                                || x + 1 == w
                                || y == 0
                                || y + 1 == h
                                || (w < 64 && h < 64)
                                || h < 3
                                || pattern == 5
                            {
                                assert_eq!(output[i], packed[i]);
                            }
                            if [0, 255].contains(&(packed[i] >> 24)) {
                                assert_eq!(output[i] >> 24, packed[i] >> 24);
                            }
                        }
                    }
                    previous = Some(output);
                }
            }
            assert_eq!(source, before);
        }
    }
    // A real active-route case changes intermediate alpha and RGB. Small-image
    // shortcut tests alone would not establish that quantization ran.
    let src = near_lossless::image(64, 3, 64, 3);
    let output = VP8ApplyNearLossless(&src, 64, 3, 64, 0).unwrap();
    assert!(src.iter().zip(&output).any(|(a, b)| a >> 24 != b >> 24));
    assert!(
        src.iter()
            .zip(&output)
            .any(|(a, b)| a & 0xffffff != b & 0xffffff)
    );
}

#[test]
fn preprocessing_rejects_invalid_geometry_and_quality() {
    use NearLosslessError::*;
    for quality in [100, 101, u32::MAX] {
        assert!(matches!(
            VP8ApplyNearLossless(&[0], 1, 1, 1, quality),
            Err(InvalidQuality)
        ));
    }
    for (w, h) in [(0, 1), (1, 0), (16384, 1), (1, 16384), (u32::MAX, 1)] {
        assert!(matches!(
            VP8ApplyNearLossless(&[], w, h, usize::MAX, 0),
            Err(InvalidDimensions)
        ));
    }
    assert!(matches!(
        VP8ApplyNearLossless(&[0; 6], 2, 2, 1, 0),
        Err(InvalidStride)
    ));
    assert!(matches!(
        VP8ApplyNearLossless(&[0; 4], 2, 2, 3, 0),
        Err(InputTooShort)
    ));
    assert!(matches!(
        VP8ApplyNearLossless(&[], 1, 2, usize::MAX, 0),
        Err(SizeOverflow)
    ));
    assert!(matches!(
        VP8ApplyNearLossless(&[], 1, 3, usize::MAX / 2 + 1, 0),
        Err(SizeOverflow)
    ));
    assert_eq!(
        VP8ApplyNearLossless(&[0x12345678], 1, 1, usize::MAX, 0).unwrap(),
        [0x12345678]
    );
    // Dimension limits use skinny buffers rather than large allocations.
    for (w, h) in [(1, 16383), (16383, 1)] {
        let src = vec![0xff123456; (w * h) as usize];
        assert_eq!(
            VP8ApplyNearLossless(&src, w, h, w as usize, 0).unwrap(),
            src
        );
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn preprocessing_matches_pinned_reference() {
    let cases = near_lossless::differential_cases();
    let outputs = native::run("near-lossless", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &near_lossless::evaluate(case));
    }
    eprintln!(
        "Compared {} near-lossless cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn preprocessing_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = near_lossless::fixture_cases();
    let outputs = native::run("near-lossless-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").is_some() {
        let mut text = String::from(
            "# Pinned scalar C standalone near-lossless fixtures.\n# upstream=1844042d1ca751e62e61df7617dba19c98588339 seed=6d2b79f5\n# See ../reference/NEAR_LOSSLESS.md for provenance and regeneration.\n",
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
                "/tests/fixtures/near_lossless.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = near_lossless::fixtures(include_str!("fixtures/near_lossless.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((case, expected), actual) in fixtures.iter().zip(&outputs) {
            native::compare(case, expected, actual);
        }
    }
    for (case, output) in cases.iter().zip(outputs) {
        native::compare(case, &output, &near_lossless::evaluate(case));
    }
}
