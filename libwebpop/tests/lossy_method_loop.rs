#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;

#[cfg(feature = "native-oracle")]
#[test]
fn method_loop_fixtures_match_pinned_c() {
    use std::fmt::Write;
    let cases = support::lossy_methods::driver_fixture_cases();
    let outputs = native::run("lossy-method-loop-fixtures", &cases);
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C method 0-4 frame drivers (milestone 4).\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for protocol and provenance.\n",
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
                "/tests/fixtures/lossy_method_loop.txt"
            ),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_method_loop.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
    for (case, out) in cases.iter().zip(&outputs) {
        native::compare(case, out, &support::lossy_methods::driver_output(case));
    }
    eprintln!("{} method-driver fixtures match pinned C", cases.len());
}

#[test]
fn committed_method_driver_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_method_loop.txt"));
    let cases = support::lossy_methods::driver_fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((stored, expected), case) in fixtures.iter().zip(cases) {
        assert_eq!(stored.function, case.function);
        assert_eq!(stored.input, case.input);
        let actual = support::lossy_methods::driver_output(&case);
        assert_eq!(
            expected.len(),
            actual.len(),
            "{} method {}",
            case.function,
            case.input[0]
        );
        for (i, (c, rust)) in expected.iter().zip(&actual).enumerate() {
            assert_eq!(
                c, rust,
                "{} method {} word {i}",
                case.function, case.input[0]
            );
        }
    }
}

#[cfg(feature = "native-oracle")]
#[test]
fn method_drivers_match_pinned_c_state_and_files() {
    let cases = support::lossy_methods::driver_cases();
    native::parallel_cases("lossy-method-drivers", &cases, |case, out| {
        native::compare(case, out, &support::lossy_methods::driver_output(case));
    });
    eprintln!("{} exact method-driver traces", cases.len());
}

#[cfg(feature = "native-oracle")]
#[test]
fn methods_0_to_3_opaque_files_match_normal_c_and_decode() {
    let cases: Vec<_> = support::lossy_methods::driver_rgba_cases()
        .iter()
        .map(|c| {
            let a = &c.input;
            let mut input = vec![a[1], a[2], a[9], a[3], 100, a[0], 0];
            input.extend_from_slice(&a[10..]);
            support::Case::new("WebPEncodeLossyMethod", input)
        })
        .collect();
    native::parallel_cases("lossy-method-driver-public", &cases, |case, out| {
        let a = &case.input;
        let mut input = vec![
            a[0],
            a[1],
            a[3],
            4,
            65536,
            libwebpop::enc::frame_enc::PARTITION0_SIZE_LIMIT as u32,
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
        "{} opaque method 0-3 files match normal C and independently decode",
        cases.len()
    );
}

use libwebpop::{
    enc::{analysis_enc::VP8EncAnalyze, frame_enc::*, vp8i_enc::*},
    utils::bit_writer_utils::*,
};
use support::{vp8_boolean::Decoder, vp8_cost as cost, vp8_tokens as tokens};

#[test]
fn direct_scans_reset_borders_fit_sampled_statistics_and_decode_residuals() {
    let mut coverage = [0usize; 6]; // statistics, coded, omitted, I4, retries, rows
    for case in support::lossy_methods::driver_cases()
        .into_iter()
        .enumerate()
        .filter(|(i, c)| c.input[0] < 3 && (i % 11 == 0 || c.input[1] >= 49 * 16))
        .map(|(_, c)| c)
    {
        let method = case.input[0];
        let a = &case.input[1..];
        let quality = f32::from_bits(a[2]);
        let pic = support::vp8_loop::picture(a);
        let mut enc =
            VP8Encoder::new_with_method(a[0], a[1], quality <= 98., method as i32).unwrap();
        enc.config.quality = quality;
        enc.segment_hdr.num_segments = a[3] as usize;
        enc.max_i4_header_bits = a[4] as i32;
        VP8EncAnalyze(&mut enc, &pic);
        let total = enc.mb_info.len();
        let probe = if method == 0 {
            total.min(if total > 200 { total / 4 } else { 50 })
        } else {
            total
        };
        let mut top_y = vec![[0u8; 4]; enc.mb_w];
        let mut top_uv = vec![[0u8; 4]; enc.mb_w];
        let mut top_dc = vec![0u8; enc.mb_w];
        let (mut left_y, mut left_uv, mut left_dc) = ([0u8; 4], [0u8; 4], 0u8);
        let mut stats = vec![0u32; 1056];
        let mut bits = Vec::<(bool, u8)>::new();
        let (mut visited, mut skips) = (0, 0);
        let mut budget = a[4];
        let bw = VP8EncLoopWithObserver(&mut enc, &pic, quality, u64::from(a[5]), |enc, event| {
            match event {
                VP8DirectLoopEvent::ScanStart { statistics, .. } => {
                    assert!(enc.y_top.iter().chain(&enc.uv_top).all(|&v| v == 127));
                    assert!(enc.nz.iter().all(|&v| v == 0));
                    if let Some(top) = &enc.top_derr {
                        assert!(top.iter().flatten().flatten().all(|&v| v == 0));
                    }
                    assert_eq!(cost::stats_words(&enc.proba), stats);
                    assert_eq!(enc.max_i4_header_bits as u32, budget);
                    if statistics {
                        assert_eq!(enc.proba.nb_skip, 0);
                    } else {
                        assert_eq!(enc.proba.nb_skip, skips);
                        // The probability denominator is the whole frame, even
                        // when method 0 sampled only its initial macroblocks.
                        let p = ((total - skips as usize) * 255 / total) as u8;
                        assert_eq!(enc.proba.skip_proba, p);
                        assert_eq!(enc.proba.use_skip_proba, p < 250);
                    }
                    visited = 0;
                    skips = 0;
                    top_y.fill([0; 4]);
                    top_uv.fill([0; 4]);
                    top_dc.fill(0);
                }
                VP8DirectLoopEvent::StatEnd {
                    header_cost, retry, ..
                } => {
                    assert_eq!(visited, probe);
                    assert_eq!(enc.proba.nb_skip, skips);
                    assert_eq!(retry, budget > 0 && header_cost > u64::from(a[5]));
                    if retry {
                        coverage[4] += 1;
                        budget /= 2;
                    }
                }
                VP8DirectLoopEvent::Macroblock {
                    statistics,
                    index,
                    score: rd,
                    iterator: it,
                    ..
                } => {
                    assert_eq!(index, visited);
                    visited += 1;
                    let mb = enc.mb_info[index];
                    let i16 = mb.r#type == 1;
                    coverage[3] += usize::from(!i16);
                    if it.x == 0 {
                        left_y.fill(0);
                        left_uv.fill(0);
                        left_dc = 0;
                        coverage[5] += usize::from(it.y > 0);
                    }
                    skips += i32::from(mb.skip);
                    if !statistics && enc.proba.use_skip_proba && mb.skip {
                        coverage[2] += 1;
                        top_y[it.x].fill(0);
                        top_uv[it.x].fill(0);
                        left_y.fill(0);
                        left_uv.fill(0);
                        if i16 {
                            top_dc[it.x] = 0;
                            left_dc = 0;
                        }
                        assert_eq!(enc.nz[it.x + 1], u32::from(top_dc[it.x]) << 24);
                    } else {
                        coverage[usize::from(!statistics)] += 1;
                        let probs = cost::probability_words(&enc.proba.coeffs);
                        let mut block =
                            |coeffs: &[i16; 16], first: usize, ty: usize, ctx: usize| {
                                let words = tokens::block(first, ty, ctx, *coeffs);
                                for t in tokens::expected_tokens(&words) {
                                    let slot = usize::from(t & 0x3fff);
                                    let bit = t & 0x8000 != 0;
                                    if statistics && t & 0x4000 == 0 {
                                        let mut v = u64::from(stats[slot]);
                                        if v >= 0xfffe0000 {
                                            v = ((v + 1) % (1u64 << 32)) / 2 & 0x7fff7fff;
                                        }
                                        stats[slot] = (v + 65536 + u64::from(bit)) as u32;
                                    } else if !statistics {
                                        bits.push((
                                            bit,
                                            if t & 0x4000 != 0 {
                                                t as u8
                                            } else {
                                                probs[slot] as u8
                                            },
                                        ));
                                    }
                                }
                                u8::from(coeffs[first..].iter().any(|&v| v != 0))
                            };
                        if i16 {
                            let nz =
                                block(&rd.y_dc_levels, 0, 1, usize::from(top_dc[it.x] + left_dc));
                            top_dc[it.x] = nz;
                            left_dc = nz;
                        }
                        for y in 0..4 {
                            for x in 0..4 {
                                let nz = block(
                                    &rd.y_ac_levels[4 * y + x],
                                    usize::from(i16),
                                    if i16 { 0 } else { 3 },
                                    usize::from(top_y[it.x][x] + left_y[y]),
                                );
                                top_y[it.x][x] = nz;
                                left_y[y] = nz;
                            }
                        }
                        for ch in 0..2 {
                            for y in 0..2 {
                                for x in 0..2 {
                                    let nz = block(
                                        &rd.uv_levels[4 * ch + 2 * y + x],
                                        0,
                                        2,
                                        usize::from(top_uv[it.x][2 * ch + x] + left_uv[2 * ch + y]),
                                    );
                                    top_uv[it.x][2 * ch + x] = nz;
                                    left_uv[2 * ch + y] = nz;
                                }
                            }
                        }
                        assert_eq!(&it.top_nz[..4], &top_y[it.x]);
                        assert_eq!(&it.left_nz[..4], &left_y);
                        assert_eq!(&it.top_nz[4..8], &top_uv[it.x]);
                        assert_eq!(&it.left_nz[4..8], &left_uv);
                        assert_eq!(it.top_nz[8], top_dc[it.x]);
                    }
                    assert_eq!(it.left_nz[8], left_dc);
                    assert_eq!(cost::stats_words(&enc.proba), stats);
                }
            }
        })
        .unwrap();
        assert_eq!(visited, total);
        let mut decoder = Decoder::new(VP8BitWriterBuf(&bw));
        for (bit, p) in bits {
            assert_eq!(decoder.bit(p), bit);
        }
    }
    assert!(coverage.iter().all(|&n| n > 0), "{coverage:?}");
    eprintln!("direct independent checks: statistics/coded/omitted/I4/retries/rows = {coverage:?}");
}

#[test]
fn direct_and_basic_token_header_retries_use_strict_limits() {
    for method in 0..=3 {
        let case = support::vp8_loop::frame_case(17, 19, 75., 4, 4, true);
        let a = &case.input;
        let pic = support::vp8_loop::picture(a);
        let first_cost = if method < 3 { 1555u64 } else { 20912 };
        for limit in [first_cost - 1, first_cost, first_cost + 1] {
            let mut enc = VP8Encoder::new_with_method(17, 19, true, method).unwrap();
            enc.config.quality = 75.;
            VP8EncAnalyze(&mut enc, &pic);
            let mut observed = None;
            if enc.use_tokens {
                VP8EncTokenLoopWithObserver(&mut enc, &pic, 75., limit, |_, event| {
                    if let VP8TokenLoopEvent::PassEnd {
                        pass: 0,
                        header_cost,
                        retry,
                        ..
                    } = event
                    {
                        observed = Some((header_cost, retry));
                    }
                })
                .unwrap();
            } else {
                VP8EncLoopWithObserver(&mut enc, &pic, 75., limit, |_, event| {
                    if let VP8DirectLoopEvent::StatEnd {
                        pass: 0,
                        header_cost,
                        retry,
                    } = event
                    {
                        observed = Some((header_cost, retry));
                    }
                })
                .unwrap();
            }
            assert_eq!(observed, Some((first_cost, limit < first_cost)));
        }
    }
}
