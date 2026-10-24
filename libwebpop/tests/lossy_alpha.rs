#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

#[test]
fn committed_alpha_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_alpha.txt"));
    let cases = alpha::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((c, out), expected) in fixtures.iter().zip(cases) {
        assert_eq!(c.function, expected.function);
        assert_eq!(c.input, expected.input);
        assert_eq!(c.rust(), *out);
    }
}
#[cfg(feature = "native-oracle")]
#[test]
fn alpha_matches_pinned_reference() {
    let cases = alpha::cases();
    let results = native::parallel_cases("vp8-alpha", &cases, |c, out| {
        let actual = c.rust();
        native::compare(c, out, &actual);
        if c.function == "VP8AlphaFile" {
            let mut original = c.clone();
            original.function = "WebPEncodeRGBAWithAlphaQuality".into();
            support::lossy::check_decoded(&original, &actual)
        } else {
            [0; 3]
        }
    });
    let mut totals = [0; 3];
    for result in results {
        for (total, n) in totals.iter_mut().zip(result) {
            *total += n;
        }
    }
    eprintln!(
        "{} independently decoded complete alpha files; changed/raw/compressed {totals:?}",
        alpha::file_cases().len()
    );
    eprintln!("{} exact alpha requests", cases.len());
}
#[cfg(feature = "native-oracle")]
#[test]
fn alpha_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = alpha::fixture_cases();
    let outputs = native::run("vp8-alpha-fixtures", &cases);
    for (c, out) in cases.iter().zip(&outputs) {
        native::compare(c, out, &c.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C alpha planes, filters and streams.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            support::SEED
        );
        for (c, out) in cases.iter().zip(&outputs) {
            text.push_str(&c.function);
            for v in &c.input {
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
                "/tests/fixtures/lossy_alpha.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_alpha.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
}

use libwebpop::{
    dsp::filters::*,
    enc::{alpha_enc::*, syntax_enc::*},
    utils::{
        bit_writer_utils::*, filters_utils::WebPEstimateBestFilter,
        quant_levels_utils::QuantizeLevels,
    },
};
use support::vp8_alpha as alpha;

fn inverse(filtered: &[u8], w: usize, h: usize, stride: usize, filter: usize) -> Vec<u8> {
    let mut out = vec![0; w * h];
    for y in 0..h {
        for x in 0..w {
            let pred = if filter == 0 || (x == 0 && y == 0) {
                0
            } else if y == 0 || (filter == 1 && x > 0) {
                out[y * w + x - 1]
            } else if x == 0 || filter == 2 {
                out[(y - 1) * w + x]
            } else {
                let sum = u16::from(out[y * w + x - 1]) + u16::from(out[(y - 1) * w + x]);
                sum.saturating_sub(u16::from(out[(y - 1) * w + x - 1]))
                    .min(255) as u8
            };
            out[y * w + x] = filtered[y * stride + x].wrapping_add(pred);
        }
    }
    out
}
fn green_decode(bytes: &[u8], w: usize, h: usize) -> Vec<u8> {
    use libwebpop::enc::vp8l_enc::WriteRiffHeader;
    let mut payload = Vec::new();
    payload.extend_from_slice(&((w as u32 - 1) | ((h as u32 - 1) << 14) | (1 << 28)).to_le_bytes());
    payload.extend_from_slice(bytes);
    let mut file = WriteRiffHeader(payload.len()).unwrap().to_vec();
    file.extend_from_slice(&payload);
    if (payload.len() + 1) & 1 != 0 {
        file.push(0);
    }
    let decoded = image::load_from_memory_with_format(&file, image::ImageFormat::WebP)
        .unwrap()
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (w as u32, h as u32));
    decoded
        .pixels()
        .map(|p| {
            assert_eq!([p.0[0], p.0[2], p.0[3]], [0, 0, 0]);
            p.0[1]
        })
        .collect()
}
#[test]
fn scalar_filters_invert_with_minimum_buffers_and_preserve_padding() {
    for case in alpha::leaf_cases().into_iter().filter(|c| c.input[0] <= 1) {
        let a = &case.input;
        let (w, h, stride) = (a[1] as usize, a[2] as usize, a[3] as usize);
        let result = case.rust();
        if a[0] == 0 {
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(result[y * a[8] as usize + x], a[9 + y * stride + x] << 8);
                }
            }
            for (i, &v) in result.iter().enumerate() {
                if i % a[8] as usize >= w || i / a[8] as usize >= h {
                    assert_eq!(v, 0x5aa5a55a);
                }
            }
        } else {
            let filtered: Vec<_> = result.iter().map(|&v| v as u8).collect();
            let decoded = inverse(&filtered, w, h, stride, a[4] as usize);
            for y in 0..h {
                assert_eq!(
                    &decoded[y * w..(y + 1) * w],
                    &a[9 + y * stride..9 + y * stride + w]
                        .iter()
                        .map(|&v| v as u8)
                        .collect::<Vec<_>>()
                );
            }
            for (i, &v) in result.iter().enumerate() {
                if i % stride >= w || i / stride >= h {
                    assert_eq!(v, 0xa5);
                }
            }
        }
    }
    for a in 0..=255u16 {
        for b in 0..=255u16 {
            for c in 0..=255u16 {
                assert_eq!(
                    GradientPredictor_C(a as u8, b as u8, c as u8),
                    (a + b).saturating_sub(c).min(255) as u8
                );
            }
        }
    }
}
#[test]
fn centroid_quantization_preserves_extrema_order_and_small_palettes() {
    for case in alpha::leaf_cases().into_iter().filter(|c| c.input[0] == 3) {
        let a = &case.input;
        let out = case.rust();
        let values = &a[9..];
        let result = &out[3..];
        if out[0] == 0 {
            assert_eq!(values, result);
            assert_eq!(out[1], 0x12345678);
            continue;
        }
        let mut levels = std::collections::BTreeMap::new();
        let distinct: std::collections::BTreeSet<_> = values.iter().collect();
        for (&src, &dst) in values.iter().zip(result) {
            assert_eq!(levels.insert(src, dst).unwrap_or(dst), dst);
        }
        assert!(
            levels
                .values()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                <= a[4] as usize
        );
        assert!(
            levels
                .values()
                .zip(levels.values().skip(1))
                .all(|(a, b)| a <= b)
        );
        assert_eq!(
            levels.first_key_value().map(|(&a, &b)| (a, b)),
            values.iter().min().map(|&v| (v, v))
        );
        assert_eq!(
            levels.last_key_value().map(|(&a, &b)| (a, b)),
            values.iter().max().map(|&v| (v, v))
        );
        if distinct.len() <= a[4] as usize {
            assert_eq!(values, result);
            if a[5] == 0 {
                assert_eq!(&out[1..3], &[0, 0]);
            }
        }
    }
    let mut values = [0, 4, 5, 6, 7, 10, 239];
    assert_eq!(QuantizeLevels(&mut values, 6, 1, 3), Some(5));
    assert_eq!(values, [0, 6, 6, 6, 6, 10, 239]);
    let mut ties = [0, 5, 10];
    assert_eq!(QuantizeLevels(&mut ties, 3, 1, 2), Some(25));
    assert_eq!(ties, [0, 0, 10]);
    for (w, h, n) in [
        (0, 1, 2),
        (1, 0, 2),
        (usize::MAX, 2, 2),
        (1, 1, 1),
        (1, 1, 257),
        (2, 1, 2),
    ] {
        let mut v = [17];
        assert_eq!(QuantizeLevels(&mut v, w, h, n), None);
        assert_eq!(v, [17]);
    }
}
#[test]
fn filter_estimation_and_maps_follow_sampling_color_thresholds_and_ties() {
    let mut chosen = [0; 4];
    for case in alpha::leaf_cases()
        .into_iter()
        .filter(|c| c.input[0] == 2 || c.input[0] == 5)
    {
        let a = &case.input;
        let (w, h) = (a[1] as usize, a[2] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let estimated = WebPEstimateBestFilter(&data, w, h);
        chosen[estimated] += 1;
        if w <= 3 || h <= 3 {
            assert_eq!(estimated, 0);
        }
        if a[0] == 5 {
            let out = case.rust()[0];
            let colors = data.iter().collect::<std::collections::BTreeSet<_>>().len();
            let expected = match a[4] {
                0 => 1,
                5 => 15,
                6 => {
                    let filter = if colors <= 16 { 0 } else { estimated };
                    (1 << filter) | u32::from(a[5] > 3 || colors > 192)
                }
                _ => unreachable!(),
            };
            assert_eq!(out, expected);
        }
    }
    assert!(chosen.iter().all(|&n| n > 0));
}
#[test]
fn headerless_alpha_streams_decode_exact_green_and_keep_zero_other_channels() {
    let mut count = 0;
    for case in alpha::stream_cases()
        .into_iter()
        .filter(|c| c.input[0] == 6)
    {
        let a = &case.input;
        let bytes: Vec<_> = case.rust().into_iter().map(|v| v as u8).collect();
        assert_eq!(
            green_decode(&bytes, a[1] as usize, a[2] as usize),
            a[9..].iter().map(|&v| v as u8).collect::<Vec<_>>()
        );
        count += 1;
    }
    eprintln!("{count} headerless method-4/q32/exact alpha streams decoded");
}
#[test]
fn trial_payloads_invert_and_preserve_compressed_raw_size_ties() {
    let (mut raw, mut compressed, mut ties) = (0, 0, 0);
    for case in alpha::stream_cases()
        .into_iter()
        .filter(|c| c.input[0] == 7)
    {
        let a = &case.input;
        let (w, h) = (a[1] as usize, a[2] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let result = case.rust();
        let header = result[0];
        assert_eq!((header >> 2) & 3, a[5]);
        assert_eq!(header >> 4, a[6]);
        let payload: Vec<_> = result[1..].iter().map(|&v| v as u8).collect();
        let filtered = if header & 1 != 0 {
            compressed += 1;
            green_decode(&payload, w, h)
        } else {
            raw += 1;
            assert_eq!(payload.len(), w * h);
            payload
        };
        assert_eq!(inverse(&filtered, w, h, w, a[5] as usize), data);
        if a[4] == 1 {
            let mut scratch = vec![0; w * h];
            WebPFilter(&data, w, h, w, a[5] as usize, &mut scratch);
            let mut bw = VP8LBitWriterInit(0).unwrap();
            EncodeLossless(&scratch, w, h, &mut bw).unwrap();
            let size = VP8LBitWriterFinish(&mut bw).unwrap().len();
            assert_eq!(header & 1, u32::from(size <= w * h));
            ties += usize::from(size == w * h);
        } else {
            assert_eq!(header & 1, 0);
        }
    }
    assert!(raw > 0 && compressed > 0 && ties > 0);
    eprintln!(
        "{raw} raw trials, {compressed} compressed trials, {ties} compressed/raw equal-size cases"
    );
}
#[test]
fn selected_filters_use_strict_size_order_and_quantization_flags() {
    let mut ties = 0;
    for case in alpha::stream_cases()
        .into_iter()
        .filter(|c| c.input[0] == 8)
    {
        let a = &case.input;
        let (w, h) = (a[1] as usize, a[2] as usize);
        let data: Vec<_> = a[9..].iter().map(|&v| v as u8).collect();
        let map = GetFilterMap(&data, w, h, a[5] as usize, 4);
        let mut trials = Vec::new();
        for f in 0..4 {
            if map & (1 << f) != 0 {
                trials.push(
                    EncodeAlphaInternal(&data, w, h, a[4] as u8, f, a[6] != 0, &mut vec![0; w * h])
                        .unwrap(),
                );
            }
        }
        let min = trials.iter().map(Vec::len).min().unwrap();
        let expected = trials.iter().find(|t| t.len() == min).unwrap();
        ties += usize::from(trials.iter().filter(|t| t.len() == min).count() > 1);
        assert_eq!(
            case.rust(),
            expected.iter().map(|&v| u32::from(v)).collect::<Vec<_>>()
        );
    }
    assert!(ties > 0);
    eprintln!("{ties} equal-size selected-filter comparisons");
    for q in [0, 70, 100] {
        let result = EncodeAlpha(&[37; 15], 3, 5, 3, q, 0, WEBP_FILTER_BEST).unwrap();
        assert_eq!(result.data[0], if q < 100 { 16 } else { 0 });
        assert_eq!(&result.data[1..], &[37; 15]);
        assert_eq!(result.sse, 0);
    }
}
#[test]
fn alpha_errors_are_checked_before_allocating_and_container_sizes_are_checked() {
    for (w, h, s) in [
        (0, 1, 1),
        (1, 0, 1),
        (16384, 1, 16384),
        (1, 16384, 1),
        (2, 1, 1),
        (1, 2, usize::MAX),
        (2, 1, 2),
    ] {
        assert!(matches!(
            EncodeAlpha(&[0], w, h, s, 100, 1, 6),
            Err(AlphaEncodeError::InvalidInput)
        ));
    }
    for q in [-1, 101, i32::MIN, i32::MAX] {
        assert!(matches!(
            EncodeAlpha(&[0], 1, 1, 1, q, 1, 6),
            Err(AlphaEncodeError::InvalidQuality)
        ));
    }
    assert!(matches!(
        EncodeAlpha(&[0], 1, 1, 1, 100, 2, 6),
        Err(AlphaEncodeError::InvalidConfiguration)
    ));
    assert!(matches!(
        WebPSizes(0, 0, Some(usize::MAX)),
        Err(VP8WriteError::FileTooBig)
    ));
    assert!(matches!(
        WebPSizes(0, 0, Some(u32::MAX as usize)),
        Err(VP8WriteError::FileTooBig)
    ));
    for alpha_len in 0..4 {
        for token_len in 0..4 {
            let alpha = vec![19; alpha_len];
            let mut bytes = Vec::new();
            AssembleWebP(
                17,
                19,
                0,
                &[1, 2, 3],
                &vec![7; token_len],
                Some(&alpha),
                &mut bytes,
            )
            .unwrap();
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
                bytes.len()
            );
            assert_eq!(&bytes[12..16], b"VP8X");
            assert_eq!(&bytes[20..24], &[16, 0, 0, 0]);
            assert_eq!(&bytes[24..30], &[16, 0, 0, 18, 0, 0]);
            assert_eq!(&bytes[30..34], b"ALPH");
            assert_eq!(
                u32::from_le_bytes(bytes[34..38].try_into().unwrap()) as usize,
                alpha_len
            );
            assert_eq!(&bytes[38..38 + alpha_len], alpha);
            if alpha_len & 1 != 0 {
                assert_eq!(bytes[38 + alpha_len], 0);
            }
            let vp8 = 38 + alpha_len + (alpha_len & 1);
            assert_eq!(&bytes[vp8..vp8 + 4], b"VP8 ");
            assert_eq!(
                u32::from_le_bytes(bytes[vp8 + 4..vp8 + 8].try_into().unwrap()) as usize,
                bytes.len() - vp8 - 8
            );
        }
    }
}
#[test]
fn composed_alpha_files_decode_and_opaque_output_ignores_alpha_quality() {
    let (mut raw, mut compressed, mut changed) = (0, 0, 0);
    for case in alpha::file_cases().into_iter().step_by(19) {
        let actual = case.rust();
        let mut original = case.clone();
        original.function = "WebPEncodeRGBAWithAlphaQuality".into();
        let result = support::lossy::check_decoded(&original, &actual);
        changed += result[0];
        raw += result[1];
        compressed += result[2];
    }
    assert!(raw > 0 && compressed > 0 && changed > 0);
    eprintln!(
        "portable alpha decodes: {raw} raw, {compressed} compressed chunks, {changed} changed alpha samples"
    );
    let image = support::lossy::image(17, 19, 0);
    let mut first = None;
    for q in [0, 4, 5, 70, 71, 99, 100] {
        let mut case = support::lossy::request(&image, 75., Some(q));
        case.function = "VP8AlphaFile".into();
        let bytes = case.rust();
        if let Some(expected) = &first {
            assert_eq!(&bytes, expected);
        } else {
            first = Some(bytes);
        }
    }
}
