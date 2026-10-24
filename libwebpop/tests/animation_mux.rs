#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
mod support;

use libwebpop::*;
use support::animation_mux as corpus;

fn le24(a: &[u8]) -> u32 {
    u32::from_le_bytes([a[0], a[1], a[2], 0])
}

// Independent RIFF walker: lengths exclude padding and all external padding
// bytes must be zero. Iterating nested ANMF checks their sizes independently.
fn chunks(mut bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        assert!(bytes.len() >= 8);
        let size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let padded = size + (size & 1);
        assert!(bytes.len() >= 8 + padded);
        if size & 1 != 0 {
            assert_eq!(bytes[8 + size], 0);
        }
        result.push((&bytes[..4], &bytes[8..8 + size]));
        bytes = &bytes[8 + padded..];
    }
    result
}

fn image_chunks(bytes: &[u8]) -> Vec<(&[u8], &[u8])> {
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    assert_eq!(&bytes[8..12], b"WEBP");
    chunks(&bytes[12..])
}

fn inspect(case: &support::Case, bytes: &[u8]) {
    let (w, h, options, frames) = corpus::parse(case);
    let all = image_chunks(bytes);
    let animated = frames.len() != 1 || frames[0].width != w || frames[0].height != h;
    let alpha = frames.iter().any(|f| {
        image_chunks(&f.still)
            .iter()
            .any(|(tag, data)| *tag == b"ALPH" || (*tag == b"VP8L" && data[4] & 0x10 != 0))
    });
    let has_alph_chunk = frames
        .iter()
        .any(|f| image_chunks(&f.still).iter().any(|c| c.0 == b"ALPH"));
    let mut flags = u32::from(options.icc_profile.is_some()) * 32
        | u32::from(options.exif.is_some()) * 8
        | u32::from(options.xmp.is_some()) * 4
        | u32::from(animated) * 2;
    if has_alph_chunk || (flags != 0 && alpha) {
        flags |= 16;
    }
    let mut at = 0;
    if flags != 0 {
        assert_eq!(all[at].0, b"VP8X");
        let data = all[at].1;
        assert_eq!(data.len(), 10);
        assert_eq!(&data[..4], &flags.to_le_bytes());
        assert_eq!((le24(&data[4..]) + 1, le24(&data[7..]) + 1), (w, h));
        at += 1;
    }
    if let Some(icc) = &options.icc_profile {
        assert_eq!(all[at], (&b"ICCP"[..], &icc[..]));
        at += 1;
    }
    if animated {
        assert_eq!(all[at].0, b"ANIM");
        let bg = options.background;
        let [lo, hi] = options.loop_count.to_le_bytes();
        assert_eq!(all[at].1, [bg.blue, bg.green, bg.red, bg.alpha, lo, hi]);
        at += 1;
    }
    for f in &frames {
        let expected: Vec<_> = image_chunks(&f.still)
            .into_iter()
            .filter(|c| matches!(c.0, b"VP8L" | b"VP8 " | b"ALPH"))
            .collect();
        if animated {
            assert_eq!(all[at].0, b"ANMF");
            let data = all[at].1;
            let c = f.control;
            assert_eq!(
                (le24(data) * 2, le24(&data[3..]) * 2),
                (c.x_offset, c.y_offset)
            );
            assert_eq!(
                (le24(&data[6..]) + 1, le24(&data[9..]) + 1),
                (f.width, f.height)
            );
            assert_eq!(le24(&data[12..]), c.duration_ms);
            assert_eq!(
                data[15],
                u8::from(c.dispose == FrameDispose::Background)
                    | (u8::from(c.blend == FrameBlend::Replace) << 1)
            );
            assert_eq!(chunks(&data[16..]), expected);
            at += 1;
        } else {
            assert_eq!(&all[at..at + expected.len()], expected);
            at += expected.len();
        }
    }
    for (tag, bytes) in [(b"EXIF", &options.exif), (b"XMP ", &options.xmp)] {
        if let Some(bytes) = bytes {
            assert_eq!(all[at], (&tag[..], &bytes[..]));
            at += 1;
        }
    }
    assert_eq!(at, all.len());
}

#[test]
fn animation_mux_fields_chunks_padding_and_codec_payloads() {
    let cases = corpus::differential_cases();
    for case in &cases {
        let output: Vec<_> = case.rust().into_iter().map(|v| v as u8).collect();
        inspect(case, &output);
    }
}

#[test]
fn animation_mux_validation_preserves_builder_and_output() {
    for (w, h) in [
        (0, 1),
        (1, 0),
        (1 << 24 | 1, 1),
        (1, 1 << 24 | 1),
        (65536, 65536),
        (u32::MAX, u32::MAX),
    ] {
        assert!(matches!(
            AnimationContainer::new(w, h, Default::default()),
            Err(MuxError::InvalidCanvas)
        ));
    }
    let mut mux = AnimationContainer::new(3, 3, Default::default()).unwrap();
    let mut output = vec![1, 2, 3];
    assert!(matches!(mux.assemble(&mut output), Err(MuxError::NoFrames)));
    assert_eq!(output, [1, 2, 3]);
    let f = corpus::frame(1, 1, 0, false, support::SEED);
    for (x, y, duration, is_duration) in [
        (1, 0, 0, false),
        (0, 1, 0, false),
        (4, 0, 0, false),
        (0, 4, 0, false),
        (1 << 24, 0, 0, false),
        (0, 0, 1 << 24, true),
        (0, 0, u32::MAX, true),
    ] {
        let err = mux
            .push_frame(
                f.encoded(),
                ContainerFrameOptions {
                    x_offset: x,
                    y_offset: y,
                    duration_ms: duration,
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(if is_duration {
            matches!(err, MuxError::InvalidDuration)
        } else {
            matches!(err, MuxError::InvalidRectangle)
        });
        assert!(matches!(mux.assemble(&mut output), Err(MuxError::NoFrames)));
        assert_eq!(output, [1, 2, 3]);
    }
    mux.push_frame(f.encoded(), Default::default()).unwrap();
    mux.assemble(&mut output).unwrap();
    assert!(image_chunks(&output).iter().any(|c| c.0 == b"ANMF"));
    let mut repeated = vec![0xff; 1000];
    mux.assemble(&mut repeated).unwrap();
    assert_eq!(repeated, output);
    assert!(matches!(
        mux.push_frame(
            f.encoded(),
            ContainerFrameOptions {
                x_offset: 1,
                ..Default::default()
            }
        ),
        Err(MuxError::InvalidRectangle)
    ));
    mux.assemble(&mut repeated).unwrap();
    assert_eq!(repeated, output);
}

#[test]
fn animation_prepared_frame_validates_inputs_and_preserves_exact_rgba() {
    let rgba = [12, 34, 56, 0];
    let original = rgba;
    let options = LosslessOptions {
        exact: true,
        ..Default::default()
    };
    assert!(EncodedFrame::encode_lossless_rgba(&rgba, 1, 1, 3, &options).is_err());
    assert!(EncodedFrame::encode_lossless_rgba(&rgba[..3], 1, 1, 4, &options).is_err());
    assert!(EncodedFrame::encode_near_lossless_rgba(&rgba, 1, 1, 4, 101, &options).is_err());
    assert!(
        EncodedFrame::encode_lossy_rgba(
            &rgba,
            1,
            1,
            4,
            &LossyOptions {
                quality: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(EncodedFrame::encode_lossless_rgba(&rgba, 16384, 1, 65536, &options).is_err());
    let f = EncodedFrame::encode_lossless_rgba(&rgba, 1, 1, 4, &options).unwrap();
    assert_eq!((f.width(), f.height(), f.has_alpha()), (1, 1, true));
    let mut mux = AnimationContainer::new(1, 1, Default::default()).unwrap();
    mux.push_frame(
        f,
        ContainerFrameOptions {
            duration_ms: 123,
            ..Default::default()
        },
    )
    .unwrap();
    let mut bytes = Vec::new();
    mux.assemble(&mut bytes).unwrap();
    let mut expected = Vec::new();
    encode_lossless_rgba(&rgba, 1, 1, 4, &options, &mut expected).unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(rgba, original);
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.as_raw(), &rgba);
}

#[test]
fn animation_mux_committed_bytes_match_c() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/animation_mux.txt"));
    let cases = corpus::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(&case.rust(), expected, "{case:?}");
        inspect(
            &case,
            &expected.iter().map(|&v| v as u8).collect::<Vec<_>>(),
        );
    }
}

// This is deliberately a reference-only gate, outside the Rust parity corpus.
// Rust supports the before representation, but cannot produce the after bytes
// until exact decode/reconstruction has been scoped and ported.
#[test]
fn animation_decode_dependent_single_frame_gate_is_recorded() {
    let fixture = fixture_support::fixtures(include_str!("fixtures/animation_single_frame.txt"));
    assert_eq!(fixture.len(), 1);
    let (stored, result) = &fixture[0];
    let case = corpus::single_frame_case();
    assert_eq!(stored.function, case.function);
    assert_eq!(stored.input, case.input);
    let before_len = result[0] as usize;
    let before = &result[1..1 + before_len];
    let after_len = result[1 + before_len] as usize;
    let after = &result[2 + before_len..];
    assert_eq!(after.len(), after_len);
    assert!(after_len < before_len);
    assert_eq!(corpus::evaluate(&case), before);
    let before: Vec<_> = before.iter().map(|&v| v as u8).collect();
    let after: Vec<_> = after.iter().map(|&v| v as u8).collect();
    assert!(image_chunks(&before).iter().any(|c| c.0 == b"ANMF"));
    assert!(!image_chunks(&after).iter().any(|c| c.0 == b"ANMF"));
    let decoded = image::load_from_memory_with_format(&after, image::ImageFormat::WebP)
        .unwrap()
        .into_rgba8();
    assert_eq!(decoded.dimensions(), (4, 4));
    let (_, _, _, frames) = corpus::parse(&case);
    for (x, y, actual) in decoded.enumerate_pixels() {
        if (x, y) == (2, 2) {
            assert_eq!(actual.0.as_slice(), &frames[0].rgba[..4]);
        } else {
            assert_eq!(actual.0[3], 0);
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_mux_matches_pinned_reference() {
    let cases = corpus::differential_cases();
    let outputs = native::run("animation-mux", &cases);
    for (case, expected) in cases.iter().zip(&outputs) {
        native::compare(case, expected, &case.rust());
    }
    eprintln!("{} animation mux files match pinned C", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn animation_mux_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    for (name, cases) in [
        ("animation_mux", corpus::fixture_cases()),
        ("animation_single_frame", vec![corpus::single_frame_case()]),
    ] {
        let outputs = native::run(&format!("{name}-fixtures"), &cases);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/{name}.txt"));
        if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1"))
        {
            let mut text = format!(
                "# Pinned scalar C {name} fixtures.\n# upstream={} seed={:08x}\n# See ../reference/ANIMATION.md for protocol and provenance.\n",
                support::UPSTREAM_REVISION,
                support::SEED
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
            std::fs::write(path, text).unwrap();
        } else {
            let text = std::fs::read_to_string(path).unwrap();
            let fixtures = fixture_support::fixtures(&text);
            assert_eq!(fixtures.len(), cases.len());
            for ((stored, expected), (case, actual)) in
                fixtures.iter().zip(cases.iter().zip(&outputs))
            {
                assert_eq!(stored.function, case.function);
                assert_eq!(stored.input, case.input);
                native::compare(case, expected, actual);
            }
        }
    }
}
