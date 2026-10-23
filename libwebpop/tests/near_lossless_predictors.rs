#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod preprocessing;
#[path = "support/near_lossless_predictors.rs"]
mod quantization;
mod support;

use libwebpop::dsp::lossless_common::{VP8LAddPixels, VP8LSubPixels};
use libwebpop::enc::predictor_enc::*;

#[test]
fn committed_predictor_quantization_fixtures() {
    let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_predictors.txt"));
    let cases = quantization::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(quantization::evaluate(case), *expected, "{case:?}");
    }
}

#[test]
fn component_quantization_respects_rotated_boundaries_and_error_bounds() {
    for q in [1, 2, 4, 8, 16, 32] {
        for value in 0..=255_u8 {
            for predict in 0..=255_u8 {
                for boundary in [0_u8, 255, predict, value.wrapping_add(1)] {
                    let residual = NearLosslessComponent(value, predict, boundary, q);
                    let reconstructed = predict.wrapping_add(residual);
                    // Rotate the inclusive upper boundary to 255; error is
                    // linear in this domain, rather than a circular distance
                    // that would accidentally allow crossing the boundary.
                    let original = value.wrapping_sub(boundary.wrapping_add(1));
                    let actual = reconstructed.wrapping_sub(boundary.wrapping_add(1));
                    assert!(u32::from(original.abs_diff(actual)) <= q / 2);
                    assert_eq!(u32::from(residual) % (q / 2).max(1), 0);
                    if q == 1 {
                        assert_eq!(reconstructed, value);
                    }
                }
            }
        }
    }
    // Ties favor the prediction. At a wrap boundary the half step prevents
    // 0 -> 255 and 255 -> 0, despite those values' small modular distance.
    for (value, predict, expected) in [(4, 0, 0), (0, 4, 0), (0, 255, 4), (255, 0, 252)] {
        assert_eq!(NearLosslessComponent(value, predict, 255, 8), expected);
    }
}

fn restore(pixel: u32) -> [u8; 4] {
    let [a, r, g, b] = pixel.to_be_bytes();
    [a, r.wrapping_add(g), g, b.wrapping_add(g)]
}

#[test]
fn difference_rows_match_independent_restored_neighborhoods() {
    for w in [1, 2, 3, 17, 64, 65] {
        for padding in [0, 3] {
            for sg in [false, true] {
                for pattern in [0, 2, 3, 5, 6, 12] {
                    let case = quantization::row_case(w, padding, sg, pattern);
                    let pixels = &case.input[3..];
                    let before = pixels.to_vec();
                    let stride = w + padding;
                    let mut actual = vec![0xa5; w + 3];
                    MaxDiffsForRow(w, stride, pixels, stride, &mut actual, sg);
                    for x in 1..w.saturating_sub(1) {
                        let i = stride + x;
                        let lanes = |p: u32| if sg { restore(p) } else { p.to_be_bytes() };
                        let expected = [i - 1, i + 1, i - stride, i + stride]
                            .into_iter()
                            .flat_map(|j| lanes(pixels[i]).into_iter().zip(lanes(pixels[j])))
                            .map(|(a, b)| a.abs_diff(b))
                            .max()
                            .unwrap();
                        assert_eq!(actual[x], expected);
                    }
                    assert_eq!(actual[0], 0xa5);
                    assert!(actual[w - 1..].iter().all(|&v| v == 0xa5));
                    assert_eq!(pixels, before);
                }
            }
        }
    }
    // Widths without interior pixels never read the neighboring rows.
    MaxDiffsForRow(2, 2, &[], usize::MAX, &mut [0xa5; 2], true);
}

#[test]
fn pixel_quantization_bounds_restored_color_error_and_preserves_alpha_endpoints() {
    let mut seed = support::SEED;
    let mut alpha_changes = [0; 2];
    let mut compensation_changes = 0;
    for q in [1, 2, 4, 8, 16, 32] {
        for max_diff in 0..=255_u8 {
            for _ in 0..64 {
                let value = support::random(&mut seed);
                let predict = support::random(&mut seed);
                for sg in [false, true] {
                    let residual = NearLossless(value, predict, q, max_diff, sg);
                    let actual = VP8LAddPixels(predict, residual);
                    if max_diff <= 2 || q == 1 {
                        assert_eq!(residual, VP8LSubPixels(value, predict));
                    }
                    let mut step = q;
                    while step >= u32::from(max_diff.max(3)) {
                        step >>= 1;
                    }
                    let bound = if max_diff <= 2 { 0 } else { step / 2 };
                    let lanes = |p| if sg { restore(p) } else { p.to_be_bytes() };
                    for (a, b) in lanes(value).into_iter().zip(lanes(actual)) {
                        assert!(
                            u32::from(a.abs_diff(b)) <= bound,
                            "{value:08x} -> {actual:08x}, q={q}, diff={max_diff}, sg={sg}"
                        );
                    }
                    if value >> 24 == 0 || value >> 24 == 255 {
                        assert_eq!(actual >> 24, value >> 24);
                    }
                    alpha_changes[usize::from(sg)] += usize::from(actual >> 24 != value >> 24);
                    if sg {
                        compensation_changes += usize::from(
                            residual != NearLossless(value, predict, q, max_diff, false),
                        );
                    }
                }
            }
        }
    }
    assert!(alpha_changes.into_iter().all(|count| count > 0));
    assert!(compensation_changes > 0);
}

#[cfg(feature = "native-oracle")]
#[test]
fn predictor_quantization_matches_pinned_reference() {
    let cases = quantization::differential_cases();
    for (case, expected) in cases
        .iter()
        .zip(native::run("near-lossless-predictors", &cases))
    {
        native::compare(case, &expected, &quantization::evaluate(case));
    }
    eprintln!(
        "Compared {} predictor quantization cases (seed {:08x})",
        cases.len(),
        support::SEED
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn predictor_quantization_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = quantization::fixture_cases();
    let outputs = native::run("near-lossless-predictor-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &quantization::evaluate(case));
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C predictor quantization fixtures.\n# upstream={} seed={:08x}\n# See ../reference/NEAR_LOSSLESS.md for provenance and regeneration.\n",
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
                "/tests/fixtures/near_lossless_predictors.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures =
            preprocessing::fixtures(include_str!("fixtures/near_lossless_predictors.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
