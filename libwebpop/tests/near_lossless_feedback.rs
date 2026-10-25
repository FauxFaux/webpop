#[path = "support/near_lossless_feedback.rs"]
mod feedback;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod preprocessing;
mod support;

#[test]
fn committed_feedback_fixtures_match_scalar_c() {
    let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_feedback.txt"));
    let cases = feedback::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.input, generated.input);
        assert_eq!(feedback::evaluate(case), *expected, "{case:?}");
    }
}

#[test]
fn quantized_residuals_reconstruct_with_bounded_error_and_bypass_exactly() {
    use libwebpop::internal::dsp::{lossless::VP8LPredictors, lossless_common::VP8LAddPixels};
    use libwebpop::internal::enc::predictor_enc::AddGreenToBlueAndRed;
    let mut changed = [0; 2];
    let mut alpha_changes = 0;
    for case in feedback::cases().into_iter().filter(|c| c.input[0] == 1) {
        let a = &case.input;
        let (w, h, q, sg) = (a[1] as usize, a[2] as usize, a[5], a[7] != 0);
        let mw = w.div_ceil(4);
        let map_len = mw * h.div_ceil(4);
        let source = &a[8 + map_len..];
        let residuals = feedback::evaluate(&case);
        let mut decoded = vec![0; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let mode = ((a[8 + (y / 4) * mw + x / 4] >> 8) & 255) as usize;
                let predict = if y == 0 {
                    if x == 0 { 0xff000000 } else { decoded[i - 1] }
                } else if x == 0 {
                    decoded[i - w]
                } else {
                    VP8LPredictors[mode](
                        decoded[i - 1],
                        &[
                            decoded[i - w - 1],
                            decoded[i - w],
                            if x + 1 == w {
                                decoded[y * w]
                            } else {
                                decoded[i - w + 1]
                            },
                        ],
                    )
                };
                decoded[i] = VP8LAddPixels(predict, residuals[i]);
                let actual = decoded[i];
                let original = source[i];
                if original >> 24 == 0 || original >> 24 == 255 {
                    assert_eq!(actual >> 24, original >> 24);
                }
                alpha_changes += usize::from(original >> 24 != actual >> 24);
                if original >> 24 != 0 && actual >> 24 != 0 {
                    let restore = |p| if sg { AddGreenToBlueAndRed(p) } else { p };
                    let bound = if mode == 0 || x == 0 || x + 1 == w || y == 0 || y + 1 == h {
                        0
                    } else {
                        q / 2
                    };
                    for (a, b) in restore(actual)
                        .to_be_bytes()
                        .into_iter()
                        .zip(restore(original).to_be_bytes())
                    {
                        assert!(u32::from(a.abs_diff(b)) <= bound);
                    }
                    changed[usize::from(sg)] += usize::from(actual != original);
                }
            }
        }
    }
    assert!(changed.into_iter().all(|n| n > 0));
    assert!(alpha_changes > 0);
}

#[cfg(feature = "native-oracle")]
#[test]
fn feedback_matches_pinned_reference() {
    let cases = feedback::cases();
    native::parallel_cases("near-lossless-feedback", &cases, |case, expected| {
        native::compare(case, expected, &feedback::evaluate(case));
    });
    eprintln!("{} exact near-lossless feedback comparisons", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn feedback_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = feedback::fixture_cases();
    let outputs = native::run("near-lossless-feedback-fixtures", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &feedback::evaluate(case));
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C near-lossless feedback fixtures.\n# upstream={} seed={:08x}\n# See ../reference/NEAR_LOSSLESS.md.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(outputs) {
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
                "/tests/fixtures/near_lossless_feedback.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_feedback.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
