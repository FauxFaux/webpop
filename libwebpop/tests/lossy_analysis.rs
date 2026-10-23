#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

use libwebpop::{
    dsp::enc::*,
    enc::{analysis_enc::*, iterator_enc::*, vp8i_enc::*},
};
use support::vp8_analysis as analysis;

#[test]
fn committed_analysis_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_analysis.txt"));
    let cases = analysis::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((case, expected), generated) in fixtures.iter().zip(cases) {
        assert_eq!(case.function, generated.function);
        assert_eq!(case.input, generated.input);
        assert_eq!(case.rust(), *expected);
    }
}

#[test]
fn susceptibility_ratios_clipping_and_signed_segment_scaling() {
    for case in analysis::scalar_cases() {
        let a = &case.input;
        let ratio = if a[0] < 2 {
            0
        } else {
            510u64 * u64::from(a[1]) / u64::from(a[0])
        };
        let clipped = if a[2] as i32 <= 0 {
            255
        } else if a[2] >= 255 {
            0
        } else {
            255 - a[2]
        };
        assert_eq!(case.rust(), [ratio as u32, clipped]);
    }
    for case in analysis::alpha_cases() {
        let a = &case.input;
        let output = case.rust();
        let nb = a[0] as usize;
        let lo = *a[2..2 + nb].iter().min().unwrap();
        let hi = (*a[2..2 + nb].iter().max().unwrap()).max(lo + 1);
        for n in 0..4 {
            let want = if n < nb {
                let distance = i64::from(a[2 + n]) - i64::from(a[1]);
                let magnitude = 255 * distance.unsigned_abs() / u64::from(hi - lo);
                let alpha = (if distance < 0 {
                    -(magnitude as i64)
                } else {
                    magnitude as i64
                })
                .clamp(-127, 127);
                [
                    alpha as i32 as u32,
                    ((u64::from(a[2 + n] - lo) * 255) / u64::from(hi - lo)).min(255) as u32,
                ]
            } else {
                [(n as i32 - 60) as u32, 70 + n as u32]
            };
            assert_eq!(&output[2 * n..2 * n + 2], &want);
        }
    }
}

// Exhaustively select among centers reachable by strict adjacent distance
// improvements, using rational rounding for centroid updates. A tied adjacent
// pair is a barrier even if a later center would be closer. Keep the assignments
// before the final update, as in the pin's ordered variant of Lloyd iteration.
fn cluster(hist: &[u32], nb: usize) -> ([i64; 4], Vec<usize>, i64, Vec<i64>) {
    let lo = hist.iter().position(|&v| v != 0).unwrap();
    let hi = hist.iter().rposition(|&v| v != 0).unwrap();
    let mut centers = [0i64; 4];
    for k in 0..nb {
        centers[k] = lo as i64 + ((hi - lo) * (2 * k + 1) / (2 * nb)) as i64;
    }
    let mut groups = vec![0; 256];
    let mut displacements = Vec::new();
    let mut middle = 0;
    for _ in 0..6 {
        let mut cursor = 0;
        for (value, &weight) in hist.iter().enumerate().filter(|(_, w)| **w != 0) {
            assert_ne!(weight, 0);
            let distance = centers.map(|c| (value as i64 - c).abs());
            groups[value] = (cursor..nb)
                .filter(|&k| (cursor + 1..=k).all(|j| distance[j] < distance[j - 1]))
                .min_by_key(|&k| (distance[k], k))
                .unwrap();
            cursor = groups[value];
        }
        let old = centers;
        let mut weight_sum = 0i64;
        let mut centroid_sum = 0i64;
        for k in 0..nb {
            let members: Vec<_> = hist
                .iter()
                .enumerate()
                .filter(|(v, w)| **w != 0 && groups[*v] == k)
                .map(|(v, &w)| (v as i64, i64::from(w)))
                .collect();
            let weight: i64 = members.iter().map(|m| m.1).sum();
            if weight != 0 {
                let dot: i64 = members.iter().map(|&(v, w)| v * w).sum();
                centers[k] = (2 * dot + weight) / (2 * weight);
                weight_sum += weight;
                centroid_sum += centers[k] * weight;
            }
        }
        middle = (2 * centroid_sum + weight_sum) / (2 * weight_sum);
        let moved = (0..nb).map(|k| (old[k] - centers[k]).abs()).sum();
        displacements.push(moved);
        if moved < 5 {
            break;
        }
    }
    (centers, groups, middle, displacements)
}

#[test]
fn segment_assignment_matches_weighted_lloyd_and_keeps_preupdate_ties() {
    let mut six_rounds = 0;
    let mut cutoff_ties = 0;
    let mut empty_clusters = 0;
    for case in analysis::assign_cases() {
        let nb = case.input[0] as usize;
        let hist = &case.input[1..];
        let (centers, groups, mid, moved) = cluster(hist, nb);
        six_rounds += usize::from(moved.len() == 6);
        cutoff_ties += moved.iter().filter(|&&v| v == 5).count();
        empty_clusters += (0..nb)
            .filter(|&k| {
                !hist
                    .iter()
                    .enumerate()
                    .any(|(v, &w)| w != 0 && groups[v] == k)
            })
            .count();
        let mut expected = Vec::new();
        let lo = *centers[..nb].iter().min().unwrap();
        let hi = (*centers[..nb].iter().max().unwrap()).max(lo + 1);
        for k in 0..4 {
            expected.extend(if k < nb {
                [
                    ((255 * (centers[k] - mid) / (hi - lo)).clamp(-127, 127)) as i32 as u32,
                    ((255 * (centers[k] - lo) / (hi - lo)).clamp(0, 255)) as u32,
                ]
            } else {
                [(k as i32 - 60) as u32, 70 + k as u32]
            });
        }
        for (value, &weight) in hist.iter().enumerate() {
            if weight != 0 {
                let k = groups[value];
                expected.extend([0, 3, 1, k as u32, centers[k] as u32]);
            }
        }
        assert_eq!(case.rust(), expected);
    }
    assert!(six_rounds > 0 && cutoff_ties > 0 && empty_clusters > 0);
    let uniform = analysis::assign_case(4, [1; 256]);
    let out = uniform.rust();
    // After one round, centers move from 31/95/159/223 to 32/96/160/224.
    // The four-unit total displacement stops iteration. Alpha 64 keeps segment
    // 1 even though it now ties between centers 32 and 96 (earliest would be 0).
    assert_eq!(&out[8 + 64 * 5 + 3..8 + 64 * 5 + 5], &[1, 96]);
    let tied = analysis::tied_centers_case().rust();
    // Centers start at [0,0,1,1]. The first tie prevents moving past center 0,
    // so both occupied bins (0 and 2) stay in segment 0 and its center becomes 2.
    assert_eq!(&tied[8..], &[0, 3, 1, 0, 2, 0, 3, 1, 0, 2]);
    eprintln!(
        "{six_rounds} six-round assignments, {cutoff_ties} exact cutoff decisions, {empty_clusters} empty clusters"
    );
}

fn independent_scores(
    input: &[u8; YUV_SIZE_ENC],
    flags: u32,
    left_y: &[u8; 17],
    left_uv: &[u8; 25],
    top_y: &[u8],
    top_uv: &[u8],
) -> ([i32; 2], [i32; 2]) {
    let mut bins = [[[0u32; 32]; 2]; 2];
    for ch in 0..3 {
        let (size, offset, left, top) = if ch == 0 {
            (16, 0, &left_y[..], top_y)
        } else {
            (
                8,
                16 + 8 * (ch - 1),
                &left_uv[16 * (ch - 1)..],
                &top_uv[8 * (ch - 1)..],
            )
        };
        let borders: Vec<_> = (if flags & 1 != 0 {
            &left[1..1 + size]
        } else {
            &[]
        })
        .iter()
        .chain(if flags & 2 != 0 { &top[..size] } else { &[] })
        .copied()
        .collect();
        let dc = if borders.is_empty() {
            128
        } else {
            (borders.iter().map(|&v| u32::from(v)).sum::<u32>() + borders.len() as u32 / 2)
                / borders.len() as u32
        } as u8;
        for mode in 0..2 {
            let mut pred = [0; YUV_SIZE_ENC];
            for y in 0..size {
                for x in 0..size {
                    pred[y * BPS + x] = if mode == 0 {
                        dc
                    } else {
                        match flags {
                            0 => 129,
                            1 => left[y + 1],
                            2 => top[x],
                            _ => (i32::from(left[y + 1]) + i32::from(top[x]) - i32::from(left[0]))
                                .clamp(0, 255) as u8,
                        }
                    };
                }
            }
            for y in (0..size).step_by(4) {
                for x in (0..size).step_by(4) {
                    let mut coeffs = [0; 16];
                    FTransform_C(
                        &input[offset + y * BPS + x..],
                        &pred[y * BPS + x..],
                        &mut coeffs,
                    );
                    for v in coeffs {
                        bins[usize::from(ch != 0)][mode]
                            [((v.unsigned_abs() as usize) / 8).min(31)] += 1;
                    }
                }
            }
        }
    }
    let scores = bins.map(|modes| {
        modes.map(|hist| {
            let max = *hist.iter().max().unwrap();
            let last = hist.iter().rposition(|&v| v != 0).unwrap();
            if max <= 1 {
                0
            } else {
                (510 * last as u64 / u64::from(max)) as i32
            }
        })
    });
    (scores[0], scores[1])
}

#[test]
fn analysis_probes_choose_opposite_luma_uv_extrema_and_preserve_scratch() {
    let mut luma_modes = [0; 2];
    let mut uv_modes = [0; 2];
    let mut ties = 0;
    let mut opposite = 0;
    let mut zero_alpha = 0;
    for case in analysis::probe_cases() {
        let (mut enc, mut it) = analysis::probe(&case.input);
        let (luma, uv) = independent_scores(
            &it.yuv_in,
            case.input[0],
            &it.y_left,
            &it.uv_left,
            it.y_top(&enc),
            it.uv_top(&enc),
        );
        let luma_mode = usize::from(luma[1] > luma[0]);
        let uv_mode = usize::from(uv[1] < uv[0]);
        ties += usize::from(luma[0] == luma[1] || uv[0] == uv[1]);
        opposite += usize::from(uv[0] != uv[1]);
        luma_modes[luma_mode] += 1;
        uv_modes[uv_mode] += 1;
        let expected = (255 - (3 * luma[luma_mode] + uv[0].max(uv[1]) + 2) / 4).clamp(0, 255) as u8;
        zero_alpha += usize::from(expected == 0);
        let before = it.clone();
        let mut stats = VP8AnalysisStats::default();
        MBAnalyze(&mut it, &mut enc, &mut stats);
        let mb = &enc.mb_info[it.y * enc.mb_w + it.x];
        assert_eq!(
            (mb.r#type, mb.uv_mode, mb.skip, mb.segment, mb.alpha),
            (1, uv_mode as u8, false, 0, expected)
        );
        assert_eq!(stats.alphas[usize::from(expected)], 1);
        assert_eq!(stats.alphas.iter().sum::<i32>(), 1);
        assert_eq!(
            (stats.alpha_sum, stats.uv_alpha_sum),
            (i64::from(expected), i64::from(uv[0].max(uv[1])))
        );
        let start = enc.preds_offset(it.x, it.y);
        for row in 0..4 {
            assert_eq!(
                &enc.preds[start + row * enc.preds_w..start + row * enc.preds_w + 4],
                &[luma_mode as u8; 4]
            );
        }
        assert_eq!(it.yuv_in, before.yuv_in);
        assert_eq!(it.yuv_out, before.yuv_out);
        assert_eq!(it.yuv_out2, before.yuv_out2);
        assert_eq!(&it.yuv_p[48 * BPS..], &[0xa5; 8 * BPS]);
        assert_eq!(it.y_left, before.y_left);
        assert_eq!(it.uv_left, before.uv_left);
    }
    assert!(luma_modes.iter().chain(&uv_modes).all(|&v| v > 0));
    assert!(ties > 0 && opposite > 0 && zero_alpha > 0);
    eprintln!(
        "luma modes {luma_modes:?}, UV modes {uv_modes:?}, {ties} ties, {opposite} distinct UV scores, {zero_alpha} clipped blocks"
    );
}

#[test]
fn analysis_accumulators_preserve_wide_sums() {
    let case = analysis::probe_case(3, 259);
    let (mut enc, mut it) = analysis::probe(&case.input);
    let mut one = VP8AnalysisStats::default();
    MBAnalyze(&mut it, &mut enc, &mut one);
    let mut wide = VP8AnalysisStats {
        alpha_sum: 1i64 << 32,
        uv_alpha_sum: i64::from(i32::MAX),
        ..Default::default()
    };
    MBAnalyze(&mut it, &mut enc, &mut wide);
    assert_eq!(wide.alphas, one.alphas);
    assert_eq!(wide.alpha_sum, (1i64 << 32) + one.alpha_sum);
    assert_eq!(wide.uv_alpha_sum, i64::from(i32::MAX) + one.uv_alpha_sum);
    assert!(wide.uv_alpha_sum > i64::from(i32::MAX));
}

#[test]
fn full_analysis_conserves_histograms_and_matches_independent_block_and_cluster_results() {
    let mut blocks = 0;
    for case in analysis::frame_cases() {
        let a = &case.input;
        let pic = analysis::picture(a);
        let original = pic.clone();
        let mut enc = analysis::seeded(a[0], a[1], a[2] as usize);
        let original_dqm = enc.dqm.clone();
        let original_preds = enc.preds.clone();
        let mut hist = [0u32; 256];
        let mut raw = Vec::new();
        let mut uv_total = 0i64;
        let mut alpha_total = 0i64;
        if a[2] > 1 {
            let mut it = VP8EncIterator::default();
            VP8IteratorInit(&mut enc, &mut it);
            loop {
                VP8IteratorImport(&mut it, &enc, &pic, true);
                let flags = u32::from(it.x != 0) + 2 * u32::from(it.y != 0);
                let (luma, uv) = independent_scores(
                    &it.yuv_in,
                    flags,
                    &it.y_left,
                    &it.uv_left,
                    it.y_top(&enc),
                    it.uv_top(&enc),
                );
                let lm = usize::from(luma[1] > luma[0]);
                let um = usize::from(uv[1] < uv[0]);
                let alpha = (255 - (3 * luma[lm] + uv[0].max(uv[1]) + 2) / 4).clamp(0, 255) as u8;
                hist[usize::from(alpha)] += 1;
                alpha_total += i64::from(alpha);
                uv_total += i64::from(uv[0].max(uv[1]));
                raw.push((alpha, lm, um));
                blocks += 1;
                if !VP8IteratorNext(&mut it, &enc) {
                    break;
                }
            }
        }
        let stats = VP8EncAnalyze(&mut enc, &pic);
        assert_eq!(stats.alphas.map(|v| v as u32), hist);
        assert_eq!(
            (stats.alpha_sum, stats.uv_alpha_sum),
            (alpha_total, uv_total)
        );
        if a[2] > 1 {
            assert_eq!(hist.iter().sum::<u32>() as usize, enc.mb_info.len());
            assert_eq!(
                (enc.alpha, enc.uv_alpha),
                (
                    alpha_total as i32 / enc.mb_info.len() as i32,
                    uv_total as i32 / enc.mb_info.len() as i32
                )
            );
            let (centers, groups, _, _) = cluster(&hist, a[2] as usize);
            for (i, mb) in enc.mb_info.iter().enumerate() {
                let (alpha, lm, um) = raw[i];
                let group = groups[usize::from(alpha)];
                assert_eq!(
                    (mb.r#type, mb.uv_mode, mb.skip, mb.segment, mb.alpha),
                    (1, um as u8, false, group as u8, centers[group] as u8)
                );
                let start = enc.preds_offset(i % enc.mb_w, i / enc.mb_w);
                for row in 0..4 {
                    assert_eq!(
                        &enc.preds[start + row * enc.preds_w..start + row * enc.preds_w + 4],
                        &[lm as u8; 4]
                    );
                }
            }
        } else {
            assert_eq!((enc.alpha, enc.uv_alpha), (0, 0));
            assert_eq!(enc.preds, original_preds);
            assert_eq!((enc.dqm[0].alpha, enc.dqm[0].beta), (0, 0));
            assert_eq!(&enc.dqm[1..], &original_dqm[1..]);
            assert!(enc.mb_info.iter().all(|mb| *mb
                == VP8MBInfo {
                    r#type: 1,
                    ..Default::default()
                }));
        }
        assert_eq!(pic, original);
        assert_eq!(
            enc.segment_hdr,
            VP8EncSegmentHeader {
                num_segments: a[2] as usize,
                update_map: a[2] > 1,
                size: 37
            }
        );
    }
    eprintln!("{blocks} independently checked analysis macroblocks");
}

#[cfg(feature = "native-oracle")]
#[test]
fn analysis_and_segments_match_pinned_reference() {
    let cases = analysis::cases();
    native::parallel_cases("vp8-analysis", &cases, |case, output| {
        native::compare(case, output, &case.rust())
    });
    eprintln!(
        "{} exact susceptibility/segment/analysis requests",
        cases.len()
    );
}
#[cfg(feature = "native-oracle")]
#[test]
fn analysis_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = analysis::fixture_cases();
    let outputs = native::run("vp8-analysis-fixtures", &cases);
    for (case, output) in cases.iter().zip(&outputs) {
        native::compare(case, output, &case.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C susceptibility/segments/default analysis.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
                "/tests/fixtures/lossy_analysis.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_analysis.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, output)) in fixtures.iter().zip(cases.iter().zip(&outputs))
        {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, output);
        }
    }
}
