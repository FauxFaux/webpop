use super::{Case, ImageCase, SEED, random};
use libwebpop::dsp::enc::{BPS, FTransform_C};

pub fn encode(case: &Case) -> Result<Vec<u8>, libwebpop::WebPEncodingError> {
    let custom = case.function == "WebPEncodeRGBAWithAlphaQuality";
    assert!(custom || case.function == "WebPEncodeRGBA");
    let a = &case.input;
    let mut rgba = vec![0xa5];
    rgba.extend(
        a[if custom { 5 } else { 4 }..]
            .iter()
            .map(|&v| u8::try_from(v).unwrap()),
    );
    let original = rgba.clone();
    let q = f32::from_bits(a[3]);
    let output = if custom {
        libwebpop::WebPEncodeRGBAWithAlphaQuality(
            &rgba[1..],
            a[0],
            a[1],
            a[2] as usize,
            q,
            a[4] as i32,
        )
    } else {
        libwebpop::WebPEncodeRGBA(&rgba[1..], a[0], a[1], a[2] as usize, q)
    };
    assert_eq!(rgba, original);
    output
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    encode(case)
        .unwrap_or_default()
        .into_iter()
        .map(u32::from)
        .collect()
}

pub fn transform(case: &Case) -> Vec<u32> {
    assert_eq!(case.function, "VP8FTransform");
    assert_eq!(case.input.len(), 32);
    let mut src = [0xa5; 3 * BPS + 4];
    let mut reference = [0x5a; 3 * BPS + 4];
    for i in 0..16 {
        src[i / 4 * BPS + i % 4] = u8::try_from(case.input[i]).unwrap();
        reference[i / 4 * BPS + i % 4] = u8::try_from(case.input[16 + i]).unwrap();
    }
    let original = (src, reference);
    let mut out = [0; 16];
    FTransform_C(&src, &reference, &mut out);
    assert_eq!((src, reference), original);
    out.map(|v| i32::from(v) as u32).to_vec()
}

pub fn transform_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for delta in -255i32..=255 {
        let mut input = vec![delta.max(0) as u32; 16];
        input.extend([(-delta).max(0) as u32; 16]);
        cases.push(Case::new("VP8FTransform", input));
    }
    for position in 0..16 {
        for value in [0, 1, 127, 128, 254, 255] {
            for reverse in [false, true] {
                let mut input = vec![128; 32];
                input[usize::from(reverse) * 16 + position] = value;
                cases.push(Case::new("VP8FTransform", input));
            }
        }
    }
    let mut seed = SEED;
    for _ in 0..1024 {
        cases.push(Case::new(
            "VP8FTransform",
            (0..32).map(|_| random(&mut seed) & 255).collect::<Vec<_>>(),
        ));
    }
    cases
}

pub fn image(w: usize, h: usize, pattern: u32) -> ImageCase {
    let stride = w * 4 + 7;
    let mut rgba = vec![0xa5; (h - 1) * stride + w * 4];
    let mut seed = SEED ^ w as u32 ^ ((h as u32) << 16) ^ pattern;
    for y in 0..h {
        for x in 0..w {
            let noise = random(&mut seed);
            let alpha = match pattern {
                0 => 255,
                1 => 0,
                2 => {
                    if (x + y) % 2 == 0 {
                        0
                    } else {
                        255
                    }
                }
                3 => ((x * 17 + y * 13) & 255) as u8,
                4 => (noise >> 24) as u8,
                5 => {
                    if (x + y) % 2 == 0 {
                        17
                    } else {
                        219
                    }
                }
                _ => unreachable!(),
            };
            let rgb = if pattern == 4 {
                [noise as u8, (noise >> 8) as u8, (noise >> 16) as u8]
            } else {
                [
                    (x * 19 + y * 7) as u8,
                    (x * 3 + y * 29) as u8,
                    (x * 11 + y * 5) as u8,
                ]
            };
            rgba[y * stride + 4 * x..][..4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
        }
    }
    ImageCase {
        name: "lossy reference RGBA",
        width: w as u32,
        height: h as u32,
        stride,
        rgba,
    }
}

pub fn request(image: &ImageCase, quality: f32, alpha_quality: Option<i32>) -> Case {
    let mut words = vec![
        image.width,
        image.height,
        image.stride as u32,
        quality.to_bits(),
    ];
    if let Some(alpha) = alpha_quality {
        words.push(alpha as u32);
    }
    words.extend(image.rgba.iter().map(|&v| u32::from(v)));
    Case::new(
        if alpha_quality.is_some() {
            "WebPEncodeRGBAWithAlphaQuality"
        } else {
            "WebPEncodeRGBA"
        },
        words,
    )
}

pub fn public_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (w, h) in [(1, 1), (1, 17), (17, 1), (3, 5), (17, 19), (33, 17)] {
        for pattern in 0..6 {
            let image = image(w, h, pattern);
            for quality in [0., 1., 30., 50., 75., 98., 98.5, 99., 100.] {
                cases.push(request(&image, quality, None));
                for alpha in [100, 0, 4, 5, 10, 30, 69, 70, 71, 99] {
                    cases.push(request(&image, quality, Some(alpha)));
                }
            }
        }
    }
    let real = super::public_images::real_image();
    cases.push(request(&real, 75., None));
    cases.push(request(&real, 75., Some(100)));
    cases
}

pub fn fixture_cases() -> Vec<Case> {
    let transforms = transform_cases();
    let mut cases: Vec<_> = [
        0, 254, 255, 256, 510, 511, 512, 527, 600, 702, 703, 704, 705, 706, 707, 708,
    ]
    .map(|i| transforms[i].clone())
    .into();
    for (w, h, pattern) in [(1, 1, 0), (3, 5, 2), (17, 19, 3), (17, 19, 4)] {
        let image = image(w, h, pattern);
        for quality in [75., 98.5] {
            cases.push(request(&image, quality, None));
            for alpha in [100, 0, 71] {
                cases.push(request(&image, quality, Some(alpha)));
            }
        }
    }
    cases
}

/// Validate the C file independently; returns changed alpha samples and counts
/// of raw/compressed ALPH chunks. RGB equality to the source is not expected.
pub fn check_decoded(case: &Case, words: &[u32]) -> [usize; 3] {
    let custom = case.function == "WebPEncodeRGBAWithAlphaQuality";
    assert!(custom || case.function == "WebPEncodeRGBA");
    let a = &case.input;
    let (w, h, stride) = (a[0] as usize, a[1] as usize, a[2] as usize);
    let pixels = &a[if custom { 5 } else { 4 }..];
    let bytes: Vec<_> = words.iter().map(|&v| u8::try_from(v).unwrap()).collect();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WEBP");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    let opaque = (0..h).all(|y| (0..w).all(|x| pixels[y * stride + x * 4 + 3] == 255));
    let mut tags = Vec::new();
    let mut cursor = 12;
    let mut stats = [0; 3];
    while cursor < bytes.len() {
        let tag = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let data = &bytes[cursor + 8..cursor + 8 + size];
        tags.push(tag);
        if tag == b"ALPH" {
            assert!(size >= 2);
            assert_eq!(data[0] & 0xc0, 0);
            let compression = data[0] & 3;
            assert!(compression <= 1);
            assert!((data[0] >> 4) & 3 <= 1);
            stats[1 + compression as usize] += 1;
            if !custom || a[4] == 100 {
                assert_eq!(data[0] & 0x30, 0);
            }
            if compression == 0 {
                assert_eq!(size, 1 + w * h);
            }
        } else if tag == b"VP8X" {
            assert_eq!(size, 10);
            assert_eq!(&data[..4], &[0x10, 0, 0, 0]);
            let read24 =
                |s: &[u8]| usize::from(s[0]) | (usize::from(s[1]) << 8) | (usize::from(s[2]) << 16);
            assert_eq!(read24(&data[4..7]) + 1, w);
            assert_eq!(read24(&data[7..10]) + 1, h);
        } else if tag == b"VP8 " {
            assert_eq!(&data[3..6], &[0x9d, 0x01, 0x2a]);
        }
        if size & 1 != 0 {
            assert_eq!(bytes[cursor + 8 + size], 0);
        }
        cursor += 8 + size + (size & 1);
    }
    assert_eq!(cursor, bytes.len());
    assert_eq!(
        tags,
        if opaque {
            vec![&b"VP8 "[..]]
        } else {
            vec![&b"VP8X"[..], &b"ALPH"[..], &b"VP8 "[..]]
        }
    );
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (a[0], a[1]));
    for y in 0..h {
        for x in 0..w {
            let source_alpha = pixels[y * stride + 4 * x + 3] as u8;
            let actual_alpha = decoded.get_pixel(x as u32, y as u32).0[3];
            if !custom || a[4] == 100 || opaque {
                assert_eq!(actual_alpha, source_alpha);
            }
            stats[0] += usize::from(source_alpha != actual_alpha);
        }
    }
    stats
}
