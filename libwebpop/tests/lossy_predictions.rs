#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{dsp::enc::*, enc::vp8i_enc::*};
use support::{SEED, random, vp8_predictions as predictions};

#[test]
fn committed_vp8_prediction_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_predictions.txt"));
    let cases = predictions::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected, "{}", case.function);
    }
}

#[test]
fn large_predictions_match_border_means_replication_and_clipped_gradients() {
    for case in predictions::cases() {
        if case.function == "VP8EncPredLuma4" {
            continue;
        }
        let chroma = case.function == "VP8EncPredChroma8";
        let a = &case.input;
        let size = if chroma { 8 } else { 16 };
        let left_len = if chroma { 25 } else { 17 };
        let left = &a[2..2 + left_len];
        let top = &a[2 + left_len..];
        let has_left = a[0] & 1 != 0;
        let has_top = a[0] & 2 != 0;
        let offsets = if chroma {
            predictions::CHROMA_OFFSETS
        } else {
            predictions::LUMA16_OFFSETS
        };
        let mut expected = vec![a[1]; PRED_SIZE_ENC];
        for channel in 0..if chroma { 2 } else { 1 } {
            let l = &left[16 * channel..];
            let t = &top[8 * channel..];
            let mut available = Vec::new();
            if has_left {
                available.extend(&l[1..=size]);
            }
            if has_top {
                available.extend(&t[..size]);
            }
            let dc = if available.is_empty() {
                128
            } else {
                (available.iter().sum::<u32>() + available.len() as u32 / 2)
                    / available.len() as u32
            };
            for y in 0..size {
                for x in 0..size {
                    let horizontal = if has_left { l[1 + y] } else { 129 };
                    let vertical = if has_top { t[x] } else { 127 };
                    let tm = match (has_left, has_top) {
                        (true, true) => {
                            (l[1 + y] as i32 + t[x] as i32 - l[0] as i32).clamp(0, 255) as u32
                        }
                        (true, false) => horizontal,
                        (false, true) => vertical,
                        (false, false) => 129,
                    };
                    for (offset, value) in offsets.into_iter().zip([dc, tm, vertical, horizontal]) {
                        expected[offset + 8 * channel + y * BPS + x] = value;
                    }
                }
            }
        }
        assert_eq!(case.rust(), expected, "{}, borders {}", case.function, a[0]);
    }
}

#[test]
fn intra4_constant_borders_and_dc_exclude_corner_and_top_right() {
    for value in 0..=255 {
        let top = [value; 13];
        let mut dst = [0xa5; PRED_SIZE_ENC];
        Intra4Preds_C(&mut dst, &top);
        for offset in predictions::LUMA4_OFFSETS {
            for y in 0..4 {
                assert_eq!(&dst[offset + y * BPS..][..4], &[value; 4]);
            }
        }
        let mut different = top;
        different[4] = 255 - value;
        different[9..].fill(255 - value);
        Intra4Preds_C(&mut dst, &different);
        for y in 0..4 {
            assert_eq!(&dst[I4DC4 + y * BPS..][..4], &[value; 4]);
        }
    }
}

#[test]
fn directional_modes_obey_transposition() {
    let pairs = [
        (I4DC4, I4DC4),
        (I4TM4, I4TM4),
        (I4VE4, I4HE4),
        (I4HE4, I4VE4),
        (I4RD4, I4RD4),
        (I4VR4, I4HD4),
        (I4HD4, I4VR4),
        (I4VL4, I4HU4),
        (I4HU4, I4VL4),
    ];
    let mut seed = SEED;
    for _ in 0..512 {
        let mut top: [u8; 13] = std::array::from_fn(|_| random(&mut seed) as u8);
        let right = top[8];
        top[9..].fill(right);
        let mut transposed = [0; 13];
        transposed[4] = top[4];
        for i in 0..4 {
            transposed[i] = top[8 - i];
            transposed[5 + i] = top[3 - i];
        }
        transposed[9..].fill(top[0]);
        let mut first = [0xa5; PRED_SIZE_ENC];
        let mut second = first;
        Intra4Preds_C(&mut first, &top);
        Intra4Preds_C(&mut second, &transposed);
        for (a, b) in pairs {
            for y in 0..4 {
                for x in 0..4 {
                    assert_eq!(
                        first[a + y * BPS + x],
                        second[b + x * BPS + y],
                        "offsets {a}/{b} at {x},{y}"
                    );
                }
            }
        }
    }
}

#[test]
fn intra4_half_and_quarter_rounding_and_extended_edges() {
    let mut dst = [0xa5; PRED_SIZE_ENC];
    for a in 0..=255u16 {
        for b in 0..=255u16 {
            for c in [0u16, 255] {
                let mut top = [128; 13];
                top[4] = a as u8;
                top[5] = b as u8;
                top[6] = c as u8;
                Intra4Preds_C(&mut dst, &top);
                assert_eq!(u16::from(dst[I4VR4]), (a + b).div_ceil(2));
                assert_eq!(u16::from(dst[I4VE4]), (a + 2 * b + c + 2) / 4);
            }
        }
    }
    let top = [7, 11, 23, 47, 89, 101, 113, 127, 139, 151, 167, 181, 199];
    Intra4Preds_C(&mut dst, &top);
    assert_eq!(dst[I4LD4 + 3 * BPS + 3], ((181 + 3 * 199 + 2) / 4) as u8);
    assert_eq!(
        dst[I4VL4 + 3 * BPS + 3],
        ((167 + 2 * 181 + 199 + 2) / 4) as u8
    );
    for x in 0..4 {
        assert_eq!(dst[I4HU4 + 3 * BPS + x], 7);
    }
}

#[test]
fn minimum_destination_slices_and_chroma_gap_preserve_surrounding_storage() {
    let left: [u8; 25] = std::array::from_fn(|i| (i * 7) as u8);
    let top: [u8; 16] = std::array::from_fn(|i| (i * 13) as u8);
    for chroma in [false, true] {
        let size = if chroma { 48 * BPS } else { 32 * BPS };
        let left_len = if chroma { 25 } else { 17 };
        let mut expected = [0xa5; PRED_SIZE_ENC];
        let mut actual = vec![0xa5; size + 8];
        let predict = if chroma {
            IntraChromaPreds_C
        } else {
            Intra16Preds_C
        };
        predict(&mut expected, Some(&left[..left_len]), Some(&top));
        predict(
            &mut actual[3..3 + size],
            Some(&left[..left_len]),
            Some(&top),
        );
        assert_eq!(&actual[3..3 + size], &expected[..size]);
        assert_eq!(&actual[..3], &[0xa5; 3]);
        assert_eq!(&actual[3 + size..], &[0xa5; 5]);
    }
    let mut changed = left;
    changed[9..16].fill(255);
    let mut first = [0xa5; PRED_SIZE_ENC];
    let mut second = first;
    IntraChromaPreds_C(&mut first, Some(&left), Some(&top));
    IntraChromaPreds_C(&mut second, Some(&changed), Some(&top));
    assert_eq!(first, second);
    let border: [u8; 13] = std::array::from_fn(|i| (i * 19) as u8);
    let size = I4HU4 + 3 * BPS + 4;
    let mut expected = [0xa5; PRED_SIZE_ENC];
    let mut actual = vec![0xa5; size + 8];
    Intra4Preds_C(&mut expected, &border);
    Intra4Preds_C(&mut actual[3..3 + size], &border);
    assert_eq!(&actual[3..3 + size], &expected[..size]);
    assert_eq!(&actual[..3], &[0xa5; 3]);
    assert_eq!(&actual[3 + size..], &[0xa5; 5]);
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_predictions_match_pinned_scalar_reference() {
    let cases = predictions::cases();
    let outputs = native::run("vp8-predictions", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    for name in ["VP8EncPredLuma16", "VP8EncPredChroma8", "VP8EncPredLuma4"] {
        eprintln!(
            "{name}: {} exact C comparisons",
            cases.iter().filter(|case| case.function == name).count()
        );
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn vp8_prediction_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = predictions::fixture_cases();
    let outputs = native::run("vp8-prediction-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C VP8 luma16/chroma8/intra4 predictions.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
            support::UPSTREAM_REVISION,
            SEED
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
                "/tests/fixtures/lossy_predictions.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_predictions.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
