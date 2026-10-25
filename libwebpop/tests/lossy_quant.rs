#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::internal::{
    dsp::enc::QuantizeBlock_C,
    enc::{filter_enc::*, quant_enc::*, vp8i_enc::*},
};
use support::vp8_quant as quant;

#[test]
fn committed_quantizer_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_quant.txt"));
    let cases = quant::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn expanded_matrices_have_exact_zero_thresholds_and_reciprocals() {
    for case in quant::matrix_cases() {
        let a = case.input;
        let mut m = quant::seed_matrix(9);
        m.q[0] = a[1] as u16;
        m.q[1] = a[2] as u16;
        let avg = ExpandMatrix(&mut m, a[0] as usize);
        assert_eq!(
            avg as f64,
            (m.q.iter().map(|&q| f64::from(q)).sum::<f64>() / 16.).round()
        );
        for i in 0..16 {
            let (q, reciprocal) = (u64::from(m.q[i]), u64::from(m.iq[i]));
            assert!(q * reciprocal <= 131072 && q * (reciprocal + 1) > 131072);
            let first = (0..=q)
                .find(|&v| (v * reciprocal + u64::from(m.bias[i])) / 131072 != 0)
                .unwrap();
            assert_eq!(first - 1, u64::from(m.zthresh[i]));
            if i >= 2 {
                assert_eq!(
                    (m.q[i], m.iq[i], m.bias[i], m.zthresh[i]),
                    (m.q[1], m.iq[1], m.bias[1], m.zthresh[1])
                );
            }
            assert_eq!(m.q[i] as u32, if i == 0 { a[1] } else { a[2] });
            if a[0] != 0 {
                assert_eq!(m.sharpen[i], 0);
            } else {
                let frequency = 30 * ((i % 4) + (i / 4)).min(3);
                assert_eq!(
                    usize::from(m.sharpen[i]),
                    frequency * usize::from(m.q[i]) / 2048
                );
            }
        }
        // Use the previously ported quantizer itself to verify every matrix's
        // two sides of the threshold, including subtraction of sharpening.
        for above in [false, true] {
            let mut input = std::array::from_fn(|i| {
                (m.zthresh[i] as i32 + i32::from(above) - i32::from(m.sharpen[i])) as i16
            });
            let mut levels = [0; 16];
            assert_eq!(QuantizeBlock_C(&mut input, &mut levels, &m), above);
            assert!(levels.iter().all(|&v| v == i16::from(above)));
            assert!(
                input
                    .iter()
                    .zip(m.q)
                    .all(|(&v, q)| v == if above { q as i16 } else { 0 })
            );
        }
    }
}

#[test]
fn filter_lookup_is_monotonic_clamped_and_sharpness_sensitive() {
    for s in 0..8 {
        let levels: Vec<_> = (0..129).map(|d| VP8FilterStrengthFromDelta(s, d)).collect();
        assert_eq!(levels[0], 0);
        assert!(levels.windows(2).all(|v| v[0] <= v[1]));
        for (d, &level) in levels.iter().enumerate() {
            assert!(level >= (d as i32).min(63) && level <= 63);
            if s == 0 {
                assert_eq!(level, (d as i32).min(63));
            }
        }
        assert_eq!(VP8FilterStrengthFromDelta(s, i32::MAX), 63);
    }
    assert!(VP8FilterStrengthFromDelta(7, 20) > VP8FilterStrengthFromDelta(0, 20));
}

fn verify_matrix_setup(enc: &VP8Encoder, original: &VP8Encoder) {
    for (i, s) in enc.dqm.iter().enumerate() {
        if i >= enc.segment_hdr.num_segments {
            assert_eq!(s, &original.dqm[i]);
            continue;
        }
        let avg = |m: &VP8Matrix| (m.q.iter().map(|&v| f64::from(v)).sum::<f64>() / 16.).round();
        let [y, dc, uv] = [avg(&s.y1), avg(&s.y2), avg(&s.uv)];
        let square = |v: f64, scale: f64| (v * v * scale).floor().max(1.) as i32;
        assert_eq!(
            [s.lambda_i4, s.lambda_i16, s.lambda_uv, s.lambda_mode],
            [
                square(y, 3. / 128.),
                square(dc, 3.),
                square(uv, 3. / 64.),
                square(y, 1. / 128.)
            ]
        );
        assert_eq!(
            [
                s.lambda_trellis_i4,
                s.lambda_trellis_i16,
                s.lambda_trellis_uv
            ],
            [square(y, 7. / 8.), square(dc, 1. / 4.), square(uv, 2.)]
        );
        let texture = if enc.config.method >= 4 {
            f64::from(enc.config.sns_strength) * y / 32.
        } else {
            0.
        };
        assert_eq!(s.tlambda, texture.floor().max(1.) as i32);
        assert_eq!(s.i4_penalty, (1000. * y * y) as i64);
        assert_eq!(s.min_disto, 20 * i32::from(s.y1.q[0]));
        assert_eq!(s.max_edge, 0);
        assert_eq!(s.y2.q[0] % 2, 0);
        assert!(s.uv.q[0] <= 132); // UV DC clips its table index to 117.
        assert!(s.y1.q.iter().all(|&q| (4..=284).contains(&q)));
        assert!(s.y2.q.iter().all(|&q| (8..=440).contains(&q)));
        assert!(s.y2.sharpen.iter().chain(&s.uv.sharpen).all(|&v| v == 0));
    }
}

#[test]
fn matrix_lambdas_clipping_and_unused_segments() {
    for case in quant::setup_cases().into_iter().filter(|c| c.input[0] == 0) {
        let mut enc = quant::seeded(&case.input);
        let original = quant::seeded(&case.input);
        SetupMatrices(&mut enc);
        verify_matrix_setup(&enc, &original);
        assert_eq!(enc.mb_info, original.mb_info);
        assert_eq!(enc.filter_hdr, original.filter_hdr);
        // Alter deltas on either side of saturation without altering quant.
        let mut saturated = quant::seeded(&case.input);
        for s in &mut saturated.dqm {
            s.quant = 0;
        }
        saturated.dq_y1_dc = -1;
        saturated.dq_y2_dc = -1;
        saturated.dq_y2_ac = -1;
        saturated.dq_uv_dc = -1;
        saturated.dq_uv_ac = -1;
        SetupMatrices(&mut saturated);
        assert_eq!(
            (
                saturated.dqm[0].y1.q[0],
                saturated.dqm[0].y2.q[0],
                saturated.dqm[0].uv.q[0]
            ),
            (4, 8, 4)
        );
    }
}

#[test]
fn filter_setup_uses_previous_sharpness_and_updates_all_segments() {
    let mut cutoffs = [0usize; 3];
    for case in quant::setup_cases().into_iter().filter(|c| c.input[0] == 1) {
        let a = &case.input;
        let mut enc = quant::seeded(a);
        let original = quant::seeded(a);
        SetupFilterStrength(&mut enc);
        for (i, s) in enc.dqm.iter().enumerate() {
            let step = i32::from(kAcTable[original.dqm[i].quant as usize]) / 4;
            let base = i64::from(VP8FilterStrengthFromDelta(
                original.filter_hdr.sharpness,
                step,
            ));
            let rational =
                base * 5 * i64::from(enc.config.filter_strength) / (256 + i64::from(s.beta));
            let expected = if rational < 2 {
                cutoffs[0] += 1;
                0
            } else if rational > 63 {
                cutoffs[2] += 1;
                63
            } else {
                cutoffs[1] += 1;
                rational as i32
            };
            let mut want = original.dqm[i].clone();
            want.fstrength = expected;
            assert_eq!(s, &want);
        }
        assert_eq!(
            enc.filter_hdr,
            VP8EncFilterHeader {
                simple: a[6] == 0,
                level: enc.dqm[0].fstrength,
                sharpness: a[5] as i32,
                i4x4_lf_delta: -3
            }
        );
        assert_eq!(enc.mb_info, original.mb_info);
    }
    assert!(cutoffs.iter().all(|&n| n > 0));
}

fn verify_merging(original: &VP8Encoder, enc: &VP8Encoder, setup_matrices: bool) {
    let old = &original.dqm[..original.segment_hdr.num_segments];
    let first: Vec<usize> = (0..old.len())
        .filter(|&i| {
            !(0..i).any(|j| (old[j].quant, old[j].fstrength) == (old[i].quant, old[i].fstrength))
        })
        .collect();
    assert_eq!(enc.segment_hdr.num_segments, first.len());
    assert_eq!(
        (enc.segment_hdr.update_map, enc.segment_hdr.size),
        (original.segment_hdr.update_map, original.segment_hdr.size)
    );
    for (i, &from) in first.iter().enumerate() {
        if !setup_matrices {
            assert_eq!(enc.dqm[i], old[from]);
        }
    }
    if first.len() < old.len() {
        for s in &enc.dqm[first.len()..old.len()] {
            assert_eq!(s, &old[*first.last().unwrap()]);
        }
    }
    for (before, after) in original.mb_info.iter().zip(&enc.mb_info) {
        let key = &old[before.segment as usize];
        let segment = first
            .iter()
            .position(|&i| (old[i].quant, old[i].fstrength) == (key.quant, key.fstrength))
            .unwrap();
        let mut want = *before;
        want.segment = segment as u8;
        assert_eq!(*after, want);
    }
}

#[test]
fn equivalent_segments_keep_first_representatives_and_copy_complete_state() {
    for case in quant::setup_cases().into_iter().filter(|c| c.input[0] == 2) {
        let original = quant::seeded(&case.input);
        let mut enc = quant::seeded(&case.input);
        SimplifySegments(&mut enc);
        verify_merging(&original, &enc, false);
        assert_eq!(enc.filter_hdr, original.filter_hdr);
        let once = quant::state(&enc);
        SimplifySegments(&mut enc);
        assert_eq!(once, quant::state(&enc));
    }
}

// Independently invert the quantization intervals in logarithmic space rather
// than repeating the nested pow calls used by the encoder.
fn quality_quant(quality: f32, sns: i32, alpha: i32) -> i32 {
    if quality == 0. {
        return 127;
    }
    if quality == 100. {
        return 0;
    }
    let q = f64::from(quality);
    let linear = if q < 75. { q / 150. } else { q / 50. - 1. };
    let log_c = linear.ln() * (1. - 0.9 * f64::from(sns) * f64::from(alpha) / 12800.) / 3.;
    // For the smallest positive f32 qualities, C's subtraction rounds 1-c
    // back to exactly one, even though mathematical c is positive.
    if log_c < (f64::EPSILON / 4.).ln() {
        return 127;
    }
    (0..127)
        .find(|&i| {
            let lower = (126 - i) as f64 / 127.;
            let upper = (127 - i) as f64 / 127.;
            log_c <= upper.ln() + 1e-13 && (lower == 0. || log_c >= lower.ln() - 1e-13)
        })
        .unwrap()
}

#[test]
fn fractional_quality_chroma_deltas_and_repeated_trials() {
    let mut merged = 0;
    for case in quant::setup_cases().into_iter().filter(|c| c.input[0] >= 3) {
        let a = &case.input;
        let mut enc = quant::seeded(a);
        let original = quant::seeded(a);
        for quality in if a[0] == 4 {
            vec![f32::from_bits(a[9]), f32::from_bits(a[31])]
        } else {
            vec![f32::from_bits(a[9])]
        } {
            // Expected state before merging: independent logarithmic mapping
            // and signed rational scaling with two separate truncations.
            let mut expected = quant::seeded(a);
            expected.segment_hdr = enc.segment_hdr.clone();
            expected.dqm = enc.dqm.clone();
            expected.mb_info = enc.mb_info.clone();
            for s in &mut expected.dqm[..enc.segment_hdr.num_segments] {
                s.quant = quality_quant(quality, enc.config.sns_strength, s.alpha);
            }
            expected.base_quant = expected.dqm[0].quant;
            for s in &mut expected.dqm[enc.segment_hdr.num_segments..] {
                s.quant = expected.base_quant;
            }
            expected.filter_hdr.sharpness = enc.filter_hdr.sharpness;
            SetupFilterStrength(&mut expected); // independently checked above
            VP8SetSegmentParams(&mut enc, quality);
            assert_eq!(enc.base_quant, expected.base_quant);
            verify_merging(&expected, &enc, true);
            let mut compact = quant::seeded(a);
            compact.dqm = expected.dqm.clone();
            compact.mb_info = expected.mb_info.clone();
            compact.segment_hdr = expected.segment_hdr.clone();
            SimplifySegments(&mut compact);
            for (s, want) in enc.dqm[..enc.segment_hdr.num_segments]
                .iter()
                .zip(&compact.dqm)
            {
                assert_eq!(
                    (s.alpha, s.beta, s.quant, s.fstrength),
                    (want.alpha, want.beta, want.quant, want.fstrength)
                );
            }
            assert_eq!(
                (enc.filter_hdr.level, enc.filter_hdr.sharpness),
                (expected.filter_hdr.level, a[5] as i32)
            );
            let trunc = |n: i64, d: i64| {
                let v = n.unsigned_abs() / d as u64;
                if n < 0 { -(v as i32) } else { v as i32 }
            };
            let ac = trunc(i64::from(enc.uv_alpha - 64) * 10, 70);
            let ac = trunc(i64::from(ac) * i64::from(enc.config.sns_strength), 100).clamp(-4, 6);
            let dc = trunc(-4 * i64::from(enc.config.sns_strength), 100);
            assert_eq!((enc.dq_uv_ac, enc.dq_uv_dc), (ac, dc));
            assert_eq!((enc.dq_y1_dc, enc.dq_y2_dc, enc.dq_y2_ac), (0, 0, 0));
            verify_matrix_setup(&enc, &compact);
            if enc.segment_hdr.num_segments < expected.segment_hdr.num_segments {
                merged += 1;
            }
        }
        assert_eq!(enc.config, original.config);
        assert_eq!(enc.preds, original.preds);
        assert_eq!(enc.nz, original.nz);
        assert_eq!(enc.y_top, original.y_top);
        assert_eq!(enc.uv_top, original.uv_top);
    }
    assert!(merged > 100);
    eprintln!("{merged} quality trials merge segments");
}

#[cfg(feature = "native-oracle")]
#[test]
fn quantizer_setup_matches_pinned_reference() {
    let cases = quant::cases();
    native::parallel_cases("vp8-quant", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!("{} exact quantizer/filter requests", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn quantizer_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = quant::fixture_cases();
    let outputs = native::run("vp8-quant-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C quantizer and initial filter setup.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_quant.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_quant.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
