#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::filters::*,
    enc::{alpha_enc::*, vp8l_enc::VP8LEncodeStream},
    utils::{bit_writer_utils::*, filters_utils::WebPEstimateBestFilter},
};
use support::vp8_alpha::{green_decode, inverse};
use support::vp8_alpha_methods as alpha;

#[cfg(feature = "native-oracle")]
#[test]
fn alpha_method_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = alpha::fixture_cases();
    let outputs = native::run("lossy-alpha-method-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C method-dependent alpha (methods milestone 7).\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for protocol and provenance.\n",
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
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/lossy_alpha_methods.txt"
            ),
            text,
        )
        .unwrap();
        return;
    }
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_alpha_methods.txt"));
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        native::compare(case, expected, out);
        native::compare(case, out, &evaluate(case));
    }
}

fn evaluate(case: &support::Case) -> Vec<u32> {
    if case.function == "AlphaMethodOps" {
        case.rust()
    } else {
        alpha::file_output(case)
    }
}

#[test]
fn committed_alpha_method_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_alpha_methods.txt"));
    let cases = alpha::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        let actual = evaluate(&case);
        assert_eq!(
            *expected, actual,
            "{} method {}",
            case.function, case.input[0]
        );
        if case.function == "WebPEncodeLossyMethod" {
            support::lossy_methods::check_decoded(&case, &actual);
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn alpha_methods_match_pinned_leaves_and_trials() {
    let cases = alpha::leaf_cases();
    native::parallel_cases("lossy-alpha-method-leaves", &cases, |case, out| {
        native::compare(case, out, &case.rust());
    });
    eprintln!(
        "{} exact method-dependent alpha leaf/trial requests",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn all_methods_match_normal_c_files_and_decode() {
    let cases = alpha::file_cases();
    let counts = native::parallel_cases("lossy-alpha-method-files", &cases, |case, out| {
        native::compare(case, out, &alpha::file_output(case));
        support::lossy_methods::check_decoded(case, out)
    });
    let mut totals = [[0; 3]; 7];
    for (case, count) in cases.iter().zip(counts) {
        for (total, n) in totals[case.input[5] as usize].iter_mut().zip(count) {
            *total += n;
        }
    }
    assert!(totals.iter().all(|c| c.iter().all(|&n| n > 0)));
    eprintln!(
        "{} complete method-dependent alpha files match C and decode: changed/raw/compressed {totals:?}",
        cases.len()
    );
}

fn stream(data: &[u8], w: usize, h: usize, effort: u32, quality: u32) -> Vec<u8> {
    // Construct the exact green-only image independently of alpha dispatch.
    let argb: Vec<_> = data.iter().map(|&v| u32::from(v) << 8).collect();
    let mut bw = VP8LBitWriterInit(0).unwrap();
    VP8LEncodeStream(&mut bw, &argb, w, h, w, effort, quality, true).unwrap();
    VP8LBitWriterFinish(&mut bw).unwrap().to_vec()
}

#[test]
fn headerless_streams_decode_and_use_method_dependent_quality() {
    let mut decoded = [0; 7];
    let mut exception_changes = 0;
    for case in alpha::leaf_cases().into_iter().filter(|c| c.input[1] == 6) {
        let effort = case.input[0];
        let a = &case.input[1..];
        let (w, h) = (a[1] as usize, a[2] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let quality = if effort == 6 && a[6] == 0 {
            100
        } else {
            8 * effort
        };
        let expected = stream(&data, w, h, effort, quality);
        assert_eq!(
            case.rust(),
            expected.iter().copied().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(green_decode(&expected, w, h), data);
        decoded[effort as usize] += 1;
        if effort == 6 && a[6] == 0 {
            exception_changes += usize::from(expected != stream(&data, w, h, 6, 48));
        }
    }
    assert!(decoded.iter().all(|&n| n > 0));
    assert!(exception_changes > 0);
    eprintln!(
        "headerless streams independently decoded {decoded:?}; {exception_changes} method-6 quality-exception byte changes"
    );
}

#[test]
fn every_method_trial_inverts_and_retains_equal_size_compression() {
    let mut counts = [[0; 3]; 7];
    for case in alpha::leaf_cases().into_iter().filter(|c| c.input[1] == 7) {
        let effort = case.input[0];
        let a = &case.input[1..];
        let (w, h, filter) = (a[1] as usize, a[2] as usize, a[5] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let result: Vec<_> = case.rust().into_iter().map(|v| v as u8).collect();
        assert_eq!(result[0] >> 2 & 3, filter as u8);
        assert_eq!(result[0] >> 4, a[6] as u8);
        let compressed = result[0] & 1 != 0;
        let filtered = if compressed {
            green_decode(&result[1..], w, h)
        } else {
            result[1..].to_vec()
        };
        assert_eq!(inverse(&filtered, w, h, w, filter), data);
        let quality = if effort == 6 && a[6] == 0 {
            100
        } else {
            8 * effort
        };
        let stream = stream(&filtered, w, h, effort, quality);
        assert_eq!(compressed, stream.len() <= w * h);
        assert_eq!(&result[1..], if compressed { &stream } else { &filtered });
        let c = &mut counts[effort as usize];
        c[usize::from(compressed)] += 1;
        c[2] += usize::from(stream.len() == w * h);
    }
    assert!(counts.iter().all(|c| c.iter().all(|&n| n > 0)));
    eprintln!("raw/compressed/equal-size trials by method: {counts:?}");
}

#[test]
fn filter_trials_follow_effort_color_boundaries_and_first_size_tie() {
    let mut ties = [0; 7];
    let mut extra_none = [0; 7];
    for case in alpha::leaf_cases().into_iter().filter(|c| c.input[1] == 10) {
        let effort = case.input[0];
        let a = &case.input[1..];
        let (w, h) = (a[1] as usize, a[2] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let colors = data
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let estimate = WebPEstimateBestFilter(&data, w, h);
        let map = match a[5] {
            0 => 1,
            5 => 15,
            6 if colors <= 16 => 1,
            6 => (1 << estimate) | u32::from(effort > 3 || colors > 192),
            _ => unreachable!(),
        };
        if a[5] == 6 && (17..=192).contains(&colors) && estimate != 0 && map & 1 != 0 {
            extra_none[effort as usize] += 1;
        }
        let out = case.rust();
        assert_eq!(out[0], map);
        let mut cursor = 1;
        let mut trials = Vec::new();
        for filter in 0..4 {
            if map & (1 << filter) != 0 {
                assert_eq!(out[cursor], filter);
                let len = out[cursor + 1] as usize;
                let trial = &out[cursor + 2..cursor + 2 + len];
                assert_eq!((trial[0] >> 2) & 3, filter);
                assert_eq!(trial[0] >> 4, a[6]);
                trials.push(trial);
                cursor += 2 + len;
            }
        }
        let min = trials.iter().map(|t| t.len()).min().unwrap();
        let first = trials.iter().find(|t| t.len() == min).unwrap();
        ties[effort as usize] += usize::from(trials.iter().filter(|t| t.len() == min).count() > 1);
        assert_eq!(out[cursor] as usize, min);
        assert_eq!(&out[cursor + 1..], *first);
    }
    assert_eq!(&extra_none[..4], &[0; 4]);
    assert!(extra_none[4..].iter().all(|&n| n > 0));
    assert!(ties.iter().all(|&n| n > 0));
    eprintln!(
        "first-size filter ties {ties:?}; extra NONE trials at 17..192 colors {extra_none:?}"
    );
}

#[test]
fn alpha_quality_request_controls_preprocessing_even_when_samples_do_not_change() {
    let mut changed = 0;
    for effort in 0..=6 {
        for pattern in [0, 3, 8, 11] {
            let source: Vec<_> = support::vp8_alpha::plane(17, 19, 20, pattern)
                .into_iter()
                .map(|v| v as u8)
                .collect();
            for quality in [0, 4, 5, 69, 70, 71, 99, 100] {
                let before = source.clone();
                let encoded =
                    EncodeAlpha(&source, 17, 19, 20, quality, 1, WEBP_FILTER_FAST, effort).unwrap();
                assert_eq!(source, before);
                assert_eq!(encoded.data[0] >> 4, u8::from(quality < 100));
                let filtered = if encoded.data[0] & 1 != 0 {
                    green_decode(&encoded.data[1..], 17, 19)
                } else {
                    encoded.data[1..].to_vec()
                };
                let quant = inverse(&filtered, 17, 19, 17, (encoded.data[0] >> 2 & 3) as usize);
                let mut sse = 0;
                for y in 0..19 {
                    for x in 0..17 {
                        let v = source[y * 20 + x];
                        let q = quant[y * 17 + x];
                        sse += i64::from(i32::from(v) - i32::from(q)).pow(2) as u64;
                        if quality == 100 || pattern != 8 {
                            assert_eq!(q, v);
                        }
                    }
                }
                // The reported SSE uses unrounded centroids. Rounding to the
                // decoded integer plane adds at most 1/4 per sample (plus the
                // final truncation of that reported convergence error).
                assert!(encoded.sse <= sse);
                assert!(sse - encoded.sse <= (17u64 * 19).div_ceil(4) + 1);
                changed += usize::from(sse > 0);
                // In these <=2-color planes quantization is a no-op even at q0.
                // Its request still disables the method-6 quality-100 exception.
                if pattern != 8 {
                    let mut raw = Vec::new();
                    for y in 0..19 {
                        raw.extend_from_slice(&source[y * 20..y * 20 + 17]);
                    }
                    let expected = ApplyFiltersAndEncode(
                        &raw,
                        17,
                        19,
                        1,
                        WEBP_FILTER_FAST,
                        quality < 100,
                        effort,
                    )
                    .unwrap();
                    assert_eq!(encoded.data, expected);
                }
            }
        }
    }
    assert!(changed > 0);
}

#[test]
fn method_six_quality_exception_depends_on_request_with_unchanged_samples() {
    let mut differences = 0;
    for pattern in [4, 7, 8, 9] {
        let data: Vec<_> = support::vp8_alpha::plane(17, 19, 17, pattern)
            .into_iter()
            .map(|v| v as u8)
            .collect();
        // All these planes have fewer than the 248 levels allowed at q99.
        assert!(data.iter().collect::<std::collections::BTreeSet<_>>().len() <= 248);
        let mut selected = Vec::new();
        for alpha_quality in [99, 100] {
            let reduce = alpha_quality < 100;
            let encoded =
                EncodeAlpha(&data, 17, 19, 17, alpha_quality, 1, WEBP_FILTER_FAST, 6).unwrap();
            assert_eq!(encoded.sse, 0);
            let map = GetFilterMap(&data, 17, 19, WEBP_FILTER_FAST, 6);
            let mut trials = Vec::new();
            for filter in 0..4 {
                if map & (1 << filter) == 0 {
                    continue;
                }
                let mut filtered = vec![0; data.len()];
                WebPFilter(&data, 17, 19, 17, filter, &mut filtered);
                let compressed = stream(&filtered, 17, 19, 6, if reduce { 48 } else { 100 });
                let use_compression = compressed.len() <= data.len();
                let mut trial =
                    vec![u8::from(use_compression) | (filter as u8) << 2 | u8::from(reduce) << 4];
                trial.extend_from_slice(if use_compression {
                    &compressed
                } else {
                    &filtered
                });
                trials.push(trial);
            }
            let size = trials.iter().map(Vec::len).min().unwrap();
            assert_eq!(
                encoded.data,
                *trials.iter().find(|t| t.len() == size).unwrap()
            );
            let mut normalized = encoded.data;
            normalized[0] &= !16;
            selected.push(normalized);
        }
        differences += usize::from(selected[0] != selected[1]);
    }
    assert!(differences > 0);
    eprintln!("{differences} method-6 q99/q100 payload differences with unchanged input samples");
}

#[test]
fn alpha_effort_is_validated_for_transparent_and_opaque_jobs() {
    for effort in [7, u32::MAX] {
        assert!(matches!(
            EncodeAlpha(&[0], 1, 1, 1, 100, 1, WEBP_FILTER_FAST, effort),
            Err(AlphaEncodeError::InvalidConfiguration)
        ));
        let mut pic =
            libwebpop::enc::picture_csp_enc::import_rgba_to_yuva(&[0, 0, 0, 255], 1, 1, 4).unwrap();
        assert!(matches!(
            VP8EncStartAlpha(&pic, 100, effort),
            Err(AlphaEncodeError::InvalidConfiguration)
        ));
        pic.a = Some(vec![0]);
        assert!(matches!(
            VP8EncStartAlpha(&pic, 100, effort),
            Err(AlphaEncodeError::InvalidConfiguration)
        ));
    }
}

#[test]
fn opaque_files_ignore_alpha_quality_for_every_method() {
    let image = support::lossy::image(17, 19, 0);
    for effort in 0..=6 {
        let mut first = None;
        for quality in [0, 4, 5, 69, 70, 71, 99, 100] {
            let case = support::lossy_methods::request(&image, 75., quality, effort, false);
            let bytes = alpha::file_output(&case);
            if let Some(first) = &first {
                assert_eq!(&bytes, first);
            } else {
                first = Some(bytes);
            }
        }
    }
}
