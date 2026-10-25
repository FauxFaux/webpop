#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::enc::picture_csp_enc::{import_rgba_to_yuva, import_rgba_to_yuva_sharp};
use libwebpop::{
    WebPEncodeRGBAWithAlphaQuality,
    internal::{LossyOptions, encode_lossy_rgba},
};
use support::sharpyuv_enc as sharp;

#[test]
fn sharp_yuv_encoder_committed_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/sharpyuv_encoder.txt"));
    let cases = sharp::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(case.rust(), *expected);
        if case.function == "WebPEncodeLossyOptions" {
            sharp::check_decoded(&case, expected);
        }
    }
}

#[test]
fn sharp_yuv_encoder_fallback_and_alpha_semantics() {
    let mut changed = 0;
    for (w, h) in [(1, 17), (3, 5), (4, 3), (4, 4), (5, 7), (17, 19)] {
        for pattern in 0..6 {
            let image = support::lossy::image(w, h, pattern);
            let ordinary =
                import_rgba_to_yuva(&image.rgba, w as u32, h as u32, image.stride).unwrap();
            let sharp =
                import_rgba_to_yuva_sharp(&image.rgba, w as u32, h as u32, image.stride).unwrap();
            assert_eq!(ordinary.a, sharp.a);
            if w < 4 || h < 4 {
                assert_eq!(ordinary, sharp);
            } else {
                if ordinary.y != sharp.y || ordinary.u != sharp.u || ordinary.v != sharp.v {
                    changed += 1;
                }
                let mut opaque = image.clone();
                for y in 0..h {
                    for x in 0..w {
                        opaque.rgba[y * image.stride + 4 * x + 3] = 255;
                    }
                }
                let q = import_rgba_to_yuva_sharp(&opaque.rgba, w as u32, h as u32, image.stride)
                    .unwrap();
                assert_eq!((&sharp.y, &sharp.u, &sharp.v), (&q.y, &q.u, &q.v));
                assert!(q.a.is_none());
            }
        }
    }
    assert!(changed > 0);
}

#[test]
fn sharp_yuv_encoder_options_preserve_defaults_and_output_guarantees() {
    let image = support::lossy::image(17, 19, 4);
    let mut reused = Vec::with_capacity(65536);
    reused.extend_from_slice(b"old output");
    let ptr = reused.as_ptr();
    for use_sharp_yuv in [false, true] {
        let options = LossyOptions {
            quality: 98.5,
            method: 4,
            alpha_quality: 30,
            use_sharp_yuv,
        };
        encode_lossy_rgba(&image.rgba, 17, 19, image.stride, &options, &mut reused).unwrap();
        assert_eq!(reused.as_ptr(), ptr);
        if !use_sharp_yuv {
            let mut expected = Vec::new();
            WebPEncodeRGBAWithAlphaQuality(
                &image.rgba,
                17,
                19,
                image.stride,
                98.5,
                30,
                &mut expected,
            )
            .unwrap();
            assert_eq!(expected, reused);
        }
        let before = reused.clone();
        for invalid in [
            LossyOptions {
                quality: f32::NAN,
                ..options
            },
            LossyOptions {
                quality: 101.,
                ..options
            },
            LossyOptions {
                alpha_quality: -1,
                ..options
            },
            LossyOptions {
                alpha_quality: 101,
                ..options
            },
        ] {
            assert!(encode_lossy_rgba(&[], 17, 19, image.stride, &invalid, &mut reused).is_err());
            assert_eq!(reused, before);
        }
        assert!(encode_lossy_rgba(&[], 0, 19, image.stride, &options, &mut reused).is_err());
        assert_eq!(reused, before);
        assert!(
            encode_lossy_rgba(
                &image.rgba[..image.rgba.len() - 1],
                17,
                19,
                image.stride,
                &options,
                &mut reused
            )
            .is_err()
        );
        assert_eq!(reused, before);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharp_yuv_encoder_matches_pinned_c() {
    let imports = sharp::import_cases();
    native::parallel_cases("sharp-yuv-import", &imports, |case, out| {
        native::compare(case, out, &case.rust())
    });
    let cases = sharp::public_cases();
    native::parallel_cases("sharp-yuv-encoder", &cases, |case, out| {
        let actual = case.rust();
        native::compare(case, out, &actual);
        sharp::check_decoded(case, &actual);
        let bytes: Vec<_> = actual.iter().map(|&v| v as u8).collect();
        let reference: Vec<_> = out.iter().map(|&v| v as u8).collect();
        assert_eq!(
            image::load_from_memory(&bytes).unwrap().into_rgba8(),
            image::load_from_memory(&reference).unwrap().into_rgba8()
        );
    });
    eprintln!(
        "{} exact sharp YUVA imports; {} complete files and independent decodes",
        imports.len(),
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn sharp_yuv_encoder_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = sharp::fixture_cases();
    let outputs = native::run("sharp-yuv-encoder-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar SharpYUV encoder integration.\n# upstream={} seed={:08x}\n# See ../reference/SHARPYUV.md.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (case, output) in cases.iter().zip(&outputs) {
            text.push_str(&case.function);
            for v in &case.input {
                write!(text, " {v:08x}").unwrap();
            }
            text.push_str(" |");
            for v in output {
                write!(text, " {v:08x}").unwrap();
            }
            text.push('\n');
        }
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/sharpyuv_encoder.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/sharpyuv_encoder.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
    for (case, out) in cases.iter().zip(&outputs) {
        native::compare(case, out, &case.rust());
    }
}
