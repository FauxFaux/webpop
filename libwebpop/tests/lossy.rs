#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use support::{Case, lossy};

#[test]
fn committed_vp8_leaf_and_public_reference_files() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy.txt"));
    let cases = lossy::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut totals = [0; 3];
    let mut standard = None;
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        if case.function == "VP8FTransform" {
            assert_eq!(case.rust(), *expected);
        } else {
            let stats = lossy::check_decoded(case, expected);
            for i in 0..3 {
                totals[i] += stats[i];
            }
            if case.function == "WebPEncodeRGBA" {
                standard = Some((case, expected));
            } else {
                let (base, output) = standard.unwrap();
                assert_eq!(&case.input[..4], &base.input[..4]);
                assert_eq!(&case.input[5..], &base.input[4..]);
                if case.input[4] == 100 || stats[1] + stats[2] == 0 {
                    assert_eq!(expected, output);
                }
            }
        }
    }
    assert!(
        totals.into_iter().all(|n| n > 0),
        "alpha changes/raw/compressed: {totals:?}"
    );
}

#[test]
fn forward_transform_constant_residuals_preserve_normalization_and_rounding_bias() {
    // A constant delta maps to 8*delta DC. The first pass's +1812 gives
    // tmp[1]=3 even for zero residual; the second pass rounds the resulting
    // column DC to 1 at coefficient 1. All remaining AC coefficients are zero.
    // Exhaust all byte pairs, including negative residuals.
    for src in 0..=255 {
        for reference in 0..=255 {
            let mut input = vec![src; 16];
            input.extend([reference; 16]);
            let output = Case::new("VP8FTransform", input).rust();
            let mut expected = vec![0; 16];
            expected[0] = (8 * (src as i32 - reference as i32)) as u32;
            expected[1] = 1;
            assert_eq!(output, expected, "src {src}, reference {reference}");
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn forward_transform_matches_pinned_scalar_reference() {
    let cases = lossy::transform_cases();
    let outputs = native::run("lossy-transform", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
    }
    eprintln!("{} VP8 forward transform comparisons", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn lossy_public_c_wrappers_decode_and_agree_at_alpha_100() {
    let cases = lossy::public_cases();
    let outputs = native::run("lossy-public", &cases);
    let mut totals = [0; 3];
    let mut standard = None;
    for (case, output) in cases.iter().zip(&outputs) {
        let stats = lossy::check_decoded(case, output);
        for i in 0..3 {
            totals[i] += stats[i];
        }
        if case.function == "WebPEncodeRGBA" {
            standard = Some((case, output));
        } else {
            let (base, expected) = standard.unwrap();
            assert_eq!(&case.input[..4], &base.input[..4]);
            assert_eq!(&case.input[5..], &base.input[4..]);
            if case.input[4] == 100 || stats[1] + stats[2] == 0 {
                native::compare(case, expected, output);
            }
        }
    }
    assert!(
        totals.into_iter().all(|n| n > 0),
        "alpha changes/raw/compressed: {totals:?}"
    );
    eprintln!(
        "{} independently decoded lossy C files; alpha changes/raw/compressed: {totals:?}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn lossy_public_c_wrappers_reject_invalid_qualities() {
    let image = lossy::image(1, 1, 0);
    let mut cases = Vec::new();
    for quality in [
        -1.,
        100.01,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
        f32::from_bits(0xffc00001),
    ] {
        cases.push(lossy::request(&image, quality, None));
        cases.push(lossy::request(&image, quality, Some(100)));
    }
    for alpha in [i32::MIN, -1, 101, i32::MAX] {
        cases.push(lossy::request(&image, 75., Some(alpha)));
    }
    for output in native::run("lossy-invalid-qualities", &cases) {
        assert!(output.is_empty());
    }
    assert_eq!(
        native::run(
            "lossy-wrapper-errors",
            &[Case::new("AlphaQualityWrapperErrors", vec![])]
        ),
        [vec![1]]
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn lossy_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = lossy::fixture_cases();
    let outputs = native::run("lossy-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        if case.function == "VP8FTransform" {
            native::compare(case, output, &case.rust());
        } else {
            lossy::check_decoded(case, output);
        }
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 forward transform and lossy public fixtures.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lossy.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
