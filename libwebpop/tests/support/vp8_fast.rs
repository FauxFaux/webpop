use super::{Case, SEED, random, vp8_analysis as analysis, vp8_search as search};

pub fn fast_case(quality: f32, pattern: u32) -> Case {
    let mut seed = SEED ^ pattern;
    let mut a = vec![quality.to_bits()];
    a.extend((0..512).map(|i| {
        let (x, y) = (i % 32, i / 32);
        match pattern {
            0 => 0,
            1 => 128,
            2 => 255,
            3 => 255 * ((x / 4 + y / 4) & 1),
            4 => {
                if x < 4 && y < 4 {
                    255
                } else {
                    0
                }
            }
            5 => (x * 13 + y * 7) & 255,
            6 => 128 + ((x + y) & 1),
            _ => random(&mut seed) & 255,
        }
    }));
    Case::new("VP8FastAnalyze", a)
}
pub fn distortion_case(method: u32, family: u32, q: u32, flags: u32, pattern: u32) -> Case {
    let mut a = vec![
        method,
        family,
        (pattern + q) % 4,
        65536,
        0,
        u32::from(method >= 2),
        u32::from(method >= 1),
        1,
    ];
    a.extend(search::search_case(2, q, flags, pattern).input);
    Case::new("VP8DistortionSearch", a)
}
pub fn analysis_case(method: u32, quality: f32, w: u32, h: u32, nb: u32, pattern: u32) -> Case {
    let mut a = vec![method, quality.to_bits()];
    a.extend(analysis::frame_case(w, h, nb, pattern).input);
    Case::new("VP8FastAnalysisFrame", a)
}
pub fn frame_case(method: u32, quality: f32, w: u32, h: u32, nb: u32, pattern: u32) -> Case {
    let frame = analysis::frame_case(w, h, nb, pattern);
    let mut a = vec![method];
    a.extend_from_slice(&frame.input[..3]);
    a.extend([quality.to_bits(), 65536]);
    a.extend_from_slice(&frame.input[3..]);
    Case::new("VP8FastSearchFrame", a)
}
pub fn boundary_cases() -> Vec<Case> {
    [16116, 16117, 16118]
        .into_iter()
        .map(|budget| {
            let mut case = distortion_case(2, 1, 31, 7, 6);
            case.input[3] = budget;
            case
        })
        .collect()
}
pub fn flat_border_case(family: u32, flags: u32) -> Case {
    let mut case = distortion_case(2, family, 63, flags, 1);
    case.input[8 + 122..8 + 634].fill(128);
    case
}
pub fn tie_case() -> Case {
    // TM and horizontal predictions are identical and have equal mode costs.
    // DC and vertical incur distortion on this row-varying source.
    let mut case = distortion_case(0, 1, 63, 3, 1);
    let a = &mut case.input[8..];
    a[16] = 100;
    a[58..90].fill(100);
    for y in 0..16 {
        a[17 + y] = 30 + 10 * y as u32;
        for x in 0..16 {
            a[122 + y * 32 + x] = a[17 + y];
        }
    }
    case
}
pub fn fixture_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in [0., 75., 99.9, 100.] {
        for p in [0, 1, 3, 4] {
            cases.push(fast_case(q, p));
        }
    }
    for method in 0..3 {
        for family in 0..2 {
            for flags in [0, 3, 7] {
                cases.push(distortion_case(
                    method,
                    family,
                    63,
                    flags,
                    if flags == 0 { 1 } else { 6 },
                ));
            }
        }
        for nb in [1, 4] {
            cases.push(analysis_case(method, 98.5, 17, 19, nb, 6));
            cases.push(frame_case(method, 75., 17, 19, nb, 6));
        }
        cases.push(frame_case(method, 100., 33, 17, 4, 1));
    }
    for budget in [0, 210, 211, 640, 65536] {
        let mut case = distortion_case(2, 1, 31, 7, 6);
        case.input[3] = budget;
        cases.push(case);
    }
    cases.extend(boundary_cases());
    cases.push(tie_case());
    for flags in [0, 1, 2] {
        cases.push(flat_border_case(0, flags));
    }
    cases
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for q in 0..=100 {
        for p in 0..8 {
            cases.push(fast_case(q as f32, p));
        }
    }
    for q in [11.99, 12., 22.99, 23., 44.99, 45., 98.5, 99.99] {
        for p in 0..8 {
            cases.push(fast_case(q, p));
        }
    }
    for method in 0..3 {
        for q in [0, 1, 7, 31, 63, 95, 126, 127] {
            for family in 0..2 {
                for flags in 0..8 {
                    for p in 0..10 {
                        cases.push(distortion_case(method, family, q, flags, p));
                    }
                }
            }
        }
        for flags in 0..8 {
            for both in 0..2 {
                for uv in 0..2 {
                    for budget in [0, 210, 211, 640, 65536] {
                        let mut case = distortion_case(method, flags % 2, 37, flags, 6);
                        case.input[3] = budget;
                        case.input[5] = both;
                        case.input[6] = uv;
                        case.input[7] = 0;
                        cases.push(case);
                    }
                }
            }
        }
        for (w, h) in [(1, 1), (1, 17), (17, 1), (16, 16), (17, 19), (33, 35)] {
            for nb in [1, 4] {
                for p in 0..10 {
                    cases.push(analysis_case(method, 98.5, w, h, nb, p));
                    cases.push(frame_case(
                        method,
                        if p % 2 == 0 { 75. } else { 100. },
                        w,
                        h,
                        nb,
                        p,
                    ));
                }
            }
        }
    }
    cases.extend(fixture_cases());
    cases
}

pub fn distortion_state(
    a: &[u32],
) -> (
    libwebpop::internal::enc::vp8i_enc::VP8Encoder,
    libwebpop::internal::enc::vp8i_enc::VP8EncIterator,
    libwebpop::internal::enc::vp8i_enc::VP8ModeScore,
    bool,
) {
    use super::vp8_reconstruct as reconstruction;
    use libwebpop::internal::enc::{quant_enc::*, vp8i_enc::*};
    let request = &a[8..];
    let (mut enc, mut it) = search::initialized(request);
    enc.config.method = a[0] as i32;
    enc.mb_header_limit = (u64::from(a[3]) | (u64::from(a[4]) << 32)) as i64;
    SetupMatrices(&mut enc);
    for (i, s) in enc.dqm.iter_mut().enumerate() {
        s.max_edge = 1 + i as i32;
    }
    let mb = &mut enc.mb_info[it.y * enc.mb_w + it.x];
    mb.r#type = a[1] as u8;
    mb.uv_mode = a[2] as u8;
    let mut rd = reconstruction::seed_score();
    rd.score = (u64::from(request[637]) | (u64::from(request[638]) << 32)) as i64;
    VP8MakeLuma16Preds(&mut it, &enc);
    VP8MakeChroma8Preds(&mut it, &enc);
    let result = if a[7] != 0 {
        VP8Decimate(&mut it, &mut enc, &mut rd, VP8RDLevel::RD_OPT_NONE)
    } else {
        RefineUsingDistortion(&mut it, &mut enc, a[5] != 0, a[6] != 0, &mut rd);
        false
    };
    (enc, it, rd, result)
}

pub fn evaluate(case: &Case) -> Vec<u32> {
    use super::vp8_reconstruct as reconstruction;
    use libwebpop::internal::enc::{analysis_enc::*, iterator_enc::*, vp8i_enc::*};
    let a = &case.input;
    match case.function.as_str() {
        "VP8FastAnalyze" => {
            let mut enc = analysis::seeded(16, 16, 1);
            enc.config.quality = f32::from_bits(a[0]);
            let mut it = VP8EncIterator::default();
            VP8IteratorInit(&mut enc, &mut it);
            it.yuv_in = std::array::from_fn(|i| a[1 + i] as u8);
            let mut out = vec![FastMBAnalyze(&mut it, &mut enc) as u32];
            analysis::mbs(&enc, &mut out);
            out.extend(enc.preds.iter().map(|&v| u32::from(v)));
            out
        }
        "VP8FastAnalysisFrame" => {
            let (method, quality) = (a[0] as i32, f32::from_bits(a[1]));
            let a = &a[2..];
            let pic = analysis::picture(a);
            let mut enc = analysis::seeded(a[0], a[1], a[2] as usize);
            enc.config.method = method;
            enc.config.quality = quality;
            let stats = if a[2] > 1 || method <= 1 {
                DoSegmentsJob(&mut enc, &pic)
            } else {
                VP8AnalysisStats::default()
            };
            let mut out = Vec::new();
            analysis::stats(&stats, &mut out);
            analysis::mbs(&enc, &mut out);
            assert_eq!(VP8EncAnalyze(&mut enc, &pic), stats);
            out.extend([
                enc.alpha as u32,
                enc.uv_alpha as u32,
                enc.segment_hdr.num_segments as u32,
                u32::from(enc.segment_hdr.update_map),
                enc.segment_hdr.size as u32,
            ]);
            analysis::segments(&enc, &mut out);
            analysis::mbs(&enc, &mut out);
            out.extend(enc.preds.iter().map(|&v| u32::from(v)));
            out
        }
        "VP8DistortionSearch" => {
            let (enc, it, rd, result) = distortion_state(a);
            let request = &a[8..];
            let mut out = search::state(request, &enc, &it, &rd, result);
            let mut score = Vec::new();
            reconstruction::score(&rd, &mut score);
            out[1..1 + reconstruction::SCORE_WORDS].copy_from_slice(&score);
            out
        }
        "VP8FastSearchFrame" => search::frame_evaluate_with_method(&a[1..], a[0] as i32),
        _ => unreachable!(),
    }
}
