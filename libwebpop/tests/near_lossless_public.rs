#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
#[path = "support/near_lossless.rs"]
mod preprocessing;
#[path = "support/near_lossless_public.rs"]
mod public;
mod support;

#[test]
fn committed_near_lossless_reference_files_decode_and_100_matches_lossless() {
    let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_public.txt"));
    let cases = public::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    let mut changes = (0, 0);
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        let changed = public::check_reference(case, expected);
        changes.0 += changed.0;
        changes.1 += changed.1;
    }
    assert!(
        changes.0 > 0 && changes.1 > 0,
        "changed RGB/alpha: {changes:?}"
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn near_lossless_public_reference_corpus() {
    let cases = public::cases();
    let outputs = native::run("near-lossless-public", &cases);
    let mut changes = (0, 0);
    // Inputs are consecutive strength runs for each image. Bucket equality is
    // a pinned mapping property, not a monotonicity assumption about file size.
    for (index, (case, output)) in cases.iter().zip(&outputs).enumerate() {
        let changed = public::check_reference(case, output);
        changes.0 += changed.0;
        changes.1 += changed.1;
        if index > 0
            && case.input[3] / 20 == cases[index - 1].input[3] / 20
            && case.input[..3] == cases[index - 1].input[..3]
            && case.input[4..] == cases[index - 1].input[4..]
        {
            native::compare(case, &outputs[index - 1], output);
        }
    }
    assert!(
        changes.0 > 0 && changes.1 > 0,
        "changed RGB/alpha: {changes:?}"
    );
    let error_case = support::Case::new("NearLosslessWrapperErrors", vec![]);
    assert_eq!(
        native::run("near-lossless-wrapper-errors", &[error_case]),
        [vec![1; 5]]
    );
    eprintln!(
        "Independently decoded {} near-lossless C files; strength 100 matches Rust lossless",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn near_lossless_public_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = public::fixture_cases();
    let outputs = native::run("near-lossless-public-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        public::check_reference(case, output);
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C custom public near-lossless wrapper fixtures.\n# upstream={} seed={:08x}\n# See ../reference/NEAR_LOSSLESS.md for wrapper and fixture provenance.\n",
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
                "/tests/fixtures/near_lossless_public.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = preprocessing::fixtures(include_str!("fixtures/near_lossless_public.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}

#[test]
fn public_strength_and_geometry_validation() {
    use libwebpop::enc::picture_csp_enc::RgbaInputError;
    use libwebpop::{LosslessEncodeError, WebPEncodeRGBAWithNearLossless as encode};
    for strength in [i32::MIN, -1, 101, i32::MAX] {
        assert!(matches!(
            encode(&[1, 2, 3, 255], 1, 1, 4, strength),
            Err(LosslessEncodeError::InvalidNearLosslessStrength)
        ));
    }
    for strength in [0, 99, 100] {
        for (pixels, w, h, stride, expected) in [
            (&[][..], 0, 1, 4, RgbaInputError::InvalidDimensions),
            (&[][..], 1, 0, 4, RgbaInputError::InvalidDimensions),
            (&[][..], 16384, 1, 65536, RgbaInputError::InvalidDimensions),
            (&[][..], 1, 16384, 4, RgbaInputError::InvalidDimensions),
            (
                &[][..],
                u32::MAX,
                1,
                usize::MAX,
                RgbaInputError::InvalidDimensions,
            ),
            (&[][..], 16383, 16383, 65532, RgbaInputError::InputTooShort),
            (&[0; 4][..], 1, 1, 3, RgbaInputError::InvalidStride),
            (&[0; 3][..], 1, 1, 4, RgbaInputError::InputTooShort),
            (&[][..], 1, 2, usize::MAX, RgbaInputError::SizeOverflow),
            (&[][..], 1, 3, usize::MAX / 2, RgbaInputError::SizeOverflow),
        ] {
            assert!(matches!(encode(pixels, w, h, stride, strength),
                Err(LosslessEncodeError::Input(actual)) if actual == expected));
        }
        assert!(encode(&[1, 2, 3, 255], 1, 1, usize::MAX, strength).is_ok());
        for (w, h) in [(1, 16383), (16383, 1)] {
            let pixels = vec![255; w as usize * h as usize * 4];
            let bytes = encode(&pixels, w, h, w as usize * 4, strength).unwrap();
            let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            assert_eq!(decoded.dimensions(), (w, h));
            assert_eq!(decoded.as_raw(), &pixels);
        }
    }
}

#[test]
fn strengths_share_buckets_and_ignore_padding_without_mutating_input() {
    use libwebpop::WebPEncodeRGBAWithNearLossless as encode;
    for pattern in [2, 9, 12, 24] {
        let image = support::public_images::image(19, 17, pattern);
        let mut packed = Vec::new();
        for y in 0..image.height as usize {
            packed.extend_from_slice(&image.rgba[y * image.stride..][..image.width as usize * 4]);
        }
        let original = image.rgba.clone();
        let mut previous = Vec::new();
        for strength in 0..=100 {
            let bytes = encode(
                &image.rgba,
                image.width,
                image.height,
                image.stride,
                strength,
            )
            .unwrap();
            assert_eq!(
                bytes,
                encode(
                    &packed,
                    image.width,
                    image.height,
                    image.width as usize * 4,
                    strength
                )
                .unwrap()
            );
            if strength % 20 != 0 {
                assert_eq!(bytes, previous);
            }
            if strength == 100 {
                assert_eq!(
                    bytes,
                    libwebpop::WebPEncodeLosslessRGBA(
                        &image.rgba,
                        image.width,
                        image.height,
                        image.stride
                    )
                    .unwrap()
                );
            }
            previous = bytes;
        }
        assert_eq!(image.rgba, original);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
#[ignore = "manual full real-image corpus parity, timing and process-memory record"]
fn real_image_corpus_parity_and_metrics() {
    use std::{path::PathBuf, process::Command, time::Instant};
    let build = native::build_dir();
    let status = Command::new("cmake")
        .arg("--build")
        .arg(build)
        .args([
            "--target",
            "webpop_near_lossless_file",
            "--config",
            "Release",
            "--parallel",
            "4",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let mut helper = build.join(format!(
        "webpop_near_lossless_file{}",
        std::env::consts::EXE_SUFFIX
    ));
    if !helper.is_file() {
        helper = build.join("Release").join(helper.file_name().unwrap());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let run = build.join("near-lossless-corpus");
    std::fs::create_dir_all(&run).unwrap();
    let mut record = String::from(
        "image,width,height,strength,bytes,rust_ms,max_r,max_g,max_b,max_a,process_peak_kib\n",
    );
    for name in [
        "graphic-alpha-transparency.webp",
        "graphic-invite.webp",
        "graphic-map.webp",
        "photo-japan-p50.webp",
        "screenshot-wikipedia.webp",
    ] {
        let source = image::load_from_memory_with_format(
            &std::fs::read(root.join("samples").join(name)).unwrap(),
            image::ImageFormat::WebP,
        )
        .unwrap()
        .into_rgba8();
        let (w, h) = source.dimensions();
        let input = run.join(format!("{name}.rgba"));
        std::fs::write(&input, source.as_raw()).unwrap();
        for strength in [0, 20, 40, 60, 80, 100] {
            let output = run.join(format!("{name}.{strength}.webp"));
            let result = Command::new(&helper)
                .arg(&input)
                .arg(w.to_string())
                .arg(h.to_string())
                .arg(strength.to_string())
                .arg(&output)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{name} {strength}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let start = Instant::now();
            let actual = libwebpop::WebPEncodeRGBAWithNearLossless(
                source.as_raw(),
                w,
                h,
                w as usize * 4,
                strength,
            )
            .unwrap();
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            let reference = std::fs::read(&output).unwrap();
            assert!(
                actual == reference,
                "{name} strength {strength}, first differing byte {:?}",
                actual.iter().zip(&reference).position(|(a, b)| a != b)
            );
            let decoded = image::load_from_memory_with_format(&actual, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            let decoded_reference =
                image::load_from_memory_with_format(&reference, image::ImageFormat::WebP)
                    .unwrap()
                    .into_rgba8();
            assert_eq!(decoded.dimensions(), (w, h));
            assert!(decoded == decoded_reference);
            let mut errors = [0u8; 4];
            for (a, b) in source.pixels().zip(decoded.pixels()) {
                if a[3] == 0 || a[3] == 255 {
                    assert_eq!(a[3], b[3]);
                }
                for lane in 0..4 {
                    if lane == 3 || (a[3] != 0 && b[3] != 0) {
                        errors[lane] = errors[lane].max(a[lane].abs_diff(b[lane]));
                    }
                }
            }
            if strength == 100 {
                assert_eq!(errors, [0; 4]);
                assert!(
                    actual
                        == libwebpop::WebPEncodeLosslessRGBA(source.as_raw(), w, h, w as usize * 4)
                            .unwrap()
                );
            }
            // Linux process high-water RSS includes input, both encoded files,
            // independent decoding and test infrastructure, not just encoding.
            let peak = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find_map(|line| line.strip_prefix("VmHWM:").map(str::to_owned))
                })
                .unwrap_or_default();
            let peak = peak.split_whitespace().next().unwrap_or("");
            let line = format!(
                "{name},{w},{h},{strength},{},{elapsed:.3},{},{},{},{},{peak}\n",
                actual.len(),
                errors[0],
                errors[1],
                errors[2],
                errors[3]
            );
            eprint!("{line}");
            record.push_str(&line);
            std::fs::write(run.join("metrics.csv"), &record).unwrap();
        }
    }
}

#[test]
fn configurable_near_lossless_validates_options_and_matches_existing_apis() {
    use libwebpop::{LosslessEncodeError, LosslessOptions, encode_near_lossless_rgba};
    let pixels = [127, 129, 131, 255];
    for strength in [i32::MIN, -1, 101, i32::MAX] {
        assert!(matches!(
            encode_near_lossless_rgba(&pixels, 1, 1, 4, strength, &LosslessOptions::default()),
            Err(LosslessEncodeError::InvalidNearLosslessStrength)
        ));
    }
    for strength in [0, 100] {
        assert!(matches!(
            encode_near_lossless_rgba(
                &pixels,
                1,
                1,
                4,
                strength,
                &LosslessOptions {
                    quality: 101,
                    ..Default::default()
                }
            ),
            Err(LosslessEncodeError::InvalidQuality)
        ));
        assert!(matches!(
            encode_near_lossless_rgba(
                &pixels,
                1,
                1,
                4,
                strength,
                &LosslessOptions {
                    method: 7,
                    ..Default::default()
                }
            ),
            Err(LosslessEncodeError::InvalidMethod)
        ));
        assert!(matches!(
            encode_near_lossless_rgba(&pixels[..3], 1, 1, 4, strength, &LosslessOptions::default()),
            Err(LosslessEncodeError::Input(_))
        ));
    }
    for pattern in [2, 9, 12, 24] {
        let image = support::public_images::image(65, 5, pattern);
        for strength in [0, 60, 100] {
            assert_eq!(
                encode_near_lossless_rgba(
                    &image.rgba,
                    image.width,
                    image.height,
                    image.stride,
                    strength,
                    &LosslessOptions::default()
                )
                .unwrap(),
                libwebpop::WebPEncodeRGBAWithNearLossless(
                    &image.rgba,
                    image.width,
                    image.height,
                    image.stride,
                    strength
                )
                .unwrap()
            );
        }
        for options in [
            LosslessOptions {
                quality: 75,
                ..Default::default()
            },
            LosslessOptions {
                quality: 90,
                method: 6,
                exact: true,
            },
            LosslessOptions {
                quality: 0,
                method: 0,
                exact: false,
            },
        ] {
            assert_eq!(
                encode_near_lossless_rgba(
                    &image.rgba,
                    image.width,
                    image.height,
                    image.stride,
                    100,
                    &options
                )
                .unwrap(),
                libwebpop::encode_lossless_rgba(
                    &image.rgba,
                    image.width,
                    image.height,
                    image.stride,
                    &options
                )
                .unwrap()
            );
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn near_lossless_options_match_pinned_reference() {
    use libwebpop::{LosslessOptions, encode_near_lossless_rgba};
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (19, 17), (65, 5), (65, 67)] {
        for pattern in [2, 9, 12, 24] {
            let image = support::public_images::image(w, h, pattern);
            for (quality, method, exact) in [
                (0, 0, false),
                (70, 4, false),
                (75, 4, false),
                (90, 6, false),
                (100, 6, false),
                (75, 4, true),
                (100, 6, true),
            ] {
                for strength in [0, 20, 60, 99, 100] {
                    let mut a = vec![
                        image.width,
                        image.height,
                        image.stride as u32,
                        quality,
                        method,
                        u32::from(exact),
                        strength,
                    ];
                    a.extend(image.rgba.iter().map(|&b| u32::from(b)));
                    cases.push(support::Case::new("WebPEncodeNearLosslessOptions", a));
                }
            }
        }
    }
    let outputs = native::run("near-lossless-public-options", &cases);
    for (case, expected) in cases.iter().zip(outputs) {
        let a = &case.input;
        let mut allocation = vec![0xa5];
        allocation.extend(a[7..].iter().map(|&b| b as u8));
        let before = allocation.clone();
        let options = LosslessOptions {
            quality: a[3],
            method: a[4],
            exact: a[5] != 0,
        };
        let actual = encode_near_lossless_rgba(
            &allocation[1..],
            a[0],
            a[1],
            a[2] as usize,
            a[6] as i32,
            &options,
        )
        .unwrap();
        assert_eq!(allocation, before);
        native::compare(
            case,
            &expected,
            &actual.iter().map(|&b| u32::from(b)).collect::<Vec<_>>(),
        );
        let decoded = image::load_from_memory_with_format(&actual, image::ImageFormat::WebP)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.dimensions(), (a[0], a[1]));
        for y in 0..a[1] as usize {
            for x in 0..a[0] as usize {
                let alpha = allocation[1 + y * a[2] as usize + x * 4 + 3];
                if alpha == 0 || alpha == 255 {
                    assert_eq!(decoded.get_pixel(x as u32, y as u32).0[3], alpha);
                }
            }
        }
        if a[6] == 100 {
            assert_eq!(
                actual,
                libwebpop::encode_lossless_rgba(
                    &allocation[1..],
                    a[0],
                    a[1],
                    a[2] as usize,
                    &options
                )
                .unwrap()
            );
        }
    }
    eprintln!(
        "{} configurable near-lossless public C comparisons",
        cases.len()
    );
}
