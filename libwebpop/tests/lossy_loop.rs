#[path = "support/near_lossless.rs"]
mod fixture_support;
#[cfg(feature = "native-oracle")]
#[path = "support/native.rs"]
mod native;
mod support;
use support::vp8_loop as frames;
#[test]
fn committed_frame_loop_fixtures() {
    let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_loop.txt"));
    let cases = frames::fixture_cases();
    assert_eq!(fixtures.len(), cases.len());
    for ((c, out), expected) in fixtures.iter().zip(cases) {
        assert_eq!(c.function, expected.function);
        assert_eq!(c.input, expected.input);
        assert_eq!(c.rust(), *out);
    }
}
#[cfg(feature = "native-oracle")]
#[test]
fn frame_loop_matches_pinned_reference() {
    let cases = frames::cases();
    native::parallel_cases("vp8-frame-loop", &cases, |c, out| {
        native::compare(c, out, &c.rust())
    });
    eprintln!("{} exact frame-loop requests", cases.len());
}
#[cfg(feature = "native-oracle")]
#[test]
fn frame_loop_fixtures_match_pinned_reference() {
    use std::fmt::Write;
    let cases = frames::fixture_cases();
    let outputs = native::run("vp8-frame-loop-fixtures", &cases);
    for (c, out) in cases.iter().zip(&outputs) {
        native::compare(c, out, &c.rust());
    }
    if std::env::var_os("WEBPOP_UPDATE_FIXTURES").as_deref() == Some(std::ffi::OsStr::new("1")) {
        let mut text = format!(
            "# Pinned scalar C default VP8 token frame loop.\n# upstream={} seed={:08x}\n# See ../reference/LOSSY.md for provenance and regeneration.\n",
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
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lossy_loop.txt"),
            text,
        )
        .unwrap();
    } else {
        let fixtures = fixture_support::fixtures(include_str!("fixtures/lossy_loop.txt"));
        assert_eq!(fixtures.len(), cases.len());
        for ((stored, expected), (case, out)) in fixtures.iter().zip(cases.iter().zip(&outputs)) {
            assert_eq!(stored.function, case.function);
            assert_eq!(stored.input, case.input);
            native::compare(case, expected, out);
        }
    }
}

use libwebpop::internal::{
    enc::{
        analysis_enc::VP8EncAnalyze, filter_enc::VP8FilterStrengthFromDelta, frame_enc::*,
        vp8i_enc::*,
    },
    utils::bit_writer_utils::*,
};
use support::{vp8_boolean::Decoder, vp8_cost as cost, vp8_tokens as tokens};

#[test]
fn segment_probabilities_round_and_reset_only_when_map_is_implicit() {
    for case in frames::leaf_cases()
        .into_iter()
        .filter(|c| c.function == "VP8FrameSegments")
    {
        let a = &case.input;
        let out = case.rust();
        let mut counts = [0u64; 4];
        for &s in &a[5..] {
            counts[s as usize] += 1;
        }
        if a[0] == 1 {
            assert_eq!(out[0], 0);
            assert_eq!(out[1], 0);
            assert_eq!(&out[2..5], &a[2..5]);
            assert_eq!(&out[5..], &a[5..]);
            continue;
        }
        let ratio = |a: u64, b: u64| {
            if a + b == 0 {
                255
            } else {
                (255. * a as f64 / (a + b) as f64).round() as u8
            }
        };
        let p = [
            ratio(counts[0] + counts[1], counts[2] + counts[3]),
            ratio(counts[0], counts[1]),
            ratio(counts[2], counts[3]),
        ];
        assert_eq!(&out[2..5], &p.map(u32::from));
        assert_eq!(out[0] != 0, p != [255; 3]);
        for (&before, &after) in a[5..].iter().zip(&out[5..]) {
            assert_eq!(after, if p == [255; 3] { 0 } else { before });
        }
        let expected: u64 = (0..4)
            .map(|s| {
                counts[s]
                    * u64::from(
                        cost::bit_cost(s >= 2, p[0]) + cost::bit_cost(s % 2 != 0, p[1 + s / 2]),
                    )
            })
            .sum();
        assert_eq!(u64::from(out[1]), expected);
    }
    assert_eq!(GetProba(1023, 1), 255);
    for a in 0..=256 {
        for b in 0..=256 {
            let p = GetProba(a, b);
            if a + b != 0 {
                assert!(
                    (i64::from(p) * i64::from(a + b) - 255 * i64::from(a)).abs() * 2
                        <= i64::from(a + b)
                );
            } else {
                assert_eq!(p, 255);
            }
        }
    }
}
#[test]
fn final_filter_adjustment_uses_largest_segment_edge_and_never_weakens() {
    for case in frames::leaf_cases()
        .into_iter()
        .filter(|c| c.function == "VP8AdjustFilter")
    {
        let a = &case.input;
        let out = case.rust();
        if a[0] == 0 {
            assert_eq!(out[0], a[2]);
            for s in 0..4 {
                assert_eq!(out[1 + s], a[5 + 3 * s]);
            }
            continue;
        }
        let mut max = 0;
        for s in 0..4 {
            let delta = u64::from(a[3 + 3 * s]) * u64::from(a[4 + 3 * s]) / 8;
            let expected =
                a[5 + 3 * s].max(VP8FilterStrengthFromDelta(a[1] as i32, delta as i32) as u32);
            assert_eq!(out[1 + s], expected);
            max = max.max(expected);
        }
        assert_eq!(out[0], max);
    }
}
fn initialized(
    a: &[u32],
) -> (
    VP8Encoder,
    libwebpop::internal::enc::picture_enc::WebPYUVAPicture,
) {
    let pic = frames::picture(a);
    let mut enc = VP8Encoder::new(a[0], a[1], f32::from_bits(a[2]) <= 98.).unwrap();
    enc.segment_hdr.num_segments = a[3] as usize;
    enc.max_i4_header_bits = a[4] as i32;
    VP8EncAnalyze(&mut enc, &pic);
    (enc, pic)
}
#[test]
fn refresh_countdown_and_retry_boundaries_preserve_pass_state() {
    let mut periodic = 0;
    let mut retries = 0;
    let mut retained = 0;
    for case in frames::frame_cases()
        .into_iter()
        .filter(|c| c.input[0] >= 95 * 16 || c.input[5] != PARTITION0_SIZE_LIMIT as u32)
    {
        let a = &case.input;
        let (mut enc, pic) = initialized(a);
        let total = enc.mb_info.len();
        let interval = 97.max(total / 8 + 1);
        let mut starts = 0;
        let mut expected_budget = a[4];
        let mut previous_coeffs = None;
        let mut refresh_indices = Vec::new();
        let mut last_index = 0;
        VP8EncTokenLoopWithObserver(
            &mut enc,
            &pic,
            f32::from_bits(a[2]),
            u64::from(a[5]),
            |enc, event| match event {
                VP8TokenLoopEvent::PassStart { pass } => {
                    assert_eq!(pass, starts);
                    starts += 1;
                    assert_eq!(enc.max_i4_header_bits as u32, expected_budget);
                    assert!(
                        enc.proba
                            .stats
                            .iter()
                            .flatten()
                            .flatten()
                            .flatten()
                            .all(|&v| v == 0)
                    );
                    assert_eq!(enc.proba.nb_skip, 0);
                    assert!(!enc.proba.use_skip_proba);
                    if let Some(p) = previous_coeffs.take() {
                        assert_eq!(enc.proba.coeffs, p);
                        if p != libwebpop::internal::enc::tree_enc::VP8CoeffsProba0 {
                            retained += 1;
                        }
                    }
                    assert!(enc.nz.iter().all(|&v| v == 0));
                    assert!(enc.y_top.iter().chain(&enc.uv_top).all(|&v| v == 127));
                    if let Some(top) = &enc.top_derr {
                        assert!(top.iter().flatten().flatten().all(|&v| v == 0));
                    }
                    refresh_indices.clear();
                    last_index = 0;
                }
                VP8TokenLoopEvent::Refresh { index, .. } => {
                    refresh_indices.push(index);
                    periodic += 1;
                    assert_eq!(index, (refresh_indices.len()) * interval - 1);
                }
                VP8TokenLoopEvent::Macroblock { index, .. } => {
                    assert_eq!(index, last_index);
                    last_index += 1;
                }
                VP8TokenLoopEvent::PassEnd {
                    header_cost, retry, ..
                } => {
                    assert_eq!(last_index, total);
                    assert_eq!(refresh_indices.len(), total / interval);
                    assert_eq!(retry, expected_budget > 0 && header_cost > u64::from(a[5]));
                    if retry {
                        retries += 1;
                        expected_budget /= 2;
                        previous_coeffs = Some(enc.proba.coeffs);
                    }
                }
                _ => {}
            },
        )
        .unwrap();
        assert!(starts <= 18);
        assert!(previous_coeffs.is_none());
    }
    assert!(periodic > 100 && retries > 20 && retained > 0);
    // Put the synthetic limit exactly on either side of a measured first pass.
    let case = frames::frame_case(17, 19, 75., 4, 4, true);
    let a = &case.input;
    let mut first_cost = 0;
    let (mut enc, pic) = initialized(a);
    VP8EncTokenLoopWithObserver(&mut enc, &pic, 75., PARTITION0_SIZE_LIMIT, |_, event| {
        if let VP8TokenLoopEvent::PassEnd { header_cost, .. } = event {
            first_cost = header_cost;
        }
    })
    .unwrap();
    for (limit, expected) in [
        (first_cost - 1, true),
        (first_cost, false),
        (first_cost + 1, false),
    ] {
        let (mut enc, pic) = initialized(a);
        let mut first_retry = None;
        VP8EncTokenLoopWithObserver(&mut enc, &pic, 75., limit, |_, event| {
            if let VP8TokenLoopEvent::PassEnd { retry, .. } = event {
                first_retry.get_or_insert(retry);
            }
        })
        .unwrap();
        assert_eq!(first_retry, Some(expected));
    }
    eprintln!(
        "{periodic} probability refreshes, {retries} retries, {retained} retained nondefault probability states"
    );
}

#[test]
fn raster_tokens_follow_independent_context_grids_and_decode() {
    let mut macroblocks = 0;
    let mut all_bits = 0;
    let mut skipped = 0;
    let mut intra4 = 0;
    for case in frames::frame_cases()
        .into_iter()
        .filter(|c| c.input[0] <= 65 && c.input[1] <= 49)
        .step_by(7)
    {
        let a = &case.input;
        let (mut enc, pic) = initialized(a);
        let mut top_y = vec![[0u8; 4]; enc.mb_w];
        let mut top_uv = vec![[0u8; 4]; enc.mb_w];
        let mut top_dc = vec![0u8; enc.mb_w];
        let mut left_y = [0u8; 4];
        let mut left_uv = [0u8; 4];
        let mut left_dc = 0u8;
        let mut expected = Vec::<u16>::new();
        let mut stats = vec![0u32; 1056];
        let mut final_probs = Vec::new();
        let bw = VP8EncTokenLoopWithObserver(
            &mut enc,
            &pic,
            f32::from_bits(a[2]),
            u64::from(a[5]),
            |enc, event| match event {
                VP8TokenLoopEvent::PassStart { .. } => {
                    top_y.fill([0; 4]);
                    top_uv.fill([0; 4]);
                    top_dc.fill(0);
                    expected.clear();
                    stats.fill(0);
                }
                VP8TokenLoopEvent::Macroblock {
                    score: rd,
                    iterator: it,
                    tokens,
                    ..
                } => {
                    macroblocks += 1;
                    let mb = enc.mb_info[it.y * enc.mb_w + it.x];
                    skipped += usize::from(mb.skip);
                    intra4 += usize::from(mb.r#type == 0);
                    if it.x == 0 {
                        left_y.fill(0);
                        left_uv.fill(0);
                        left_dc = 0;
                    }
                    let mut block = |coeffs: &[i16; 16], first, ty, ctx| {
                        let mut words = vec![first as u32, ty as u32, ctx as u32];
                        words.extend(coeffs.iter().map(|&v| v as i32 as u32));
                        for t in tokens::expected_tokens(&words) {
                            if t & 0x4000 == 0 {
                                let s = &mut stats[usize::from(t & 0x3fff)];
                                let mut v = u64::from(*s);
                                if v >= 0xfffe0000 {
                                    v = ((v + 1) % (1u64 << 32)) / 2 & 0x7fff7fff;
                                }
                                *s = (v + 65536 + u64::from(t >> 15)) as u32;
                            }
                            expected.push(t);
                        }
                        u8::from(coeffs[first..].iter().any(|&v| v != 0))
                    };
                    let i16 = mb.r#type == 1;
                    if i16 {
                        let nz = block(&rd.y_dc_levels, 0, 1, usize::from(top_dc[it.x] + left_dc));
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
                    assert_eq!(it.left_nz[8], left_dc);
                    assert_eq!(tokens.len(), expected.len());
                    assert_eq!(cost::stats_words(&enc.proba), stats);
                }
                VP8TokenLoopEvent::Emit { tokens } => {
                    assert_eq!(tokens.tokens().collect::<Vec<_>>(), expected);
                    final_probs = cost::probability_words(&enc.proba.coeffs);
                }
                _ => {}
            },
        )
        .unwrap();
        let mut decoder = Decoder::new(VP8BitWriterBuf(&bw));
        for t in expected {
            let p = if t & 0x4000 != 0 {
                t as u8
            } else {
                final_probs[usize::from(t & 0x3fff)] as u8
            };
            assert_eq!(decoder.bit(p), t & 0x8000 != 0);
            all_bits += 1;
        }
    }
    assert!(macroblocks > 100 && skipped > 0 && intra4 > 0 && all_bits > 10000);
    eprintln!(
        "independent context grids: {macroblocks} macroblocks, {intra4} I4, {skipped} skip flags; {all_bits} token bits decoded"
    );
}

#[test]
fn opaque_default_files_decode_with_expected_geometry_and_alpha() {
    let mut total = 0;
    for case in frames::frame_cases()
        .into_iter()
        .chain(frames::rgba_cases())
        .enumerate()
        .filter(|(i, c)| i % 5 == 0 || (c.input[6] == 1 && c.input[0] == 128 && c.input[1] == 128))
        .map(|(_, c)| c)
    {
        let (_, bytes) = frames::encode(&case.input);
        let pic = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(pic.dimensions(), (case.input[0], case.input[1]));
        assert!(pic.pixels().all(|p| p.0[3] == 255));
        if case.input[6] == 1 && case.input[0] == 128 && case.input[1] == 128 {
            let a = &case.input;
            let mut error = 0u64;
            for y in 0..128 {
                for x in 0..128 {
                    for c in 0..3 {
                        let delta = i64::from(pic.get_pixel(x as u32, y as u32).0[c])
                            - i64::from(a[9 + y * a[8] as usize + 4 * x + c]);
                        error += (delta * delta) as u64;
                    }
                }
            }
            let psnr = 10. * (255f64.powi(2) * 128. * 128. * 3. / error as f64).log10();
            eprintln!(
                "real sample quality {}: {} bytes, RGB PSNR {psnr:.3} dB",
                f32::from_bits(a[2]),
                bytes.len()
            );
        }
        total += 1;
    }
    eprintln!("{total} independent opaque decodes");
}
#[cfg(feature = "native-oracle")]
#[test]
fn opaque_files_match_normal_public_c_encoder_and_decode() {
    let loops = frames::rgba_cases();
    let cases: Vec<_> = loops
        .iter()
        .map(|c| {
            let a = &c.input;
            let mut words = vec![a[0], a[1], a[8], a[2]];
            words.extend_from_slice(&a[9..]);
            support::Case::new("WebPEncodeRGBA", words)
        })
        .collect();
    native::parallel_cases("vp8-default-public", &cases, |case, out| {
        let a = &case.input;
        let mut words = vec![
            a[0],
            a[1],
            a[3],
            4,
            65536,
            PARTITION0_SIZE_LIMIT as u32,
            1,
            0,
            a[2],
        ];
        words.extend_from_slice(&a[4..]);
        let actual = frames::encode(&words).0;
        native::compare(case, out, &actual);
        support::lossy::check_decoded(case, &actual);
    });
    eprintln!(
        "{} default-path files match normal public C encoding",
        cases.len()
    );
}
