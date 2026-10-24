use libwebpop::{WebPEncodeRGBA as encode, WebPEncodeRGBAWithAlphaQuality as encode_alpha};

#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use support::{ImageCase, lossy};

fn decode(bytes: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8()
}

fn real_fixture_cases() -> Vec<(support::Case, &'static str)> {
    let images = baseline_images();
    vec![
        (lossy::request(&images[3], 75., None), "test_ref.lossy.webp"),
        (
            lossy::request(&images[4], 75., Some(70)),
            "test_ref.alpha.webp",
        ),
    ]
}

#[test]
fn committed_real_vp8_and_alpha_files_match_public_apis() {
    for (case, name) in real_fixture_cases() {
        let bytes = std::fs::read(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/corpus/").to_owned() + name,
        )
        .unwrap();
        let expected: Vec<_> = bytes.into_iter().map(u32::from).collect();
        assert_eq!(case.rust(), expected);
        lossy::check_decoded(&case, &expected);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn real_public_fixtures_match_pinned_c() {
    let fixtures = real_fixture_cases();
    let cases: Vec<_> = fixtures.iter().map(|(case, _)| case.clone()).collect();
    let outputs = native::run("lossy-public-real-fixtures", &cases);
    let update =
        std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1"));
    for ((case, name), expected) in fixtures.iter().zip(outputs) {
        native::compare(case, &expected, &case.rust());
        lossy::check_decoded(case, &expected);
        let bytes: Vec<_> = expected
            .into_iter()
            .map(|v| u8::try_from(v).unwrap())
            .collect();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/corpus")
            .join(name);
        if update {
            std::fs::write(path, bytes).unwrap();
        } else {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
}

#[test]
fn public_quality_and_geometry_validation() {
    use libwebpop::{WebPEncodingError as Error, enc::picture_csp_enc::RgbaInputError as Input};
    for q in [
        -1.,
        -f32::MIN_POSITIVE,
        100.01,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
        f32::from_bits(0xffc00001),
    ] {
        assert!(matches!(
            encode(&[0; 4], 1, 1, 4, q),
            Err(Error::InvalidQuality)
        ));
        assert!(matches!(
            encode_alpha(&[0; 4], 1, 1, 4, q, 100),
            Err(Error::InvalidQuality)
        ));
    }
    // Alpha settings are checked even for opaque input.
    for aq in [i32::MIN, -1, 101, i32::MAX] {
        assert!(matches!(
            encode_alpha(&[255; 4], 1, 1, 4, 75., aq),
            Err(Error::InvalidAlphaQuality)
        ));
    }
    for (pixels, w, h, stride, expected) in [
        (&[][..], 0, 1, 4, Input::InvalidDimensions),
        (&[][..], 1, 0, 4, Input::InvalidDimensions),
        (&[][..], 16384, 1, 65536, Input::InvalidDimensions),
        (&[][..], 1, 16384, 4, Input::InvalidDimensions),
        (&[][..], u32::MAX, 1, usize::MAX, Input::InvalidDimensions),
        (&[][..], 1, u32::MAX, 4, Input::InvalidDimensions),
        (&[][..], 16383, 16383, 65532, Input::InputTooShort),
        (&[0; 4][..], 1, 1, 3, Input::InvalidStride),
        (&[0; 3][..], 1, 1, 4, Input::InputTooShort),
        (&[][..], 1, 2, usize::MAX, Input::SizeOverflow),
        (&[][..], 1, 3, usize::MAX / 2, Input::SizeOverflow),
    ] {
        assert!(
            matches!(encode(pixels, w, h, stride, 75.), Err(Error::Input(actual)) if actual == expected)
        );
        for aq in [0, 71, 100] {
            assert!(
                matches!(encode_alpha(pixels, w, h, stride, 75., aq), Err(Error::Input(actual)) if actual == expected)
            );
        }
    }
    // A single row does not consume the stride, including usize::MAX.
    for q in [-0., 0., f32::MIN_POSITIVE, 98.5, 100.] {
        assert!(encode(&[1, 2, 3, 255], 1, 1, usize::MAX, q).is_ok());
    }
    for (w, h) in [(1, 16383), (16383, 1)] {
        let image = lossy::image(w, h, 3);
        let bytes = encode(&image.rgba, image.width, image.height, image.stride, 75.).unwrap();
        let decoded = decode(&bytes);
        assert_eq!(decoded.dimensions(), (w as u32, h as u32));
        for y in 0..h {
            for x in 0..w {
                assert_eq!(
                    decoded.get_pixel(x as u32, y as u32)[3],
                    image.rgba[y * image.stride + 4 * x + 3]
                );
            }
        }
    }
}

#[test]
fn padding_unaligned_input_and_transparent_cleanup_preserve_source() {
    for pattern in 0..6 {
        let image = lossy::image(19, 17, pattern);
        let mut packed = Vec::new();
        for y in 0..image.height as usize {
            packed.extend_from_slice(&image.rgba[y * image.stride..][..image.width as usize * 4]);
        }
        let mut unaligned = vec![0xa5];
        unaligned.extend_from_slice(&image.rgba);
        let original = unaligned.clone();
        for q in [0., 75., 98.5, 100.] {
            for aq in [0, 71, 100] {
                let expected = encode_alpha(&unaligned[1..], 19, 17, image.stride, q, aq).unwrap();
                assert_eq!(
                    expected,
                    encode_alpha(&packed, 19, 17, 19 * 4, q, aq).unwrap()
                );
                let mut surplus = packed.clone();
                surplus.extend([0x5a; 7]);
                assert_eq!(
                    expected,
                    encode_alpha(&surplus, 19, 17, 19 * 4, q, aq).unwrap()
                );
                assert_eq!(unaligned, original);
            }
        }
    }
}

#[test]
fn standard_and_custom_100_agree_and_opaque_output_ignores_alpha_quality() {
    let mut fractional_changes = 0;
    let mut rgb_changes_at_100 = 0;
    for pattern in 0..6 {
        let image = lossy::image(17, 19, pattern);
        for q in [0., 1., 30., 50., 75., 98., 98.5, 99., 100.] {
            let standard = encode(&image.rgba, 17, 19, image.stride, q).unwrap();
            assert_eq!(
                standard,
                encode_alpha(&image.rgba, 17, 19, image.stride, q, 100).unwrap()
            );
            let decoded = decode(&standard);
            for y in 0..19 {
                for x in 0..17 {
                    let pixel = &image.rgba[y * image.stride + 4 * x..][..4];
                    let actual = decoded.get_pixel(x as u32, y as u32);
                    assert_eq!(actual[3], pixel[3]);
                    rgb_changes_at_100 +=
                        usize::from(q == 100. && pixel[3] != 0 && actual.0[..3] != pixel[..3]);
                }
            }
            if pattern == 0 {
                for aq in 0..=100 {
                    assert_eq!(
                        standard,
                        encode_alpha(&image.rgba, 17, 19, image.stride, q, aq).unwrap()
                    );
                }
            }
            if q == 98.5 {
                fractional_changes += usize::from(
                    standard != encode(&image.rgba, 17, 19, image.stride, 98.).unwrap(),
                );
            }
        }
    }
    assert!(fractional_changes > 0);
    assert!(
        rgb_changes_at_100 > 0,
        "quality 100 still quantizes YUV420 color"
    );
}

fn extended_cases() -> Vec<support::Case> {
    // Reuse real-image alpha and dimension-limit cases already measured at the
    // internal milestone, now through the exported custom API.
    let inherited = lossy::public_cases()
        .iter()
        .filter(|c| c.function == "WebPEncodeRGBAWithAlphaQuality")
        .count();
    let mut cases: Vec<_> = support::vp8_alpha::file_cases()
        .into_iter()
        .skip(inherited)
        .map(|mut c| {
            c.function = "WebPEncodeRGBAWithAlphaQuality".into();
            c
        })
        .collect();
    let mut seed = support::SEED;
    for _ in 0..64 {
        let w = 1 + support::random(&mut seed) as usize % 67;
        let h = 1 + support::random(&mut seed) as usize % 67;
        let pattern = support::random(&mut seed) % 36;
        let image = support::public_images::image(w, h, pattern);
        let q = (support::random(&mut seed) % 100001) as f32 / 1000.;
        let aq = (support::random(&mut seed) % 101) as i32;
        cases.push(lossy::request(&image, q, None));
        cases.push(lossy::request(&image, q, Some(aq)));
    }
    let image = lossy::image(17, 19, 4);
    for q in 0..=100 {
        cases.push(lossy::request(&image, q as f32, Some(q)));
    }
    // Adjacent f32 settings around the chroma diffusion cutoff and endpoints.
    for q in [0f32, 75., 98., 100.] {
        for bits in [q.to_bits().saturating_sub(1), q.to_bits(), q.to_bits() + 1] {
            let q = f32::from_bits(bits);
            if q <= 100. {
                cases.push(lossy::request(&image, q, None));
            }
        }
    }
    // Public frames span probability refreshes, including the growing interval.
    for (w, h) in [(1537, 17), (257, 769)] {
        for pattern in [0, 2, 35] {
            let image = support::public_images::image(w, h, pattern);
            for q in [75., 98.5] {
                cases.push(lossy::request(&image, q, Some(30)));
            }
        }
    }
    for image in baseline_images() {
        for q in [30., 75., 98.5, 100.] {
            for aq in [0, 100] {
                cases.push(lossy::request(&image, q, Some(aq)));
            }
        }
    }
    cases
}

#[test]
fn public_extended_corpus_decodes() {
    for case in extended_cases().iter().step_by(7) {
        let output = case.rust();
        lossy::check_decoded(case, &output);
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn extended_public_apis_match_pinned_c() {
    let cases = extended_cases();
    let stats = native::parallel_cases("lossy-public-extended", &cases, |case, expected| {
        let actual = case.rust();
        native::compare(case, expected, &actual);
        let stats = lossy::check_decoded(case, &actual);
        let reference: Vec<_> = expected.iter().map(|&v| v as u8).collect();
        let actual: Vec<_> = actual.iter().map(|&v| v as u8).collect();
        assert_eq!(decode(&actual), decode(&reference));
        stats
    });
    eprintln!(
        "{} additional public Rust/C byte matches and independent decodes; {} alpha changes",
        cases.len(),
        stats.iter().map(|s| s[0]).sum::<usize>()
    );
}

fn baseline_images() -> Vec<ImageCase> {
    let real = support::public_images::real_image();
    let mut alpha = real.clone();
    alpha.name = "real with synthetic alpha";
    for y in 0..alpha.height as usize {
        for x in 0..alpha.width as usize {
            alpha.rgba[y * alpha.stride + 4 * x + 3] = ((x * 17 + y * 13) & 255) as u8;
        }
    }
    vec![
        support::public_images::image(128, 128, 0),
        support::public_images::image(128, 128, 2),
        support::public_images::image(128, 128, 24),
        real,
        alpha,
    ]
}

#[test]
#[ignore = "manual release size/quality/time baseline; timings are not assertions"]
fn public_lossy_baseline() {
    use std::{hint::black_box, time::Instant};
    let selected = std::env::var("WEBPOP_LOSSY_BASELINE_CASE")
        .ok()
        .map(|s| s.parse::<usize>().unwrap());
    let mut index = 0;
    eprintln!(
        "case,image,width,height,q,alpha_q,bytes,median_ms,visible_rgb_psnr_db,alpha_psnr_db"
    );
    for (image_index, image) in baseline_images().into_iter().enumerate() {
        for q in [30., 75., 98.5, 100.] {
            let opaque = (0..image.height as usize).all(|y| {
                (0..image.width as usize).all(|x| image.rgba[y * image.stride + 4 * x + 3] == 255)
            });
            for aq in if opaque { &[100][..] } else { &[0, 100][..] } {
                let case_index = index;
                index += 1;
                if selected.is_some_and(|i| i != case_index) {
                    continue;
                }
                let mut timings = Vec::new();
                let mut encoded = Vec::new();
                for iteration in 0..6 {
                    let start = Instant::now();
                    encoded = encode_alpha(
                        black_box(&image.rgba),
                        image.width,
                        image.height,
                        image.stride,
                        q,
                        *aq,
                    )
                    .unwrap();
                    let elapsed = start.elapsed();
                    if iteration != 0 {
                        timings.push(elapsed);
                    }
                    black_box(&encoded);
                }
                timings.sort();
                let decoded = decode(&encoded);
                let (mut rgb_sse, mut alpha_sse, mut visible) = (0u64, 0u64, 0u64);
                for y in 0..image.height as usize {
                    for x in 0..image.width as usize {
                        let p = &image.rgba[y * image.stride + 4 * x..][..4];
                        let d = decoded.get_pixel(x as u32, y as u32);
                        let square = |c: usize| (i64::from(p[c]) - i64::from(d[c])).pow(2) as u64;
                        alpha_sse += square(3);
                        if p[3] != 0 {
                            visible += 3;
                            rgb_sse += square(0) + square(1) + square(2);
                        }
                    }
                }
                let psnr = |sse: u64, count: u64| {
                    if sse == 0 {
                        f64::INFINITY
                    } else {
                        10. * (255. * 255. * count as f64 / sse as f64).log10()
                    }
                };
                eprintln!(
                    "{case_index},{image_index},{},{},{q},{aq},{},{:.3},{:.3},{:.3}",
                    image.width,
                    image.height,
                    encoded.len(),
                    timings[2].as_secs_f64() * 1000.,
                    psnr(rgb_sse, visible),
                    psnr(alpha_sse, u64::from(image.width) * u64::from(image.height))
                );
            }
        }
    }
}
