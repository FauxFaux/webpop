#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use support::{Case, lossy_methods as methods};

fn check_reference(case: &Case, output: &[u32]) {
    match case.function.as_str() {
        "VP8MethodTools" => {
            methods::check_tools(case, output);
            if case.input[4..6] == [0, 0] {
                assert_eq!(output, methods::tools_output(case));
            }
        }
        "VP8MethodLoopFrame" => {
            let file = methods::check_loop(case, output);
            if case.input[0] <= 6 {
                assert_eq!(output, methods::loop_output(case));
            }
            if case.input[0] == 4 {
                // Compare the new observed C route against the existing Rust
                // frame loop without changing the old request/fixture layout.
                let old = Case::new("VP8LoopFrame", case.input[1..].to_vec());
                assert_eq!(file, old.rust());
            }
        }
        "WebPEncodeLossyMethod" => {
            methods::check_decoded(case, output);
            assert_eq!(output, methods::public_output(case));
        }
        _ => unreachable!(),
    }
}

#[test]
fn committed_method_references_match_public_api_and_preserve_defaults() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_methods.txt"));
    let cases = methods::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut coverage = [[0; 3]; 7];
    for ((stored, output), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        check_reference(&case, output);
        let (method, kind) = match case.function.as_str() {
            "VP8MethodTools" => (case.input[3], 0),
            "WebPEncodeLossyMethod" => (case.input[5], 1),
            "VP8MethodLoopFrame" => (case.input[0], 2),
            _ => unreachable!(),
        };
        coverage[method as usize][kind] += 1;
    }
    assert!(coverage.iter().flatten().all(|&count| count > 0));
}

#[test]
fn invalid_methods_fail_before_encoder_allocation() {
    use libwebpop::{WebPEncodingError, internal::enc::vp8i_enc::VP8Encoder};
    for method in [i32::MIN, -1, 7, i32::MAX] {
        for (width, height) in [(1, 1), (0, 0), (u32::MAX, u32::MAX)] {
            assert!(matches!(
                VP8Encoder::new_with_method(width, height, true, method),
                Err(WebPEncodingError::InvalidMethod)
            ));
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn public_methods_decode_and_match_pinned_c() {
    let cases = methods::public_cases();
    let counts = native::parallel_cases("lossy-methods-public", &cases, |case, output| {
        assert!(!output.is_empty());
        let stats = methods::check_decoded(case, output);
        native::compare(case, output, &methods::public_output(case));
        stats
    });
    let totals = counts.iter().fold([0; 3], |mut total, stats| {
        for i in 0..3 {
            total[i] += stats[i];
        }
        total
    });
    assert!(totals.into_iter().all(|n| n > 0));
    // Paired outputs prove alpha quality has no effect on opaque inputs and
    // both new and inherited C adapters preserve the simple API at method 4.
    let image = support::lossy::image(17, 19, 0);
    let mut paired = Vec::new();
    for method in 0..=6 {
        for alpha in [0, 99, 100] {
            paired.push(methods::request(&image, 98.5, alpha, method, false));
        }
    }
    let outputs = native::run("lossy-methods-opaque", &paired);
    for group in outputs.chunks_exact(3) {
        assert_eq!(group[0], group[1]);
        assert_eq!(group[0], group[2]);
    }
    let new = methods::request(&image, 98.5, 100, 4, false);
    let old = support::lossy::request(&image, 98.5, None);
    let outputs = native::run("lossy-method4-simple-identity", &[new, old]);
    assert_eq!(outputs[0], outputs[1]);
    eprintln!(
        "{} configurable C files independently decoded; alpha changes/raw/compressed: {totals:?}",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn method_tool_mapping_and_loop_branches_match_pinned_c() {
    let tools = methods::tools_cases();
    let mut rust_mappings = 0;
    for (case, out) in tools.iter().zip(native::run("lossy-methods-tools", &tools)) {
        methods::check_tools(case, &out);
        if case.input[4..6] == [0, 0] {
            native::compare(case, &out, &methods::tools_output(case));
            rust_mappings += 1;
        }
    }
    assert_eq!(rust_mappings, 84);
    let cases = methods::loop_cases();
    let coverage = native::parallel_cases("lossy-methods-loop", &cases, |case, out| {
        check_reference(case, out);
        let tail = 5 + 6 * out[4] as usize;
        let mut coverage = [0; 4];
        for pass in 0..out[4] as usize {
            coverage[0] += out[5 + 6 * pass + 2] as usize;
            coverage[1] += out[5 + 6 * pass + 3] as usize;
        }
        if case.input[0] < 3 {
            coverage[2 + usize::from(out[tail + 2] != 0)] += 1;
        }
        coverage
    });
    let mut by_method = [[0; 4]; 7];
    for (case, coverage) in cases.iter().zip(coverage) {
        for (total, count) in by_method[case.input[0] as usize].iter_mut().zip(coverage) {
            *total += count;
        }
    }
    for (method, counts) in by_method.iter().enumerate() {
        assert!(
            counts[..2].iter().all(|&n| n > 0),
            "method {method}: {counts:?}"
        );
        if method < 3 {
            assert!(
                counts[2..].iter().all(|&n| n > 0),
                "method {method}: {counts:?}"
            );
        }
    }
    let rgba: Vec<_> = cases.iter().filter(|c| c.input[7] != 0).cloned().collect();
    let mut paired = Vec::new();
    for case in &rgba {
        let a = &case.input;
        let mut public = vec![a[1], a[2], a[9], a[3], 100, a[0], 0];
        public.extend_from_slice(&a[10..]);
        paired.push(case.clone());
        paired.push(Case::new("WebPEncodeLossyMethod", public));
    }
    let outputs = native::run("lossy-methods-loop-public", &paired);
    for (cases, outputs) in paired.chunks_exact(2).zip(outputs.chunks_exact(2)) {
        let file = methods::check_loop(&cases[0], &outputs[0]);
        native::compare(&cases[1], &outputs[1], file);
    }
    eprintln!(
        "{} C tool mappings ({rust_mappings} Rust matches); {} loop traces, including {} normal-public-C comparisons",
        tools.len(),
        cases.len(),
        rgba.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn configurable_c_methods_reject_invalid_settings() {
    let image = support::lossy::image(1, 1, 0);
    let valid = methods::request(&image, 75., 100, 4, false);
    let mut cases = Vec::new();
    for quality in [-1., 100.01, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
        let mut case = valid.clone();
        case.input[3] = quality.to_bits();
        cases.push(case);
    }
    for (index, values) in [
        (4, vec![i32::MIN as u32, u32::MAX, 101, i32::MAX as u32]),
        (5, vec![7, i32::MAX as u32, u32::MAX]),
    ] {
        for value in values {
            let mut case = valid.clone();
            case.input[index] = value;
            cases.push(case);
        }
    }
    for out in native::run("lossy-methods-invalid", &cases) {
        assert!(out.is_empty());
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn method_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = methods::fixture_cases();
    let outputs = native::run("lossy-methods-fixtures", &cases);
    for (case, out) in cases.iter().zip(&outputs) {
        check_reference(case, out);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C remaining-lossy-method reference fixtures (milestones 0-1).\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for protocol and provenance.\n",
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
                "/tests/fixtures/lossy_methods.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_methods.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
    eprintln!("{} method fixtures match pinned scalar C", cases.len());
}

use libwebpop::{
    WebPEncodingError as Error,
    internal::{LossyOptions, encode_lossy_rgba},
};

#[test]
fn public_options_validate_before_import_and_preserve_output() {
    use libwebpop::internal::enc::picture_csp_enc::RgbaInputError as Input;
    let mut output = b"keep output".to_vec();
    let before = output.clone();
    for method in [7, 255, i32::MAX as u32, 1 << 31, u32::MAX] {
        let options = LossyOptions {
            method,
            ..Default::default()
        };
        assert!(matches!(
            encode_lossy_rgba(&[], 0, 0, usize::MAX, &options, &mut output),
            Err(Error::InvalidMethod)
        ));
        assert_eq!(output, before);
    }
    for method in 0..=6 {
        for use_sharp_yuv in [false, true] {
            let options = LossyOptions {
                method,
                use_sharp_yuv,
                ..Default::default()
            };
            for quality in [-1., 100.01, f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
                assert!(matches!(
                    encode_lossy_rgba(
                        &[],
                        0,
                        0,
                        0,
                        &LossyOptions { quality, ..options },
                        &mut output
                    ),
                    Err(Error::InvalidQuality)
                ));
                assert_eq!(output, before);
            }
            for alpha_quality in [i32::MIN, -1, 101, i32::MAX] {
                assert!(matches!(
                    encode_lossy_rgba(
                        &[],
                        0,
                        0,
                        0,
                        &LossyOptions {
                            alpha_quality,
                            ..options
                        },
                        &mut output
                    ),
                    Err(Error::InvalidAlphaQuality)
                ));
                assert_eq!(output, before);
            }
            for (pixels, w, h, stride, error) in [
                (&[][..], 0, 1, 4, Input::InvalidDimensions),
                (&[][..], 1, 0, 4, Input::InvalidDimensions),
                (&[][..], 16384, 1, 65536, Input::InvalidDimensions),
                (&[][..], 1, 16384, 4, Input::InvalidDimensions),
                (
                    &[][..],
                    u32::MAX,
                    u32::MAX,
                    usize::MAX,
                    Input::InvalidDimensions,
                ),
                (&[][..], 16383, 16383, 65532, Input::InputTooShort),
                (&[0; 4][..], 1, 1, 3, Input::InvalidStride),
                (&[0; 3][..], 1, 1, 4, Input::InputTooShort),
                (&[][..], 1, 2, usize::MAX, Input::SizeOverflow),
                (&[][..], 1, 3, usize::MAX / 2, Input::SizeOverflow),
            ] {
                assert!(
                    matches!(encode_lossy_rgba(pixels, w, h, stride, &options, &mut output), Err(Error::Input(actual)) if actual == error)
                );
                assert_eq!(output, before);
            }
            for quality in [-0., f32::MIN_POSITIVE, 98.5, 100.] {
                let mut out = Vec::new();
                encode_lossy_rgba(
                    &[1, 2, 3, 255],
                    1,
                    1,
                    usize::MAX,
                    &LossyOptions { quality, ..options },
                    &mut out,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn every_method_preserves_source_minimum_buffers_and_output_capacity() {
    for method in 0..=6 {
        for pattern in [0, 1, 4] {
            let image = support::lossy::image(19, 17, pattern);
            let mut storage = vec![0xa5];
            storage.extend_from_slice(&image.rgba);
            let before = storage.clone();
            let mut packed = Vec::new();
            for y in 0..17 {
                packed.extend_from_slice(&image.rgba[y * image.stride..][..19 * 4]);
            }
            for quality in [30.25, 98.5] {
                for alpha_quality in [71, 100] {
                    for use_sharp_yuv in [false, true] {
                        let options = LossyOptions {
                            quality,
                            method,
                            alpha_quality,
                            use_sharp_yuv,
                        };
                        let mut output = Vec::with_capacity(65536);
                        output.extend_from_slice(b"previous file");
                        let ptr = output.as_ptr();
                        encode_lossy_rgba(
                            &storage[1..],
                            19,
                            17,
                            image.stride,
                            &options,
                            &mut output,
                        )
                        .unwrap();
                        assert_eq!(output.as_ptr(), ptr);
                        assert_eq!(storage, before);
                        let mut expected = Vec::new();
                        encode_lossy_rgba(&packed, 19, 17, 19 * 4, &options, &mut expected)
                            .unwrap();
                        assert_eq!(output, expected);
                        let mut surplus = packed.clone();
                        surplus.extend_from_slice(&[0x5a; 7]);
                        encode_lossy_rgba(&surplus, 19, 17, 19 * 4, &options, &mut expected)
                            .unwrap();
                        assert_eq!(output, expected);
                        let case =
                            methods::request(&image, quality, alpha_quality, method, use_sharp_yuv);
                        methods::check_decoded(
                            &case,
                            &output.into_iter().map(u32::from).collect::<Vec<_>>(),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn public_defaults_preserve_both_method4_convenience_apis() {
    assert_eq!(
        LossyOptions::default(),
        LossyOptions {
            quality: 75.,
            method: 4,
            alpha_quality: 100,
            use_sharp_yuv: false
        }
    );
    for pattern in 0..6 {
        let image = support::lossy::image(17, 19, pattern);
        for quality in [0., 1., 30., 50., 75., 98., 98.5, 99., 100.] {
            let mut standard = Vec::new();
            libwebpop::WebPEncodeRGBA(&image.rgba, 17, 19, image.stride, quality, &mut standard)
                .unwrap();
            let mut custom = Vec::new();
            libwebpop::internal::WebPEncodeRGBAWithAlphaQuality(
                &image.rgba,
                17,
                19,
                image.stride,
                quality,
                100,
                &mut custom,
            )
            .unwrap();
            let mut options = Vec::new();
            encode_lossy_rgba(
                &image.rgba,
                17,
                19,
                image.stride,
                &LossyOptions {
                    quality,
                    ..Default::default()
                },
                &mut options,
            )
            .unwrap();
            assert_eq!(standard, custom);
            assert_eq!(standard, options);
        }
    }
}

#[test]
fn prepared_and_sequence_frames_carry_lossy_method() {
    use libwebpop::internal::*;
    let image = support::lossy::image(17, 19, 4);
    for method in 0..=6 {
        let options = LossyOptions {
            method,
            alpha_quality: 71,
            use_sharp_yuv: method & 1 != 0,
            ..Default::default()
        };
        let mut still = Vec::new();
        encode_lossy_rgba(&image.rgba, 17, 19, image.stride, &options, &mut still).unwrap();
        let frame =
            EncodedFrame::encode_lossy_rgba(&image.rgba, 17, 19, image.stride, &options).unwrap();
        let mut container = AnimationContainer::new(17, 19, Default::default()).unwrap();
        container.push_frame(frame, Default::default()).unwrap();
        let mut output = Vec::new();
        container.assemble(&mut output).unwrap();
        assert_eq!(output, still);
        let mut sequence = AnimationEncoder::new(17, 19, Default::default()).unwrap();
        sequence
            .add_rgba(
                &image.rgba,
                image.stride,
                0,
                &AnimationFrameOptions::Lossy(options),
            )
            .unwrap();
        sequence.finish(100, &mut output).unwrap();
        assert_eq!(output, still);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn public_methods_match_c_on_seeded_and_fractional_frame_boundaries() {
    let mut cases = Vec::new();
    let mut seed = support::SEED;
    for method in 0..=6 {
        for _ in 0..32 {
            let w = 1 + support::random(&mut seed) as usize % 67;
            let h = 1 + support::random(&mut seed) as usize % 67;
            let pattern = support::random(&mut seed) % 36;
            let image = support::public_images::image(w, h, pattern);
            let quality = (support::random(&mut seed) % 100001) as f32 / 1000.;
            let alpha = (support::random(&mut seed) % 101) as i32;
            cases.push(methods::request(&image, quality, alpha, method, false));
        }
        for (w, h) in [
            (15, 16),
            (16, 16),
            (31, 33),
            (65, 49),
            (1537, 17),
            (257, 769),
        ] {
            let image = support::public_images::image(w, h, 35);
            for quality in [75., 98.5] {
                cases.push(methods::request(&image, quality, 71, method, false));
            }
        }
        let image = support::lossy::image(33, 17, 4);
        for quality in [0f32, 30., 75., 98., 100.] {
            for bits in [
                quality.to_bits().saturating_sub(1),
                quality.to_bits(),
                quality.to_bits() + 1,
            ] {
                let quality = f32::from_bits(bits);
                if quality <= 100. {
                    cases.push(methods::request(&image, quality, 99, method, false));
                }
            }
        }
    }
    native::parallel_cases("lossy-methods-public-extended", &cases, |case, out| {
        native::compare(case, out, &case.rust());
        methods::check_decoded(case, out);
    });
    eprintln!(
        "{} public method files match C at seeded/fractional/frame boundaries",
        cases.len()
    );
}
