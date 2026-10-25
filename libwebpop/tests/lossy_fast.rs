#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;
use support::vp8_fast as fast;

#[cfg(feature = "native-oracle")]
#[test]
fn fast_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = fast::fixture_cases();
    let outputs = native::run("vp8-fast-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C fast analysis and non-RD mode decisions.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lossy_fast.txt"),
            text,
        )
        .unwrap();
    } else {
        assert_eq!(
            fixture_support::fixtures(include_str!("fixtures/lossy_fast.txt")).len(),
            cases.len()
        );
        for ((stored, expected), (case, output)) in
            fixture_support::fixtures(include_str!("fixtures/lossy_fast.txt"))
                .iter()
                .zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}

#[test]
fn committed_fast_paths_match_pinned_c() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_fast.txt"));
    let cases = fast::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(&cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        assert_eq!(
            &case.rust(),
            expected,
            "{} {:?}",
            case.function,
            &case.input[..8.min(case.input.len())]
        );
    }
}
#[cfg(feature = "native-oracle")]
#[test]
fn fast_paths_match_pinned_c() {
    let cases = fast::cases();
    let observations = native::parallel_cases("vp8-fast", &cases, |case, output| {
        native::compare(case, output, &case.rust());
        let mut counts = [0usize; 9];
        if case.function == "VP8DistortionSearch" {
            // The full score and scratch blocks precede errors, contexts,
            // I4 rotation state and the four macroblock records.
            let mb_start =
                1 + support::vp8_reconstruct::SCORE_WORDS + 2 * 512 + 1792 + 12 + 18 + 2 + 37;
            let family = output[mb_start + 5 * (case.input[11] as usize & 3)] as usize;
            counts[2 * case.input[0] as usize + family] = 1;
            if case.input[0] == 2 && family != case.input[1] as usize {
                counts[6 + family] = 1;
            }
            if family == 1 && output[948..1460].iter().any(|&v| v != 0xc3) {
                counts[8] = 1;
            }
        }
        counts
    });
    let mut counts = [0; 9];
    for observed in observations {
        for (total, n) in counts.iter_mut().zip(observed) {
            *total += n;
        }
    }
    assert!(counts.iter().all(|&n| n > 0));
    eprintln!(
        "selected I4/I16 by method: {:?}; method-2 switches to I4/I16: {:?}; partial I4 early-outs: {}",
        &counts[..6],
        &counts[6..8],
        counts[8]
    );
    eprintln!(
        "{} exact fast analysis, distortion and raster requests",
        cases.len()
    );
}

#[test]
fn fast_family_thresholds_use_block_concentration_and_strict_ties() {
    // With n identical nonzero block sums, the concentration ratio is exactly n.
    // At quality 0 the cutoff is 8; just below 100 it is 16; at 100 it is 17.
    for (quality, cutoff) in [
        (0., 8),
        (11.99, 8),
        (12., 9),
        (75., 14),
        (99.99, 16),
        (100., 17),
    ] {
        for n in 0..=16 {
            let mut case = fast::fast_case(quality, 0);
            for block in 0..n {
                for y in 0..4 {
                    for x in 0..4 {
                        case.input[1 + (block / 4 * 4 + y) * 32 + block % 4 * 4 + x] = 100;
                    }
                }
            }
            let output = case.rust();
            assert_eq!(output[0], 0);
            assert_eq!(output[1], u32::from(n > cutoff), "q={quality} blocks={n}");
            // Fast analysis changes only the luma family/modes.
            assert_eq!(&output[2..6], &[3, 1, 3, 199]);
        }
    }
}

#[test]
fn flat_borders_force_i16_without_trying_i4() {
    for flags in [0, 1, 2] {
        for family in 0..2 {
            let case = fast::flat_border_case(family, flags);
            let (enc, it, rd, _) = fast::distortion_state(&case.input);
            assert_eq!(enc.mb_info[it.y * enc.mb_w + it.x].r#type, 1);
            assert_eq!(
                enc.preds[enc.preds_offset(it.x, it.y)],
                if flags == 1 { 2 } else { 0 }
            );
            assert_eq!(rd.modes_i4, support::vp8_reconstruct::seed_score().modes_i4);
            assert!(it.yuv_out2.iter().all(|&v| v == 0xc3));
        }
    }
}

#[test]
fn exact_header_limit_accepts_i4_and_one_less_keeps_partial_reconstruction() {
    let states: Vec<_> = fast::boundary_cases()
        .iter()
        .map(|case| fast::distortion_state(&case.input))
        .collect();
    for (n, (enc, it, _, _)) in states.iter().enumerate() {
        assert_eq!(enc.mb_info[it.y * enc.mb_w + it.x].r#type, u8::from(n == 0));
        assert_eq!(it.i4, if n == 0 { 15 } else { 16 });
    }
    assert!(states[0].1.yuv_out2.iter().any(|&v| v != 0xc3));
    assert_eq!(states[1].2.y_ac_levels, states[2].2.y_ac_levels);
    assert_eq!(states[1].1.yuv_out, states[2].1.yuv_out);
}

#[test]
fn equal_i16_scores_keep_the_earliest_mode() {
    use libwebpop::internal::{
        dsp::enc::BPS,
        enc::{cost_enc::VP8FixedCostsI16, quant_enc::VP8I16ModeOffsets},
    };
    let (enc, it, _, _) = fast::distortion_state(&fast::tie_case().input);
    let scores: Vec<_> = VP8I16ModeOffsets
        .iter()
        .enumerate()
        .map(|(mode, &at)| {
            let sse: i64 = (0..16)
                .flat_map(|y| (0..16).map(move |x| (y, x)))
                .map(|(y, x)| {
                    let d =
                        i64::from(it.yuv_in[y * BPS + x]) - i64::from(it.yuv_p[at + y * BPS + x]);
                    d * d
                })
                .sum();
            sse * 256 + i64::from(VP8FixedCostsI16[mode]) * 106
        })
        .collect();
    assert_eq!(scores[1], scores[3]);
    assert!(scores[1] < scores[0] && scores[1] < scores[2]);
    assert_eq!(enc.preds[enc.preds_offset(it.x, it.y)], 1);
}

#[test]
fn non_rd_preserves_seeded_modes_and_does_not_commit_diffusion() {
    let mut changed_errors = 0;
    for method in 0..3 {
        for family in 0..2 {
            for pattern in 0..10 {
                let case = fast::distortion_case(method, family, 63, 7, pattern);
                let (enc, it, rd, skipped) = fast::distortion_state(&case.input);
                let seed = support::vp8_reconstruct::seed_score();
                assert_eq!((rd.D, rd.SD, rd.R, rd.H), (0, 0, 0, 0));
                assert_eq!((rd.mode_i16, rd.mode_uv), (seed.mode_i16, seed.mode_uv));
                assert_eq!(skipped, rd.nz == 0);
                let mb = enc.mb_info[it.y * enc.mb_w + it.x];
                if method <= 1 {
                    assert_eq!(mb.r#type, family as u8);
                }
                if method == 0 {
                    assert_eq!(mb.uv_mode, case.input[2] as u8);
                }
                for ch in 0..2 {
                    for k in 0..2 {
                        assert_eq!(
                            enc.top_derr.as_ref().unwrap()[it.x][ch][k],
                            case.input[8 + 8 + 2 * ch + k] as i32 as i8
                        );
                        assert_eq!(
                            it.left_derr[ch][k],
                            case.input[8 + 12 + 2 * ch + k] as i32 as i8
                        );
                    }
                }
                changed_errors += usize::from(rd.derr != seed.derr);
                let mut refine = case.clone();
                refine.input[7] = 0;
                let (_, _, refined, _) = fast::distortion_state(&refine.input);
                assert_eq!(
                    (refined.D, refined.SD, refined.R, refined.H),
                    (seed.D, seed.SD, seed.R, seed.H)
                );
            }
        }
    }
    assert!(changed_errors > 0);
}

#[test]
fn one_segment_fast_analysis_initializes_predictions() {
    use libwebpop::internal::enc::analysis_enc::VP8EncAnalyze;
    for method in 0..3 {
        let frame = support::vp8_analysis::frame_case(32, 32, 1, 1);
        let pic = support::vp8_analysis::picture(&frame.input);
        let mut enc = support::vp8_analysis::seeded(32, 32, 1);
        enc.config.method = method;
        enc.config.quality = 100.;
        let stats = VP8EncAnalyze(&mut enc, &pic);
        if method <= 1 {
            assert_eq!(stats.alphas.iter().sum::<i32>(), 4);
            assert!(enc.mb_info.iter().all(|mb| mb.r#type == 0));
            assert!(enc.preds.iter().all(|&v| v == 0));
        } else {
            assert_eq!(stats.alphas, [0; 256]);
            assert!(enc.mb_info.iter().all(|mb| mb.r#type == 1));
            for y in 0..8 {
                for x in 0..8 {
                    assert_eq!(enc.preds[(y + 1) * enc.preds_w + x + 1], 7);
                }
            }
        }
    }
}

#[test]
fn raster_masks_match_levels_and_diffusion_stays_uncommitted() {
    let mut macroblocks = 0;
    let mut nonzero_errors = 0;
    for method in 0..3 {
        for pattern in 0..10 {
            let case = fast::frame_case(method, 75., 33, 35, 4, pattern);
            let output = case.rust();
            for p in output[6..6 + 9 * 1047].chunks_exact(1047) {
                let is16 = p[948] == 1;
                let mut mask = 0;
                for block in 0..24 {
                    let start = 27 + block * 16;
                    if block < 16 && is16 {
                        assert_eq!(p[start], 0);
                    }
                    mask |= u32::from(p[start..start + 16].iter().any(|&v| v != 0)) << block;
                }
                if is16 {
                    mask |= u32::from(p[11..27].iter().any(|&v| v != 0)) << 24;
                }
                assert_eq!(p[429], mask);
                assert_eq!(p[0], u32::from(mask == 0));
                assert_eq!(p[950], p[0]);
                assert!(p[1..9].iter().all(|&v| v == 0));
                assert!(p[972..980].iter().all(|&v| v == 0));
                nonzero_errors += usize::from(p[430..436].iter().any(|&v| v != 0));
                macroblocks += 1;
            }
        }
    }
    assert!(nonzero_errors > 0);
    assert_eq!(macroblocks, 270);
}
