#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use support::vp8_trellis_modes as trellis;

#[cfg(feature = "native-oracle")]
#[test]
fn trellis_mode_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = trellis::fixture_cases();
    let outputs = native::run("lossy-trellis-mode-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C trellis reconstruction and modes (methods milestone 6).\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for protocol and provenance.\n",
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
                "/tests/fixtures/lossy_trellis_modes.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_trellis_modes.txt"));
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
    eprintln!("{} trellis fixtures match pinned C", cases.len());
}

#[test]
fn committed_trellis_mode_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_trellis_modes.txt"));
    let cases = trellis::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        let actual = case.rust();
        assert_eq!(
            expected.len(),
            actual.len(),
            "{} {:?}",
            case.function,
            &case.input[..5]
        );
        for (i, (a, b)) in expected.iter().zip(&actual).enumerate() {
            assert_eq!(a, b, "{} method {} word {i}", case.function, case.input[0]);
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn trellis_reconstruction_and_search_match_c() {
    let cases: Vec<_> = trellis::reconstruction_cases()
        .into_iter()
        .chain(trellis::search_cases())
        .collect();
    native::parallel_cases("lossy-trellis-modes", &cases, |case, out| {
        native::compare(case, out, &case.rust());
    });
    eprintln!(
        "{} exact trellis reconstruction/search requests",
        cases.len()
    );
}

#[cfg(feature = "native-oracle")]
#[test]
fn trellis_frame_traces_match_c() {
    let cases = trellis::loop_cases();
    native::parallel_cases("lossy-trellis-mode-loops", &cases, |case, out| {
        native::compare(case, out, &support::lossy_methods::driver_output(case));
    });
    eprintln!("{} exact trellis frame traces", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn methods_5_6_opaque_files_match_normal_c_and_decode() {
    let cases: Vec<_> = trellis::public_cases()
        .iter()
        .map(|c| {
            let a = &c.input;
            let mut input = vec![a[1], a[2], a[9], a[3], 100, a[0], 0];
            input.extend_from_slice(&a[10..]);
            support::Case::new("WebPEncodeLossyMethod", input)
        })
        .collect();
    native::parallel_cases("lossy-trellis-modes-public", &cases, |case, out| {
        let a = &case.input;
        let mut input = vec![
            a[0],
            a[1],
            a[3],
            4,
            65536,
            libwebpop::internal::enc::frame_enc::PARTITION0_SIZE_LIMIT as u32,
            1,
            0,
            a[2],
        ];
        input.extend_from_slice(&a[7..]);
        let actual = support::vp8_loop::encode_method(&input, a[5] as i32, true).0;
        native::compare(case, out, &actual);
        support::lossy_methods::check_decoded(case, out);
    });
    eprintln!(
        "{} method 5/6 opaque files match normal C and independently decode",
        cases.len()
    );
}

use libwebpop::internal::{
    dsp::enc::*,
    enc::{
        analysis_enc::VP8EncAnalyze, cost_enc::VP8CalculateLevelCosts, frame_enc::*,
        iterator_enc::*, quant_enc::*, vp8i_enc::*,
    },
};
use support::{vp8_reconstruct as reconstruct, vp8_search as search};

#[test]
fn trellis_luma16_composes_scalar_dc_and_raster_ac_contexts() {
    let mut coverage = [0usize; 3];
    for case in trellis::reconstruction_cases()
        .into_iter()
        .filter(|c| c.input[1] == 0)
        .step_by(3)
    {
        let a = &case.input[1..];
        let method = case.input[0] as i32;
        let (mut enc, mut it) = reconstruct::initialized_with_method(a, method);
        VP8CalculateLevelCosts(&mut enc.proba);
        it.do_trellis = true;
        VP8MakeLuma16Preds(&mut it, &enc);
        let m = &enc.dqm[a[2] as usize];
        let mode = a[4] as usize;
        let reference = &it.yuv_p[VP8I16ModeOffsets[mode]..];
        let mut coeffs = [[0i16; 16]; 16];
        for n in 0..16 {
            FTransform_C(
                &it.yuv_in[VP8Scan[n]..],
                &reference[VP8Scan[n]..],
                &mut coeffs[n],
            );
        }
        // A separate 4x4 Hadamard matrix checks the WHT and its rounding.
        let h = [
            [1i64, 1, 1, 1],
            [1, 1, -1, -1],
            [1, -1, -1, 1],
            [1, -1, 1, -1],
        ];
        let mut dc = std::array::from_fn(|n| {
            let mut sum = 0i64;
            for y in 0..4 {
                for x in 0..4 {
                    sum += h[n / 4][y] * i64::from(coeffs[4 * y + x][0]) * h[n % 4][x];
                }
            }
            sum.div_euclid(2) as i16
        });
        let mut expected = reconstruct::seed_score();
        let mut nz = u32::from(QuantizeBlock_C(&mut dc, &mut expected.y_dc_levels, &m.y2)) << 24;
        let mut top: [u8; 4] = std::array::from_fn(|x| ((enc.nz[it.x + 1] >> (12 + x)) & 1) as u8);
        let mut left: [u8; 4] = std::array::from_fn(|y| ((enc.nz[it.x] >> (3 + 4 * y)) & 1) as u8);
        for y in 0..4 {
            for x in 0..4 {
                let n = 4 * y + x;
                let context = usize::from(top[x] + left[y]);
                let nonzero = TrellisQuantizeBlock(
                    &enc,
                    &mut coeffs[n],
                    &mut expected.y_ac_levels[n],
                    context,
                    0,
                    &m.y1,
                    m.lambda_trellis_i16,
                );
                expected.y_ac_levels[n][0] = 0;
                top[x] = u8::from(nonzero);
                left[y] = u8::from(nonzero);
                nz |= u32::from(nonzero) << n;
            }
        }
        let inverse: [i16; 16] = std::array::from_fn(|n| {
            let mut sum = 0i64;
            for y in 0..4 {
                for x in 0..4 {
                    sum += h[n / 4][y] * i64::from(dc[4 * y + x]) * h[n % 4][x];
                }
            }
            (sum + 3).div_euclid(8) as i16
        });
        let mut pixels = [0xa5; 512];
        for n in 0..16 {
            coeffs[n][0] = inverse[n];
            ITransform_C(
                &reference[VP8Scan[n]..],
                &coeffs[n],
                &mut pixels[VP8Scan[n]..],
                false,
            );
        }
        let packed = enc.nz.clone();
        let errors = enc.top_derr.clone();
        let mut rd = reconstruct::seed_score();
        let mut actual = [0xa5; 512];
        let mask = ReconstructIntra16(&mut it, &enc, &mut rd, &mut actual, mode);
        assert_eq!(rd, expected);
        assert_eq!(actual, pixels);
        assert_eq!(mask, nz);
        assert_eq!(&it.top_nz[..4], &top);
        assert_eq!(&it.left_nz[..4], &left);
        assert_eq!(enc.nz, packed);
        assert_eq!(enc.top_derr, errors);
        coverage[0] += 1;
        coverage[1] += usize::from(nz == 0);
        coverage[2] += usize::from(nz & 0xffff != 0);
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!("independent luma16 composition: blocks/zero/nonzero AC = {coverage:?}");
}

#[test]
fn selected_requantization_preserves_scores_modes_and_committed_diffusion() {
    let mut coverage = [0usize; 3];
    for case in trellis::search_cases()
        .into_iter()
        .filter(|c| c.input[1 + 634] == 6)
        .step_by(3)
    {
        let a = &case.input[1..];
        let (mut enc, mut it) = search::initialized_with_method(a, case.input[0] as i32);
        let index = it.y * enc.mb_w + it.x;
        enc.mb_info[index].r#type = a[0] as u8;
        enc.mb_info[index].uv_mode = ((a[4] + 1) % 4) as u8;
        if a[0] != 0 {
            VP8SetIntra16Mode(&it, &mut enc, a[4] as u8);
        }
        it.do_trellis = true;
        VP8MakeLuma16Preds(&mut it, &enc);
        VP8MakeChroma8Preds(&mut it, &enc);
        let before = reconstruct::seed_score();
        let mut rd = before.clone();
        let mb = enc.mb_info.clone();
        let preds = enc.preds.clone();
        let nz = enc.nz.clone();
        let errors = enc.top_derr.clone();
        let left_errors = it.left_derr;
        let edges = enc.dqm.iter().map(|m| m.max_edge).collect::<Vec<_>>();
        let top: [u8; 9] = std::array::from_fn(|n| {
            ((enc.nz[it.x + 1] >> [12, 13, 14, 15, 18, 19, 22, 23, 24][n]) & 1) as u8
        });
        let left: [u8; 8] = std::array::from_fn(|n| {
            ((enc.nz[it.x] >> [3, 7, 11, 15, 17, 19, 21, 23][n]) & 1) as u8
        });
        SimpleQuantize(&mut it, &enc, &mut rd);
        assert_eq!(
            [rd.D, rd.SD, rd.H, rd.R, rd.score],
            [before.D, before.SD, before.H, before.R, before.score]
        );
        assert_eq!(
            (rd.mode_i16, rd.mode_uv, rd.modes_i4),
            (before.mode_i16, before.mode_uv, before.modes_i4)
        );
        assert_eq!(enc.mb_info, mb);
        assert_eq!(enc.preds, preds);
        assert_eq!(enc.nz, nz);
        assert_eq!(enc.top_derr, errors);
        assert_eq!(it.left_derr, left_errors);
        assert_eq!(
            enc.dqm.iter().map(|m| m.max_edge).collect::<Vec<_>>(),
            edges
        );
        if a[0] == 0 {
            // I4 uses incoming contexts on every block in this pass, including
            // contexts for blocks already reconstructed earlier in the scan.
            assert_eq!(it.top_nz, top);
            assert_eq!(&it.left_nz[..8], &left);
        }
        coverage[usize::from(a[0] != 0)] += 1;
        coverage[2] += usize::from(rd.y_ac_levels != before.y_ac_levels);
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!("selected requantization invariants: I4/I16/changed levels = {coverage:?}");
}

#[test]
fn candidate_order_and_refreshed_costs_follow_selected_rd_level() {
    let mut coverage = [0usize; 6]; // macroblocks, selected I16, selected I4, refresh, retries, changed fitted costs
    for method in [5, 6] {
        for (w, h, quality, pattern, retry) in [
            (97 * 16, 1, 75., 4, false),
            (193, 129, 98.5, 2, false),
            (776 * 16, 1, 75., 4, false),
            (97 * 16, 1, 75., 4, true),
        ] {
            let case = support::vp8_loop::frame_case(w, h, quality, 4, pattern, true);
            let pic = support::vp8_loop::picture(&case.input);
            let mut enc = VP8Encoder::new_with_method(w, h, quality <= 98., method).unwrap();
            enc.config.quality = quality;
            VP8EncAnalyze(&mut enc, &pic);
            if retry {
                enc.max_i4_header_bits = 1;
            }
            let mut calls = Vec::new();
            let mut first_index = true;
            let mut changed = false;
            VP8EncTokenLoopWithObserver(
                &mut enc,
                &pic,
                quality,
                if retry { 0 } else { PARTITION0_SIZE_LIMIT },
                |enc, event| match event {
                    VP8TokenLoopEvent::PassStart { .. } => {
                        calls.clear();
                        first_index = true;
                    }
                    VP8TokenLoopEvent::Refresh { .. } => {
                        coverage[3] += 1;
                        let mut expected = enc.proba.clone();
                        expected.dirty = true;
                        VP8CalculateLevelCosts(&mut expected);
                        assert_eq!(enc.proba.level_cost, expected.level_cost);
                        assert!(!enc.proba.dirty);
                        changed =
                            enc.proba.coeffs != libwebpop::internal::enc::tree_enc::VP8CoeffsProba0;
                    }
                    VP8TokenLoopEvent::Reconstruct {
                        reconstruction: r, ..
                    } => {
                        assert!(!enc.proba.dirty);
                        if calls.is_empty() && changed {
                            coverage[5] += 1;
                        }
                        calls.push(r);
                    }
                    VP8TokenLoopEvent::Macroblock {
                        iterator: it,
                        index,
                        ..
                    } => {
                        coverage[0] += 1;
                        assert_eq!(first_index, index == 0);
                        first_index = false;
                        assert_eq!(
                            calls[..4]
                                .iter()
                                .map(|r| (r.kind, r.mode, r.trellis))
                                .collect::<Vec<_>>(),
                            (0..4).map(|m| (0, m, method == 6)).collect::<Vec<_>>()
                        );
                        let uv = calls.iter().position(|r| r.kind == 2).unwrap();
                        assert!(
                            calls[4..uv]
                                .iter()
                                .all(|r| r.kind == 1 && r.trellis == (method == 6))
                        );
                        assert_eq!(
                            calls[uv..uv + 4]
                                .iter()
                                .map(|r| (r.kind, r.mode, r.trellis))
                                .collect::<Vec<_>>(),
                            (0..4).map(|m| (2, m, method == 6)).collect::<Vec<_>>()
                        );
                        if method == 5 {
                            let selected = &calls[uv + 4..];
                            let mb = enc.mb_info[index];
                            assert!(selected.iter().all(|r| r.trellis));
                            assert_eq!(selected.last().unwrap().kind, 2);
                            if mb.r#type == 1 {
                                coverage[1] += 1;
                                assert_eq!(selected.len(), 2);
                                assert_eq!(selected[0].kind, 0);
                            } else {
                                coverage[2] += 1;
                                assert_eq!(selected.len(), 17);
                                assert_eq!(
                                    selected[..16]
                                        .iter()
                                        .map(|r| (r.kind, r.block))
                                        .collect::<Vec<_>>(),
                                    (0..16).map(|n| (1, n)).collect::<Vec<_>>()
                                );
                            }
                        } else {
                            assert_eq!(calls.len(), uv + 4);
                        }
                        assert!(it.do_trellis);
                        calls.clear();
                    }
                    VP8TokenLoopEvent::PassEnd { retry, .. } => coverage[4] += usize::from(retry),
                    _ => {}
                },
            )
            .unwrap();
        }
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!(
        "trellis frame branches: blocks/selected I16/I4/refresh/retries/refreshed blocks = {coverage:?}"
    );
}

#[test]
fn selected_trellis_keeps_basic_decisions_while_candidate_trellis_can_change_them() {
    let mut coverage = [0usize; 3];
    for q in [0, 1, 17, 37, 63, 75, 98, 127] {
        for flags in [3, 7] {
            for pattern in [0, 2, 4, 6] {
                let a = search::search_case(2, q, flags, pattern).input;
                let (mut basic, mut bi) = search::initialized_with_method(&a, 5);
                let (mut selected, mut si) = search::initialized_with_method(&a, 5);
                let (mut candidate, mut ci) = search::initialized_with_method(&a, 6);
                let mut b = reconstruct::seed_score();
                let mut s = b.clone();
                let mut c = b.clone();
                VP8Decimate(&mut bi, &mut basic, &mut b, VP8RDLevel::RD_OPT_BASIC);
                VP8Decimate(&mut si, &mut selected, &mut s, VP8RDLevel::RD_OPT_TRELLIS);
                VP8Decimate(
                    &mut ci,
                    &mut candidate,
                    &mut c,
                    VP8RDLevel::RD_OPT_TRELLIS_ALL,
                );
                assert_eq!(
                    [s.D, s.SD, s.H, s.R, s.score],
                    [b.D, b.SD, b.H, b.R, b.score]
                );
                assert_eq!(
                    (s.mode_i16, s.mode_uv, s.modes_i4),
                    (b.mode_i16, b.mode_uv, b.modes_i4)
                );
                assert_eq!(selected.preds, basic.preds);
                assert_eq!(selected.top_derr, basic.top_derr);
                assert_eq!(si.left_derr, bi.left_derr);
                assert_eq!(selected.dqm, basic.dqm);
                assert_eq!(selected.proba, basic.proba);
                let index = bi.y * basic.mb_w + bi.x;
                assert_eq!(selected.mb_info[index].r#type, basic.mb_info[index].r#type);
                assert_eq!(
                    selected.mb_info[index].uv_mode,
                    basic.mb_info[index].uv_mode
                );
                coverage[0] += 1;
                coverage[1] += usize::from(s.y_ac_levels != b.y_ac_levels);
                coverage[2] += usize::from(
                    selected.preds != candidate.preds
                        || s.mode_uv != c.mode_uv
                        || selected.mb_info[index].r#type != candidate.mb_info[index].r#type,
                );
            }
        }
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!(
        "RD dispatch: comparisons/selected level changes/candidate mode changes = {coverage:?}"
    );
}

#[test]
fn chroma_reconstruction_ignores_trellis_flag() {
    for case in trellis::reconstruction_cases()
        .into_iter()
        .filter(|c| c.input[1] == 2)
        .step_by(7)
    {
        let a = &case.input[1..];
        let (enc, mut it) = reconstruct::initialized_with_method(a, case.input[0] as i32);
        VP8MakeChroma8Preds(&mut it, &enc);
        let mut other = it.clone();
        it.do_trellis = true;
        other.do_trellis = false;
        let mut rd = reconstruct::seed_score();
        let mut scalar = rd.clone();
        let mut dst = [0xa5; 512];
        let mut expected = dst;
        let mask = ReconstructUV(&it, &enc, &mut rd, &mut dst[16..], a[4] as usize);
        let scalar_mask = ReconstructUV(
            &other,
            &enc,
            &mut scalar,
            &mut expected[16..],
            a[4] as usize,
        );
        assert_eq!((mask, rd, dst), (scalar_mask, scalar, expected));
    }
}
